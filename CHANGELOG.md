## 0.2.0 (2026-11-24)

### Breaking Changes

#### Warnings are plain code strings; details moved to `data`

- `baseline_size_changed` no longer embeds JSON. The sizes are `data.baseline_size {w,h}` and `data.size {w,h}`.
- `other_candidate_window` is now one warning per command, and the other windows are listed in `data.other_candidates [{hwnd,title,rect}]`.
- `list_truncated` no longer carries "showing N of M" text (`count`, `total` and the new `limit` are in `data`).
- `processes_exited_during_sampling` and `access_denied_for_some_processes` lose their explanatory text; the denied processes are in `data.access_denied [{pid,exe}]`, and each `processes` entry already has its `exited` flag.

### Features

#### `--baseline <image>` for `shot` and `wait`

- Compare against an earlier shot instead of the first captured frame, so a change that finished before `sight` started (an instant UI reaction after a click) is still detected. Implies `--expect-change` / `--change`.
- Every shot now writes a sidecar `<name>.sight.json` (target, window-relative crop, pixel size before scaling, saved size, format). `--baseline` checks it and fails with `INVALID_ARGS` and a hint on a different target or a different `--crop` (a different pixel size counts as a change), instead of comparing mismatched images.
- JPEG (default) and downscaled baselines compare exactly: the live frame goes through the same resize and JPEG encode as `shot`, so no tolerance is needed.

#### Change-aware waiting, window-relative crop and window coordinate formulas

- `shot --expect-change` waits for pixels to differ from the first frame before `--settle`, so a click that has not started changing the UI yet no longer returns the old frame. `--min-change-px` (default 16) ignores noise such as a blinking caret. Output always has `changed` and `change_ms`; no change before `--timeout` is a `TIMEOUT` with `changed:false`.
- New `wait` command with the same waiting logic and no image: `sight wait --hwnd N --change --settle 300` prints `changed`, `change_ms`, `settled`, `elapsed_ms`.
- `--crop x,y,w,h` (on `shot` and `wait`) is relative to the window's DWM frame top-left, so it works wherever the window is. It is applied before `--scale` and `--max-edge`, and also works with the popup screen-capture fallback.
- Window shots add `window` (`frame`, `client`, `client_offset`), `formula_window` and `formula_client` next to the screen `formula`.
- Documented that `--max-edge 0` disables downscaling.

#### `sight rec`: record a window or region to an animated GIF

- `sight rec (--hwnd N | --pid N | --region x,y,w,h) [--crop ...] --duration ms [--fps N] [--max-edge N] [--until-settled ms] [--keyframes N] [--out path.gif]` blocks until the duration has passed (or, with `--until-settled`, until the image changed and then stayed unchanged), so agents run it in the background while they act.
- Uses the `gif` crate and supports multi-monitor regions. Identical frames are merged by extending the previous frame's delay; `gif_large` warns above about 10 MB.
- Output has `path`, `frames`, `fps`, `duration_ms`, `image`, `size_bytes` and the same `maps_to`, `formula`, `formula_window` and `formula_client` as `shot`. `--keyframes N` also saves N PNG stills of the most changed moments and lists them with `t_ms`.

#### `rec` picks one capture source at the start: `--source auto|screen|window`

- `--source auto` (default) records the window's screen rect (monitor capture, so menus, tooltips and popups are included) unless another visible window above the target overlaps its frame; then it uses window capture and warns `rec_window_source_no_popups`. `screen` and `window` force a source. The source is never switched mid-recording. Output has `source` and `source_reason` (`auto_not_occluded`, `auto_occluded`, `forced`).
- `--pad N` records N px around the window in a screen recording (for dropdowns that stick out), `--cursor` includes the mouse cursor.
- The screen capture follows a window that moves (fixed canvas size, `moved:true`, formulas valid for the start position) and keeps the start size when it resizes (`rec_window_resized`). Windows covering or uncovering the target mid-recording are listed in `occlusion_events` (`rec_occlusion_changed`).

### Fixes

#### `--baseline`: a different window size now counts as a change

- When the target window (or the `--crop` result) has another pixel size than the baseline, `shot`/`wait --baseline` now return `changed:true` with the warning `baseline_size_changed` (sizes in `data.baseline_size {w,h}` and `data.size {w,h}`) and then settle as usual, instead of `INVALID_ARGS`. Clicks often resize or expand a window.
- Still `INVALID_ARGS`: a different target kind (window vs region), a different `--crop`, a missing file, a foreign image with no sidecar and a different size, and a `--crop` that falls outside the window's new size.

## 0.1.0 (2026-11-23)

### Features

#### Initial release: observe and measure Windows apps

- `list` finds windows (hwnd, pid, exe, title, rect, minimized) with `--pid`, `--exe`, `--title` and `--all` filters.
- `shot` captures a window (`--pid` / `--hwnd`) or a screen `--region`, with `--settle`, `--scale`, `--max-edge`, `--timeout` and `--out`. Popups are included, and a mapping formula from image pixels to screen coordinates is returned.
- `stat` samples CPU and memory of a process tree over time.
- `doctor` lists monitors and their DPI scale.
- Every command prints a single-line JSON envelope (`v`, `ok`, `data`, `warnings` or `error`) with stable error codes and exit codes.

### Features

- initial project setup
