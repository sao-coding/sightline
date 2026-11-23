# sightline (for agents)

The command is `sight` (`sight.exe`).

Windows-only CLI to **observe and measure** apps. It **never clicks, types or moves the mouse**. Use your own input tool to act; use sightline to look.

Every command prints one JSON line on stdout: `{"v":1,"ok":true,"data":{...},"warnings":[...]}` or `{"v":1,"ok":false,"error":{"code","message","hint"}}`. Check `ok`, read `warnings`. Add `--pretty` only for humans.

Screenshots are saved to a file; you get `data.path` (an image path), **never base64**. Open that file with your image-viewing tool.

## Commands

- `list [--pid N] [--exe TEXT] [--title TEXT] [--all] [--full-title] [--limit N]`: find windows, topmost first. Gives `hwnd`, `pid`, `exe`, `title`, `rect`, `minimized`. Use first to get a target. Default hides invisible/untitled/cloaked/desktop/tiny (<10px) windows; `--all` disables the filters. Titles cut to 40 chars (`title_truncated:true`; `--full-title` disables; `--title` matches the full title). Minimized windows omit `rect`/`monitor`. `--limit` (default 50) cuts the list: `total`, `truncated:true` and warning `list_truncated` appear; narrow with `--exe`/`--title`/`--pid`.
- `shot (--pid N | --hwnd N | --region x,y,w,h) [--settle ms] [--scale F] [--max-edge N] [--timeout ms] [--out path]`: screenshot. Exactly one target. `--pid` picks the largest visible window. `--region` is physical screen px and includes popups. Use `--settle 300` (or so) after an action so animations finish. Default long edge is capped at 1568 px.
- `stat --pid N --duration ms [--interval ms] [--no-tree]`: CPU and memory over time, includes child processes unless `--no-tree`. Use for perf/leak checks. `cpu_core_pct` 100 = one core; `mem_mb` is private bytes.
- `doctor`: lists monitors (`index`, `rect`, `dpi_scale`, `primary`); run when you get `INTERNAL`.

## Observe loop

1. `sight list --exe myapp` -> note `pid` / `hwnd`.
2. `sight shot --hwnd H --settle 300` -> view `data.path`.
3. Act with your own tool (click/type) at screen coordinates.
4. `sight shot --hwnd H --settle 300` again; compare.

## Image px -> screen coords

Use `data.formula` from the same `shot` result:

`screen_x = <x0> + px / <scale>; screen_y = <y0> + py / <scale>`

`px, py` are pixels in the saved image. `scale` is often not 1.0 (large windows are downscaled by `--max-edge`), so never assume it. Results are physical screen pixels (can be negative on multi-monitor setups); `maps_to` is the covered screen rect. If the window has popups (menu, dropdown), `shot` falls back to screen capture (`popups_included_via_screen_capture`) and `maps_to` is the union; `formula` already accounts for it. Warning `window_dpi_unaware_or_virtualized`: `dpi_scale` != `monitor_dpi_scale`; coordinates are still physical px, but verify your input tool uses physical px too. Re-run `shot` after any move/resize, since `formula` is only valid for that image.

## Errors: follow `error.hint`

Exit codes: 0 ok, 1 INVALID_ARGS, 2 TARGET_NOT_FOUND, 3 WINDOW_MINIMIZED/WINDOW_PROTECTED, 4 PERMISSION_DENIED, 5 TIMEOUT, 6 INTERNAL.

| Code | Do |
|---|---|
| `INVALID_ARGS` | Fix flags (one of --pid/--hwnd/--region; scale in (0,16]; duration/interval > 0). |
| `TARGET_NOT_FOUND` | `list` again; pid/hwnd is gone or has no visible window. |
| `WINDOW_MINIMIZED` | Restore the window with your own tool, retry. |
| `WINDOW_PROTECTED` | Window opted out of capture; do not retry. Tell the user. |
| `PERMISSION_DENIED` | Target is likely elevated; need a higher-privilege shell, or tell the user. |
| `TIMEOUT` | Image never settled or no frame. If `data.path` exists the latest frame was saved anyway; usable. Otherwise raise `--timeout` / lower `--settle`. |
| `INTERNAL` | Run `doctor`; report message. |

## Notes

- `stat` memory is private bytes, not Task Manager's number. Treat `warnings` like `target_exited` as meaningful.
- Untested: WINDOW_PROTECTED, classic Win32 `#32768` menus.
