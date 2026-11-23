//! Frame acquisition on top of windows-capture (Windows Graphics Capture).
//!
//! `grab` runs one capture session on a window or monitor, crops each frame on
//! the GPU, and (optionally) waits until the cropped pixels stop changing.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use image::RgbaImage;
use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::error::SightError;
use crate::win::{self, Rect};

/// What part of each frame to keep.
pub enum CropSpec {
    /// A fixed rect in frame coordinates (monitor capture).
    Rect(Rect),
    /// A window's DWM frame bounds. WGC window frames are usually exactly the
    /// DWM bounds, but some are the full GetWindowRect (with invisible borders);
    /// the frame size at arrival time tells which.
    WindowFrame { frame: Rect, window: Rect },
}

impl CropSpec {
    fn resolve(&self, fw: i32, fh: i32) -> (Rect, (i32, i32)) {
        match self {
            Self::Rect(r) => (*r, (0, 0)),
            Self::WindowFrame { frame, window } => {
                if (fw, fh) == (window.w, window.h) && (fw, fh) != (frame.w, frame.h) {
                    let r = Rect {
                        x: frame.x - window.x,
                        y: frame.y - window.y,
                        w: frame.w,
                        h: frame.h,
                    };
                    (r, (window.x, window.y))
                } else {
                    (
                        Rect {
                            x: 0,
                            y: 0,
                            w: frame.w,
                            h: frame.h,
                        },
                        (frame.x, frame.y),
                    )
                }
            }
        }
    }
}

pub enum Source {
    Window(u64),
    Monitor(isize),
}

pub struct Grab {
    pub img: RgbaImage,
    /// Crop actually applied, in frame (texture) coordinates.
    pub crop: Rect,
    /// Screen position of the frame texture's (0,0); only set for window sources.
    pub origin: (i32, i32),
    /// False when `--settle` was requested and the timeout hit first.
    pub settled: bool,
}

struct Latest {
    bgra: Vec<u8>,
    crop: Rect,
    origin: (i32, i32),
    hash: u64,
    changed_at: Instant,
}

#[derive(Default)]
struct Shared {
    latest: Mutex<Option<Latest>>,
    error: Mutex<Option<String>>,
}

struct Handler {
    shared: Arc<Shared>,
    want: Arc<CropSpec>,
    tmp: Vec<u8>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (Arc<Shared>, Arc<CropSpec>);
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (shared, want) = ctx.flags;
        Ok(Self {
            shared,
            want,
            tmp: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let (fw, fh) = (frame.width() as i32, frame.height() as i32);
        let (w, origin) = self.want.resolve(fw, fh);
        let (x0, y0) = (w.x.clamp(0, fw), w.y.clamp(0, fh));
        let (x1, y1) = (w.right().clamp(0, fw), w.bottom().clamp(0, fh));
        if x1 <= x0 || y1 <= y0 {
            *self.shared.error.lock().unwrap() =
                Some(format!("crop {w:?} lies outside the {fw}x{fh} frame"));
            return Ok(());
        }
        let buf = frame
            .buffer_crop(x0 as u32, y0 as u32, x1 as u32, y1 as u32)
            .map_err(|e| e.to_string())?;
        let data = buf.as_nopadding_buffer(&mut self.tmp);
        let mut hasher = DefaultHasher::new();
        hasher.write(data);
        let hash = hasher.finish();

        let mut latest = self.shared.latest.lock().unwrap();
        if latest.as_ref().is_some_and(|l| l.hash == hash) {
            return Ok(()); // new frame, identical pixels: not a change
        }
        *latest = Some(Latest {
            bgra: data.to_vec(),
            crop: Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            },
            origin,
            hash,
            changed_at: Instant::now(),
        });
        Ok(())
    }
}

fn internal(e: impl std::fmt::Display) -> SightError {
    SightError::Internal(format!("capture failed: {e}"))
}

fn cursor_setting() -> CursorCaptureSettings {
    if GraphicsCaptureApi::is_cursor_settings_supported().unwrap_or(false) {
        CursorCaptureSettings::WithoutCursor
    } else {
        CursorCaptureSettings::Default
    }
}

fn border_setting() -> DrawBorderSettings {
    if GraphicsCaptureApi::is_border_settings_supported().unwrap_or(false) {
        DrawBorderSettings::WithoutBorder
    } else {
        DrawBorderSettings::Default
    }
}

/// Capture `crop` (frame coordinates) from `source`. With `settle`, keep
/// capturing until the cropped pixels have been unchanged for that long or
/// `deadline` passes (then `settled` is false but the latest image is kept).
pub fn grab(
    source: &Source,
    crop: CropSpec,
    settle: Option<Duration>,
    deadline: Instant,
) -> Result<Grab, SightError> {
    let shared = Arc::new(Shared::default());
    let flags = (shared.clone(), Arc::new(crop));
    macro_rules! start {
        ($item:expr) => {
            Handler::start_free_threaded(Settings::new(
                $item,
                cursor_setting(),
                border_setting(),
                SecondaryWindowSettings::Default,
                MinimumUpdateIntervalSettings::Default,
                DirtyRegionSettings::Default,
                ColorFormat::Bgra8,
                flags,
            ))
        };
    }
    let control = match source {
        Source::Window(h) => {
            start!(Window::from_raw_hwnd(win::hwnd_from_u64(*h).0))
        }
        Source::Monitor(h) => {
            start!(Monitor::from_raw_hmonitor(*h as *mut std::ffi::c_void))
        }
    }
    .map_err(internal)?;

    let mut settled = false;
    let mut failure = None;
    loop {
        std::thread::sleep(Duration::from_millis(5));
        if let Some(e) = shared.error.lock().unwrap().clone() {
            failure = Some(e);
            break;
        }
        let now = Instant::now();
        let have = {
            let l = shared.latest.lock().unwrap();
            l.as_ref()
                .map(|l| now.saturating_duration_since(l.changed_at))
        };
        match (have, settle) {
            (Some(_), None) => {
                settled = true;
                break;
            }
            (Some(quiet), Some(s)) if quiet >= s => {
                settled = true;
                break;
            }
            _ => {}
        }
        if now >= deadline {
            break;
        }
    }
    let _ = control.stop();

    if let Some(e) = failure {
        return Err(SightError::Internal(e));
    }
    let latest = shared.latest.lock().unwrap().take();
    let Some(l) = latest else {
        return Err(SightError::Timeout(
            "no frame arrived before --timeout".into(),
        ));
    };
    let mut rgba = l.bgra;
    let (pixels, _) = rgba.as_chunks_mut::<4>();
    for px in pixels {
        px.swap(0, 2);
        px[3] = 255;
    }
    let img = RgbaImage::from_raw(l.crop.w as u32, l.crop.h as u32, rgba)
        .ok_or_else(|| SightError::Internal("frame buffer size mismatch".into()))?;
    Ok(Grab {
        img,
        crop: l.crop,
        origin: l.origin,
        settled,
    })
}

/// Screen-capture an arbitrary physical-pixel region (so popups and menus are
/// included), stitching across monitors. Areas outside every monitor are black.
pub fn grab_region(
    region: Rect,
    settle: Option<Duration>,
    deadline: Instant,
    warnings: &mut Vec<String>,
) -> Result<(RgbaImage, bool), SightError> {
    let mons = win::monitors();
    let mut canvas = RgbaImage::from_pixel(
        region.w as u32,
        region.h as u32,
        image::Rgba([0, 0, 0, 255]),
    );
    let mut settled = true;
    let mut covered = 0i64;
    for m in &mons {
        let Some(inter) = region.intersect(&m.rect) else {
            continue;
        };
        covered += inter.area();
        let local = Rect {
            x: inter.x - m.rect.x,
            y: inter.y - m.rect.y,
            w: inter.w,
            h: inter.h,
        };
        let g = grab(
            &Source::Monitor(m.hmon),
            CropSpec::Rect(local),
            settle,
            deadline,
        )?;
        settled &= g.settled;
        image::imageops::replace(
            &mut canvas,
            &g.img,
            i64::from(inter.x - region.x),
            i64::from(inter.y - region.y),
        );
    }
    if covered == 0 {
        return Err(SightError::InvalidArgs(format!(
            "region {},{},{},{} does not intersect any monitor",
            region.x, region.y, region.w, region.h
        )));
    }
    if covered < region.area() {
        warnings.push("region_partially_offscreen".into());
    }
    Ok((canvas, settled))
}
