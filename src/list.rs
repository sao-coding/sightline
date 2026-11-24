use clap::Args;
use serde_json::{Value, json};

use crate::error::SightError;
use crate::win;

#[derive(Args)]
pub struct ListArgs {
    /// Only windows owned by this process id
    #[arg(long)]
    pub pid: Option<u32>,
    /// Only windows whose exe name contains this text (case-insensitive)
    #[arg(long)]
    pub exe: Option<String>,
    /// Only windows whose title contains this text (case-insensitive)
    #[arg(long)]
    pub title: Option<String>,
    /// Include invisible, cloaked, untitled, desktop and tiny windows
    #[arg(long)]
    pub all: bool,
    /// Do not truncate titles to 40 chars
    #[arg(long)]
    pub full_title: bool,
    /// Max windows to return
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

const TITLE_MAX: usize = 40;

fn noisy(w: &win::WinInfo) -> bool {
    w.cloaked
        || w.class == "Progman"
        || w.class == "WorkerW"
        || (!w.minimized && (w.rect.w < 10 || w.rect.h < 10))
}

fn to_json(w: &win::WinInfo, full_title: bool) -> Value {
    let mut v = json!({"hwnd": w.hwnd, "pid": w.pid, "exe": w.exe});
    if !full_title && w.title.chars().count() > TITLE_MAX {
        let t: String = w.title.chars().take(TITLE_MAX).collect();
        v["title"] = json!(format!("{t}…"));
        v["title_truncated"] = json!(true);
    } else {
        v["title"] = json!(w.title);
    }
    if !w.minimized {
        v["rect"] = json!(w.rect);
        v["monitor"] = json!(w.monitor);
    }
    v["dpi_scale"] = json!(w.dpi_scale);
    v["monitor_dpi_scale"] = json!(w.monitor_dpi_scale);
    v["minimized"] = json!(w.minimized);
    v
}

pub fn run(args: &ListArgs) -> Result<(Value, Vec<String>), SightError> {
    let exe = args.exe.as_ref().map(|s| s.to_lowercase());
    let title = args.title.as_ref().map(|s| s.to_lowercase());
    // enum_windows is z-ordered (topmost first); filters keep that order.
    let windows: Vec<_> = win::enum_windows()
        .into_iter()
        .filter(|w| args.all || (w.visible && !w.title.is_empty() && !noisy(w)))
        .filter(|w| args.pid.is_none_or(|p| w.pid == p))
        .filter(|w| {
            exe.as_ref()
                .is_none_or(|e| w.exe.to_lowercase().contains(e))
        })
        .filter(|w| {
            title
                .as_ref()
                .is_none_or(|t| w.title.to_lowercase().contains(t))
        })
        .collect();
    let total = windows.len();
    let shown: Vec<Value> = windows
        .iter()
        .take(args.limit)
        .map(|w| to_json(w, args.full_title))
        .collect();
    let mut data = json!({ "count": shown.len(), "windows": shown });
    let mut warnings = Vec::new();
    if total > args.limit {
        data["total"] = json!(total);
        data["truncated"] = json!(true);
        data["limit"] = json!(args.limit);
        warnings.push("list_truncated".into());
    }
    Ok((data, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::{Rect, WinInfo};

    fn win(class: &str, w: i32, h: i32) -> WinInfo {
        WinInfo {
            hwnd: 1,
            pid: 1,
            exe: "a.exe".into(),
            title: "t".into(),
            rect: Rect { x: 0, y: 0, w, h },
            monitor: 1,
            dpi_scale: 1.0,
            monitor_dpi_scale: 1.0,
            minimized: false,
            class: class.into(),
            owner: 0,
            z: 0,
            visible: true,
            cloaked: false,
            popup: false,
        }
    }

    #[test]
    fn noisy_filters_desktop_tiny_and_cloaked() {
        assert!(!noisy(&win("Notepad", 800, 600)));
        assert!(noisy(&win("Progman", 800, 600)));
        assert!(noisy(&win("WorkerW", 800, 600)));
        assert!(noisy(&win("Notepad", 9, 600)));
        let mut c = win("Notepad", 800, 600);
        c.cloaked = true;
        assert!(noisy(&c));
    }

    #[test]
    fn minimized_windows_are_not_tiny() {
        let mut m = win("Notepad", 0, 0);
        m.minimized = true;
        assert!(!noisy(&m));
    }
}
