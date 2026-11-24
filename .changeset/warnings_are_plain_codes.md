---
default: patch
---

#### Warnings are plain code strings; details moved to `data`

- `baseline_size_changed` no longer embeds JSON. The sizes are `data.baseline_size {w,h}` and `data.size {w,h}`.
- `other_candidate_window` is now one warning per command, and the other windows are listed in `data.other_candidates [{hwnd,title,rect}]`.
- `list_truncated` no longer carries "showing N of M" text (`count`, `total` and the new `limit` are in `data`).
- `processes_exited_during_sampling` and `access_denied_for_some_processes` lose their explanatory text; the denied processes are in `data.access_denied [{pid,exe}]`, and each `processes` entry already has its `exited` flag.
