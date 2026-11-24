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

每个窗口包含：`hwnd`、`pid`、`exe`、`title`、`rect`（屏幕物理像素）、`monitor`（从 1 开始）、`dpi_scale`、`monitor_dpi_scale`、`minimized`。按 Z 序排列，最上层在前。最小化的窗口不含 `rect` 与 `monitor`。标题被截断时多一个 `title_truncated:true`，`--title` 仍按完整标题匹配。超过 `--limit` 时，`data` 多出 `total`、`limit` 与 `truncated:true`，并给出 `list_truncated` 警告，提示用 `--exe`、`--title` 或 `--pid` 缩小范围。

### `shot`

截图并保存为文件。`--pid`、`--hwnd`、`--region` 必须且只能指定一个。

| 参数 | 默认值 | 含义 |
|---|---|---|
| `--pid <PID>` | | 截该进程最大的可见窗口（其余窗口列在 `data.other_candidates` 中，并给出一条 `other_candidate_window` 警告） |
| `--hwnd <HWND>` | | 截这个窗口（十进制，与 `list` 输出一致） |
| `--region x,y,w,h` | | 屏幕区域，物理像素；真正的屏幕截图，盖在上面的东西都会截进去 |
| `--crop x,y,w,h` | | 只保留这个矩形，坐标相对窗口 DWM 边框的左上角，物理像素。仅限 `--pid` / `--hwnd`（与 `--region` 同用，或超出窗口，都返回 `INVALID_ARGS`，`hint` 会给出窗口大小）。先于 `--scale` 和 `--max-edge` 应用 |
| `--baseline <image>` | | 与这张更早的截图（它的 `data.path`）比较，而不是与第一帧比较。隐含 `--expect-change`。见[基准图](#基准图) |
| `--scale <F>` | `1.0` | 缩放倍数，范围 (0, 16]。大于 1 时用最近邻插值，并输出 PNG |
| `--max-edge <N>` | `1568` | 缩小到长边不超过 N 像素。**`0` 表示不缩小**（1:1 像素） |
| `--expect-change` | 关闭 | 以第一帧为基准，等画面与它出现差异，之后再套用 `--settle`。适合在外部点击或按键之后立刻使用 |
| `--min-change-px <N>` | 用 `--expect-change` 时为 `16`，否则为 `1` | 差异像素达到多少才算“变化”（>= 1）；`1` 即精确比较。可忽略闪烁光标之类的噪声 |
| `--settle <ms>` | 关闭 | 等画面持续这么久没有变化（受 `--timeout` 限制） |
| `--timeout <ms>` | `5000` | 总超时，“等变化”和“等稳定”两个阶段共用 |
| `--out <path>` | 临时目录 | 扩展名 `.png` / `.jpg` / `.jpeg` 决定格式。默认 `%TEMP%\sightline\shot-<时间戳>.jpg`（`--scale` > 1 时为 PNG） |

JPEG 质量为 80。窗口截图（`--pid` / `--hwnd`）还会返回 `popups`（可能为空）。

警告都是纯代码，细节放在 `data` 字段里（`other_candidates`、`baseline_size`、`size`）。代码：`other_candidate_window`、`popups_included_via_screen_capture`、`region_partially_offscreen`、`region_spans_mixed_dpi`、`window_dpi_unaware_or_virtualized`、`client_rect_unavailable`。

输出字段：`path`、`image`、`maps_to`、`scale`、`dpi_scale`、`monitor_dpi_scale`、`formula`；窗口目标另有 `window`、`formula_window`、`formula_client`（见[坐标与 DPI](#坐标与-dpi)）；以及始终存在的 `settled`、`changed`、`change_ms`、`elapsed_ms`。

**感知变化的等待。** 单用 `--settle` 是等画面不再变化，但外部点击刚发生时，UI 可能还没开始变，于是它会把旧画面当作“已稳定”返回。使用 `--expect-change` 时：

1. 第一帧作为基准。
2. 等到某一帧与上一个被接受的帧至少有 `--min-change-px` 个像素不同（第一个这样的帧就是首次变化），再照常套用 `--settle`。
3. 两个阶段共用 `--timeout`。若超时前没有任何变化，仍会保存最新一帧，并以 `TIMEOUT`（退出码 5）失败，`data.changed:false`、`data.settled:false`。

两种模式下，`changed` 表示是否看到与基准不同的帧，`change_ms` 是从命令启动到该帧的时间（没有则为 `null`）。窗口被调整大小也算变化。`--min-change-px` 大于 1 时，小于阈值的差异会被丢弃，因此可能不会出现在保存的图片里。

基准是捕获到的第一帧，它在 `sight` 启动后约 0.3 秒才到达（捕获会话启动耗时）。在那之前就已完成的变化不会被看到：`TIMEOUT` 且 `changed:false` 可能意味着 UI 其实已经更新，这时用普通 `shot` 看一眼即可。对反应是瞬时的 UI，只用 `--settle`，或者在操作之前一刻再启动命令。

#### 基准图

`--baseline <image>` 用你之前截的一张图取代“第一帧”，这样即使变化在 `sight` 启动之前就已完成，也能检测到。符合实际的顺序：

```
> sight shot --hwnd 11736012 --out before.png
# 操作：UI 瞬间就变了
> sight wait --hwnd 11736012 --baseline before.png --settle 300
{"v":1,"ok":true,"data":{"changed":true,"change_ms":318,"settled":true,"elapsed_ms":502},"warnings":[]}
> sight wait --hwnd 11736012 --change --timeout 1500      # 同样的顺序，没有基准图
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"no change detected within --timeout 1500ms (no image saved)",...},"data":{"changed":false,"change_ms":null,"settled":false,"elapsed_ms":1534}}
```

捕获到的第一帧会与这张图比较；若至少有 `--min-change-px` 个像素不同，就立刻算作变化（此时 `change_ms` 约等于第一帧到达的时间），然后照常套用 `--settle`。若一致，则像 `--expect-change` 那样继续等待之后的变化。

- **必须可比较。** 每次 `shot` 都会在图片旁写一个旁置文件 `<name>.sight.json`（替换扩展名），记录目标（`window` + `hwnd`，或 `region` + 矩形）、相对窗口的 `crop`、裁剪后缩放前的像素大小（`source`）、保存大小（`image`）与 `format`。`--baseline` 会拿它与新的捕获核对：目标类型或 `--crop` 不同、图片与旁置文件不再一致、或文件不存在时，返回带 `hint` 的 `INVALID_ARGS`。移动窗口没问题，因为裁剪和比较都是相对窗口的。不是 sight 截的图（没有旁置文件）会被当作未缩放的 1:1 图片，并给出警告 `baseline_without_metadata`；它的大小与本次捕获不同时是 `INVALID_ARGS`。
- **窗口大小变了算作变化。** 窗口（或 `--crop` 的结果）的像素大小（裁剪后、缩放前）与基准图不同时，会报告为已变化（`changed:true`，`change_ms` 约等于第一帧到达的时间），并带警告 `baseline_size_changed`（大小在 `data.baseline_size` 和 `data.size` 里，各为 `{w,h}`，例如 `{"w":642,"h":452}` 与 `{"w":742,"h":512}`），之后照常套用 `--settle`。点击经常会让窗口变大或展开，智能体需要的是这个结果，而不是一个错误。若新的大小使 `--crop` 超出窗口，仍照常返回 `INVALID_ARGS`，`hint` 给出窗口大小。
- **JPEG 和缩放过的基准图不需要容差。** `shot` 默认存 JPEG（质量 80），默认的 `--max-edge` 也会缩小大窗口。这里不做模糊匹配，而是把实时帧先经过与 `shot` 相同的缩放、相同的 JPEG 编码/解码再比较，所以相同的像素必然完全相等。`--max-edge 0` 的 `.png` 仍然最省（不用逐帧转换）。唯一的例外：缩放过或 JPEG 的基准图不能用于跨多台显示器或超出屏幕边缘的 `--region`（或弹出窗口的屏幕截图回退），那会返回 `INVALID_ARGS`，请改用 `--max-edge 0` 的 `.png`。
- 其他警告：`baseline_size_changed`、`baseline_metadata_unreadable`、`baseline_metadata_not_written`（旁置文件写不出来）。

### `wait`

目标、`--crop`、`--settle`、`--timeout`、`--min-change-px`、`--baseline` 参数与 `shot` 相同，等待逻辑也相同，但**不保存图片**。`--change` 即 `--expect-change`。智能体需要在下一次点击前确认 UI 已就绪时使用它。

```
> sight wait --hwnd 11736012 --change --settle 300
{"v":1,"ok":true,"data":{"changed":true,"change_ms":326,"settled":true,"elapsed_ms":492},"warnings":[]}
> sight wait --hwnd 11736012 --change --timeout 1000
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"no change detected within --timeout 1000ms (no image saved)","hint":"increase --timeout or --settle; with --expect-change/--change, make sure the action really changes pixels in the target"},"data":{"changed":false,"change_ms":null,"settled":false,"elapsed_ms":1020}}
```

输出：`changed`、`change_ms`、`settled`、`elapsed_ms`。用 `--crop` 可以只盯窗口里应该发生变化的那一块。

### `rec`

把窗口或屏幕区域录成 GIF 动图，用于 README 演示和测试证据。与 `shot` 一样，`--pid`、`--hwnd`、`--region` 必须且只能指定一个。

| 参数 | 默认值 | 含义 |
|---|---|---|
| `--duration <ms>` | 必填 | 录制时长，从捕获到第一帧时开始算 |
| `--source auto\|screen\|window` | `auto` | 捕获来源，开始时选定一次。仅限 `--pid` / `--hwnd`（`--region` 永远是屏幕捕获） |
| `--pad <N>` | `0` | 屏幕来源：在窗口四周多录 N 像素（限制在桌面范围内）。仅限 `--pid` / `--hwnd` |
| `--cursor` | 关闭 | 录入鼠标指针（捕获 API 允许隐藏时，默认隐藏） |
| `--fps <N>` | `10` | 采样帧率，1 到 30 |
| `--max-edge <N>` | `800` | 缩小到长边不超过 N 像素（`0` 表示原尺寸） |
| `--until-settled <ms>` | 关闭 | 图像发生变化后又连续这么久没变，就提前结束（不会超过 `--duration`） |
| `--keyframes <N>` | `0` | 另外保存 N 张变化最大时刻的 PNG 静帧 |
| `--crop x,y,w,h` | | 与 `shot` 相同的相对窗口边框的裁剪（仅限 `--pid` / `--hwnd`）。屏幕来源下可以伸进 `--pad` 区域，但不能超出边框加 pad |
| `--out <path>` | 临时目录 | GIF 路径。默认 `%TEMP%\sightline\rec-<时间戳>.gif`；静帧存为旁边的 `<name>-key1.png` 等 |

`rec` **会阻塞**到录制结束，所以智能体应当在操作的同时让它在后台运行（见下）。它使用与 `shot` 相同的 Windows Graphics Capture 会话。

#### 捕获来源

`rec` 在启动时选定**一个**来源，录制过程中绝不切换：

- **`screen`**：对窗口边框矩形加上 `--pad` 做显示器捕获，并限制在虚拟桌面内。录到的是用户看到的画面：菜单、提示框、弹出窗口都在，盖在窗口上的东西也会录进去。每个显示器启动一个捕获会话（并行启动），每一帧都按窗口当前位置从显示器画面中裁出来。
- **`window`**：对窗口本身做窗口捕获。盖在上面的窗口不影响它，但独立的弹出窗口（菜单、下拉列表、提示框）不会被录到。这里 `--pad` 不起作用（警告 `rec_pad_ignored`）。
- **`auto`**（默认）：开始时检查：是否有任何其他可见、未被 cloak 的顶层窗口位于目标之上（Z 序），并与目标的 DWM 边框（加 `--pad`）相交。不计入：目标自己拥有的窗口和弹出窗口、同进程且属于已知弹出类的窗口（菜单、提示框、下拉列表）、不可见/最小化/被 cloak 的窗口，以及固定 alpha 为 0 的分层窗口（它们什么也不画）。工具窗口和穿透点击的覆盖层会计入，因为它们看得见。没被盖住：`screen`，`source_reason:"auto_not_occluded"`。被盖住：`window`，`source_reason:"auto_occluded"`，并给出警告 `rec_window_source_no_popups`。

`--source screen|window` 强制指定来源（`source_reason:"forced"`）。`--region` 永远是屏幕捕获。

录制过程中：

- **移动。** 屏幕来源下，捕获矩形每一帧都跟着窗口走；画布保持开始时的大小，只有原点在移动，所以窗口在 GIF 里待在原位。输出里的公式对开始时的位置有效，窗口移动过则多一个 `moved:true`。
- **调整大小。** 保持开始时的画布大小（之后的帧会裁剪或补边），并只给一次警告 `rec_window_resized`。
- **遮挡变化。** 别的窗口盖住或露出目标，绝不会改变来源。每次变化都记录在 `occlusion_events:[{t_ms, occluded}]`（每 250 毫秒检查一次；`t_ms` 从 GIF 开头算起），并给出警告 `rec_occlusion_changed`。

目标被盖住*而且*会弹出菜单时用哪个来源：真正的菜单（置顶的工具窗口，比如 WinForms 的右键菜单）画在遮挡窗口之上，所以 `screen` 录像里看得到它们，重叠的地方也会看到遮挡窗口。普通的被拥有的弹出窗口则紧贴在其所有者之上、位于遮挡窗口之下，所以在屏幕录像里被遮住，在 `window` 录像里不存在。弹出窗口很重要时，请先把窗口露出来。

#### 输出

- **帧。** 按 `--fps` 采样；只有像素变化时才写入新帧，没变化的时段只是延长上一帧的延迟。静止的窗口只录出一帧，延迟很长。延迟以厘秒（GIF 的单位）为单位，取自连续的时间线；机器繁忙时实际帧数可能少于 `--fps`。
- **计时。** 从捕获到第一帧时开始计时（捕获启动约需 0.3 秒，屏幕来源会并行启动每个显示器的会话），所以 `elapsed_ms` 比 `duration_ms` 大约多 0.7 秒。
- **`--until-settled`** 在第一帧之后的第一次变化时才开始计时。录制开始时窗口是静止的，不会导致它结束；如果始终没有变化，就录满 `--duration`。
- **字段。** `path`、`frames`、`fps`、`duration_ms`（GIF 自身的长度）、`image`、`size_bytes`、`source`、`source_reason`、`stopped`（`duration` 或 `settled`）、`elapsed_ms`，以及与 `shot` 相同的 `maps_to`、`scale`、`dpi_scale`、`monitor_dpi_scale`、`formula`，窗口还有 `window`、`formula_window`、`formula_client`，另有 `occlusion_events`（窗口目标）和 `moved`（仅为 true 时出现）。公式把 GIF 像素（和静帧像素）换算成录制开始时的坐标；使用 `--pad` 时窗口偏移和客户区偏移是负数，因为画布从窗口外面开始。指定 `--keyframes N` 时，`keyframes` 列出 `{t_ms, path}`：第一帧、最后一帧，以及中间变化最大的帧（共 N 张，不同的帧不够时更少），按时间排序；`t_ms` 从 GIF 开头算起。
- **警告。** `gif_large`（超过约 10 MB）、`rec_window_source_no_popups`、`rec_window_resized`、`rec_occlusion_changed`、`rec_pad_ignored`。

```
> sight rec --hwnd 11736012 --pad 20 --duration 3000 --keyframes 2 --out demo.gif
{"v":1,"ok":true,"data":{"path":"...\\demo.gif","frames":2,"fps":10,"duration_ms":3000,"image":{"w":682,"h":492},"size_bytes":63545,"maps_to":{"space":"screen_physical","x":187,"y":130,"w":682,"h":492},"scale":1.0,"dpi_scale":1.0,"monitor_dpi_scale":1.0,"formula":"screen_x = 187 + px / 1.0; screen_y = 130 + py / 1.0","window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},"formula_window":"win_x = -20 + px / 1.0; win_y = -20 + py / 1.0","formula_client":"client_x = -21 + px / 1.0; client_y = -51 + py / 1.0","source":"screen","source_reason":"auto_not_occluded","occlusion_events":[],"stopped":"duration","elapsed_ms":3378,"keyframes":[{"t_ms":1,"path":"...\\demo-key1.png"},{"t_ms":800,"path":"...\\demo-key2.png"}]},"warnings":[]}
```

在操作的同时放到后台运行（启动约需 0.3 秒，各显示器的会话并行启动；第一次操作前先等约 0.7 秒）：

```powershell
$job = Start-Job { sight rec --hwnd 123456 --duration 10000 --until-settled 500 --keyframes 3 --out C:\tmp\demo.gif }
Start-Sleep -Milliseconds 700      # capture needs ~0.3 s per monitor session to start
# ... act with your own tool (click, type) ...
$rec = Receive-Job $job -Wait -AutoRemoveJob | ConvertFrom-Json   # data.path, data.keyframes[].path
```

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
- 警告（纯代码）：`target_exited`；`processes_exited_during_sampling`（它们在最后一次采样之后用掉的 CPU 不计入；`processes` 里每一项都有自己的 `exited` 标志）；`access_denied_for_some_processes`（拿不到它们的 CPU 和内存数据；`data.access_denied` 列出 `{pid, exe}`）。

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
| `INVALID_ARGS` | 1 | 参数错误或互相冲突（含解析错误）、`--crop` 与 `--region` 同用、`--crop` 超出窗口、`--baseline` 读不出、目标或 `--crop` 不同，或是大小不同的外来图片 | run `sight --help` for usage（`--crop` 出错时 `hint` 给出窗口大小） |
| `TARGET_NOT_FOUND` | 2 | 找不到该 pid / hwnd / 可见窗口 | run `sight list` to see available targets |
| `WINDOW_MINIMIZED` | 3 | 目标窗口已最小化 | restore the window and retry |
| `WINDOW_PROTECTED` | 3 | 窗口禁止被截图，不要重试 | the window opted out of capture (display affinity) |
| `PERMISSION_DENIED` | 4 | 无法读取该进程（比如它以管理员权限运行） | run with sufficient privileges for the target process |
| `TIMEOUT` | 5 | 没拿到画面、画面没能及时稳定，或用 `--expect-change` / `--change` 时没看到变化（`data.changed:false`） | increase --timeout or --settle; with --expect-change/--change, make sure the action really changes pixels in the target |
| `INTERNAL` | 6 | 意外失败（截图、IO、系统信息） | run `sight doctor` to check the environment |

