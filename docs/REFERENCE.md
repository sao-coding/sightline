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
| `--scale <F>` | `1.0` | Resize factor in (0, 16]. Above 1 uses nearest neighbour and writes PNG |
| `--max-edge <N>` | `1568` | Downscale so the long edge is at most N px (`0` = no limit) |
| `--settle <ms>` | off | Wait until the image has not changed for this long |
| `--timeout <ms>` | `5000` | Overall capture timeout |
| `--out <path>` | temp dir | `.png` / `.jpg` / `.jpeg` picks the format. Default `%TEMP%\sightline\shot-<timestamp>.jpg` |

JPEG quality is 80. Window captures also return `popups` (possibly empty). Warnings:
`other_candidate_window`, `popups_included_via_screen_capture`, `region_partially_offscreen`,
`region_spans_mixed_dpi`, `window_dpi_unaware_or_virtualized`.

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

**`dpi_scale` vs `monitor_dpi_scale`.** The first is the window's own DPI / 96, the second is its
monitor's scale. A DPI-unaware app reports `1.0` on a 150% monitor because Windows stretches its
bitmap. When they differ, `shot` warns `window_dpi_unaware_or_virtualized`: the formula is still
in physical px, but check that your input tool uses physical px too.

**Popups.** Window capture does not include menus, dropdowns or tooltips, because they are
separate top-level windows. If visible popups of the same process overlap the target (owned by
it, or class `#32768`, `tooltips_class32`, `ComboLBox`, `Xaml_WindowedPopupClass`,
`Microsoft.UI.Content.PopupWindowSiteBridge`), `shot` captures the screen area covering the
window plus its popups instead. It warns `popups_included_via_screen_capture`, lists them in
`data.popups`, and `formula` already accounts for the larger area. In this mode other apps drawn
over that area also appear in the image.

## Output contract

Every command prints exactly one JSON line on stdout:

```json
{"v":1,"ok":true,"data":{},"warnings":[]}
{"v":1,"ok":false,"error":{"code":"TARGET_NOT_FOUND","message":"no visible window found for pid 99999999","hint":"run `sight list` to see available targets"}}
```

Check `ok`, then read `warnings`. `hint` names the next step. A failure can still carry `data`:
on a `--settle` `TIMEOUT` the latest frame was saved anyway and is usable.

| Code | Exit | Meaning | `hint` |
|---|---|---|---|
| (success) | 0 | | |
| `INVALID_ARGS` | 1 | Bad or conflicting flags | run `sight --help` for usage |
| `TARGET_NOT_FOUND` | 2 | No such pid / hwnd / visible window | run `sight list` to see available targets |
| `WINDOW_MINIMIZED` | 3 | Target is minimized | restore the window and retry |
| `WINDOW_PROTECTED` | 3 | Window opted out of capture; do not retry | the window opted out of capture (display affinity) |
| `PERMISSION_DENIED` | 4 | Cannot read the process (for example, elevated) | run with sufficient privileges for the target process |
| `TIMEOUT` | 5 | No frame, or the image did not settle in time | increase --timeout or --settle |
| `INTERNAL` | 6 | Unexpected failure | run `sight doctor` to check the environment |

