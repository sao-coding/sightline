//! Is a window covered by other windows? Used by `rec --source auto`.

use crate::shot::POPUP_CLASSES;
use crate::win::{self, Rect};

/// Windows above `target` in z-order that overlap `region` (screen px) and so
/// would show up in a screen capture of it.
///
/// Not counted: invisible, minimized, cloaked or empty windows; windows owned
/// (directly or through other owners) by the target, and same-process popup
/// windows of the known popup classes (the target's own menus and tooltips are
/// exactly what a screen capture is meant to show); and layered windows whose
/// constant alpha is 0, which draw nothing. Tool windows and click-through
/// overlays are counted: they are visible on screen.
pub fn occluders(target: u64, region: Rect) -> Vec<u64> {
    let target_pid = win::pid_of(target);
    let mut out = Vec::new();
    for h in win::top_level_hwnds() {
        if h == target {
            return out;
        }
        if !win::is_visible(h) || win::is_minimized(h) || win::is_cloaked_hwnd(h) {
            continue;
        }
        let Some(rect) = win::frame_bounds(h).filter(|r| !r.is_empty()) else {
            continue;
        };
        if rect.intersect(&region).is_none() {
            continue;
        }
        if owned_by(h, target) {
            continue;
        }
        if win::pid_of(h) == target_pid && POPUP_CLASSES.contains(&win::class_name(h).as_str()) {
            continue;
        }
        if win::is_fully_transparent(h) {
            continue;
        }
        out.push(h);
    }
    // Target not in the list (gone): nothing covers it.
    Vec::new()
}

/// True when `h`'s owner chain reaches `target`.
fn owned_by(h: u64, target: u64) -> bool {
    let mut owner = win::owner(h);
    for _ in 0..16 {
        if owner == 0 {
            return false;
        }
        if owner == target {
            return true;
        }
        owner = win::owner(owner);
    }
    false
}
