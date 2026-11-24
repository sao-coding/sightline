//! `sight rec`: record a window or region to an animated GIF.
//!
//! The capture sessions are the same Windows Graphics Capture sessions `shot`
//! uses. A sampler thread polls them at `--fps`, composes and downscales each
//! changed frame, and hands it to an encoder thread, which streams a GIF with
//! the `gif` crate. Identical frames are merged by extending the previous
//! frame's delay.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::{Args, ValueEnum};
use image::{DynamicImage, ImageFormat, RgbaImage};
use serde_json::{Value, json};

use crate::baseline;
use crate::capture::{self, CropSpec, FrameCopy, Opts, Session, Source};
use crate::error::SightError;
use crate::occlusion;
use crate::shot::{self, How, Region};
use crate::win::{self, Rect};

/// Output over this size gets the `gif_large` warning.
const LARGE_GIF_BYTES: u64 = 10 * 1024 * 1024;
/// How long to wait for the first frame of every capture session.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);
/// How often the window's occlusion is re-checked while recording.
const OCCLUSION_POLL: Duration = Duration::from_millis(250);

/// Which capture API records a window.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceArg {
    /// Screen when nothing covers the window at the start, else window capture
    Auto,
    /// Screen (monitor) capture of the window's rect: shows menus and popups
    Screen,
    /// Window capture: only the window itself, even when covered
    Window,
}

#[derive(Args)]
pub struct RecArgs {
    /// Record the largest visible window of this process
    #[arg(long)]
    pub pid: Option<u32>,
    /// Record this window handle (decimal, as printed by `list`)
    #[arg(long)]
    pub hwnd: Option<u64>,
    /// Screen region x,y,w,h in physical pixels (screen capture, popups included)
    #[arg(long, value_parser = shot::parse_region)]
    pub region: Option<Region>,
    /// Crop x,y,w,h in physical px, relative to the window's DWM frame top-left
    /// (--hwnd/--pid only)
    #[arg(long, value_parser = shot::parse_region)]
    pub crop: Option<Region>,
    /// Capture source, chosen once at the start (--hwnd/--pid): auto uses screen
    /// capture unless another window covers the target
    #[arg(long, value_enum, default_value_t = SourceArg::Auto)]
    pub source: SourceArg,
    /// Extra px around the window in a screen recording, so dropdowns that stick
    /// out of it are included (--hwnd/--pid only)
    #[arg(long, default_value_t = 0)]
    pub pad: u32,
    /// Include the mouse cursor
    #[arg(long)]
    pub cursor: bool,
    /// Recording length (ms), counted from the first captured frame
    #[arg(long)]
    pub duration: u64,
    /// Sampling rate in frames per second (1-30)
    #[arg(long, default_value_t = 10)]
    pub fps: u32,
    /// Downscale so the long edge is at most this many pixels (0 = no limit)
    #[arg(long, default_value_t = 800)]
    pub max_edge: u32,
    /// Stop early once the image changed and then stayed unchanged for this many
    /// ms (never longer than --duration)
    #[arg(long)]
    pub until_settled: Option<u64>,
    /// Also save N PNG stills of the most changed moments (first, last and the
    /// biggest changes in between)
    #[arg(long, default_value_t = 0)]
    pub keyframes: u32,
    /// Output path (.gif); default: temp dir
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// One capture session and where its pixels land on the canvas.
struct Part {
    session: Session,
    /// Top-left on the canvas; `None` when each frame carries its own position.
    dest: Option<(i64, i64)>,
    last: Option<FrameCopy>,
}

enum Msg {
    Frame { t_ms: u64, img: RgbaImage },
    End { t_ms: u64 },
}

/// Streams frames into a GIF. Returns (frames written, GIF duration in ms).
fn encode(rx: mpsc::Receiver<Msg>, path: &Path, w: u32, h: u32) -> Result<(u32, u64), SightError> {
    let io = |e: &dyn std::fmt::Display| SightError::Internal(format!("cannot write gif: {e}"));
    let file = BufWriter::new(File::create(path).map_err(|e| io(&e))?);
    let mut enc = gif::Encoder::new(file, w as u16, h as u16, &[]).map_err(|e| io(&e))?;
    enc.set_repeat(gif::Repeat::Infinite).map_err(|e| io(&e))?;

    let mut pending: Option<(u64, RgbaImage)> = None;
    let mut emitted_cs = 0u64;
    let mut frames = 0u32;
    let mut flush = |enc: &mut gif::Encoder<_>,
                     (_, img): (u64, RgbaImage),
                     until_ms: u64|
     -> Result<(), SightError> {
        // Delays are whole centiseconds; deriving them from the cumulative
        // timeline stops rounding error from piling up. Browsers treat < 2 as 10.
        let target = (until_ms + 5) / 10;
        let delay = target
            .saturating_sub(emitted_cs)
            .clamp(2, u64::from(u16::MAX));
        emitted_cs += delay;
        let mut raw = img.into_raw();
        let mut frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut raw, 10);
        frame.delay = delay as u16;
        enc.write_frame(&frame).map_err(|e| io(&e))?;
        frames += 1;
        Ok(())
    };
    for msg in rx {
        match msg {
            Msg::Frame { t_ms, img } => {
                if let Some(prev) = pending.replace((t_ms, img)) {
                    flush(&mut enc, prev, t_ms)?;
                }
            }
            Msg::End { t_ms } => {
                if let Some(prev) = pending.take() {
                    let until = t_ms.max(prev.0 + 20);
                    flush(&mut enc, prev, until)?;
                }
            }
        }
    }
    Ok((frames, emitted_cs * 10))
}

/// Copy `f` (BGRA) onto `canvas` at `dest`.
fn paste(canvas: &mut RgbaImage, f: &FrameCopy, dest: (i64, i64)) {
    let (w, h) = (f.crop.w as u32, f.crop.h as u32);
    let mut rgba = f.bgra.clone();
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        px[3] = 255;
    }
    if let Some(img) = RgbaImage::from_raw(w, h, rgba) {
        image::imageops::replace(canvas, &img, dest.0, dest.1);
    }
}

/// How many pixels differ between two same-size images.
fn differing(a: &RgbaImage, b: &RgbaImage) -> usize {
    if a.dimensions() != b.dimensions() {
        return usize::MAX;
    }
    a.as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_raw().as_chunks::<4>().0)
        .filter(|(p, q)| p != q)
        .count()
}

/// A candidate keyframe.
struct Cand {
    t_ms: u64,
    score: usize,
    img: RgbaImage,
}

/// Picks `n` stills: the first frame, the last frame and the frames with the
/// biggest change in between.
struct Keyframes {
    n: usize,
    first: Option<Cand>,
    middle: Vec<Cand>,
    last: Option<Cand>,
    count: usize,
}

impl Keyframes {
    fn new(n: usize) -> Self {
        Self {
            n,
            first: None,
            middle: Vec::new(),
            last: None,
            count: 0,
        }
    }

    fn push(&mut self, t_ms: u64, score: usize, img: &RgbaImage) {
        if self.n == 0 {
            return;
        }
        let cand = Cand {
            t_ms,
            score,
            img: img.clone(),
        };
        self.count += 1;
        if self.count == 1 {
            self.first = Some(Cand {
                t_ms,
                score,
                img: img.clone(),
            });
        } else if let Some(old) = self.last.take() {
            let room = self.n.saturating_sub(2);
            if room > 0 && self.count > 2 {
                self.middle.push(old);
                if self.middle.len() > room {
                    let min = (0..self.middle.len())
                        .min_by_key(|&i| self.middle[i].score)
                        .expect("non-empty");
                    self.middle.swap_remove(min);
                }
            }
        }
        self.last = Some(cand);
    }

    /// The chosen stills in time order.
    fn finish(self) -> Vec<Cand> {
        let mut out = Vec::new();
        if self.n >= 2 && self.count > 1 {
            out.extend(self.first);
        }
        out.extend(self.middle);
        out.extend(self.last);
        out.sort_by_key(|c| c.t_ms);
        out
    }
}

fn save_png(img: &RgbaImage, path: &Path) -> Result<(), SightError> {
    DynamicImage::ImageRgba8(img.clone())
        .save_with_format(path, ImageFormat::Png)
        .map_err(|e| SightError::Internal(format!("cannot write image: {e}")))
}

fn default_path() -> Result<PathBuf, SightError> {
    let dir = std::env::temp_dir().join("sightline");
    std::fs::create_dir_all(&dir)
        .map_err(|e| SightError::Internal(format!("cannot prepare output path: {e}")))?;
    Ok(dir.join(format!("rec-{}.gif", shot::timestamp())))
}

/// What a part is made of, before its session starts.
struct PartSpec {
    source: Source,
    crop: CropSpec,
    dest: Option<(i64, i64)>,
}

/// Start all sessions at once (each start takes about 0.3 s).
fn start_parts(specs: Vec<PartSpec>, opts: &Opts) -> Result<Vec<Part>, SightError> {
    let results: Vec<Result<Session, SightError>> = std::thread::scope(|s| {
        let handles: Vec<_> = specs
            .iter()
            .map(|sp| s.spawn(move || Session::start(&sp.source, sp.crop.clone(), opts)))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(SightError::Internal("capture thread panicked".into())))
            })
            .collect()
    });
    let mut parts = Vec::new();
    let mut error = None;
    for (sp, r) in specs.iter().zip(results) {
        match r {
            Ok(session) => parts.push(Part {
                session,
                dest: sp.dest,
                last: None,
            }),
            Err(e) => {
                error.get_or_insert(e);
            }
        }
    }
    if let Some(e) = error {
        for p in parts {
            p.session.stop();
        }
        return Err(e);
    }
    Ok(parts)
}

/// One monitor session per slice of a fixed screen `region`.
fn region_specs(region: Rect) -> Result<Vec<PartSpec>, SightError> {
    let (slices, _) = capture::region_parts(region)?;
    Ok(slices
        .iter()
        .map(|s| PartSpec {
            source: Source::Monitor(s.hmon),
            crop: CropSpec::Rect(s.local),
            dest: Some((
                i64::from(s.inter.x - region.x),
                i64::from(s.inter.y - region.y),
            )),
        })
        .collect())
}

/// One monitor session per monitor, each following the window's frame.
fn follow_specs(hwnd: u64, frame: Rect, offset: (i32, i32), size: (i32, i32)) -> Vec<PartSpec> {
    win::monitors()
        .iter()
        .map(|m| PartSpec {
            source: Source::Monitor(m.hmon),
            crop: CropSpec::Follow {
                hwnd,
                offset,
                size,
                monitor: m.rect,
                fallback: frame,
            },
            dest: None,
        })
        .collect()
}

/// Which capture API records the window.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Src {
    Screen,
    Window,
}

impl Src {
    fn name(self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::Window => "window",
        }
    }
}

/// The region to watch for occluders: the frame plus `pad`, kept on the desktop.
fn watch_region(frame: Rect, pad: i32) -> Rect {
    let r = frame.expand(pad);
    win::virtual_desktop()
        .and_then(|d| r.intersect(&d))
        .unwrap_or(r)
}

pub fn run(args: &RecArgs) -> Result<(Value, Vec<String>), SightError> {
    let start = Instant::now();
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
    if args.region.is_some() && args.pad > 0 {
        return Err(SightError::InvalidArgs(
            "--pad works only with --hwnd/--pid; a --region already says exactly what to record"
                .into(),
        ));
    }
    if args.region.is_some() && args.source == SourceArg::Window {
        return Err(SightError::InvalidArgs(
            "--region is always a screen capture; --source window needs --hwnd/--pid".into(),
        ));
    }
    if args.duration == 0 || args.until_settled == Some(0) {
        return Err(SightError::InvalidArgs(
            "--duration and --until-settled must be greater than 0".into(),
        ));
    }
    if !(1..=30).contains(&args.fps) {
        return Err(SightError::InvalidArgs("--fps must be in 1..=30".into()));
    }
    let mut warnings = Vec::new();
    let opts =
        Opts::new(None, 1, false, start, start + FIRST_FRAME_TIMEOUT).with_cursor(args.cursor);

    // Plan: ONE source, chosen now and never switched. `win` is set for window targets.
    struct Watch {
        hwnd: u64,
        frame: Rect,
        pad: i32,
    }
    let pad = args.pad as i32;
    let mut watch: Option<Watch> = None;
    let mut plan = None;
    let mut source = Src::Screen;
    let mut reason = "forced";
    let mut occluded0 = false;
    let (specs, fixed_canvas): (Vec<PartSpec>, Option<Rect>) = if let Some(shot::Region(r)) =
        &args.region
    {
        (region_specs(*r)?, Some(*r))
    } else {
        let hwnd = match (args.hwnd, args.pid) {
            (Some(h), _) => h,
            (None, Some(pid)) => shot::pick_pid_window(pid, &mut warnings)?.0,
            (None, None) => unreachable!("validated above"),
        };
        // Validates the window (minimized, protected, ...) and gives its frame.
        let p0 = shot::plan_window(hwnd, None, false, &mut warnings)?;
        let frame = p0.geo.frame;
        occluded0 = !occlusion::occluders(hwnd, watch_region(frame, pad)).is_empty();
        (source, reason) = match args.source {
            SourceArg::Screen => (Src::Screen, "forced"),
            SourceArg::Window => (Src::Window, "forced"),
            SourceArg::Auto if occluded0 => {
                warnings.push("rec_window_source_no_popups".into());
                (Src::Window, "auto_occluded")
            }
            SourceArg::Auto => (Src::Screen, "auto_not_occluded"),
        };
        watch = Some(Watch { hwnd, frame, pad });
        let out = match source {
            Src::Window => {
                if pad > 0 {
                    warnings.push("rec_pad_ignored".into());
                }
                let p =
                    shot::plan_window(hwnd, args.crop.as_ref().map(|c| c.0), false, &mut warnings)?;
                let How::Frame(spec) = p.how else {
                    unreachable!("popup fallback is off")
                };
                (
                    vec![PartSpec {
                        source: Source::Window(hwnd),
                        crop: spec,
                        dest: Some((0, 0)),
                    }],
                    None,
                )
            }
            Src::Screen => {
                let cap = match args.crop.as_ref().map(|c| c.0) {
                    Some(c) => {
                        if c.x < -pad
                            || c.y < -pad
                            || c.right() > frame.w + pad
                            || c.bottom() > frame.h + pad
                        {
                            return Err(SightError::InvalidArgsHint {
                                msg: format!(
                                    "--crop {},{},{},{} lies outside the window frame plus --pad {pad}",
                                    c.x, c.y, c.w, c.h
                                ),
                                hint: format!(
                                    "the window frame is {}x{} px: crop x,y,w,h must satisfy x>={}, y>={}, x+w<={}, y+h<={}",
                                    frame.w,
                                    frame.h,
                                    -pad,
                                    -pad,
                                    frame.w + pad,
                                    frame.h + pad
                                ),
                            });
                        }
                        Rect {
                            x: frame.x + c.x,
                            y: frame.y + c.y,
                            w: c.w,
                            h: c.h,
                        }
                    }
                    None => watch_region(frame, pad),
                };
                let offset = (cap.x - frame.x, cap.y - frame.y);
                (follow_specs(hwnd, frame, offset, (cap.w, cap.h)), Some(cap))
            }
        };
        plan = Some(p0);
        out
    };
    let mut parts = start_parts(specs, &opts)?;

    // Wait for every session's first frame.
    let first_deadline = Instant::now() + FIRST_FRAME_TIMEOUT;
    let fail = |parts: Vec<Part>, e: SightError| {
        for p in parts {
            p.session.stop();
        }
        e
    };
    loop {
        if let Some(e) = parts.iter().find_map(|p| p.session.error()) {
            return Err(fail(parts, SightError::Internal(e)));
        }
        if parts.iter().all(|p| p.session.seq() > 0) {
            break;
        }
        if Instant::now() >= first_deadline {
            return Err(fail(
                parts,
                SightError::Timeout("no frame arrived before the capture timeout".into()),
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let t0 = Instant::now();

    // Canvas: a fixed rect (region, screen source), or the window's first frame.
    let (canvas_w, canvas_h, maps_to) = match fixed_canvas {
        Some(r) => (r.w as u32, r.h as u32, r),
        None => {
            let f = parts[0].session.frame().expect("first frame arrived above");
            let maps_to = Rect {
                x: f.origin.0 + f.crop.x,
                y: f.origin.1 + f.crop.y,
                w: f.crop.w,
                h: f.crop.h,
            };
            (f.crop.w as u32, f.crop.h as u32, maps_to)
        }
    };
    let long = f64::from(canvas_w.max(canvas_h));
    let s = if args.max_edge > 0 && long > f64::from(args.max_edge) {
        f64::from(args.max_edge) / long
    } else {
        1.0
    };
    let (ow, oh) = (
        (f64::from(canvas_w) * s).round().max(1.0) as u32,
        (f64::from(canvas_h) * s).round().max(1.0) as u32,
    );
    if ow > u32::from(u16::MAX) || oh > u32::from(u16::MAX) {
        for p in parts {
            p.session.stop();
        }
        return Err(SightError::InvalidArgs(
            "the GIF would be larger than 65535 px; lower --max-edge".into(),
        ));
    }
    let scale = ((f64::from(ow.max(oh)) / long) * 10000.0).round() / 10000.0;

    let path = match &args.out {
        Some(p) => {
            if let Some(dir) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
                let _ = std::fs::create_dir_all(dir);
            }
            std::path::absolute(p).unwrap_or_else(|_| p.clone())
        }
        None => default_path()?,
    };
    let (tx, rx) = mpsc::channel::<Msg>();
    let enc_path = path.clone();
    let encoder = std::thread::spawn(move || encode(rx, &enc_path, ow, oh));

    // Sampler.
    let mut keys = Keyframes::new(args.keyframes as usize);
    let period = Duration::from_secs_f64(1.0 / f64::from(args.fps));
    let duration = Duration::from_millis(args.duration);
    let settle = args.until_settled.map(Duration::from_millis);
    let mut canvas = RgbaImage::from_pixel(canvas_w, canvas_h, image::Rgba([0, 0, 0, 255]));
    let mut last_img: Option<RgbaImage> = None;
    let mut last_change: Option<Instant> = None;
    let mut resized = false;
    let mut moved = false;
    let mut occluded = occluded0;
    let mut events: Vec<Value> = Vec::new();
    let mut next_occlusion_check = t0;
    let mut error: Option<SightError> = None;
    let mut stopped = "duration";
    let mut k = 0u32;
    loop {
        let due = t0 + period * k;
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(t0);
        let finished = elapsed >= duration;
        let t_ms = elapsed.as_millis() as u64;

        if let Some(e) = parts.iter().find_map(|p| p.session.error()) {
            error = Some(SightError::Internal(e));
            break;
        }
        // The window may move or resize, and other windows may come and go. None of
        // that changes the source; it is only reported.
        if let Some(w) = &watch {
            let frame = win::frame_bounds(w.hwnd).unwrap_or(w.frame);
            moved |= (frame.x, frame.y) != (w.frame.x, w.frame.y);
            resized |= (frame.w, frame.h) != (w.frame.w, w.frame.h);
            if now >= next_occlusion_check {
                next_occlusion_check = now + OCCLUSION_POLL;
                let now_occluded =
                    !occlusion::occluders(w.hwnd, watch_region(frame, w.pad)).is_empty();
                if now_occluded != occluded {
                    occluded = now_occluded;
                    events.push(json!({"t_ms": t_ms, "occluded": occluded}));
                }
            }
        }
        // Refresh parts whose pixels changed since the last tick.
        let mut dirty = last_img.is_none();
        for p in &mut parts {
            if p.session.seq() != p.last.as_ref().map_or(0, |l| l.seq) {
                p.last = p.session.frame();
                dirty = true;
            }
        }
        if dirty {
            for p in &parts {
                if let Some(f) = &p.last {
                    let dest = p.dest.unwrap_or((
                        i64::from(f.origin.0 + f.crop.x),
                        i64::from(f.origin.1 + f.crop.y),
                    ));
                    if f.origin.0 != capture::OFF_CANVAS {
                        paste(&mut canvas, f, dest);
                    }
                }
            }
            let img = baseline::resize(&canvas, ow, oh);
            let score = last_img.as_ref().map_or(0, |l| differing(l, &img));
            if last_img.is_none() || score > 0 {
                if last_img.is_some() {
                    last_change = Some(now);
                }
                keys.push(t_ms, score, &img);
                last_img = Some(img.clone());
                let _ = tx.send(Msg::Frame { t_ms, img });
            }
        }
        if finished {
            break;
        }
        if let (Some(s), Some(c)) = (settle, last_change)
            && now.saturating_duration_since(c) >= s
        {
            stopped = "settled";
            break;
        }
        // Skip ticks we are already late for instead of bursting.
        let late = (Instant::now().saturating_duration_since(t0).as_secs_f64()
            / period.as_secs_f64()) as u32;
        k = (k + 1).max(late);
    }
    let end_ms = Instant::now().saturating_duration_since(t0).as_millis() as u64;
    for p in parts {
        p.session.stop();
    }
    let _ = tx.send(Msg::End { t_ms: end_ms });
    drop(tx);
    let encoded = encoder
        .join()
        .map_err(|_| SightError::Internal("gif encoder thread panicked".into()))?;
    if let Some(e) = error {
        return Err(e);
    }
    let (frames, gif_ms) = encoded?;
    let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    if resized {
        warnings.push("rec_window_resized".into());
    }
    if !events.is_empty() {
        warnings.push("rec_occlusion_changed".into());
    }
    if size_bytes > LARGE_GIF_BYTES {
        warnings.push("gif_large".into());
    }

    let sc = shot::fmt_num(scale);
    let mut data = json!({
        "path": path.to_string_lossy(),
        "frames": frames,
        "fps": args.fps,
        "duration_ms": gif_ms,
        "image": {"w": ow, "h": oh},
        "size_bytes": size_bytes,
        "maps_to": {"space": "screen_physical", "x": maps_to.x, "y": maps_to.y, "w": maps_to.w, "h": maps_to.h},
        "scale": scale,
    });
    if let Some(p) = &plan {
        data["dpi_scale"] = json!(p.dpi_scale);
        data["monitor_dpi_scale"] = json!(p.monitor_dpi_scale);
    } else {
        let dpi = args
            .region
            .as_ref()
            .map_or(1.0, |r| win::dpi_scale_at(r.0.x, r.0.y));
        data["dpi_scale"] = json!(dpi);
        data["monitor_dpi_scale"] = json!(dpi);
    }
    shot::add_formulas(&mut data, maps_to, &sc, plan.as_ref().map(|p| &p.geo));
    data["source"] = json!(if args.region.is_some() {
        "screen"
    } else {
        source.name()
    });
    data["source_reason"] = json!(if args.region.is_some() {
        "forced"
    } else {
        reason
    });
    if moved {
        data["moved"] = json!(true);
    }
    if watch.is_some() {
        data["occlusion_events"] = Value::Array(events);
    }
    data["stopped"] = json!(stopped);
    data["elapsed_ms"] = json!(start.elapsed().as_millis() as u64);

    if args.keyframes > 0 {
        let stem = path.with_extension("");
        let mut list = Vec::new();
        for (i, c) in keys.finish().into_iter().enumerate() {
            let p = PathBuf::from(format!("{}-key{}.png", stem.display(), i + 1));
            save_png(&c.img, &p)?;
            list.push(json!({"t_ms": c.t_ms, "path": p.to_string_lossy()}));
        }
        data["keyframes"] = Value::Array(list);
    }
    Ok((data, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(v: u8) -> RgbaImage {
        RgbaImage::from_pixel(8, 6, image::Rgba([v, v, v, 255]))
    }

    #[test]
    fn keyframes_keep_first_last_and_biggest_changes() {
        let mut k = Keyframes::new(4);
        for (t, score) in [(0, 0), (100, 5), (200, 90), (300, 7), (400, 60), (500, 1)] {
            k.push(t, score, &solid(t as u8));
        }
        let ts: Vec<u64> = k.finish().iter().map(|c| c.t_ms).collect();
        assert_eq!(ts, vec![0, 200, 400, 500]);
    }

    #[test]
    fn keyframes_handle_few_frames() {
        let mut k = Keyframes::new(3);
        k.push(0, 0, &solid(1));
        assert_eq!(k.finish().len(), 1);
        let mut k = Keyframes::new(1);
        k.push(0, 0, &solid(1));
        k.push(50, 9, &solid(2));
        let out = k.finish();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].t_ms, 50);
        assert!(Keyframes::new(0).finish().is_empty());
    }

    #[test]
    fn encoder_writes_delays_from_the_timeline() {
        let path = std::env::temp_dir().join(format!("sight-rec-test-{}.gif", std::process::id()));
        let (tx, rx) = mpsc::channel();
        for (t, v) in [(0, 10), (130, 200), (700, 90)] {
            tx.send(Msg::Frame {
                t_ms: t,
                img: solid(v),
            })
            .unwrap();
        }
        tx.send(Msg::End { t_ms: 1000 }).unwrap();
        drop(tx);
        let (frames, ms) = encode(rx, &path, 8, 6).unwrap();
        assert_eq!((frames, ms), (3, 1000));

        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut dec = opts.read_info(File::open(&path).unwrap()).unwrap();
        let mut delays = Vec::new();
        while let Some(f) = dec.read_next_frame().unwrap() {
            delays.push(u32::from(f.delay) * 10);
        }
        let _ = std::fs::remove_file(&path);
        assert_eq!(delays, vec![130, 570, 300]);
    }
}
