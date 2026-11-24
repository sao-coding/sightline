---
default: minor
---

#### `sight rec`: record a window or region to an animated GIF

- `sight rec (--hwnd N | --pid N | --region x,y,w,h) [--crop ...] --duration ms [--fps N] [--max-edge N] [--until-settled ms] [--keyframes N] [--out path.gif]` blocks until the duration has passed (or, with `--until-settled`, until the image changed and then stayed unchanged), so agents run it in the background while they act.
- Uses the same capture sessions as `shot` (popup screen-capture fallback, multi-monitor regions) and the `gif` crate. Identical frames are merged by extending the previous frame's delay; `gif_large` warns above about 10 MB.
- Output has `path`, `frames`, `fps`, `duration_ms`, `image`, `size_bytes` and the same `maps_to`, `formula`, `formula_window` and `formula_client` as `shot`. `--keyframes N` also saves N PNG stills of the most changed moments and lists them with `t_ms`.
