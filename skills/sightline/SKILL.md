---
name: sightline
description: Windows-only CLI that observes and measures apps. Use it to take a screenshot of a Windows app window, a screen region or an open popup/menu, wait for the UI to settle, check whether the UI changed after an action, map screenshot pixels to screen coordinates, list windows (hwnd, pid, exe, title), or measure CPU and RAM of a process tree over time. The command is `sight`. It never clicks, types or moves the mouse; use a separate input tool to act, then sightline to look. Output is JSON plus an image file path, never base64.
license: MIT
compatibility: Windows 10/11 only. Needs sight.exe (GitHub Release or cargo build). Cannot capture elevated or capture-protected windows.
---

# sightline

sightline (command: `sight`) looks at Windows apps and measures them. It **never clicks, types or moves the mouse**. Act with your own input tool, then use sightline to see the result.

Every command prints exactly one JSON line on stdout:

```json
{"v":1,"ok":true,"data":{...},"warnings":[...]}
{"v":1,"ok":false,"error":{"code":"...","message":"...","hint":"..."}}
```

Always check `ok` and read `warnings`. Add `--pretty` only for human reading. Screenshots are written to a file and you get `data.path`; open that file with your image-viewing tool.

## Get the binary

- Download `sight.exe` (or `sight-windows-x64.zip`, which also holds the READMEs, AGENTS.md and this skill) from the GitHub Releases page of the repository, and put it on `PATH`.
- Or build from source (Rust, Windows): `cargo build --release`; the binary is `target/release/sight.exe`.

Check it works with `sight doctor`.

## Commands

### `list`: find windows

`sight list [--pid N] [--exe TEXT] [--title TEXT] [--all] [--full-title] [--limit N]`

- `--exe` and `--title` are case-insensitive substring filters.
- By default invisible, untitled, cloaked, desktop (`Progman`/`WorkerW`) and tiny (<10 px) windows are hidden; `--all` disables all filters. Results are z-ordered, topmost first.
- Titles are cut to 40 chars + `…` with `title_truncated:true`; `--full-title` disables this. `--title` matches the full title.
- `--limit N` (default 50, also with `--all`): when cut, `total`, `truncated:true` and warning `list_truncated` appear. Narrow with `--exe`, `--title` or `--pid`.
- Minimized windows omit `rect` and `monitor`.
- `data`: `count` and `windows[]`, each with `hwnd`, `pid`, `exe`, `title`, `rect` (`x,y,w,h`, physical px), `monitor`, `dpi_scale`, `monitor_dpi_scale`, `minimized`.
- Use it first to get a `pid` or `hwnd`.

### `shot`: screenshot

`sight shot (--pid N | --hwnd N | --region x,y,w,h) [--settle ms] [--scale F] [--max-edge N] [--timeout ms] [--out path]`

Exactly one target:

| Flag | Meaning |
|---|---|
| `--pid N` | largest visible window of that process |
| `--hwnd N` | that window (decimal, as printed by `list`) |
| `--region x,y,w,h` | screen rectangle in physical pixels; plain screen capture, so popups and menus are included |

Options:

| Flag | Default | Meaning |
|---|---|---|
| `--settle ms` | off | wait until the image has not changed for this many ms (bounded by `--timeout`). Use ~300 after an action so animations finish |
| `--timeout ms` | 5000 | overall capture timeout |
| `--scale F` | 1 | resize factor, must be in (0,16]; nearest neighbour when > 1; a scale > 1 writes PNG |
| `--max-edge N` | 1568 | downscale so the long edge is at most N px; `0` = no limit |
| `--out path` | temp dir `sightline/` | output path; `.png` / `.jpg` picks the format |

`data`: `path`, `image` (`w`,`h`), `maps_to` (`space`, `x`,`y`,`w`,`h`: the screen rect covered), `scale`, `dpi_scale`, `monitor_dpi_scale`, `formula`, `settled`, `elapsed_ms`, and `popups[]` when popups were detected.

Use `--settle` and compare two shots to see whether the UI changed after an action. `settled:false` means the image was still changing at the timeout.

### `stat`: CPU and memory

`sight stat --pid N --duration ms [--interval ms] [--no-tree]`

- Samples the process and, unless `--no-tree`, all its child processes. `--interval` defaults to 500 ms; duration and interval must be > 0.
- `data`: `pid`, `exe`, `duration_ms`, `samples`, `logical_cores`, `cpu_core_pct` and `cpu_total_pct` (each `avg`, `p95`, `max`), `mem_mb` (`avg`, `p95`, `max`), `mem_measure`, `processes[]`.
- `cpu_core_pct`: 100 = one full core. `mem_mb` is **private bytes**, not the Task Manager number.
- Warning `target_exited` means the process ended during sampling; treat it as meaningful.
- For a leak check, run a long `--duration` while exercising the app and compare `mem_mb` over runs.

### `doctor`: environment

`sight doctor` lists `monitors[]` (`index`, `rect`, `dpi_scale`, `primary`). Run it on `INTERNAL`, or to understand a multi-monitor/DPI layout.

All commands accept `--pretty`.

## Observe loop

1. `sight list --exe myapp` and note `pid` / `hwnd`.
2. `sight shot --hwnd H --settle 300` and view `data.path`.
3. Act with your own tool (click/type) at screen coordinates.
4. `sight shot --hwnd H --settle 300` again and compare.

## Image pixels to screen coordinates

Use `data.formula` from the same `shot` result:

```
screen_x = <x0> + px / <scale>
screen_y = <y0> + py / <scale>
```

`px, py` are pixels in the saved image. `x0,y0` are `maps_to.x/y`. `scale` is often not 1.0 (large windows are downscaled by `--max-edge`), so never assume it. Results are physical screen pixels and can be negative on multi-monitor setups. `formula` is only valid for that image: re-run `shot` after any move or resize.

## Errors

Exit codes: 0 ok, 1 INVALID_ARGS, 2 TARGET_NOT_FOUND, 3 WINDOW_MINIMIZED / WINDOW_PROTECTED, 4 PERMISSION_DENIED, 5 TIMEOUT, 6 INTERNAL. Follow `error.hint`.

| Code | Exit | Do |
|---|---|---|
| `INVALID_ARGS` | 1 | Fix flags: exactly one of `--pid`/`--hwnd`/`--region`; scale in (0,16]; duration/interval > 0. Also returned for bad CLI syntax. |
| `TARGET_NOT_FOUND` | 2 | The pid/hwnd is gone or has no visible window. Run `list` again. |
| `WINDOW_MINIMIZED` | 3 | Restore the window with your own tool, then retry. |
| `WINDOW_PROTECTED` | 3 | The window opted out of capture. Do not retry; tell the user. |
| `PERMISSION_DENIED` | 4 | Target is likely elevated. Use a higher-privilege shell, or tell the user. |
| `TIMEOUT` | 5 | Image never settled or no frame arrived. If `data.path` exists the latest frame was saved and is usable. Otherwise raise `--timeout` or lower `--settle`. |
| `INTERNAL` | 6 | Run `doctor` and report the message. |

On failure the envelope may still carry `data` (for example the saved image after a timeout) and `warnings`.

## Pitfalls

- **DPI-unaware windows.** Warning `window_dpi_unaware_or_virtualized`: `dpi_scale` differs from `monitor_dpi_scale`. Coordinates are still physical pixels, but check that your input tool also uses physical pixels.
- **Mixed DPI regions.** Warning `region_spans_mixed_dpi`: the `--region` crosses monitors with different DPI scales.
- **Popups and menus.** A window with a popup (menu, dropdown) makes `shot` fall back to screen capture (warning `popups_included_via_screen_capture`); `maps_to` is the union rect and `formula` already accounts for it. To capture a popup yourself, use `--region` (screen capture).
- **Minimized windows** cannot be captured (`WINDOW_MINIMIZED`) have no `rect`/`monitor` in `list`. Restore them first.
- **Stale formula.** Moving or resizing the window invalidates `formula`; shoot again.
- **`--pid` picks the largest visible window**; when an app has several windows use `list` and `--hwnd`.
- **Large images.** The long edge is capped at 1568 px by default; use `--max-edge 0` or `--scale` when you need full resolution, and remember the formula divides by the reported `scale`.
- Untested: `WINDOW_PROTECTED`, and classic Win32 `#32768` menus.
