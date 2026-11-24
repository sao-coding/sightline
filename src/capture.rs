//! Frame acquisition on top of windows-capture (Windows Graphics Capture).
//!
//! `grab` runs one capture session on a window or monitor, crops each frame on
//! the GPU, and (optionally) waits for the cropped pixels to change and/or to
//! stop changing.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use image::RgbaImage;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use crate::baseline::Baseline;
use crate::error::SightError;
use crate::win::{self, Rect};

/// What part of each frame to keep.
#[derive(Clone)]
pub enum CropSpec {
    /// A fixed rect in frame coordinates (monitor capture).
    Rect(Rect),
    /// A window's DWM frame bounds. WGC window frames are usually exactly the
    /// DWM bounds, but some are the full GetWindowRect (with invisible borders);
    /// the frame size at arrival time tells which. `sub` optionally narrows the
    /// result to a rect relative to the DWM frame's top-left.
    WindowFrame {
        frame: Rect,
        window: Rect,
        sub: Option<Rect>,
    },
}

impl CropSpec {
    fn resolve(&self, fw: i32, fh: i32) -> (Rect, (i32, i32)) {
        match self {
            Self::Rect(r) => (*r, (0, 0)),
            Self::WindowFrame { frame, window, sub } => {
                let (base, origin) =
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
                    };
                let r = sub.map_or(base, |s| Rect {
                    x: base.x + s.x,
                    y: base.y + s.y,
                    w: s.w,
                    h: s.h,
                });
                (r, origin)
            }
        }
    }
}

pub enum Source {
    Window(u64),
    Monitor(isize),
}

/// How long and for what to wait. Cheap to clone; clones share the change
/// signal, so several concurrent sessions (one per monitor) agree on it.
#[derive(Clone)]
pub struct Opts {
    /// Quiet period required after the last change.
    pub settle: Option<Duration>,
    /// Number of differing pixels that counts as "the image changed". 1 is an
    /// exact compare; larger values ignore noise such as a blinking caret.
    pub min_change_px: usize,
    /// Wait for the first change (against the first frame) before settling.
    pub expect_change: bool,
    /// Reference point for `change_ms`.
    pub start: Instant,
    pub deadline: Instant,
    /// Compare the first frame against this earlier image instead of using it
    /// as the baseline (implies `expect_change`).
    pub baseline: Option<Arc<Baseline>>,
    changed: Arc<OnceLock<Instant>>,
}

impl Opts {
    pub fn new(
        settle: Option<Duration>,
        min_change_px: usize,
        expect_change: bool,
        start: Instant,
        deadline: Instant,
    ) -> Self {
        Self {
            settle,
            min_change_px: min_change_px.max(1),
            expect_change,
            start,
            deadline,
            baseline: None,
            changed: Arc::default(),
        }
    }

    pub fn with_baseline(&self, baseline: Option<Arc<Baseline>>) -> Self {
        Self {
            baseline,
            ..self.clone()
        }
    }

    /// Milliseconds from `start` to the first change, if one was seen.
    pub fn change_ms(&self) -> Option<u64> {
        self.changed
            .get()
            .map(|t| t.saturating_duration_since(self.start).as_millis() as u64)
    }
}

pub struct Grab {
    pub img: RgbaImage,
    /// Crop actually applied, in frame (texture) coordinates.
    pub crop: Rect,
    /// Screen position of the frame texture's (0,0); only set for window sources.
    pub origin: (i32, i32),
    /// False when the wait (change and/or settle) hit the deadline first.
    pub settled: bool,
}

struct Latest {
    bgra: Vec<u8>,
    crop: Rect,
    origin: (i32, i32),
    /// When this frame (the last accepted one) arrived.
    changed_at: Instant,
    /// When the first frame that differed from the baseline arrived.
    first_change: Option<Instant>,
    /// Counts accepted frames, so a poller can tell whether anything changed.
    seq: u64,
}

#[derive(Default)]
struct Shared {
    latest: Mutex<Option<Latest>>,
    error: Mutex<Option<String>>,
}

/// Number of pixels (4-byte BGRA groups) that differ; `usize::MAX` when the
/// buffers have different sizes (the window was resized).
fn differing_px(a: &[u8], b: &[u8]) -> usize {
    if a.len() != b.len() {
        return usize::MAX;
    }
    if a == b {
        return 0;
    }
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .filter(|(p, q)| p != q)
        .count()
}

struct Handler {
    shared: Arc<Shared>,
    want: Arc<CropSpec>,
    min_px: usize,
    baseline: Option<Arc<Baseline>>,
    tmp: Vec<u8>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (Arc<Shared>, Arc<CropSpec>, usize, Option<Arc<Baseline>>);
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (shared, want, min_px, baseline) = ctx.flags;
        Ok(Self {
            shared,
            want,
            min_px,
            baseline,
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
        let crop = Rect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        };

        let mut latest = self.shared.latest.lock().unwrap();
        let now = Instant::now();
        match latest.as_mut() {
            // Compared against the last *accepted* frame, so sub-threshold
            // noise is dropped but slow drift still adds up to a change.
            Some(l) => {
                if l.crop == crop && differing_px(&l.bgra, data) < self.min_px {
                    return Ok(());
                }
                l.bgra.clear();
                l.bgra.extend_from_slice(data);
                l.crop = crop;
                l.origin = origin;
                l.changed_at = now;
                l.first_change.get_or_insert(now);
                l.seq += 1;
            }
            None => {
                // With a baseline image, the first frame may already be a change.
                let first_change = self
                    .baseline
                    .as_ref()
                    .filter(|b| b.diff(data, crop.w as u32, crop.h as u32) >= self.min_px)
                    .map(|_| now);
                *latest = Some(Latest {
                    bgra: data.to_vec(),
                    crop,
                    origin,
                    changed_at: now,
                    first_change,
                    seq: 1,
                });
            }
        }
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

/// A running capture session; frames accumulate in `shared`.
pub struct Session {
    control: CaptureControl<Handler, String>,
    shared: Arc<Shared>,
}

/// A copy of the most recent accepted frame (raw BGRA).
pub struct FrameCopy {
    pub bgra: Vec<u8>,
    pub crop: Rect,
    pub origin: (i32, i32),
    /// Changes whenever the pixels changed since the previous copy.
    pub seq: u64,
}

impl Session {
    /// Start capturing `source`, cropped to `crop`; `opts` supplies the change
    /// threshold and baseline.
    pub fn start(source: &Source, crop: CropSpec, opts: &Opts) -> Result<Self, SightError> {
        let shared = Arc::new(Shared::default());
        let flags = (
            shared.clone(),
            Arc::new(crop),
            opts.min_change_px,
            opts.baseline.clone(),
        );
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
        Ok(Self { control, shared })
    }

    /// A failure reported by the capture thread, if any.
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().unwrap().clone()
    }

    /// Copy of the latest frame, or `None` before the first one arrives.
    pub fn frame(&self) -> Option<FrameCopy> {
        let l = self.shared.latest.lock().unwrap();
        l.as_ref().map(|l| FrameCopy {
            bgra: l.bgra.clone(),
            crop: l.crop,
            origin: l.origin,
            seq: l.seq,
        })
    }

    /// Sequence number of the latest frame (0 before the first); cheap.
    pub fn seq(&self) -> u64 {
        self.shared
            .latest
            .lock()
            .unwrap()
            .as_ref()
            .map_or(0, |l| l.seq)
    }

    pub fn stop(self) {
        let _ = self.control.stop();
    }
}

/// Capture `crop` (frame coordinates) from `source`.
///
/// The first frame is the baseline. With `opts.expect_change`, the wait first
/// needs a frame that differs from the baseline by at least `min_change_px`
/// pixels; then, with `opts.settle`, the image must stay unchanged for that
/// long. If `opts.deadline` passes first, `settled` is false but the latest
/// image is kept (`opts.change_ms()` tells whether a change was seen).
pub fn grab(source: &Source, crop: CropSpec, opts: &Opts) -> Result<Grab, SightError> {
    let Session { control, shared } = Session::start(source, crop, opts)?;

    let mut settled = false;
    let mut failure = None;
    loop {
        std::thread::sleep(Duration::from_millis(5));
        if let Some(e) = shared.error.lock().unwrap().clone() {
            failure = Some(e);
            break;
        }
        let now = Instant::now();
        let snapshot = {
            let l = shared.latest.lock().unwrap();
            l.as_ref()
                .map(|l| (now.saturating_duration_since(l.changed_at), l.first_change))
        };
        if let Some((quiet, first_change)) = snapshot {
            if let Some(t) = first_change {
                let _ = opts.changed.set(t);
            }
            let ready = !opts.expect_change || opts.changed.get().is_some();
            let calm = opts.settle.is_none_or(|s| quiet >= s);
            if ready && calm {
                settled = true;
                break;
            }
        }
        if now >= opts.deadline {
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

/// The slices of a screen region that lie on each monitor.
pub struct Part {
    pub hmon: isize,
    /// In screen coordinates.
    pub inter: Rect,
    /// In the monitor's own coordinates (for `CropSpec::Rect`).
    pub local: Rect,
}

/// Split `region` per monitor; the second value is the number of covered px.
pub fn region_parts(region: Rect) -> Result<(Vec<Part>, i64), SightError> {
    let mut parts = Vec::new();
    let mut covered = 0i64;
    for m in &win::monitors() {
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
        parts.push(Part {
            hmon: m.hmon,
            inter,
            local,
        });
    }
    if covered == 0 {
        return Err(SightError::InvalidArgs(format!(
            "region {},{},{},{} does not intersect any monitor",
            region.x, region.y, region.w, region.h
        )));
    }
    Ok((parts, covered))
}

/// Screen-capture an arbitrary physical-pixel region (so popups and menus are
/// included), stitching across monitors. Areas outside every monitor are black.
/// Monitors are captured concurrently, so they share one `--timeout`.
pub fn grab_region(
    region: Rect,
    opts: &Opts,
    warnings: &mut Vec<String>,
) -> Result<(RgbaImage, bool), SightError> {
    let mut canvas = RgbaImage::from_pixel(
        region.w as u32,
        region.h as u32,
        image::Rgba([0, 0, 0, 255]),
    );
    let (parts, covered) = region_parts(region)?;
    // Each monitor part compares against its own slice of the baseline.
    let part_opts = parts
        .iter()
        .map(|p| {
            let rel = Rect {
                x: p.inter.x - region.x,
                y: p.inter.y - region.y,
                w: p.inter.w,
                h: p.inter.h,
            };
            opts.baseline
                .as_ref()
                .map(|b| b.sub(rel))
                .transpose()
                .map(|b| opts.with_baseline(b))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let results: Vec<Result<Grab, SightError>> = std::thread::scope(|s| {
        let handles: Vec<_> = parts
            .iter()
            .zip(&part_opts)
            .map(|(p, o)| {
                s.spawn(move || grab(&Source::Monitor(p.hmon), CropSpec::Rect(p.local), o))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(SightError::Internal("capture thread panicked".into())))
            })
            .collect()
    });
    let mut settled = true;
    for (p, g) in parts.iter().zip(results) {
        let inter = p.inter;
        let g = g?;
        settled &= g.settled;
        image::imageops::replace(
            &mut canvas,
            &g.img,
            i64::from(inter.x - region.x),
            i64::from(inter.y - region.y),
        );
    }
    if covered < region.area() {
        warnings.push("region_partially_offscreen".into());
    }
    Ok((canvas, settled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differing_px_counts_pixels_not_bytes() {
        let a = [0u8, 0, 0, 255, 1, 1, 1, 255];
        let mut b = a;
        assert_eq!(differing_px(&a, &b), 0);
        b[0] = 9;
        b[1] = 9; // same pixel, two bytes
        assert_eq!(differing_px(&a, &b), 1);
        b[4] = 2;
        assert_eq!(differing_px(&a, &b), 2);
        assert_eq!(differing_px(&a, &b[..4]), usize::MAX);
    }

    #[test]
    fn window_frame_crop_is_relative_to_the_dwm_frame() {
        let frame = Rect {
            x: 100,
            y: 50,
            w: 800,
            h: 600,
        };
        let window = Rect {
            x: 92,
            y: 50,
            w: 816,
            h: 608,
        };
        let sub = Some(Rect {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        });
        let spec = CropSpec::WindowFrame { frame, window, sub };
        // Frame texture equals the DWM frame.
        let (r, origin) = spec.resolve(800, 600);
        assert_eq!((r.x, r.y, r.w, r.h), (10, 20, 30, 40));
        assert_eq!(origin, (100, 50));
        // Frame texture equals GetWindowRect (invisible borders included).
        let (r, origin) = spec.resolve(816, 608);
        assert_eq!((r.x, r.y, r.w, r.h), (8 + 10, 20, 30, 40));
        assert_eq!(origin, (92, 50));
        assert_eq!(origin.0 + r.x, 100 + 10); // same screen x either way
    }
}
