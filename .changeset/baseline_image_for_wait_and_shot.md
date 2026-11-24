---
default: minor
---

#### `--baseline <image>` for `shot` and `wait`

- Compare against an earlier shot instead of the first captured frame, so a change that finished before `sight` started (an instant UI reaction after a click) is still detected. Implies `--expect-change` / `--change`.
- Every shot now writes a sidecar `<name>.sight.json` (target, window-relative crop, pixel size before scaling, saved size, format). `--baseline` checks it and fails with `INVALID_ARGS` and a hint on a different target, a different `--crop`, a different pixel size or a resized window, instead of comparing mismatched images.
- JPEG (default) and downscaled baselines compare exactly: the live frame goes through the same resize and JPEG encode as `shot`, so no tolerance is needed.
