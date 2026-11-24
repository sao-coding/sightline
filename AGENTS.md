# sightline (for agents)

The command is `sight` (`sight.exe`).

Windows-only CLI to **observe and measure** apps. It **never clicks, types or moves the mouse**. Use your own input tool to act; use sightline to look.

Every command prints one JSON line on stdout: `{"v":1,"ok":true,"data":{...},"warnings":[...]}` or `{"v":1,"ok":false,"error":{"code","message","hint"}}`. Check `ok`, read `warnings`. Add `--pretty` only for humans.

Screenshots are saved to a file; you get `data.path` (an image path), **never base64**. Open that file with your image-viewing tool.

## Commands

- `list [--pid N] [--exe TEXT] [--title TEXT] [--all] [--full-title] [--limit N]`: find windows, topmost first. Gives `hwnd`, `pid`, `exe`, `title`, `rect`, `minimized`. Use first to get a target. Default hides invisible/untitled/cloaked/desktop/tiny (<10px) windows; `--all` disables the filters. Titles cut to 40 chars (`title_truncated:true`; `--full-title` disables; `--title` matches the full title). Minimized windows omit `rect`/`monitor`. `--limit` (default 50) cuts the list: `total`, `truncated:true` and warning `list_truncated` appear; narrow with `--exe`/`--title`/`--pid`.
- `shot (--pid N | --hwnd N | --region x,y,w,h) [--expect-change] [--settle ms] [--min-change-px N] [--crop x,y,w,h] [--scale F] [--max-edge N] [--timeout ms] [--out path]`: screenshot. Exactly one target. `--pid` picks the largest visible window. `--region` is physical screen px and includes popups. Default long edge is capped at 1568 px; **`--max-edge 0` disables downscaling** (1:1 pixels).
  - `--settle 300` (or so): wait until the image stops changing.
  - `--expect-change`: first frame is a baseline; wait until pixels differ from it, *then* settle. Use right after an external click/keypress, because plain `--settle` can return the old, not-yet-updated frame. If nothing changes before `--timeout`: saves the latest frame anyway and fails with `TIMEOUT` (exit 5) and `data.changed:false`.
  - `--min-change-px N`: pixels that must differ to count as a change (default 16 with `--expect-change`, else 1 = exact). Ignores a blinking caret.
  - `--crop x,y,w,h`: window-relative rect (see below); `--hwnd`/`--pid` only (`INVALID_ARGS` with `--region`). Applied before `--scale`/`--max-edge`, so `--crop ... --max-edge 0` is exact 1:1 pixels.
  - Output always has `settled`, `changed`, `change_ms` (ms from command start to the first change, or `null`), `elapsed_ms`.
- `wait (--hwnd N | --pid N | --region x,y,w,h) [--change] [--settle ms] [--timeout ms] [--min-change-px N] [--crop x,y,w,h]`: same waiting as `shot`, but **saves no image**. Output `{changed, change_ms, settled, elapsed_ms}`. Use it to know the UI is ready for your next click.
- `stat --pid N --duration ms [--interval ms] [--no-tree]`: CPU and memory over time, includes child processes unless `--no-tree`. Use for perf/leak checks. `cpu_core_pct` 100 = one core; `mem_mb` is private bytes.
- `doctor`: lists monitors (`index`, `rect`, `dpi_scale`, `primary`); run when you get `INTERNAL`.

## Observe loop

1. `sight list --exe myapp` -> note `pid` / `hwnd`.
2. Act with your own tool (click/type).
3. `sight wait --hwnd H --change --settle 300` (UI ready?) or `sight shot --hwnd H --expect-change --settle 300` (and look at `data.path`).
4. Compare / act again. Don't add fixed sleeps after actions; don't move the window to a fixed spot before each shot.

`--expect-change` baselines on the first captured frame, which arrives about 0.3 s after `sight` starts. A change that finished before that is invisible: you get `TIMEOUT` with `changed:false` even though the UI did update. Then just `sight shot --hwnd H` and look. For instant UI reactions, use plain `--settle` (or start `wait --change` just before the action).

## Image px -> coordinates (pick the origin your input tool uses)

`px, py` are pixels in the saved image. `scale` is often not 1.0 (large windows are downscaled by `--max-edge`), so never assume it; use the formula from the same `shot` result. Re-run `shot` after any move/resize, since formulas are only valid for that image.

| Field | Origin | Use when your input tool takes |
|---|---|---|
| `formula` | screen (virtual desktop), physical px, can be negative | absolute screen coords |
| `formula_window` | top-left of the window's DWM frame | coords relative to the window |
| `formula_client` | top-left of the client area (below the title bar) | client-area coordinates (e.g. mouse messages sent to the window) |

`formula` -> `screen_x = <x0> + px / <scale>; screen_y = <y0> + py / <scale>`; the others have the same shape with `win_x`/`client_x`. `formula_window` and `formula_client` (and `window`) appear only for `--hwnd`/`--pid`, not `--region`. `data.window` = `frame {x,y,w,h}` (DWM extended frame bounds, screen px), `client {x,y,w,h}` (screen px), `client_offset {x,y}` (client top-left minus frame top-left). `--crop` uses the **frame** origin, the same as `formula_window`, so it keeps working wherever the window is.

If the window has popups (menu, dropdown), `shot` falls back to screen capture (`popups_included_via_screen_capture`) and `maps_to` is the union (with `--crop`, just the cropped rect); all formulas already account for it. Warning `window_dpi_unaware_or_virtualized`: `dpi_scale` != `monitor_dpi_scale`; coordinates are still physical px, but verify your input tool uses physical px too.

## Errors: follow `error.hint`

Exit codes: 0 ok, 1 INVALID_ARGS, 2 TARGET_NOT_FOUND, 3 WINDOW_MINIMIZED/WINDOW_PROTECTED, 4 PERMISSION_DENIED, 5 TIMEOUT, 6 INTERNAL.

| Code | Do |
|---|---|
| `INVALID_ARGS` | Fix flags (one of --pid/--hwnd/--region; scale in (0,16]; duration/interval > 0; `--crop` only with --hwnd/--pid and inside the window: the hint gives the window size). |
| `TARGET_NOT_FOUND` | `list` again; pid/hwnd is gone or has no visible window. |
| `WINDOW_MINIMIZED` | Restore the window with your own tool, retry. |
| `WINDOW_PROTECTED` | Window opted out of capture; do not retry. Tell the user. |
| `PERMISSION_DENIED` | Target is likely elevated; need a higher-privilege shell, or tell the user. |
| `TIMEOUT` | Image never settled, no change was seen (`data.changed:false`) or no frame. If `data.path` exists the latest frame was saved anyway; usable. Otherwise raise `--timeout` / lower `--settle`. |
| `INTERNAL` | Run `doctor`; report message. |

## Notes

- `stat` memory is private bytes, not Task Manager's number. Treat `warnings` like `target_exited` as meaningful.
- Untested: WINDOW_PROTECTED, classic Win32 `#32768` menus, windows at a DPI scale other than 100%.
