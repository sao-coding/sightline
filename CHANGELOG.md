## 0.1.0 (2026-11-23)

### Breaking Changes

#### Initial release: observe and measure Windows apps

- `list` finds windows (hwnd, pid, exe, title, rect, minimized) with `--pid`, `--exe`, `--title` and `--all` filters.
- `shot` captures a window (`--pid` / `--hwnd`) or a screen `--region`, with `--settle`, `--scale`, `--max-edge`, `--timeout` and `--out`. Popups are included, and a mapping formula from image pixels to screen coordinates is returned.
- `stat` samples CPU and memory of a process tree over time.
- `doctor` lists monitors and their DPI scale.
- Every command prints a single-line JSON envelope (`v`, `ok`, `data`, `warnings` or `error`) with stable error codes and exit codes.

### Features

- initial project setup
