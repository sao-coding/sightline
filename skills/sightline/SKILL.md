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
- `--limit N` (default 50, also with `--all`): when cut, `total`, `limit`, `truncated:true` and warning `list_truncated` appear. Narrow with `--exe`, `--title` or `--pid`.
- Minimized windows omit `rect` and `monitor`.
- `data`: `count` and `windows[]`, each with `hwnd`, `pid`, `exe`, `title`, `rect` (`x,y,w,h`, physical px), `monitor`, `dpi_scale`, `monitor_dpi_scale`, `minimized`.
- Use it first to get a `pid` or `hwnd`.

### `shot`: screenshot

`sight shot (--pid N | --hwnd N | --region x,y,w,h) [--expect-change] [--settle ms] [--min-change-px N] [--crop x,y,w,h] [--baseline image] [--scale F] [--max-edge N] [--timeout ms] [--out path]`

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
| `--expect-change` | off | take the first frame as a baseline and wait until pixels differ from it, **then** apply `--settle`. Use right after an external click or keypress: plain `--settle` can return the old frame because the UI has not started changing yet. Both phases share `--timeout` |
| `--min-change-px N` | 16 with `--expect-change`, else 1 (exact) | how many pixels must differ to count as a change; ignores a blinking caret. With N > 1 a smaller difference can be missing from the saved image |
| `--baseline image` | none | compare against this earlier shot (its `data.path`) instead of the first frame, so a change that finished before `sight` started still counts. Implies `--expect-change`. See *Baselines* below |
| `--crop x,y,w,h` | none | keep only this rect, relative to the window's DWM frame top-left, in physical px. `--hwnd`/`--pid` only (`INVALID_ARGS` with `--region`, or when it falls outside the window; the hint gives the window size). Applied before `--scale`/`--max-edge`, so `--crop ... --max-edge 0` gives exactly w×h pixels. Works wherever the window is |
| `--timeout ms` | 5000 | overall timeout, shared by the change and settle phases |
| `--scale F` | 1 | resize factor, must be in (0,16]; nearest neighbour when > 1; a scale > 1 writes PNG |
| `--max-edge N` | 1568 | downscale so the long edge is at most N px; **`0` disables downscaling** (1:1 pixels) |
| `--out path` | temp dir `sightline/` | output path; `.png` / `.jpg` picks the format |

`data`: `path`, `image` (`w`,`h`), `maps_to` (`space`, `x`,`y`,`w`,`h`: the screen rect covered, crop included), `scale`, `dpi_scale`, `monitor_dpi_scale`, `formula`, then for windows `window`, `formula_window`, `formula_client`, and `settled`, `changed`, `change_ms`, `elapsed_ms`, and `popups[]` when popups were detected.

```json
{"v":1,"ok":true,"data":{"path":"...\\shot-20261124-142440-362.png","image":{"w":300,"h":200},"maps_to":{"space":"screen_physical","x":307,"y":270,"w":300,"h":200},"scale":1.0,"dpi_scale":1.0,"monitor_dpi_scale":1.0,"formula":"screen_x = 307 + px / 1.0; screen_y = 270 + py / 1.0","window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},"formula_window":"win_x = 100 + px / 1.0; win_y = 120 + py / 1.0","formula_client":"client_x = 99 + px / 1.0; client_y = 89 + py / 1.0","settled":true,"changed":true,"change_ms":307,"elapsed_ms":562,"popups":[]},"warnings":[]}
```

`changed` / `change_ms` are always present: whether a frame differing from the first one was seen, and ms from command start to it (`null` when none). `settled:false` means the wait did not finish before the timeout; with `--expect-change` and no change the command fails with `TIMEOUT` (exit 5), still saves the latest frame, and `data.changed` is `false`.

### `wait`: UI ready, no image

`sight wait (--hwnd N | --pid N | --region x,y,w,h) [--change] [--settle ms] [--timeout ms] [--min-change-px N] [--crop x,y,w,h] [--baseline image]`

Same logic and flags as `shot` (`--change` is `--expect-change`), but nothing is saved. Use it when you need the UI ready before your next click, not a picture. Output: `changed`, `change_ms`, `settled`, `elapsed_ms`.

```json
{"v":1,"ok":true,"data":{"changed":true,"change_ms":326,"settled":true,"elapsed_ms":492},"warnings":[]}
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"no change detected within --timeout 1000ms (no image saved)","hint":"..."},"data":{"changed":false,"change_ms":null,"settled":false,"elapsed_ms":1020}}
```

Use `--crop` on `wait` to watch only the part of the window that should change.

**Limit of `--expect-change` / `--change`.** The baseline is the first captured frame, which arrives about 0.3 s after `sight` starts. A change that finished before then is not seen, so `TIMEOUT` with `changed:false` can mean "already updated": take a plain `shot` and look, or use `--baseline`.

#### Baselines: catch changes that finish before `sight` starts

Take a shot *before* acting, act, then wait against that shot:

```
sight shot --hwnd H --out before.png
# act with your own tool
sight wait --hwnd H --baseline before.png --settle 300
```

The first frame is compared with the image, so an instant UI reaction still gives `changed:true` (`change_ms` is then about the first frame time, 300 ms or so). Without `--baseline` the same order gives a false `TIMEOUT` with `changed:false`.

- Must be the same target (the same window, or the same `--region`) and the same `--crop`. The window may have moved. If the window (or the `--crop` result) now has a different pixel size, that counts as a change: `changed:true` and warning `baseline_size_changed` (the sizes are in `data.baseline_size {w,h}` and `data.size {w,h}`), then `--settle` applies as usual, because a click often resizes or expands the window. If the new size puts the `--crop` outside the window, you get `INVALID_ARGS` (the hint gives the window size). Every shot writes a sidecar `<name>.sight.json` (target, crop, pixel size before scaling, saved size, format) that is checked; a different target kind (window vs region), a different `--crop`, a missing file or a foreign image without a sidecar is `INVALID_ARGS` with a hint, never a silent wrong comparison. A foreign image without a sidecar is assumed to be unscaled.
- Default JPEG shots and downscaled shots work: the live frame goes through the same resize and JPEG encode as `shot`, so equal pixels compare equal and no tolerance is needed. Exception: a scaled or JPEG baseline cannot be used for a `--region` (or popup screen capture) that spans several monitors; take that baseline as `.png` with `--max-edge 0`.
- Warnings: `baseline_size_changed`, `baseline_without_metadata`, `baseline_metadata_unreadable`, `baseline_metadata_not_written` (the shot could not write its sidecar).

### `rec`: record a GIF

`sight rec (--hwnd N | --pid N | --region x,y,w,h) [--crop x,y,w,h] [--source auto|screen|window] [--pad N] [--cursor] --duration ms [--fps N] [--max-edge N] [--until-settled ms] [--keyframes N] [--out path.gif]`

Records an animated GIF for README demos or test evidence. It **blocks** until `--duration` ms have passed (counted from the first captured frame, so wall time is about 0.7 s longer), or, with `--until-settled ms`, until the image has changed and then stayed unchanged for that long (never longer than `--duration`).

| Flag | Default | Meaning |
|---|---|---|
| `--duration ms` | required | recording length |
| `--source auto\|screen\|window` | auto | capture source, chosen once at the start (see below); `--hwnd`/`--pid` only (`--region` is always screen) |
| `--pad N` | 0 | screen source: also record N px around the window (clamped to the desktop), so a dropdown that sticks out is included. `--hwnd`/`--pid` only |
| `--cursor` | off | include the mouse cursor |
| `--fps N` | 10 | sampling rate, 1 to 30 |
| `--max-edge N` | 800 | downscale so the long edge is at most N px; `0` = full size |
| `--until-settled ms` | off | stop early once the image changed and then stayed unchanged this long. It arms at the first change, so a static window at the start does not end the recording |
| `--keyframes N` | 0 | also save N PNG stills: first, last and the biggest changes in between |
| `--crop`, target flags | | same meaning as for `shot`; with the screen source `--crop` is relative to the frame and may reach into the `--pad` area |
| `--out path` | temp dir `sightline/` | GIF path (stills go next to it as `<name>-keyN.png`) |

**One source, chosen at the start, never switched.**

- `screen` records the screen rect of the window (frame plus `--pad`): what the user sees, so menus, tooltips and popups are included, and so is anything covering the window.
- `window` records the window itself with window capture: unaffected by covering windows, but separate popup windows are not recorded.
- `auto` (default) checks at the start whether another visible, non-cloaked top-level window above the target in z-order overlaps its frame (plus `--pad`). The target's own popups and owned windows, same-process popup windows (menus, tooltips) and fully transparent layered windows do not count. Not covered: `screen`. Covered: `window`, with the warning `rec_window_source_no_popups`.

`source` and `source_reason` (`auto_not_occluded`, `auto_occluded`, `forced`) say what happened. If the window moves while the screen source records, the captured rect follows it every frame and the canvas keeps its start size; if it resizes, the canvas size is kept and the warning `rec_window_resized` is added once. The formulas are valid for the start position, and `moved:true` is added if the window moved. Windows covering or uncovering the target mid-recording never change the source: each change is in `occlusion_events:[{t_ms, occluded}]` and adds the warning `rec_occlusion_changed`.

Identical frames are merged by extending the previous frame's delay, so a mostly static window gives few frames. `data`: `path`, `frames`, `fps`, `duration_ms` (length of the GIF), `image`, `size_bytes`, `source`, `source_reason`, `stopped` (`duration` or `settled`), `elapsed_ms`, the same `maps_to`, `scale`, `formula`, `window`, `formula_window`, `formula_client` as `shot` (they map GIF pixels, and keyframe pixels, to coordinates as of the start of recording), `occlusion_events` (window targets), `moved` and `keyframes:[{t_ms,path}]` when applicable (`t_ms` counts from the start of the GIF). Warnings: `gif_large` (over about 10 MB), `rec_window_source_no_popups`, `rec_window_resized`, `rec_occlusion_changed`, `rec_pad_ignored`.

```json
{"v":1,"ok":true,"data":{"path":"...\\demo.gif","frames":2,"fps":10,"duration_ms":3000,"image":{"w":682,"h":492},"size_bytes":63545,"maps_to":{"space":"screen_physical","x":187,"y":130,"w":682,"h":492},"scale":1.0,"dpi_scale":1.0,"monitor_dpi_scale":1.0,"formula":"screen_x = 187 + px / 1.0; screen_y = 130 + py / 1.0","window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},"formula_window":"win_x = -20 + px / 1.0; win_y = -20 + py / 1.0","formula_client":"client_x = -21 + px / 1.0; client_y = -51 + py / 1.0","source":"screen","source_reason":"auto_not_occluded","occlusion_events":[],"stopped":"duration","elapsed_ms":3378,"keyframes":[{"t_ms":1,"path":"...\\demo-key1.png"},{"t_ms":800,"path":"...\\demo-key2.png"}]},"warnings":[]}
```

Run it in the background while you act, because it blocks:

```powershell
$job = Start-Job { sight rec --hwnd 123456 --duration 10000 --until-settled 500 --keyframes 3 --out C:\tmp\demo.gif }
Start-Sleep -Milliseconds 700      # capture needs ~0.3 s per monitor session to start
# ... act with your own tool (click, type) ...
$rec = Receive-Job $job -Wait -AutoRemoveJob | ConvertFrom-Json   # data.path, data.keyframes[].path
```

Or `Start-Process sight -ArgumentList 'rec','--hwnd','123456','--duration','10000','--out','C:\tmp\demo.gif' -NoNewWindow -RedirectStandardOutput C:\tmp\rec.json`, then read `rec.json` once the process has exited. Use `--keyframes` to check the recording without decoding the GIF.

A covered window that opens a menu: real menus are topmost windows and are drawn above the covering window, but ordinary owned popup windows stay right above their owner, below the covering window. So a `screen` recording of a covered window shows the covering window and catches menus but may miss plain popups, and the `window` source never has popups. Uncover the window if you need its popups.

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
2. Act with your own tool (click/type).
3. `sight wait --hwnd H --change --settle 300` (is the UI ready?) or `sight shot --hwnd H --expect-change --settle 300` and view `data.path`.
4. Compare, then act again.

Do not add fixed sleeps after actions, and do not move the window to a fixed place before each shot: `--crop` and the window formulas work wherever the window is.

## Image pixels to coordinates

`px, py` are pixels in the saved image. `scale` is often not 1.0 (large windows are downscaled by `--max-edge`), so never assume it; always use the formula from the same `shot` result. Results are physical pixels, and screen coordinates can be negative on multi-monitor setups. Formulas are only valid for that image: re-run `shot` after any move or resize.

Three formulas, three origins. **Pick the one that matches the origin your input tool uses.**

| Field | Origin | Example |
|---|---|---|
| `formula` | the screen (virtual desktop) | `screen_x = 307 + px / 1.0; screen_y = 270 + py / 1.0` |
| `formula_window` | top-left of the window's DWM frame (the visible window, title bar included) | `win_x = 100 + px / 1.0; win_y = 120 + py / 1.0` |
| `formula_client` | top-left of the client area (below the title bar and borders) | `client_x = 99 + px / 1.0; client_y = 89 + py / 1.0` |

- Use `formula` for a tool that clicks at absolute screen coordinates.
- Use `formula_window` for a tool relative to the window, and for `--crop`, which uses the same origin.
- Use `formula_client` for a tool relative to the client area, for example a mouse message sent to the window.
- `data.window` has `frame {x,y,w,h}` (DWM extended frame bounds, screen px), `client {x,y,w,h}` (screen px) and `client_offset {x,y}` (client top-left minus frame top-left, so `client = frame + client_offset`). It and the two window formulas appear for `--hwnd`/`--pid` only, not `--region`.
- Example: with the output above, image point (50,40) is screen (357,310), window (150,160) and client (149,129).

## Errors

Exit codes: 0 ok, 1 INVALID_ARGS, 2 TARGET_NOT_FOUND, 3 WINDOW_MINIMIZED / WINDOW_PROTECTED, 4 PERMISSION_DENIED, 5 TIMEOUT, 6 INTERNAL. Follow `error.hint`.

| Code | Exit | Do |
|---|---|---|
| `INVALID_ARGS` | 1 | Fix flags (`--baseline` must be readable, from the same target kind and `--crop`; the hint says how to retake it): exactly one of `--pid`/`--hwnd`/`--region`; scale in (0,16]; duration/interval > 0; `--crop` only with `--pid`/`--hwnd` and inside the window (the hint gives the window size). Also returned for bad CLI syntax. |
| `TARGET_NOT_FOUND` | 2 | The pid/hwnd is gone or has no visible window. Run `list` again. |
| `WINDOW_MINIMIZED` | 3 | Restore the window with your own tool, then retry. |
| `WINDOW_PROTECTED` | 3 | The window opted out of capture. Do not retry; tell the user. |
| `PERMISSION_DENIED` | 4 | Target is likely elevated. Use a higher-privilege shell, or tell the user. |
| `TIMEOUT` | 5 | Image never settled, no change was seen (`data.changed:false`), or no frame arrived. If `data.path` exists the latest frame was saved and is usable. Otherwise raise `--timeout` or lower `--settle`. |
| `INTERNAL` | 6 | Run `doctor` and report the message. |

On failure the envelope may still carry `data` (for example the saved image after a timeout) and `warnings`.

## Pitfalls

- **DPI-unaware windows.** Warning `window_dpi_unaware_or_virtualized`: `dpi_scale` differs from `monitor_dpi_scale`. Coordinates are still physical pixels, but check that your input tool also uses physical pixels.
- **Mixed DPI regions.** Warning `region_spans_mixed_dpi`: the `--region` crosses monitors with different DPI scales.
- **Popups and menus.** A window with a popup (menu, dropdown) makes `shot` fall back to screen capture (warning `popups_included_via_screen_capture`); `maps_to` is the union rect (with `--crop`, just the cropped rect) and all formulas already account for it. To capture a popup yourself, use `--region` (screen capture).
- **Minimized windows** cannot be captured (`WINDOW_MINIMIZED`) have no `rect`/`monitor` in `list`. Restore them first.
- **`rec` blocks.** Run it in the background (see above) and wait ~0.7 s before acting.
- **Stale formula.** Moving or resizing the window invalidates all three formulas; shoot again. `--crop` is window-relative, so it keeps selecting the same part of the window after a move.
- **Race after a click.** Plain `--settle` can return the old frame if the UI has not started changing yet; use `--expect-change` / `wait --change`.
- **`--pid` picks the largest visible window**; when an app has several windows use `list` and `--hwnd`.
- **Large images.** The long edge is capped at 1568 px by default; use `--max-edge 0` (no downscaling) or `--scale` when you need full resolution, and remember the formulas divide by the reported `scale`.
- Untested: `WINDOW_PROTECTED`, classic Win32 `#32768` menus, and windows at a DPI scale other than 100%.
