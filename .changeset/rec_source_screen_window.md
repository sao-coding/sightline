---
default: minor
---

#### `rec` picks one capture source at the start: `--source auto|screen|window`

- `--source auto` (default) records the window's screen rect (monitor capture, so menus, tooltips and popups are included) unless another visible window above the target overlaps its frame; then it uses window capture and warns `rec_window_source_no_popups`. `screen` and `window` force a source. The source is never switched mid-recording. Output has `source` and `source_reason` (`auto_not_occluded`, `auto_occluded`, `forced`).
- `--pad N` records N px around the window in a screen recording (for dropdowns that stick out), `--cursor` includes the mouse cursor.
- The screen capture follows a window that moves (fixed canvas size, `moved:true`, formulas valid for the start position) and keeps the start size when it resizes (`rec_window_resized`). Windows covering or uncovering the target mid-recording are listed in `occlusion_events` (`rec_occlusion_changed`).
- Replaces the old behaviour that detected popups once at the start. `window_resized_during_recording` is now `rec_window_resized`.
