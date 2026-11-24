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
| `--crop x,y,w,h` | | 只保留这个矩形，坐标相对窗口 DWM 边框的左上角，物理像素。仅限 `--pid` / `--hwnd`（与 `--region` 同用，或超出窗口，都返回 `INVALID_ARGS`，`hint` 会给出窗口大小）。先于 `--scale` 和 `--max-edge` 应用 |
| `--scale <F>` | `1.0` | 缩放倍数，范围 (0, 16]。大于 1 时用最近邻插值，并输出 PNG |
| `--max-edge <N>` | `1568` | 缩小到长边不超过 N 像素。**`0` 表示不缩小**（1:1 像素） |
| `--expect-change` | 关闭 | 以第一帧为基准，等画面与它出现差异，之后再套用 `--settle`。适合在外部点击或按键之后立刻使用 |
| `--min-change-px <N>` | 用 `--expect-change` 时为 `16`，否则为 `1` | 差异像素达到多少才算“变化”（>= 1）；`1` 即精确比较。可忽略闪烁光标之类的噪声 |
| `--settle <ms>` | 关闭 | 等画面持续这么久没有变化（受 `--timeout` 限制） |
| `--timeout <ms>` | `5000` | 总超时，“等变化”和“等稳定”两个阶段共用 |
| `--out <path>` | 临时目录 | 扩展名 `.png` / `.jpg` / `.jpeg` 决定格式。默认 `%TEMP%\sightline\shot-<时间戳>.jpg`（`--scale` > 1 时为 PNG） |

JPEG 质量为 80。窗口截图（`--pid` / `--hwnd`）还会返回 `popups`（可能为空）。

警告：`other_candidate_window: ...`、`popups_included_via_screen_capture`、`region_partially_offscreen`、`region_spans_mixed_dpi`、`window_dpi_unaware_or_virtualized`、`client_rect_unavailable`。

输出字段：`path`、`image`、`maps_to`、`scale`、`dpi_scale`、`monitor_dpi_scale`、`formula`；窗口目标另有 `window`、`formula_window`、`formula_client`（见[坐标与 DPI](#坐标与-dpi)）；以及始终存在的 `settled`、`changed`、`change_ms`、`elapsed_ms`。

**感知变化的等待。** 单用 `--settle` 是等画面不再变化，但外部点击刚发生时，UI 可能还没开始变，于是它会把旧画面当作“已稳定”返回。使用 `--expect-change` 时：

1. 第一帧作为基准。
2. 等到某一帧与上一个被接受的帧至少有 `--min-change-px` 个像素不同（第一个这样的帧就是首次变化），再照常套用 `--settle`。
3. 两个阶段共用 `--timeout`。若超时前没有任何变化，仍会保存最新一帧，并以 `TIMEOUT`（退出码 5）失败，`data.changed:false`、`data.settled:false`。

两种模式下，`changed` 表示是否看到与基准不同的帧，`change_ms` 是从命令启动到该帧的时间（没有则为 `null`）。窗口被调整大小也算变化。`--min-change-px` 大于 1 时，小于阈值的差异会被丢弃，因此可能不会出现在保存的图片里。

基准是捕获到的第一帧，它在 `sight` 启动后约 0.3 秒才到达（捕获会话启动耗时）。在那之前就已完成的变化不会被看到：`TIMEOUT` 且 `changed:false` 可能意味着 UI 其实已经更新，这时用普通 `shot` 看一眼即可。对反应是瞬时的 UI，只用 `--settle`，或者在操作之前一刻再启动命令。

### `wait`

目标、`--crop`、`--settle`、`--timeout`、`--min-change-px` 参数与 `shot` 相同，等待逻辑也相同，但**不保存图片**。`--change` 即 `--expect-change`。智能体需要在下一次点击前确认 UI 已就绪时使用它。

```
> sight wait --hwnd 11736012 --change --settle 300
{"v":1,"ok":true,"data":{"changed":true,"change_ms":326,"settled":true,"elapsed_ms":492},"warnings":[]}
> sight wait --hwnd 11736012 --change --timeout 1000
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"no change detected within --timeout 1000ms (no image saved)","hint":"increase --timeout or --settle; with --expect-change/--change, make sure the action really changes pixels in the target"},"data":{"changed":false,"change_ms":null,"settled":false,"elapsed_ms":1020}}
```

输出：`changed`、`change_ms`、`settled`、`elapsed_ms`。用 `--crop` 可以只盯窗口里应该发生变化的那一块。

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

**窗口原点与客户区原点。** `formula` 换算到屏幕。对 `--hwnd` / `--pid` 的截图，还会再给两条公式换算到窗口，因为很多输入工具是相对窗口取坐标的。请选择与你自己的输入工具一致的原点：

| 字段 | 原点 |
|---|---|
| `formula` | 屏幕（虚拟桌面），物理像素 |
| `formula_window` | 窗口 DWM 扩展边框的左上角（可见窗口，含标题栏） |
| `formula_client` | 客户区左上角（标题栏和边框之下） |

`data.window` 包含 `frame {x,y,w,h}`（DWM 扩展边框范围，屏幕像素）、`client {x,y,w,h}`（GetClientRect + ClientToScreen，屏幕像素）和 `client_offset {x,y}`（客户区左上角减去边框左上角）。`--region` 模式下这三个字段和两条公式都不会出现。带标题栏窗口的真实输出：

```
"maps_to":{"x":307,"y":270,"w":300,"h":200},"scale":1.0,
"formula":"screen_x = 307 + px / 1.0; screen_y = 270 + py / 1.0",
"window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},
"formula_window":"win_x = 100 + px / 1.0; win_y = 120 + py / 1.0",
"formula_client":"client_x = 99 + px / 1.0; client_y = 89 + py / 1.0"
```

图片上的点 (50, 40) 对应屏幕 (307+50, 270+40) = (357, 310)、窗口 (150, 160)、客户区 (149, 129)。偏移量是 `maps_to` 减去边框或客户区原点，因此与 `--crop`、`--scale`、`--max-edge` 以及弹出窗口回退一起使用时依然正确。

**`--crop`。** `--crop x,y,w,h` 相对 DWM 边框左上角（即 `formula_window` 的原点），单位为物理像素，所以无论窗口在哪里，选中的都是窗口的同一部分，不必先把窗口移到固定位置。它先于 `--scale` / `--max-edge` 应用：`--crop 100,120,300,200 --max-edge 0` 输出的恰好是 300×200 像素，与从完整 1:1 截图里切出同一矩形的结果逐像素相同。裁剪区超出边框会返回 `INVALID_ARGS`，`hint` 给出窗口大小。弹出窗口与裁剪区重叠时，屏幕截图回退使用的恰好是换算后的裁剪矩形，所以含义仍是窗口的同一部分。

**`dpi_scale` 与 `monitor_dpi_scale`。** `dpi_scale` 是窗口自身的 DPI / 96；`monitor_dpi_scale` 是窗口所在显示器的缩放。不支持 DPI 的老程序在 150% 显示器上会报 `1.0`，因为 Windows 对它做了位图拉伸。两者不一致时 `shot` 会给出 `window_dpi_unaware_or_virtualized` 警告。公式仍然是物理像素，但要确认你的输入工具也用物理像素。`--region` 模式下，两个值都取自区域左上角所在的显示器。

**弹出窗口。** 窗口截图（Windows Graphics Capture）截不到菜单、下拉列表和提示框，因为它们是独立的顶层窗口。如果同一进程有可见的弹出窗口与目标重叠（由目标拥有，或类名为 `#32768`、`tooltips_class32`、`ComboLBox`、`Xaml_WindowedPopupClass`、`Microsoft.UI.Content.PopupWindowSiteBridge`），`shot` 会改为截取覆盖窗口和弹出窗口的屏幕区域，附加 `popups_included_via_screen_capture` 警告，在 `data.popups` 中列出它们（`hwnd`、`class`、`via`），并把 `maps_to` 设为两者的并集，所有公式都已经算进去了（使用 `--crop` 时区域就是裁剪矩形）。此模式下，盖在这片区域上的其他程序也会出现在图里。

## 输出约定

每条命令都只在 stdout 输出一行 JSON。

```json
{"v":1,"ok":true,"data":{},"warnings":[]}
{"v":1,"ok":false,"error":{"code":"TARGET_NOT_FOUND","message":"no visible window found for pid 99999999","hint":"run `sight list` to see available targets"}}
```

先看 `ok`，再看 `warnings`。`hint` 是写给智能体看的，直接指出下一步。失败时也可能带 `data`：`--settle` 或 `--expect-change` 导致的 `TIMEOUT` 仍会保存最后一帧，可以直接用：

```
> sight shot --hwnd 15603054 --settle 300 --timeout 200
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"image did not settle within --timeout 200ms (saved the latest frame anyway)","hint":"increase --timeout or --settle"},"data":{"path":"...\\shot-20261123-214420-128.jpg",...,"settled":false,"elapsed_ms":365,"popups":[]}}
```

| 错误码 | 退出码 | 含义 | `hint` |
|---|---|---|---|
| （成功） | 0 | | |
| `INVALID_ARGS` | 1 | 参数错误或互相冲突（含解析错误）、`--crop` 与 `--region` 同用、`--crop` 超出窗口 | run `sight --help` for usage（`--crop` 出错时 `hint` 给出窗口大小） |
| `TARGET_NOT_FOUND` | 2 | 找不到该 pid / hwnd / 可见窗口 | run `sight list` to see available targets |
| `WINDOW_MINIMIZED` | 3 | 目标窗口已最小化 | restore the window and retry |
| `WINDOW_PROTECTED` | 3 | 窗口禁止被截图，不要重试 | the window opted out of capture (display affinity) |
| `PERMISSION_DENIED` | 4 | 无法读取该进程（比如它以管理员权限运行） | run with sufficient privileges for the target process |
| `TIMEOUT` | 5 | 没拿到画面、画面没能及时稳定，或用 `--expect-change` / `--change` 时没看到变化（`data.changed:false`） | increase --timeout or --settle; with --expect-change/--change, make sure the action really changes pixels in the target |
| `INTERNAL` | 6 | 意外失败（截图、IO、系统信息） | run `sight doctor` to check the environment |

