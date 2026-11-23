use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Args;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use serde_json::{Value, json};
use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::capture::{self, CropSpec, Source};
use crate::error::SightError;
use crate::win::{self, Rect, WinInfo};

#[derive(Clone, Debug)]
pub struct Region(Rect);

fn parse_region(s: &str) -> Result<Region, String> {
    let p: Vec<&str> = s.split(',').collect();
    if p.len() != 4 {
        return Err("expected x,y,w,h".into());
    }
    let n = |i: usize| p[i].trim().parse::<i32>().map_err(|e| e.to_string());
    let (w, h) = (n(2)?, n(3)?);
    if w <= 0 || h <= 0 {
        return Err("w and h must be positive".into());
    }
    Ok(Region(Rect {
        x: n(0)?,
        y: n(1)?,
        w,
        h,
    }))
}

#[derive(Args)]
pub struct ShotArgs {
    /// Capture the largest visible window of this process
    #[arg(long)]
    pub pid: Option<u32>,
    /// Capture this window handle (decimal, as printed by `list`)
    #[arg(long)]
    pub hwnd: Option<u64>,
    /// Screen region x,y,w,h in physical pixels (screen capture, popups included)
    #[arg(long, value_parser = parse_region)]
    pub region: Option<Region>,
    /// Resize factor (nearest neighbour when > 1); > 1 writes PNG
    #[arg(long, default_value_t = 1.0)]
    pub scale: f32,
    /// Downscale so the long edge is at most this many pixels (0 = no limit)
    #[arg(long, default_value_t = 1568)]
    pub max_edge: u32,
    /// Wait until the image has not changed for this many ms (up to --timeout)
    #[arg(long)]
    pub settle: Option<u64>,
    /// Overall capture timeout (ms)
    #[arg(long, default_value_t = 5000)]
    pub timeout: u64,
    /// Output path (.png / .jpg pick the format); default: temp dir
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// A failure that may still have produced an image (settle timeout).
pub struct ShotFail {
    pub err: SightError,
    pub data: Option<Value>,
    pub warnings: Vec<String>,
}

impl From<SightError> for Box<ShotFail> {
    fn from(err: SightError) -> Self {
        Box::new(ShotFail {
            err,
            data: None,
            warnings: Vec::new(),
        })
    }
}

struct Captured {
    img: RgbaImage,
    maps_to: Rect,
    dpi_scale: f64,
    monitor_dpi_scale: f64,
    settled: bool,
    /// Popups that forced the screen-capture fallback (window sources only).
    popups: Vec<Value>,
}

/// Window classes that are popups by nature (menus, tooltips, dropdown lists,
/// XAML flyouts) even when they are not owned by the target.
const POPUP_CLASSES: &[&str] = &[
    "#32768",                                     // Win32 menu
    "tooltips_class32",                           // Win32 tooltip
    "ComboLBox",                                  // combo box dropdown list
    "Xaml_WindowedPopupClass",                    // XAML island flyout
    "Microsoft.UI.Content.PopupWindowSiteBridge", // WinUI 3 menus/flyouts (new Notepad)
];

/// Why `w` counts as a popup of `target`: owner chain reaches it, or known class.
fn popup_reason(w: &WinInfo, target: u64, all: &[WinInfo]) -> Option<&'static str> {
    let mut owner = w.owner;
    for _ in 0..16 {
        if owner == 0 {
            break;
        }
        if owner == target {
            return Some("owner");
        }
        owner = all.iter().find(|o| o.hwnd == owner).map_or(0, |o| o.owner);
    }
    POPUP_CLASSES.contains(&w.class.as_str()).then_some("class")
}

fn capture_window(
    hwnd: u64,
    settle: Option<Duration>,
    deadline: Instant,
    warnings: &mut Vec<String>,
) -> Result<Captured, SightError> {
    let all = win::enum_windows();
    let info = all
        .iter()
        .find(|w| w.hwnd == hwnd)
        .ok_or_else(|| SightError::TargetNotFound(format!("no window with hwnd {hwnd}")))?;
    if info.minimized {
        return Err(SightError::WindowMinimized);
    }
    if win::is_protected(hwnd) {
        return Err(SightError::WindowProtected);
    }
    let frame = info.rect;
    let wr = win::window_rect(hwnd).unwrap_or(frame);
    if frame.is_empty() {
        return Err(SightError::TargetNotFound(format!(
            "window {hwnd} has an empty frame"
        )));
    }

    // Visible same-process WS_POPUP windows overlapping this one (menus,
    // dropdowns, tooltips) are not part of a window capture.
    let popups: Vec<(&WinInfo, &str)> = all
        .iter()
        .filter(|w| {
            w.hwnd != hwnd
                && w.pid == info.pid
                && w.visible
                && !w.cloaked
                && w.popup
                && !w.minimized
                && w.z < info.z
                && !w.rect.is_empty()
                && w.rect.intersect(&frame).is_some()
        })
        .filter_map(|w| popup_reason(w, hwnd, &all).map(|why| (w, why)))
        .collect();
    let popup_json: Vec<Value> = popups
        .iter()
        .map(|(w, why)| json!({"hwnd": w.hwnd, "class": w.class, "via": why}))
        .collect();
    if !popups.is_empty() {
        let union = popups.iter().fold(frame, |u, (p, _)| u.union(&p.rect));
        let (img, settled) = capture::grab_region(union, settle, deadline, warnings)?;
        warnings.push("popups_included_via_screen_capture".into());
        return Ok(Captured {
            img,
            maps_to: union,
            dpi_scale: info.dpi_scale,
            monitor_dpi_scale: info.monitor_dpi_scale,
            settled,
            popups: popup_json,
        });
    }

    let spec = CropSpec::WindowFrame { frame, window: wr };
    let g = capture::grab(&Source::Window(hwnd), spec, settle, deadline)?;
    let maps_to = Rect {
        x: g.origin.0 + g.crop.x,
        y: g.origin.1 + g.crop.y,
        w: g.crop.w,
        h: g.crop.h,
    };
    Ok(Captured {
        img: g.img,
        maps_to,
        dpi_scale: info.dpi_scale,
        monitor_dpi_scale: info.monitor_dpi_scale,
        settled: g.settled,
        popups: popup_json,
    })
}

/// Pick the largest visible window of a pid; the rest become warnings.
fn pick_pid_window(pid: u32, warnings: &mut Vec<String>) -> Result<u64, SightError> {
    let all: Vec<WinInfo> = win::enum_windows()
        .into_iter()
        .filter(|w| w.pid == pid && w.visible && !w.cloaked && !(w.rect.is_empty() && !w.minimized))
        .collect();
    // Shell desktop/taskbar windows are never what a caller means by "the app window".
    let shell = |w: &WinInfo| {
        POPUP_CLASSES.contains(&w.class.as_str())
            || matches!(
                w.class.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            )
    };
    let titled: Vec<&WinInfo> = all
        .iter()
        .filter(|w| !w.title.is_empty() && !shell(w))
        .collect();
    let mut cands: Vec<&WinInfo> = if titled.is_empty() {
        all.iter().collect()
    } else {
        titled
    };
    // Non-minimized first, then by area descending.
    cands.sort_by_key(|w| (w.minimized, std::cmp::Reverse(w.rect.area())));
    let Some(best) = cands.first() else {
        return Err(SightError::TargetNotFound(format!(
            "no visible window found for pid {pid}"
        )));
    };
    for o in &cands[1..] {
        warnings.push(format!(
            "other_candidate_window: hwnd={} title={:?} rect={},{},{}x{}",
            o.hwnd, o.title, o.rect.x, o.rect.y, o.rect.w, o.rect.h
        ));
    }
    Ok(best.hwnd)
}

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0');
    if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_string()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Fmt {
    Png,
    Jpeg,
}

fn timestamp() -> String {
    let t = unsafe { GetLocalTime() };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}-{:03}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds
    )
}

fn choose_output(args: &ShotArgs) -> Result<(PathBuf, Fmt), SightError> {
    let default = if args.scale > 1.0 {
        Fmt::Png
    } else {
        Fmt::Jpeg
    };
    let io = |e: std::io::Error| SightError::Internal(format!("cannot prepare output path: {e}"));
    let (path, fmt) = if let Some(p) = &args.out {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase());
        let fmt = match ext.as_deref() {
            Some("png") => Fmt::Png,
            Some("jpg" | "jpeg") => Fmt::Jpeg,
            _ => default,
        };
        (p.clone(), fmt)
    } else {
        let ext = if default == Fmt::Png { "png" } else { "jpg" };
        let dir = std::env::temp_dir().join("sightline");
        std::fs::create_dir_all(&dir).map_err(io)?;
        (dir.join(format!("shot-{}.{ext}", timestamp())), default)
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let path = std::path::absolute(&path).unwrap_or(path);
    Ok((path, fmt))
}

fn save(img: RgbaImage, path: &Path, fmt: Fmt) -> Result<(), SightError> {
    let io = |e: &dyn std::fmt::Display| SightError::Internal(format!("cannot write image: {e}"));
    let w = BufWriter::new(File::create(path).map_err(|e| io(&e))?);
    match fmt {
        Fmt::Png => DynamicImage::ImageRgba8(img)
            .write_with_encoder(PngEncoder::new(w))
            .map_err(|e| io(&e)),
        Fmt::Jpeg => DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(img).to_rgb8())
            .write_with_encoder(JpegEncoder::new_with_quality(w, 80))
            .map_err(|e| io(&e)),
    }
}

pub fn run(args: &ShotArgs) -> Result<(Value, Vec<String>), Box<ShotFail>> {
    let start = Instant::now();
    let targets = [
        args.pid.is_some(),
        args.hwnd.is_some(),
        args.region.is_some(),
    ];
    if targets.iter().filter(|t| **t).count() != 1 {
        return Err(SightError::InvalidArgs(
            "specify exactly one of --hwnd, --pid, --region".into(),
        )
        .into());
    }
    if !args.scale.is_finite() || args.scale <= 0.0 || args.scale > 16.0 {
        return Err(SightError::InvalidArgs("--scale must be in (0, 16]".into()).into());
    }
    let settle = args.settle.map(Duration::from_millis);
    let deadline = start + Duration::from_millis(args.timeout);
    let mut warnings = Vec::new();

    let cap = if let Some(Region(r)) = &args.region {
        let (img, settled) = capture::grab_region(*r, settle, deadline, &mut warnings)?;
        if win::spans_mixed_dpi(r) {
            warnings.push("region_spans_mixed_dpi".into());
        }
        // Region mode: the monitor of the top-left corner defines both values.
        let dpi = win::dpi_scale_at(r.x, r.y);
        Captured {
            img,
            maps_to: *r,
            dpi_scale: dpi,
            monitor_dpi_scale: dpi,
            settled,
            popups: Vec::new(),
        }
    } else {
        let hwnd = match (args.hwnd, args.pid) {
            (Some(h), _) => h,
            (None, Some(pid)) => pick_pid_window(pid, &mut warnings)?,
            (None, None) => unreachable!("validated above"),
        };
        capture_window(hwnd, settle, deadline, &mut warnings)?
    };

    if (cap.dpi_scale - cap.monitor_dpi_scale).abs() > f64::EPSILON {
        warnings.push("window_dpi_unaware_or_virtualized".into());
    }

    // Scale: --scale first, then clamp the long edge to --max-edge. Both are
    // folded into one resize so the reported scale is the true px-per-screen-px.
    let (sw, sh) = cap.img.dimensions();
    let long = f64::from(sw.max(sh));
    let mut s = f64::from(args.scale);
    if args.max_edge > 0 && long * s > f64::from(args.max_edge) {
        s = f64::from(args.max_edge) / long;
    }
    let (ow, oh) = (
        (f64::from(sw) * s).round().max(1.0) as u32,
        (f64::from(sh) * s).round().max(1.0) as u32,
    );
    let reported = {
        let out_long = f64::from(ow.max(oh));
        ((out_long / long) * 10000.0).round() / 10000.0
    };
    let img = if (ow, oh) == (sw, sh) {
        cap.img
    } else {
        let filter = if s > 1.0 {
            FilterType::Nearest
        } else {
            FilterType::Lanczos3
        };
        image::imageops::resize(&cap.img, ow, oh, filter)
    };

    let (path, fmt) = choose_output(args)?;
    save(img, &path, fmt)?;

    let m = cap.maps_to;
    let sc = fmt_num(reported);
    let mut data = json!({
        "path": path.to_string_lossy(),
        "image": {"w": ow, "h": oh},
        "maps_to": {"space": "screen_physical", "x": m.x, "y": m.y, "w": m.w, "h": m.h},
        "scale": reported,
        "dpi_scale": cap.dpi_scale,
        "monitor_dpi_scale": cap.monitor_dpi_scale,
        "formula": format!("screen_x = {} + px / {sc}; screen_y = {} + py / {sc}", m.x, m.y),
        "settled": cap.settled,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    });
    if args.region.is_none() {
        data["popups"] = Value::Array(cap.popups);
    }
    if cap.settled {
        Ok((data, warnings))
    } else {
        Err(Box::new(ShotFail {
            err: SightError::Timeout(format!(
                "image did not settle within --timeout {}ms (saved the latest frame anyway)",
                args.timeout
            )),
            data: Some(data),
            warnings,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_region_accepts_x_y_w_h() {
        let r = parse_region("10, -20,300,200").unwrap();
        assert_eq!(
            r.0,
            Rect {
                x: 10,
                y: -20,
                w: 300,
                h: 200
            }
        );
    }

    #[test]
    fn parse_region_rejects_bad_input() {
        assert!(parse_region("1,2,3").is_err());
        assert!(parse_region("a,b,c,d").is_err());
        assert!(parse_region("0,0,0,10").is_err());
        assert!(parse_region("0,0,10,-1").is_err());
    }

    #[test]
    fn fmt_num_trims_zeros_but_keeps_one_decimal() {
        assert_eq!(fmt_num(1.0), "1.0");
        assert_eq!(fmt_num(0.9273), "0.9273");
        assert_eq!(fmt_num(1.5), "1.5");
    }

    fn w(hwnd: u64, owner: u64, class: &str) -> WinInfo {
        WinInfo {
            hwnd,
            pid: 1,
            exe: String::new(),
            title: String::new(),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            monitor: 1,
            dpi_scale: 1.0,
            monitor_dpi_scale: 1.0,
            minimized: false,
            class: class.into(),
            owner,
            z: 0,
            visible: true,
            cloaked: false,
            popup: false,
        }
    }

    #[test]
    fn popup_reason_follows_owner_chain_and_class() {
        let all = vec![
            w(1, 0, "Main"),
            w(2, 1, "Dlg"),
            w(3, 2, "Menu"),
            w(4, 0, "#32768"),
            w(5, 0, "Other"),
        ];
        assert_eq!(popup_reason(&all[2], 1, &all), Some("owner"));
        assert_eq!(popup_reason(&all[3], 1, &all), Some("class"));
        assert_eq!(popup_reason(&all[4], 1, &all), None);
    }
}
