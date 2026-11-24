use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Args;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, RgbaImage};
use serde_json::{Value, json};
use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::baseline::{self, Loaded, Meta, Size, TargetId};
use crate::capture::{self, CropSpec, Opts, Source};
use crate::error::SightError;
use crate::win::{self, Rect, WinInfo};

#[derive(Clone, Debug)]
pub struct Region(pub Rect);

pub fn parse_region(s: &str) -> Result<Region, String> {
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

/// Target and waiting options shared by `shot` and `wait`.
#[derive(Args)]
pub struct CaptureArgs {
    /// Capture the largest visible window of this process
    #[arg(long)]
    pub pid: Option<u32>,
    /// Capture this window handle (decimal, as printed by `list`)
    #[arg(long)]
    pub hwnd: Option<u64>,
    /// Screen region x,y,w,h in physical pixels (screen capture, popups included)
    #[arg(long, value_parser = parse_region)]
    pub region: Option<Region>,
    /// Crop x,y,w,h in physical px, relative to the window's DWM frame top-left
    /// (--hwnd/--pid only); applied before --scale and --max-edge
    #[arg(long, value_parser = parse_region)]
    pub crop: Option<Region>,
    /// Wait until the image has not changed for this many ms (up to --timeout)
    #[arg(long)]
    pub settle: Option<u64>,
    /// Earlier shot image (its data.path) to compare against instead of the first
    /// frame: a change that finished before sight started is still seen. Implies
    /// --expect-change/--change
    #[arg(long)]
    pub baseline: Option<PathBuf>,
    /// Overall timeout (ms), shared by the change and settle phases
    #[arg(long, default_value_t = 5000)]
    pub timeout: u64,
    /// Pixels that must differ for the image to count as changed (default 16
    /// with --expect-change/--change, else 1 = exact); ignores caret blink etc.
    #[arg(long)]
    pub min_change_px: Option<u32>,
}

#[derive(Args)]
pub struct ShotArgs {
    #[command(flatten)]
    pub cap: CaptureArgs,
    /// Take the first frame as a baseline and wait until pixels differ from it
    /// (then apply --settle). Use right after an external click or keypress
    #[arg(long)]
    pub expect_change: bool,
    /// Resize factor (nearest neighbour when > 1); > 1 writes PNG
    #[arg(long, default_value_t = 1.0)]
    pub scale: f32,
    /// Downscale so the long edge is at most this many pixels (0 = no limit)
    #[arg(long, default_value_t = 1568)]
    pub max_edge: u32,
    /// Output path (.png / .jpg pick the format); default: temp dir
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// Default for `--min-change-px` when waiting for a change.
const DEFAULT_MIN_CHANGE_PX: u32 = 16;

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

/// Window geometry in screen physical px (window sources only).
pub struct WindowGeo {
    pub frame: Rect,
    pub client: Option<Rect>,
}

pub struct Captured {
    pub img: RgbaImage,
    pub maps_to: Rect,
    pub dpi_scale: f64,
    pub monitor_dpi_scale: f64,
    /// Waiting finished (change seen if required, quiet for --settle) in time.
    pub settled: bool,
    /// A frame differing from the baseline was seen.
    pub changed: bool,
    /// Ms from the start of the command to the first such frame.
    pub change_ms: Option<u64>,
    /// Popups that forced the screen-capture fallback (window sources only).
    popups: Vec<Value>,
    window: Option<WindowGeo>,
    target: TargetId,
    crop: Option<Rect>,
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

/// Validate a window-relative `--crop` against the DWM frame and return it as
/// a screen rect.
fn crop_to_screen(crop: Rect, frame: Rect) -> Result<Rect, SightError> {
    if crop.x < 0 || crop.y < 0 || crop.right() > frame.w || crop.bottom() > frame.h {
        return Err(SightError::InvalidArgsHint {
            msg: format!(
                "--crop {},{},{},{} lies outside the window frame",
                crop.x, crop.y, crop.w, crop.h
            ),
            hint: format!(
                "the window frame is {}x{} px: crop x,y,w,h must satisfy x>=0, y>=0, x+w<={}, y+h<={}",
                frame.w, frame.h, frame.w, frame.h
            ),
        });
    }
    Ok(Rect {
        x: frame.x + crop.x,
        y: frame.y + crop.y,
        w: crop.w,
        h: crop.h,
    })
}

/// `opts` with `--baseline` matched to a capture of `source` px (after crop, before scaling).
fn with_baseline(
    opts: &Opts,
    baseline: Option<&Loaded>,
    target: TargetId,
    crop: Option<Rect>,
    source: (i32, i32),
) -> Result<Opts, SightError> {
    let Some(b) = baseline else {
        return Ok(opts.clone());
    };
    let b = b.prepare(target, crop, (source.0 as u32, source.1 as u32))?;
    Ok(opts.with_baseline(Some(b)))
}

/// How a window target is captured.
pub enum How {
    /// Screen capture of this region (popups present, or a cropped area).
    Region(Rect),
    /// Window capture (Windows Graphics Capture).
    Frame(CropSpec),
}

/// Everything decided about a window capture before any frame is taken.
pub struct WinPlan {
    pub how: How,
    /// Pixel size after the crop, before scaling.
    pub source: (i32, i32),
    pub dpi_scale: f64,
    pub monitor_dpi_scale: f64,
    pub geo: WindowGeo,
    pub popups: Vec<Value>,
}

/// Validate the window, apply `--crop` and decide between window capture and the
/// popup screen-capture fallback.
pub fn plan_window(
    hwnd: u64,
    crop: Option<Rect>,
    warnings: &mut Vec<String>,
) -> Result<WinPlan, SightError> {
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

    let crop_screen = crop.map(|c| crop_to_screen(c, frame)).transpose()?;
    let geo = WindowGeo {
        frame,
        client: win::client_rect_screen(hwnd),
    };
    if geo.client.is_none() {
        warnings.push("client_rect_unavailable".into());
    }
    // What the image shows: the whole frame, or just the crop.
    let visible = crop_screen.unwrap_or(frame);

    // Visible same-process WS_POPUP windows overlapping what we capture
    // (menus, dropdowns, tooltips) are not part of a window capture.
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
                && w.rect.intersect(&visible).is_some()
        })
        .filter_map(|w| popup_reason(w, hwnd, &all).map(|why| (w, why)))
        .collect();
    let popup_json: Vec<Value> = popups
        .iter()
        .map(|(w, why)| json!({"hwnd": w.hwnd, "class": w.class, "via": why}))
        .collect();
    let (how, source) = if popups.is_empty() {
        let spec = CropSpec::WindowFrame {
            frame,
            window: wr,
            sub: crop,
        };
        (
            How::Frame(spec),
            crop.map_or((frame.w, frame.h), |c| (c.w, c.h)),
        )
    } else {
        // With --crop the screen region is exactly the translated crop rect, so
        // the crop keeps meaning the same window-relative area.
        let region =
            crop_screen.unwrap_or_else(|| popups.iter().fold(frame, |u, (p, _)| u.union(&p.rect)));
        warnings.push("popups_included_via_screen_capture".into());
        (How::Region(region), (region.w, region.h))
    };
    Ok(WinPlan {
        how,
        source,
        dpi_scale: info.dpi_scale,
        monitor_dpi_scale: info.monitor_dpi_scale,
        geo,
        popups: popup_json,
    })
}

fn capture_window(
    hwnd: u64,
    crop: Option<Rect>,
    baseline: Option<&Loaded>,
    opts: &Opts,
    warnings: &mut Vec<String>,
) -> Result<Captured, SightError> {
    let plan = plan_window(hwnd, crop, warnings)?;
    let target = TargetId::Window { hwnd };
    let opts = &with_baseline(opts, baseline, target, crop, plan.source)?;
    let (img, maps_to, settled) = match plan.how {
        How::Region(region) => {
            let (img, settled) = capture::grab_region(region, opts, warnings)?;
            (img, region, settled)
        }
        How::Frame(spec) => {
            let g = capture::grab(&Source::Window(hwnd), spec, opts)?;
            let maps_to = Rect {
                x: g.origin.0 + g.crop.x,
                y: g.origin.1 + g.crop.y,
                w: g.crop.w,
                h: g.crop.h,
            };
            (g.img, maps_to, g.settled)
        }
    };
    Ok(Captured {
        img,
        maps_to,
        dpi_scale: plan.dpi_scale,
        monitor_dpi_scale: plan.monitor_dpi_scale,
        settled,
        changed: false,
        change_ms: None,
        popups: plan.popups,
        window: Some(plan.geo),
        target,
        crop,
    })
}

/// Pick the largest visible window of a pid; the rest become warnings.
pub fn pick_pid_window(pid: u32, warnings: &mut Vec<String>) -> Result<u64, SightError> {
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

pub fn fmt_num(v: f64) -> String {
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

pub fn timestamp() -> String {
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
            .write_with_encoder(JpegEncoder::new_with_quality(w, baseline::JPEG_QUALITY))
            .map_err(|e| io(&e)),
    }
}

/// Validate the shared target/wait flags, then capture (waiting for a change
/// and/or for the image to settle). Shared by `shot` and `wait`.
pub fn acquire(
    args: &CaptureArgs,
    expect_change: bool,
    start: Instant,
    warnings: &mut Vec<String>,
) -> Result<Captured, SightError> {
    let targets = [
        args.pid.is_some(),
        args.hwnd.is_some(),
        args.region.is_some(),
    ];
    if targets.iter().filter(|t| **t).count() != 1 {
        return Err(SightError::InvalidArgs(
            "specify exactly one of --hwnd, --pid, --region".into(),
        ));
    }
    if args.region.is_some() && args.crop.is_some() {
        return Err(SightError::InvalidArgs(
            "--crop is window-relative and works only with --hwnd/--pid; \
             with --region, pass the sub-rectangle as the region itself"
                .into(),
        ));
    }
    if args.min_change_px == Some(0) {
        return Err(SightError::InvalidArgs(
            "--min-change-px must be at least 1".into(),
        ));
    }
    let expect_change = expect_change || args.baseline.is_some();
    let loaded = args
        .baseline
        .as_deref()
        .map(|p| Loaded::load(p, warnings))
        .transpose()?;
    let min_px = args.min_change_px.unwrap_or(if expect_change {
        DEFAULT_MIN_CHANGE_PX
    } else {
        1
    });
    let opts = Opts::new(
        args.settle.map(Duration::from_millis),
        min_px as usize,
        expect_change,
        start,
        start + Duration::from_millis(args.timeout),
    );

    let mut cap = if let Some(Region(r)) = &args.region {
        let target = TargetId::Region { rect: *r };
        let opts = with_baseline(&opts, loaded.as_ref(), target, None, (r.w, r.h))?;
        let (img, settled) = capture::grab_region(*r, &opts, warnings)?;
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
            changed: false,
            change_ms: None,
            popups: Vec::new(),
            window: None,
            target,
            crop: None,
        }
    } else {
        let hwnd = match (args.hwnd, args.pid) {
            (Some(h), _) => h,
            (None, Some(pid)) => pick_pid_window(pid, warnings)?,
            (None, None) => unreachable!("validated above"),
        };
        capture_window(
            hwnd,
            args.crop.as_ref().map(|c| c.0),
            loaded.as_ref(),
            &opts,
            warnings,
        )?
    };
    cap.change_ms = opts.change_ms();
    cap.changed = cap.change_ms.is_some();

    if (cap.dpi_scale - cap.monitor_dpi_scale).abs() > f64::EPSILON {
        warnings.push("window_dpi_unaware_or_virtualized".into());
    }
    Ok(cap)
}

pub fn run(args: &ShotArgs) -> Result<(Value, Vec<String>), Box<ShotFail>> {
    let start = Instant::now();
    if !args.scale.is_finite() || args.scale <= 0.0 || args.scale > 16.0 {
        return Err(SightError::InvalidArgs("--scale must be in (0, 16]".into()).into());
    }
    let mut warnings = Vec::new();
    let cap = acquire(&args.cap, args.expect_change, start, &mut warnings)?;

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
    // The same resize `--baseline` applies to live frames, so they compare equal.
    let img = if (ow, oh) == (sw, sh) {
        cap.img
    } else {
        baseline::resize(&cap.img, ow, oh)
    };

    let (path, fmt) = choose_output(args)?;
    save(img, &path, fmt)?;
    // Sidecar for later `--baseline` use; a failure only costs that convenience.
    let meta = Meta {
        v: 1,
        target: cap.target,
        crop: cap.crop,
        source: Size { w: sw, h: sh },
        image: Size { w: ow, h: oh },
        format: if fmt == Fmt::Png { "png" } else { "jpeg" }.into(),
    };
    if baseline::write_meta(&path, &meta).is_err() {
        warnings.push("baseline_metadata_not_written".into());
    }

    let m = cap.maps_to;
    let sc = fmt_num(reported);
    let mut data = json!({
        "path": path.to_string_lossy(),
        "image": {"w": ow, "h": oh},
        "maps_to": {"space": "screen_physical", "x": m.x, "y": m.y, "w": m.w, "h": m.h},
        "scale": reported,
        "dpi_scale": cap.dpi_scale,
        "monitor_dpi_scale": cap.monitor_dpi_scale,
    });
    add_formulas(&mut data, m, &sc, cap.window.as_ref());
    data["settled"] = json!(cap.settled);
    data["changed"] = json!(cap.changed);
    data["change_ms"] = json!(cap.change_ms);
    data["elapsed_ms"] = json!(start.elapsed().as_millis() as u64);
    if args.cap.region.is_none() {
        data["popups"] = Value::Array(cap.popups);
    }
    if cap.settled {
        Ok((data, warnings))
    } else {
        Err(Box::new(ShotFail {
            err: SightError::Timeout(timeout_message(
                args.expect_change || args.cap.baseline.is_some(),
                cap.changed,
                args.cap.timeout,
                "saved the latest frame anyway",
            )),
            data: Some(data),
            warnings,
        }))
    }
}

/// `{prefix}_x = {ox} + px / {sc}; {prefix}_y = {oy} + py / {sc}`
/// Add `formula` (screen) and, for window targets, `window`, `formula_window`
/// and `formula_client`: how image px map to each origin. `m` is what the image
/// covers on screen, `sc` the formatted px-per-screen-px scale.
pub fn add_formulas(data: &mut Value, m: Rect, sc: &str, window: Option<&WindowGeo>) {
    data["formula"] = json!(formula("screen", m.x, m.y, sc));
    if let Some(g) = window {
        // Same image px -> other origins: the frame's and the client area's.
        data["window"] = window_json(g);
        data["formula_window"] = json!(formula("win", m.x - g.frame.x, m.y - g.frame.y, sc));
        if let Some(c) = &g.client {
            data["formula_client"] = json!(formula("client", m.x - c.x, m.y - c.y, sc));
        }
    }
}

fn formula(prefix: &str, ox: i32, oy: i32, sc: &str) -> String {
    format!("{prefix}_x = {ox} + px / {sc}; {prefix}_y = {oy} + py / {sc}")
}

fn window_json(g: &WindowGeo) -> Value {
    let mut v = json!({"frame": g.frame});
    if let Some(c) = &g.client {
        v["client"] = json!(c);
        v["client_offset"] = json!({"x": c.x - g.frame.x, "y": c.y - g.frame.y});
    }
    v
}

pub fn timeout_message(expect_change: bool, changed: bool, timeout: u64, tail: &str) -> String {
    if expect_change && !changed {
        format!("no change detected within --timeout {timeout}ms ({tail})")
    } else {
        format!("image did not settle within --timeout {timeout}ms ({tail})")
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

#[cfg(test)]
mod window_tests {
    use super::*;

    const FRAME: Rect = Rect {
        x: 207,
        y: 150,
        w: 642,
        h: 452,
    };

    #[test]
    fn crop_is_translated_to_screen_and_validated() {
        let c = |x, y, w, h| Rect { x, y, w, h };
        assert_eq!(
            crop_to_screen(c(100, 120, 300, 200), FRAME).unwrap(),
            c(307, 270, 300, 200)
        );
        assert!(crop_to_screen(c(0, 0, 642, 452), FRAME).is_ok());
        for bad in [c(-1, 0, 10, 10), c(0, 0, 643, 10), c(600, 400, 100, 100)] {
            match crop_to_screen(bad, FRAME) {
                Err(SightError::InvalidArgsHint { hint, .. }) => assert!(hint.contains("642x452")),
                other => panic!("expected InvalidArgsHint, got {other:?}"),
            }
        }
    }

    #[test]
    fn formulas_use_the_requested_origin() {
        assert_eq!(
            formula("win", 90, 100, "1.0"),
            "win_x = 90 + px / 1.0; win_y = 100 + py / 1.0"
        );
        let g = WindowGeo {
            frame: FRAME,
            client: Some(Rect {
                x: 208,
                y: 181,
                w: 640,
                h: 420,
            }),
        };
        let v = window_json(&g);
        assert_eq!(v["client_offset"], json!({"x": 1, "y": 31}));
        assert_eq!(v["frame"]["w"], 642);
    }
}
