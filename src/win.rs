//! Thin Win32 helpers for what windows-capture does not cover: window
//! enumeration, DWM frame bounds, per-monitor DPI, minimized/protected state.

use std::collections::HashMap;
use std::ffi::c_void;

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MONITORINFOEXW, MonitorFromPoint, MonitorFromWindow,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, GetDpiForWindow,
    MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_STYLE, GetClassNameW, GetClientRect, GetWindow,
    GetWindowDisplayAffinity, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, WS_POPUP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetLayeredWindowAttributes, LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA,
    WS_EX_LAYERED,
};
use windows::core::{BOOL, PWSTR};

/// Make the whole process Per-Monitor V2 DPI aware so every coordinate we see
/// and report is in physical pixels. Must run before any window is queried.
pub fn init_dpi_awareness() {
    // Fails only if awareness was already set (e.g. by a manifest); either way
    // the process ends up aware, so the error is ignorable.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn from_ltrb(r: RECT) -> Self {
        Self {
            x: r.left,
            y: r.top,
            w: r.right - r.left,
            h: r.bottom - r.top,
        }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
    pub fn area(&self) -> i64 {
        i64::from(self.w.max(0)) * i64::from(self.h.max(0))
    }
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let (l, t) = (self.x.max(o.x), self.y.max(o.y));
        let (r, b) = (self.right().min(o.right()), self.bottom().min(o.bottom()));
        (r > l && b > t).then_some(Rect {
            x: l,
            y: t,
            w: r - l,
            h: b - t,
        })
    }
    /// Grow by `n` px on every side.
    pub fn expand(&self, n: i32) -> Rect {
        Rect {
            x: self.x - n,
            y: self.y - n,
            w: self.w + 2 * n,
            h: self.h + 2 * n,
        }
    }
    pub fn union(&self, o: &Rect) -> Rect {
        let (l, t) = (self.x.min(o.x), self.y.min(o.y));
        let (r, b) = (self.right().max(o.right()), self.bottom().max(o.bottom()));
        Rect {
            x: l,
            y: t,
            w: r - l,
            h: b - t,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MonitorInfo {
    pub hmon: isize,
    pub rect: Rect,
    /// 1-based, from the `\\.\DISPLAYn` device name.
    pub index: u32,
    pub dpi_scale: f64,
    pub primary: bool,
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn monitor_info(h: HMONITOR, fallback_index: u32) -> Option<MonitorInfo> {
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !unsafe { GetMonitorInfoW(h, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool()
    {
        return None;
    }
    let dev = String::from_utf16_lossy(&mi.szDevice);
    let dev = dev.trim_end_matches('\0');
    let digits: String = dev.chars().rev().take_while(char::is_ascii_digit).collect();
    let digits: String = digits.chars().rev().collect();
    let index = digits.parse().unwrap_or(fallback_index);
    let (mut dx, mut dy) = (96u32, 96u32);
    let _ = unsafe { GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
    Some(MonitorInfo {
        hmon: h.0 as isize,
        rect: Rect::from_ltrb(mi.monitorInfo.rcMonitor),
        index,
        dpi_scale: round2(f64::from(dx) / 96.0),
        primary: mi.monitorInfo.dwFlags & 1 != 0,
    })
}

unsafe extern "system" fn monitor_cb(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let v = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    v.push(h);
    BOOL(1)
}

pub fn monitors() -> Vec<MonitorInfo> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_cb),
            LPARAM(&mut handles as *mut Vec<HMONITOR> as isize),
        );
    }
    handles
        .into_iter()
        .enumerate()
        .filter_map(|(i, h)| monitor_info(h, i as u32 + 1))
        .collect()
}

/// DPI scale of the monitor nearest to a screen point.
pub fn dpi_scale_at(x: i32, y: i32) -> f64 {
    let h = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    monitor_info(h, 1).map_or(1.0, |m| m.dpi_scale)
}

/// True when the monitors overlapping `r` do not all share one DPI scale.
pub fn spans_mixed_dpi(r: &Rect) -> bool {
    let mut scales = monitors()
        .into_iter()
        .filter(|m| m.rect.intersect(r).is_some())
        .map(|m| m.dpi_scale);
    scales
        .next()
        .is_some_and(|first| scales.any(|s| (s - first).abs() > f64::EPSILON))
}

pub fn hwnd_from_u64(v: u64) -> HWND {
    HWND(v as usize as *mut c_void)
}

pub fn hwnd_to_u64(h: HWND) -> u64 {
    h.0 as usize as u64
}

/// True when the window opted out of capture (SetWindowDisplayAffinity).
pub fn is_protected(hwnd: u64) -> bool {
    let mut aff = 0u32;
    unsafe { GetWindowDisplayAffinity(hwnd_from_u64(hwnd), &mut aff) }.is_ok() && aff != 0
}

/// `GetWindowRect`: the rect WGC window capture frames are sized to.
pub fn window_rect(hwnd: u64) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd_from_u64(hwnd), &mut r) }.ok()?;
    Some(Rect::from_ltrb(r))
}

/// DWMWA_EXTENDED_FRAME_BOUNDS (visible frame, physical screen px), falling
/// back to GetWindowRect.
pub fn frame_bounds(hwnd: u64) -> Option<Rect> {
    let mut r = RECT::default();
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd_from_u64(hwnd),
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .is_ok();
    if ok {
        Some(Rect::from_ltrb(r))
    } else {
        window_rect(hwnd)
    }
}

/// The client area in screen physical px (GetClientRect + ClientToScreen).
pub fn client_rect_screen(hwnd: u64) -> Option<Rect> {
    let h = hwnd_from_u64(hwnd);
    let mut r = RECT::default();
    unsafe { GetClientRect(h, &mut r) }.ok()?;
    let mut origin = POINT { x: 0, y: 0 };
    if !unsafe { ClientToScreen(h, &mut origin) }.as_bool() {
        return None;
    }
    Some(Rect {
        x: origin.x,
        y: origin.y,
        w: r.right - r.left,
        h: r.bottom - r.top,
    })
}

pub fn window_dpi_scale(hwnd: u64) -> f64 {
    let dpi = unsafe { GetDpiForWindow(hwnd_from_u64(hwnd)) };
    if dpi == 0 {
        1.0
    } else {
        round2(f64::from(dpi) / 96.0)
    }
}

/// Bounding box of all monitors (the virtual desktop).
pub fn virtual_desktop() -> Option<Rect> {
    monitors().iter().map(|m| m.rect).reduce(|a, b| a.union(&b))
}

/// Every top-level window handle, topmost first.
pub fn top_level_hwnds() -> Vec<u64> {
    let mut hs: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(window_cb), LPARAM(&mut hs as *mut Vec<HWND> as isize));
    }
    hs.into_iter().map(hwnd_to_u64).collect()
}

pub fn is_visible(hwnd: u64) -> bool {
    unsafe { IsWindowVisible(hwnd_from_u64(hwnd)).as_bool() }
}

pub fn is_minimized(hwnd: u64) -> bool {
    unsafe { IsIconic(hwnd_from_u64(hwnd)).as_bool() }
}

pub fn is_cloaked_hwnd(hwnd: u64) -> bool {
    is_cloaked(hwnd_from_u64(hwnd))
}

pub fn class_name(hwnd: u64) -> String {
    class_of(hwnd_from_u64(hwnd))
}

pub fn owner(hwnd: u64) -> u64 {
    owner_of(hwnd_from_u64(hwnd))
}

pub fn pid_of(hwnd: u64) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd_from_u64(hwnd), Some(&mut pid)) };
    pid
}

/// True for a layered window whose constant alpha is 0: it draws nothing.
pub fn is_fully_transparent(hwnd: u64) -> bool {
    let h = hwnd_from_u64(hwnd);
    let ex = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } as u32;
    if ex & WS_EX_LAYERED.0 == 0 {
        return false;
    }
    let (mut alpha, mut flags) = (255u8, LAYERED_WINDOW_ATTRIBUTES_FLAGS(0));
    unsafe { GetLayeredWindowAttributes(h, None, Some(&mut alpha), Some(&mut flags)) }.is_ok()
        && flags.0 & LWA_ALPHA.0 != 0
        && alpha == 0
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut c = 0u32;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut c as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        )
    }
    .is_ok()
        && c != 0
}

fn title_of(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len as usize + 1];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn owner_of(hwnd: HWND) -> u64 {
    unsafe { GetWindow(hwnd, GW_OWNER) }.map_or(0, hwnd_to_u64)
}

fn exe_of(pid: u32) -> String {
    let Ok(h) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return String::new();
    };
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    let r = unsafe {
        QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
    };
    let _ = unsafe { CloseHandle(h) };
    if r.is_err() {
        return String::new();
    }
    let full = String::from_utf16_lossy(&buf[..len as usize]);
    full.rsplit(['\\', '/']).next().unwrap_or("").to_string()
}

/// One top-level window. Only the serialized fields appear in `list` output.
#[derive(Clone, Debug, Serialize)]
pub struct WinInfo {
    pub hwnd: u64,
    pub pid: u32,
    pub exe: String,
    pub title: String,
    pub rect: Rect,
    pub monitor: u32,
    /// The window's own DPI (GetDpiForWindow/96): 1.0 for DPI-unaware apps even
    /// on a scaled monitor, where the OS bitmap-stretches them.
    pub dpi_scale: f64,
    /// Effective display scale of the monitor the window is on.
    pub monitor_dpi_scale: f64,
    pub minimized: bool,
    #[serde(skip)]
    pub class: String,
    /// Direct owner window (0 = none).
    #[serde(skip)]
    pub owner: u64,
    /// Position in EnumWindows order: 0 is topmost.
    #[serde(skip)]
    pub z: usize,
    #[serde(skip)]
    pub visible: bool,
    #[serde(skip)]
    pub cloaked: bool,
    #[serde(skip)]
    pub popup: bool,
}

unsafe extern "system" fn window_cb(h: HWND, data: LPARAM) -> BOOL {
    let v = unsafe { &mut *(data.0 as *mut Vec<HWND>) };
    v.push(h);
    BOOL(1)
}

/// Every top-level window, in z-order, with no filtering.
pub fn enum_windows() -> Vec<WinInfo> {
    let mut hs: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(window_cb), LPARAM(&mut hs as *mut Vec<HWND> as isize));
    }
    let mons = monitors();
    let mut exes: HashMap<u32, String> = HashMap::new();
    hs.into_iter()
        .enumerate()
        .map(|(z, h)| {
            let id = hwnd_to_u64(h);
            let mut pid = 0u32;
            unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
            let exe = exes.entry(pid).or_insert_with(|| exe_of(pid)).clone();
            let mon = unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) };
            let mon_info = mons.iter().find(|m| m.hmon == mon.0 as isize);
            let monitor = mon_info.map_or(0, |m| m.index);
            let style = unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32;
            WinInfo {
                hwnd: id,
                pid,
                exe,
                title: title_of(h),
                rect: frame_bounds(id).unwrap_or(Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 0,
                }),
                monitor,
                dpi_scale: window_dpi_scale(id),
                monitor_dpi_scale: mon_info.map_or(1.0, |m| m.dpi_scale),
                minimized: unsafe { IsIconic(h).as_bool() },
                class: class_of(h),
                owner: owner_of(h),
                z,
                visible: unsafe { IsWindowVisible(h).as_bool() },
                cloaked: is_cloaked(h),
                popup: style & WS_POPUP.0 != 0,
            }
        })
        .collect()
}
