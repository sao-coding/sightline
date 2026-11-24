---
default: patch
---

#### `--baseline`: a different window size now counts as a change

- When the target window (or the `--crop` result) has another pixel size than the baseline, `shot`/`wait --baseline` now return `changed:true` with the warning `baseline_size_changed: {"baseline":{"w":..,"h":..},"now":{"w":..,"h":..}}` and then settle as usual, instead of `INVALID_ARGS`. Clicks often resize or expand a window.
- Still `INVALID_ARGS`: a different target kind (window vs region), a different `--crop`, a missing file, a foreign image with no sidecar and a different size, and a `--crop` that falls outside the window's new size.
