# sightline reference

Back to the [README](../README.md).

## Command reference

Global flag: `--pretty` (indented JSON for humans) on every command.

### `list`

List top-level windows, topmost first. By default only visible, titled windows are shown; the
desktop, cloaked windows and windows under 10 px are hidden.

| Flag | Meaning |
|---|---|
| `--pid <PID>` | Only windows of this process |
| `--exe <TEXT>` | Exe name contains text (case-insensitive) |
| `--title <TEXT>` | Title contains text (case-insensitive, matches the full title) |
| `--all` | Disable all default filters |
| `--full-title` | Do not truncate titles (default: first 40 chars + `…`) |
| `--limit <N>` | Max windows returned (default 50) |

Each window has `hwnd`, `pid`, `exe`, `title`, `rect` (physical screen px), `monitor` (1-based),
`dpi_scale`, `monitor_dpi_scale`, `minimized`. Minimized windows omit `rect` and `monitor`. If
`--limit` cuts the list, `data` gets `total` and `truncated:true`, plus a `list_truncated`
warning: narrow with `--exe`, `--title` or `--pid`.

### `shot`

Screenshot to a file. Exactly one of `--pid`, `--hwnd`, `--region` is required.

| Flag | Default | Meaning |
|---|---|---|
| `--pid <PID>` | | Largest visible window of this process (others appear as `other_candidate_window` warnings) |
| `--hwnd <HWND>` | | This window (decimal, as printed by `list`) |
| `--region x,y,w,h` | | Screen region in physical px; a real screen capture, so anything on top is included |
| `--crop x,y,w,h` | | Keep only this rect, relative to the window's DWM frame top-left, in physical px. `--pid` / `--hwnd` only (`INVALID_ARGS` with `--region`, or when it falls outside the window; the hint gives the window size). Applied before `--scale` and `--max-edge` |
| `--baseline <image>` | | Compare against this earlier shot (its `data.path`) instead of the first frame. Implies `--expect-change`. See "Baselines" below |
| `--scale <F>` | `1.0` | Resize factor in (0, 16]. Above 1 uses nearest neighbour and writes PNG |
| `--max-edge <N>` | `1568` | Downscale so the long edge is at most N px. **`0` disables downscaling** (1:1 pixels) |
| `--expect-change` | off | Take the first frame as a baseline and wait until pixels differ from it, then apply `--settle`. For use right after an external click or keypress |
| `--min-change-px <N>` | `16` with `--expect-change`, else `1` | Differing pixels needed to count as a change (>= 1); `1` is an exact compare. Ignores noise such as a blinking caret |
| `--settle <ms>` | off | Wait until the image has not changed for this long |
| `--timeout <ms>` | `5000` | Overall timeout, shared by the change and settle phases |
| `--out <path>` | temp dir | `.png` / `.jpg` / `.jpeg` picks the format. Default `%TEMP%\sightline\shot-<timestamp>.jpg` |

JPEG quality is 80. Window captures also return `popups` (possibly empty). Warnings:
`other_candidate_window`, `popups_included_via_screen_capture`, `region_partially_offscreen`,
`region_spans_mixed_dpi`, `window_dpi_unaware_or_virtualized`, `client_rect_unavailable`.

Output fields: `path`, `image`, `maps_to`, `scale`, `dpi_scale`, `monitor_dpi_scale`, `formula`; for
window targets `window`, `formula_window` and `formula_client` (see
[Coordinates and DPI](#coordinates-and-dpi)); and always `settled`, `changed`, `change_ms`,
`elapsed_ms`.

**Change-aware waiting.** Plain `--settle` waits for the image to stop changing, but right after an
external click the UI may not have started changing yet, so it returns the old frame as "settled".
With `--expect-change`:

1. The first frame is the baseline.
2. The command waits until a frame differs from the last accepted frame by at least
   `--min-change-px` pixels (the first such frame is the first change), then applies `--settle` as
   usual.
3. Both phases share `--timeout`. If nothing changes in time, the latest frame is still saved and
   the command fails with `TIMEOUT` (exit 5), `data.changed:false`, `data.settled:false`.

`changed` says whether a frame differing from the baseline was seen, and `change_ms` is the time
from command start to it (`null` if none), in both modes. A resized window counts as a change. With
`--min-change-px` above 1, differences smaller than the threshold are dropped, so they can be
missing from the saved image.

The baseline is the first captured frame, which arrives about 0.3 s after `sight` starts (capture
start-up). A change that finished before then is not seen: `TIMEOUT` with `changed:false` can mean
the UI already updated, so take a plain `shot` and look. For UI that reacts instantly use
`--settle` alone, or start the command just before you act.

**Baselines.** `--baseline <image>` replaces "the first frame" with an image you took earlier, so a
change that completed before `sight` started is still detected. Realistic order:

```
> sight shot --hwnd 11736012 --out before.png
# act: the UI reacts instantly
> sight wait --hwnd 11736012 --baseline before.png --settle 300
{"v":1,"ok":true,"data":{"changed":true,"change_ms":318,"settled":true,"elapsed_ms":502},"warnings":[]}
> sight wait --hwnd 11736012 --change --timeout 1500      # same order, no baseline
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"no change detected within --timeout 1500ms (no image saved)",...},"data":{"changed":false,"change_ms":null,"settled":false,"elapsed_ms":1534}}
```

The first captured frame is compared with the image; if it differs by at least `--min-change-px`
pixels the change is counted at once (`change_ms` is about the first frame time), then `--settle`
applies as usual. If it matches, waiting continues for a later change as with `--expect-change`.

- **Comparable only.** Every `shot` writes a sidecar `<name>.sight.json` next to the image (replacing
  its extension): the target (`window` + `hwnd`, or `region` + rect), the window-relative `crop`, the
  pixel size after cropping and before scaling (`source`), the saved size (`image`) and the `format`.
  `--baseline` checks it against the new capture and returns `INVALID_ARGS` with a hint when the
  target or `--crop` differs, when the pixel size after `--crop` and before scaling differs (including a
  window that was resized: take the baseline again), or when the image no longer matches its sidecar.
  Moving a window without resizing it is fine, because crops and comparisons are window-relative.
  A baseline that is not a sight shot (no sidecar) is assumed to be an unscaled 1:1 image, with the
  warning `baseline_without_metadata`.
- **JPEG and scaled baselines need no tolerance.** The default `shot` format is JPEG (quality 80) and
  the default `--max-edge` downscales big windows. Instead of fuzzy matching, the live frame is run
  through the same resize and the same JPEG encode/decode as `shot` before it is compared, so
  identical pixels compare equal exactly. A `.png` with `--max-edge 0` is still the cheapest (no
  per-frame conversion). The one exception: a scaled or JPEG baseline cannot be cut up for a `--region`
  (or popup screen capture) that spans several monitors or the screen edge; that is `INVALID_ARGS`, so
  take that baseline as `.png` with `--max-edge 0`.
- Other warnings: `baseline_metadata_unreadable`, `baseline_metadata_not_written` (the sidecar could not
  be written).

### `wait`

Same target, `--crop`, `--settle`, `--timeout`, `--min-change-px` and `--baseline` flags as `shot`, and the same
waiting logic, but **no image is saved**. `--change` is `--expect-change`. Use it when the agent
needs the UI ready before its next click.

```
> sight wait --hwnd 11736012 --change --settle 300
{"v":1,"ok":true,"data":{"changed":true,"change_ms":326,"settled":true,"elapsed_ms":492},"warnings":[]}
> sight wait --hwnd 11736012 --change --timeout 1000
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"no change detected within --timeout 1000ms (no image saved)","hint":"increase --timeout or --settle; with --expect-change/--change, make sure the action really changes pixels in the target"},"data":{"changed":false,"change_ms":null,"settled":false,"elapsed_ms":1020}}
```

Output: `changed`, `change_ms`, `settled`, `elapsed_ms`. Use `--crop` to watch only the part of the
window that should change.

### `stat`

Sample CPU and memory of a process and, by default, all its descendants.

| Flag | Default | Meaning |
|---|---|---|
| `--pid <PID>` | required | Root process |
| `--duration <ms>` | required | Sampling duration (> 0) |
| `--interval <ms>` | `500` | Time between samples (> 0) |
| `--no-tree` | off | Root process only |

- `cpu_core_pct`: 100 = one full logical core. `cpu_total_pct` = `cpu_core_pct` / `logical_cores`.
- `mem_mb`: private bytes in MiB, summed over the tree. `processes` lists every process sampled.
- Why the tree matters: on one machine, an Edge PID reported 46 processes and 6293 MiB; with
  `--no-tree` the same PID showed 561 MiB.
- Warnings: `target_exited`, `processes_exited_during_sampling`, `access_denied_for_some_processes`.

### `doctor`

Lists monitors: `index`, `rect` (physical virtual-desktop px), `dpi_scale`, `primary`.

## Coordinates and DPI

Every coordinate sightline reports is a **physical screen pixel** on the virtual desktop
(negative values are normal on multi-monitor setups). It runs Per-Monitor V2 DPI aware, so no
scaled "logical" coordinates leak through.

The saved image is usually smaller than the window. `shot` gives `scale` (image px per screen px)
and a ready formula, so the agent never has to work it out.

**Worked example.** A File Explorer window at 150% DPI, 1691×953 physical px, default
`--max-edge 1568`:

```
> sight shot --hwnd 3870832 --settle 300
... "maps_to":{"x":798,"y":278,"w":1691,"h":953},"scale":0.9273,
    "formula":"screen_x = 798 + px / 0.9273; screen_y = 278 + py / 0.9273" ...
```

The model sees a button at image pixel (400, 200):

```
screen_x = 798 + 400 / 0.9273 ≈ 1229
screen_y = 278 + 200 / 0.9273 ≈ 494
```

Click at physical screen (1229, 494). Re-run `shot` after any move or resize: `formula` is only
valid for that image.

**Window and client origins.** `formula` maps to the screen. For `--hwnd` / `--pid` shots, two more
formulas map to the window, because many input tools are window-relative. Pick the origin your own
input tool uses:

| Field | Origin |
|---|---|
| `formula` | the screen (virtual desktop), physical px |
| `formula_window` | top-left of the window's DWM extended frame (the visible window, title bar included) |
| `formula_client` | top-left of the client area (below the title bar and borders) |

`data.window` holds `frame {x,y,w,h}` (DWM extended frame bounds, screen px), `client {x,y,w,h}`
(GetClientRect + ClientToScreen, screen px) and `client_offset {x,y}` (client top-left minus frame
top-left). These three fields and the two formulas are omitted for `--region`. Real output for a
window with a title bar:

```
"maps_to":{"x":307,"y":270,"w":300,"h":200},"scale":1.0,
"formula":"screen_x = 307 + px / 1.0; screen_y = 270 + py / 1.0",
"window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},
"formula_window":"win_x = 100 + px / 1.0; win_y = 120 + py / 1.0",
"formula_client":"client_x = 99 + px / 1.0; client_y = 89 + py / 1.0"
```

Image point (50, 40) is screen (307+50, 270+40) = (357, 310), window (150, 160) and client
(149, 129). The offsets are `maps_to` minus the frame or client origin, so they stay correct with
`--crop`, `--scale`, `--max-edge` and the popup fallback.

**`--crop`.** `--crop x,y,w,h` is relative to the DWM frame top-left (the `formula_window` origin)
in physical px, so it selects the same part of the window wherever the window is, with no need to
move it first. It is applied before `--scale` / `--max-edge`: `--crop 100,120,300,200 --max-edge 0`
writes exactly 300×200 pixels, identical to that rect cut from a full 1:1 shot. A crop outside the
frame is `INVALID_ARGS` and the hint gives the window size. When popups overlap the cropped area,
the screen-capture fallback uses exactly the translated crop rect, so it still means the same part
of the window.

**`dpi_scale` vs `monitor_dpi_scale`.** The first is the window's own DPI / 96, the second is its
monitor's scale. A DPI-unaware app reports `1.0` on a 150% monitor because Windows stretches its
bitmap. When they differ, `shot` warns `window_dpi_unaware_or_virtualized`: the formula is still
in physical px, but check that your input tool uses physical px too.

**Popups.** Window capture does not include menus, dropdowns or tooltips, because they are
separate top-level windows. If visible popups of the same process overlap the target (owned by
it, or class `#32768`, `tooltips_class32`, `ComboLBox`, `Xaml_WindowedPopupClass`,
`Microsoft.UI.Content.PopupWindowSiteBridge`), `shot` captures the screen area covering the
window plus its popups instead. It warns `popups_included_via_screen_capture`, lists them in
`data.popups`, and every formula already accounts for the larger area (with `--crop`, the area is just the cropped
rect). In this mode other apps drawn
over that area also appear in the image.

## Output contract

Every command prints exactly one JSON line on stdout:

```json
{"v":1,"ok":true,"data":{},"warnings":[]}
{"v":1,"ok":false,"error":{"code":"TARGET_NOT_FOUND","message":"no visible window found for pid 99999999","hint":"run `sight list` to see available targets"}}
```

Check `ok`, then read `warnings`. `hint` names the next step. A failure can still carry `data`:
on a `--settle` or `--expect-change` `TIMEOUT` the latest frame was saved anyway and is usable.

| Code | Exit | Meaning | `hint` |
|---|---|---|---|
| (success) | 0 | | |
| `INVALID_ARGS` | 1 | Bad or conflicting flags, `--crop` with `--region`, a `--crop` outside the window, or a `--baseline` that is unreadable or not comparable | run `sight --help` for usage (for a bad `--crop`, the hint gives the window size) |
| `TARGET_NOT_FOUND` | 2 | No such pid / hwnd / visible window | run `sight list` to see available targets |
| `WINDOW_MINIMIZED` | 3 | Target is minimized | restore the window and retry |
| `WINDOW_PROTECTED` | 3 | Window opted out of capture; do not retry | the window opted out of capture (display affinity) |
| `PERMISSION_DENIED` | 4 | Cannot read the process (for example, elevated) | run with sufficient privileges for the target process |
| `TIMEOUT` | 5 | No frame, the image did not settle in time, or no change was seen with `--expect-change` / `--change` (`data.changed:false`) | increase --timeout or --settle; with --expect-change/--change, make sure the action really changes pixels in the target |
| `INTERNAL` | 6 | Unexpected failure | run `sight doctor` to check the environment |

