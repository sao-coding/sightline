---
default: minor
---

#### Change-aware waiting, window-relative crop and window coordinate formulas

- `shot --expect-change` waits for pixels to differ from the first frame before `--settle`, so a click that has not started changing the UI yet no longer returns the old frame. `--min-change-px` (default 16) ignores noise such as a blinking caret. Output always has `changed` and `change_ms`; no change before `--timeout` is a `TIMEOUT` with `changed:false`.
- New `wait` command with the same waiting logic and no image: `sight wait --hwnd N --change --settle 300` prints `changed`, `change_ms`, `settled`, `elapsed_ms`.
- `--crop x,y,w,h` (on `shot` and `wait`) is relative to the window's DWM frame top-left, so it works wherever the window is. It is applied before `--scale` and `--max-edge`, and also works with the popup screen-capture fallback.
- Window shots add `window` (`frame`, `client`, `client_offset`), `formula_window` and `formula_client` next to the screen `formula`.
- Documented that `--max-edge 0` disables downscaling.
