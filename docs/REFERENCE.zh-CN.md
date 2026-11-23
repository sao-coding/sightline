# sightline 参考文档

回到 [README](../README.zh-CN.md)。

## 命令参考

全局参数：`--pretty`（缩进的 JSON，给人看），所有命令都可用。

### `list`

列出顶层窗口。默认只列出可见且有标题的窗口，桌面、cloaked 窗口和小于 10 px 的窗口会被隐藏。

| 参数 | 含义 |
|---|---|
| `--pid <PID>` | 只看该进程的窗口 |
| `--exe <TEXT>` | exe 名包含该文本（不区分大小写） |
| `--title <TEXT>` | 标题包含该文本（不区分大小写） |
| `--all` | 关闭所有默认过滤（不可见、cloaked、无标题、桌面 `Progman`/`WorkerW`、小于 10 px 的窗口） |
| `--full-title` | 不截断标题（默认前 40 个字符 + `…`） |
| `--limit <N>` | 最多返回的窗口数（默认 50，`--all` 也适用） |

每个窗口包含：`hwnd`、`pid`、`exe`、`title`、`rect`（屏幕物理像素）、`monitor`（从 1 开始）、`dpi_scale`、`monitor_dpi_scale`、`minimized`。按 Z 序排列，最上层在前。最小化的窗口不含 `rect` 与 `monitor`。标题被截断时多一个 `title_truncated:true`，`--title` 仍按完整标题匹配。超过 `--limit` 时，`data` 多出 `total` 与 `truncated:true`，并给出 `list_truncated` 警告，提示用 `--exe`、`--title` 或 `--pid` 缩小范围。

### `shot`

截图并保存为文件。`--pid`、`--hwnd`、`--region` 必须且只能指定一个。

| 参数 | 默认值 | 含义 |
|---|---|---|
| `--pid <PID>` | | 截该进程最大的可见窗口（其余窗口以 `other_candidate_window` 警告列出） |
| `--hwnd <HWND>` | | 截这个窗口（十进制，与 `list` 输出一致） |
| `--region x,y,w,h` | | 屏幕区域，物理像素；真正的屏幕截图，盖在上面的东西都会截进去 |
| `--scale <F>` | `1.0` | 缩放倍数，范围 (0, 16]。大于 1 时用最近邻插值，并输出 PNG |
| `--max-edge <N>` | `1568` | 缩小到长边不超过 N 像素（`0` 表示不限制） |
| `--settle <ms>` | 关闭 | 等画面持续这么久没有变化（受 `--timeout` 限制） |
| `--timeout <ms>` | `5000` | 截图总超时 |
| `--out <path>` | 临时目录 | 扩展名 `.png` / `.jpg` / `.jpeg` 决定格式。默认 `%TEMP%\sightline\shot-<时间戳>.jpg`（`--scale` > 1 时为 PNG） |

JPEG 质量为 80。窗口截图（`--pid` / `--hwnd`）还会返回 `popups`（可能为空）。

警告：`other_candidate_window: ...`、`popups_included_via_screen_capture`、`region_partially_offscreen`、`region_spans_mixed_dpi`、`window_dpi_unaware_or_virtualized`。

### `stat`

采样某个进程（默认连同全部子孙进程）的 CPU 和内存。

| 参数 | 默认值 | 含义 |
|---|---|---|
| `--pid <PID>` | 必填 | 根进程 ID |
| `--duration <ms>` | 必填 | 采样时长（> 0） |
| `--interval <ms>` | `500` | 采样间隔（> 0） |
| `--no-tree` | 关闭 | 只统计根进程 |

- `cpu_core_pct`：100 表示占满一个逻辑核。`cpu_total_pct` = `cpu_core_pct` / `logical_cores`。
- `mem_mb`：私有字节，单位 MiB，整棵树求和。`processes` 列出所有被采样的进程。
- 为什么要看整棵树：在一台机器上对 Edge 浏览器的 PID 跑 `stat`，结果是 46 个进程、`mem_mb` 6293 MiB；加上 `--no-tree` 只有 561 MiB。
- 警告：`target_exited`、`processes_exited_during_sampling: ...`、`access_denied_for_some_processes: ...`。

### `doctor`

列出显示器：`index`、`rect`（虚拟桌面物理像素）、`dpi_scale`、`primary`。

```
> sight doctor
{"v":1,"ok":true,"data":{"monitors":[{"index":2,"rect":{"x":0,"y":0,"w":2560,"h":1440},"dpi_scale":1.5,"primary":true},{"index":1,"rect":{"x":2560,"y":0,"w":2560,"h":1440},"dpi_scale":1.0,"primary":false}]},"warnings":[]}
```

## 坐标与 DPI

sightline 报告的所有坐标都是虚拟桌面上的**屏幕物理像素**。它以 Per-Monitor V2 DPI 感知模式运行，不会混进被缩放过的“逻辑坐标”。多显示器时坐标可能是负数。

保存的图片通常比窗口小。`shot` 会给出缩放比例（`scale`，每个屏幕像素对应多少图片像素）和现成的换算公式，智能体不用自己算。

**算例。** 一个 150% DPI 下的文件资源管理器窗口，物理尺寸 1691×953，用默认的 `--max-edge 1568` 截图：

```
> sight shot --hwnd 3870832 --settle 300
{"v":1,"ok":true,"data":{"path":"...\\shot-20261123-214403-422.jpg","image":{"w":1568,"h":884},"maps_to":{"space":"screen_physical","x":798,"y":278,"w":1691,"h":953},"scale":0.9273,"dpi_scale":1.5,"monitor_dpi_scale":1.5,"formula":"screen_x = 798 + px / 0.9273; screen_y = 278 + py / 0.9273","settled":true,"elapsed_ms":726,"popups":[]},"warnings":[]}
```

模型在图片的 (400, 200) 处看到一个按钮，那么：

```
screen_x = 798 + 400 / 0.9273 ≈ 798 + 431.4 = 1229
screen_y = 278 + 200 / 0.9273 ≈ 278 + 215.7 = 494
```

就在屏幕物理坐标 (1229, 494) 点击。`maps_to` 是图片覆盖的屏幕矩形。窗口移动或改变大小后要重新 `shot`，`formula` 只对当次的图片有效。

**`dpi_scale` 与 `monitor_dpi_scale`。** `dpi_scale` 是窗口自身的 DPI / 96；`monitor_dpi_scale` 是窗口所在显示器的缩放。不支持 DPI 的老程序在 150% 显示器上会报 `1.0`，因为 Windows 对它做了位图拉伸。两者不一致时 `shot` 会给出 `window_dpi_unaware_or_virtualized` 警告。公式仍然是物理像素，但要确认你的输入工具也用物理像素。`--region` 模式下，两个值都取自区域左上角所在的显示器。

**弹出窗口。** 窗口截图（Windows Graphics Capture）截不到菜单、下拉列表和提示框，因为它们是独立的顶层窗口。如果同一进程有可见的弹出窗口与目标重叠（由目标拥有，或类名为 `#32768`、`tooltips_class32`、`ComboLBox`、`Xaml_WindowedPopupClass`、`Microsoft.UI.Content.PopupWindowSiteBridge`），`shot` 会改为截取覆盖窗口和弹出窗口的屏幕区域，附加 `popups_included_via_screen_capture` 警告，在 `data.popups` 中列出它们（`hwnd`、`class`、`via`），并把 `maps_to` 设为两者的并集，`formula` 已经算进去了。此模式下，盖在这片区域上的其他程序也会出现在图里。

## 输出约定

每条命令都只在 stdout 输出一行 JSON。

```json
{"v":1,"ok":true,"data":{},"warnings":[]}
{"v":1,"ok":false,"error":{"code":"TARGET_NOT_FOUND","message":"no visible window found for pid 99999999","hint":"run `sight list` to see available targets"}}
```

先看 `ok`，再看 `warnings`。`hint` 是写给智能体看的，直接指出下一步。失败时也可能带 `data`：`--settle` 导致的 `TIMEOUT` 仍会保存最后一帧，可以直接用：

```
> sight shot --hwnd 15603054 --settle 300 --timeout 200
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"image did not settle within --timeout 200ms (saved the latest frame anyway)","hint":"increase --timeout or --settle"},"data":{"path":"...\\shot-20261123-214420-128.jpg",...,"settled":false,"elapsed_ms":365,"popups":[]}}
```

| 错误码 | 退出码 | 含义 | `hint` |
|---|---|---|---|
| （成功） | 0 | | |
| `INVALID_ARGS` | 1 | 参数错误或互相冲突（含解析错误） | run `sight --help` for usage |
| `TARGET_NOT_FOUND` | 2 | 找不到该 pid / hwnd / 可见窗口 | run `sight list` to see available targets |
| `WINDOW_MINIMIZED` | 3 | 目标窗口已最小化 | restore the window and retry |
| `WINDOW_PROTECTED` | 3 | 窗口禁止被截图，不要重试 | the window opted out of capture (display affinity) |
| `PERMISSION_DENIED` | 4 | 无法读取该进程（比如它以管理员权限运行） | run with sufficient privileges for the target process |
| `TIMEOUT` | 5 | 没拿到画面，或画面没能及时稳定 | increase --timeout or --settle |
| `INTERNAL` | 6 | 意外失败（截图、IO、系统信息） | run `sight doctor` to check the environment |

