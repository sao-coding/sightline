# sightline (for agents)

The command is `sight` (`sight.exe`).

Windows-only CLI to **observe and measure** apps. It **never clicks, types or moves the mouse**. Use your own input tool to act; use sightline to look.

Every command prints one JSON line on stdout: `{"v":1,"ok":true,"data":{...},"warnings":[...]}` or `{"v":1,"ok":false,"error":{"code","message","hint"}}`. Check `ok`, read `warnings`. Add `--pretty` only for humans.

Screenshots are saved to a file; you get `data.path` (an image path), **never base64**. Open that file with your image-viewing tool.

## Commands

- `list [--pid N] [--exe TEXT] [--title TEXT] [--all] [--full-title] [--limit N]`: find windows, topmost first. Gives `hwnd`, `pid`, `exe`, `title`, `rect`, `minimized`. Use first to get a target. Default hides invisible/untitled/cloaked/desktop/tiny (<10px) windows; `--all` disables the filters. Titles cut to 40 chars (`title_truncated:true`; `--full-title` disables; `--title` matches the full title). Minimized windows omit `rect`/`monitor`. `--limit` (default 50) cuts the list: `total`, `truncated:true` and warning `list_truncated` appear; narrow with `--exe`/`--title`/`--pid`.
- `shot (--pid N | --hwnd N | --region x,y,w,h) [--expect-change] [--settle ms] [--min-change-px N] [--crop x,y,w,h] [--baseline image] [--scale F] [--max-edge N] [--timeout ms] [--out path]`: screenshot. Exactly one target. `--pid` picks the largest visible window. `--region` is physical screen px and includes popups. Default long edge is capped at 1568 px; **`--max-edge 0` disables downscaling** (1:1 pixels).
  - `--settle 300` (or so): wait until the image stops changing.
  - `--expect-change`: first frame is a baseline; wait until pixels differ from it, *then* settle. Use right after an external click/keypress, because plain `--settle` can return the old, not-yet-updated frame. If nothing changes before `--timeout`: saves the latest frame anyway and fails with `TIMEOUT` (exit 5) and `data.changed:false`.
  - `--min-change-px N`: pixels that must differ to count as a change (default 16 with `--expect-change`, else 1 = exact). Ignores a blinking caret.
  - `--baseline image`: compare against an earlier shot (its `data.path`) instead of the first frame; implies `--expect-change`. See below.
  - `--crop x,y,w,h`: window-relative rect (see below); `--hwnd`/`--pid` only (`INVALID_ARGS` with `--region`). Applied before `--scale`/`--max-edge`, so `--crop ... --max-edge 0` is exact 1:1 pixels.
  - Output always has `settled`, `changed`, `change_ms` (ms from command start to the first change, or `null`), `elapsed_ms`.
- `wait (--hwnd N | --pid N | --region x,y,w,h) [--change] [--settle ms] [--timeout ms] [--min-change-px N] [--crop x,y,w,h] [--baseline image]`: same waiting as `shot`, but **saves no image**. Output `{changed, change_ms, settled, elapsed_ms}`. Use it to know the UI is ready for your next click.
- `rec (--hwnd N | --pid N | --region x,y,w,h) [--crop x,y,w,h] --duration ms [--fps N] [--max-edge N] [--until-settled ms] [--keyframes N] [--out path.gif]`: record an animated GIF (README demos, test evidence). **Blocks** until `--duration` ms (counted from the first frame; wall time is about 0.7 s more) has passed, or with `--until-settled` until the image changed and then stayed unchanged for that many ms. `--fps` default 10 (max 30), `--max-edge` default 800 (0 = full size). Identical frames are merged into one longer frame. Output: `path`, `frames`, `fps`, `duration_ms`, `image {w,h}`, `size_bytes`, `stopped` (`duration`/`settled`), the same `maps_to`/`formula`/`formula_window`/`formula_client` as `shot` (valid for every frame; the window position is the one at start), and with `--keyframes N` also `keyframes:[{t_ms, path}]`: N PNG stills (first, last and the biggest changes) so you can check the recording without decoding the GIF (`t_ms` is from the start of the GIF). Warning `gif_large` above about 10 MB. Popups are detected when recording starts only; for menus that open mid-recording use `--region`.
- `stat --pid N --duration ms [--interval ms] [--no-tree]`: CPU and memory over time, includes child processes unless `--no-tree`. Use for perf/leak checks. `cpu_core_pct` 100 = one core; `mem_mb` is private bytes.
- `doctor`: lists monitors (`index`, `rect`, `dpi_scale`, `primary`); run when you get `INTERNAL`.

## Observe loop

1. `sight list --exe myapp` -> note `pid` / `hwnd`.
2. Act with your own tool (click/type).
3. `sight wait --hwnd H --change --settle 300` (UI ready?) or `sight shot --hwnd H --expect-change --settle 300` (and look at `data.path`).
4. Compare / act again. Don't add fixed sleeps after actions; don't move the window to a fixed spot before each shot.

`--expect-change` baselines on the first captured frame, which arrives about 0.3 s after `sight` starts. A change that finished before that is invisible: you get `TIMEOUT` with `changed:false` even though the UI did update.

**Fix: `--baseline`.** Take a `sight shot` *before* acting, act, then pass that shot's `data.path`:

```
sight shot --hwnd H --out before.png        # 1. baseline
# 2. act (click/type) with your own tool
sight wait --hwnd H --baseline before.png --settle 300   # 3. changed:true even if the UI reacted instantly
```

The first frame is compared with the baseline image, so a change that completed before `sight` started still counts (`change_ms` is then about the first frame time). Rules: same target (`--hwnd`/`--pid` window or same `--region`), same `--crop`. If the window (or the `--crop` result) has a different pixel size now, that counts as a change: `changed:true` plus warning `baseline_size_changed: {"baseline":{"w":..,"h":..},"now":{"w":..,"h":..}}`, then settle as usual (clicks often resize or expand a window). If the new size makes the `--crop` fall outside the window you get `INVALID_ARGS` (the hint gives the window size). Each shot writes a sidecar `<name>.sight.json` that is checked; a different target kind (window vs region), a different `--crop`, a missing file or a foreign image without a sidecar is `INVALID_ARGS` with a hint, never a silent wrong comparison. JPEG and downscaled baselines work (the live frame goes through the same resize and JPEG encode, so no tolerance is needed); a baseline cannot be a scaled/JPEG image for a `--region`/popup capture that spans several monitors (use `--max-edge 0` and a `.png` there).

## Recording while you act (`rec` blocks)

`rec` returns only when it is finished, so start it in the background, act, then collect the result. It needs about 0.3 s to start capturing: wait ~0.5 s before the first action.

```powershell
$job = Start-Job { sight rec --hwnd 123456 --duration 10000 --until-settled 500 --keyframes 3 --out C:\tmp\demo.gif }
Start-Sleep -Milliseconds 500      # capture needs ~0.3 s to start
# ... act with your own tool (click, type) ...
$rec = Receive-Job $job -Wait -AutoRemoveJob | ConvertFrom-Json   # data.path, data.keyframes[].path
```

Or, without a job: `Start-Process sight -ArgumentList 'rec','--hwnd','123456','--duration','10000','--out','C:\tmp\demo.gif' -NoNewWindow -RedirectStandardOutput C:\tmp\rec.json`, act, then read `C:\tmp\rec.json` after the process exits. `--until-settled` only arms once the image has changed, so a static window at the start does not end the recording; if nothing ever changes the full `--duration` is recorded.

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
