<p align="center">
  <a href="../../releases"><img alt="Release" src="https://img.shields.io/github/v/release/sao-coding/sightline?include_prereleases"></a>
  <a href="../../actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/sao-coding/sightline/ci.yml?branch=main&label=CI"></a>
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4">
  <a href="LICENSE"><img alt="License: AGPL-3.0" src="https://img.shields.io/badge/license-AGPL--3.0-blue"></a>
</p>

<p align="center"><a href="README.md">English</a> · <b>简体中文</b> · <a href="README.zh-TW.md">繁體中文</a></p>

# sightline

让 Windows 上的 AI 编程智能体有眼睛、也有秒表。单个小型可执行文件，命令是 `sight`。

sightline 能找出窗口、被挡住也照样截图、等画面先变化再稳定才出图，并告诉智能体图像像素要怎么换算成屏幕、窗口和客户区坐标；也能测量整棵进程树的 CPU 与内存。它**只看、只量**：从不点击、不输入、不移动鼠标。操作交给智能体自己的工具，sightline 负责确认结果。

[为什么](#为什么需要它) • [安装](#安装) • [快速开始](#快速开始) • [配合智能体](#配合-ai-智能体使用) • [参考文档](docs/REFERENCE.zh-CN.md) • [常见问题](#常见问题)

```
sight list --exe notepad                  # 哪个窗口？
sight shot --hwnd 15603054 --settle 300   # 等画面稳定后长什么样？
sight wait --hwnd 15603054 --change --settle 300   # 我点完之后，UI 变了并稳定了吗？
sight stat --pid 19668 --duration 2000    # 用了多少 CPU 和内存？
```

## 为什么需要它

让 Windows 上的智能体“看一下对话框显示得对不对”，它多半会临时写一段 PowerShell：`Start-Sleep 3`、`CopyFromScreen`、写死的 PID。sightline 就是用来取代这些。

| 现在的脚本 | 问题 | sightline 的做法 |
|---|---|---|
| `Start-Sleep 3` | 等短了不稳定，等长了浪费时间 | `--settle 300` 等到像素不再变化；有超时，没稳定就如实返回 `settled:false` |
| 点击后 `Start-Sleep 0.5` | UI 可能还没开始变，单用 settle 会返回旧画面 | `--expect-change` 先等像素与基准画面出现差异，再等稳定 |
| `CopyFromScreen` | 叠在上面的其他窗口也被截进去 | `shot --hwnd` 直接截目标窗口，被遮挡也没关系 |
| 窗口截图 API | 右键菜单是独立的顶层窗口，会被漏掉 | 自动识别并带上该程序自己的弹出菜单 |
| 模型自动缩放图片 | 图上的位置对不上屏幕 | 每张图都附带从图片像素到屏幕像素的 `formula` |
| 手工 `Crop`、先把窗口移到固定位置 | 窗口在别处就失效 | `--crop x,y,w,h` 相对窗口、与窗口位置无关；`formula_window`、`formula_client` 把像素换算到窗口和客户区坐标 |
| DPI 缩放 | 150% 缩放下手算坐标全错 | 以 Per-Monitor DPI 感知运行，并给出 `dpi_scale` 和 `monitor_dpi_scale` |
| `Get-Process` | 只看一个进程，PID 还是写死的 | `list` 找 PID；`stat` 统计整棵进程树，给出 avg/p95/max |
| 自由格式的文本输出 | 智能体只能读字猜哪里出错 | 一行 JSON、固定的错误码，`hint` 直接说下一步该做什么 |

图像写成文件，只返回**路径**，不返回 base64，所以一张截图只花一次读文件，而不是几千个 token。

## 命令

| 命令 | 回答的问题 | 关键好处 |
|---|---|---|
| `list` | 有哪些窗口？该对准哪个 PID / HWND？ | 不用写死 PID 或标题；给出位置、所在显示器、DPI、是否最小化 |
| `shot` | 这个窗口现在长什么样？ | 被遮挡也能截，带上弹出菜单，等变化再等稳定，相对窗口的 `--crop`，多种坐标公式 |
| `wait` | 我操作之后，UI 变了并稳定了吗？ | 与 `shot` 相同的等待逻辑，但不出图；返回 `changed`、`change_ms`、`settled` |
| `rec` | 把发生的事录成动图给我看 | 窗口、裁剪区或屏幕区域录成 GIF；合并相同的帧；支持 `--until-settled`、`--keyframes` 静帧 |
| `stat` | 这个程序一段时间内占多少 CPU 和内存？ | 统计整棵进程树（Electron、浏览器、游戏），avg/p95/max，按私有字节计，共享页不会重复计算 |
| `doctor` | 这台机器有几台显示器、各自缩放多少？ | 结果不对劲时先跑它 |

截图写入文件，只返回**路径**，不返回 base64。智能体读一次文件就行，不用为一张图耗掉几千个 token。`--max-edge`（默认 1568）把图片控制在视觉模型不会再缩放的尺寸。

## 安装

**PowerShell 一行安装**（安装到 `%LOCALAPPDATA%\Programs\sight`，并加入用户 `PATH`）：

```powershell
irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex
```

装完请**打开新的终端**，让 `PATH` 生效，再运行 `sight doctor` 确认。指定版本、修改安装目录或卸载：

```powershell
$env:SIGHT_VERSION = "0.1.0"; irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex
$env:SIGHT_INSTALL_DIR = 'D:\tools\sight'; irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex
$env:SIGHT_UNINSTALL = 1; irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex
```

脚本会用每个版本附带的 `sight.exe.sha256` 校验下载内容。

**手动安装。** 从 [Releases](../../releases) 下载 `sight.exe`（`sight-windows-x64.zip` 另含文档和 skill），放进某个文件夹，再把该文件夹加入 `PATH`：

```powershell
# 示例：C:\tools\sight（用户 PATH，不需要管理员）
[Environment]::SetEnvironmentVariable('Path', "$([Environment]::GetEnvironmentVariable('Path','User'));C:\tools\sight", 'User')
```

或用“设置 → 系统 → 关于 → 高级系统设置 → 环境变量 → `Path` → 新建”。完成后请重新打开终端（以及智能体）。智能体必须能在它的 `PATH` 中找到 `sight`。

**从源码构建**（Windows 10/11，需要 Rust 工具链）：

```powershell
cargo build --release     # target\release\sight.exe
```

## 快速开始

```
sight doctor                              # 显示器与 DPI
sight list --exe notepad                  # 找出目标窗口
sight shot --hwnd 15603054 --settle 300   # 等画面稳定后截图
```

智能体循环中的一轮：

1. **找到目标。**

   ```
   > sight list --exe notepad
   {"v":1,"ok":true,"data":{"count":1,"windows":[{"hwnd":15603054,"pid":19668,"exe":"Notepad.exe","title":"未命名 - 記事本","rect":{"x":3010,"y":362,"w":1178,"h":637},"monitor":1,"dpi_scale":1.0,"monitor_dpi_scale":1.0,"minimized":false}]},"warnings":[]}
   ```

2. **看。** 用智能体的看图工具打开 `data.path`。

   ```
   > sight shot --hwnd 15603054 --settle 300
   {"v":1,"ok":true,"data":{"path":"C:\\Users\\me\\AppData\\Local\\Temp\\sightline\\shot-20261123-214402-405.jpg","image":{"w":1178,"h":637},"maps_to":{"space":"screen_physical","x":3010,"y":362,"w":1178,"h":637},"scale":1.0,"dpi_scale":1.0,"monitor_dpi_scale":1.0,"formula":"screen_x = 3010 + px / 1.0; screen_y = 362 + py / 1.0","settled":true,"elapsed_ms":945,"popups":[]},"warnings":[]}
   ```

3. **操作。** 用智能体自己的输入工具，按下面的公式算出坐标后点击或输入。
4. **等 UI 就绪**，再去看。外部点击刚发生时，UI 可能还没开始变，单用 `--settle` 会返回旧画面。`--expect-change`（或 `wait --change`）先等像素与基准画面出现差异，再等稳定，两个阶段共用 `--timeout`：

   ```
   > sight wait --hwnd 15603054 --change --settle 300
   {"v":1,"ok":true,"data":{"changed":true,"change_ms":326,"settled":true,"elapsed_ms":492},"warnings":[]}
   > sight shot --hwnd 15603054 --expect-change --settle 300     # 同样的等待，之后保存图片
   ```

   若超时前没有任何变化，`shot` 仍会保存最新一帧，两个命令都以 `TIMEOUT`（退出码 5）失败，并带 `data.changed:false`。闪烁光标之类的微小噪声不算变化：需要至少 `--min-change-px` 个像素不同（默认 16）。基准是捕获到的第一帧，它在 `sight` 启动后约 0.3 秒才到达，所以在那之前已完成的变化不会被看到（`TIMEOUT` 且 `changed:false`：用普通 `shot` 看一眼）。

   **瞬时反应：`--baseline`。** 在操作*之前*先截一张图，再拿它当基准来等，这样即使变化在 `sight` 启动之前就已完成，也能检测到：

   ```
   > sight shot --hwnd 15603054 --out before.png
   # 用你自己的工具操作；UI 瞬间就变了
   > sight wait --hwnd 15603054 --baseline before.png --settle 300
   {"v":1,"ok":true,"data":{"changed":true,"change_ms":318,"settled":true,"elapsed_ms":502},"warnings":[]}
   ```

   同样的顺序，用 `wait --change` 而不带 `--baseline`，会得到假的 `TIMEOUT`。每张截图都会写一个旁置文件 `before.sight.json`；来自别的目标类型（窗口 vs 区域）、`--crop` 不同的基准会返回 `INVALID_ARGS`，绝不会悄悄做错误的比较。窗口自基准图之后被调整或展开了大小，则算作变化（`changed:true`，并带警告 `baseline_size_changed`）。

5. **再验证。** 再跑一次同样的 `shot` 做对比。要看性能，就在程序干活时跑 `stat`：

   ```
   > sight stat --pid 17400 --duration 2000
   {"v":1,"ok":true,"data":{"pid":17400,"exe":"powershell.exe","duration_ms":2038,"samples":4,"logical_cores":16,"cpu_core_pct":{"avg":100.42,"p95":104.98,"max":104.98},"cpu_total_pct":{"avg":6.28,"p95":6.56,"max":6.56},"mem_mb":{"avg":52.87,"p95":52.87,"max":52.87},"mem_measure":"private_bytes: ...","processes":[...]},"warnings":[]}
   ```

   这是一个 PowerShell 死循环：吃满一个核（`cpu_core_pct` ≈ 100），占 16 核的约 6.3%（`cpu_total_pct`）。

### 录制 GIF

`sight rec` 把窗口或屏幕区域录下来，用于 README 演示和测试证据。它会一直阻塞到录完，所以请在你操作的同时让它在后台运行：

```powershell
$job = Start-Job { sight rec --hwnd 15603054 --duration 10000 --until-settled 500 --keyframes 3 --out demo.gif }
Start-Sleep -Milliseconds 700                       # capture needs ~0.3 s to start
# ... act with your own tool ...
$rec = Receive-Job $job -Wait -AutoRemoveJob | ConvertFrom-Json
```

返回结果包含 `frames`、`fps`、`duration_ms`、`size_bytes`，以及与 `shot` 相同的坐标公式。相同的帧会被合并（静止的窗口只有一帧很长的画面），`--keyframes 3` 还会把变化最大的几个时刻存成 PNG 静帧，智能体不用解码 GIF 就能检查录像。默认 `--fps 10`（最大 30）、`--max-edge 800`。

`rec` 在启动时选定一个捕获来源，之后绝不切换：`--source auto`（默认）对窗口的矩形做屏幕捕获，菜单、提示框和弹出窗口都会录到；只有当别的窗口盖住目标时才改用窗口捕获（`source`、`source_reason` 和警告 `rec_window_source_no_popups` 会告诉你用了哪个）。`--pad 40` 还会多录窗口四周 40 像素，让伸出窗口的下拉列表也能录进去。窗口移动时捕获区域会跟着走；录制中途盖住它的窗口会列在 `occlusion_events` 里。

### 坐标：选与你的输入工具一致的原点

对窗口截图，`shot` 会为每种原点各给一条公式。请选与你自己的输入工具一致的那一条，其中 `px, py` 是保存图片里的像素：

```
> sight shot --hwnd 11736012 --crop 100,120,300,200 --max-edge 0
{"v":1,"ok":true,"data":{"image":{"w":300,"h":200},"maps_to":{"space":"screen_physical","x":307,"y":270,"w":300,"h":200},"scale":1.0,"formula":"screen_x = 307 + px / 1.0; screen_y = 270 + py / 1.0","window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},"formula_window":"win_x = 100 + px / 1.0; win_y = 120 + py / 1.0","formula_client":"client_x = 99 + px / 1.0; client_y = 89 + py / 1.0","settled":true,"changed":false,"change_ms":null,"elapsed_ms":351,"popups":[]},"warnings":[]}
```

| 字段 | 原点 | 适用于 |
|---|---|---|
| `formula` | 屏幕（虚拟桌面），物理像素 | 按绝对屏幕坐标点击的工具 |
| `formula_window` | 窗口 DWM 边框的左上角 | 相对窗口取坐标的工具；与 `--crop` 同一原点 |
| `formula_client` | 客户区左上角（标题栏之下） | 相对客户区取坐标的工具 |

`window.frame` 与 `window.client` 是屏幕像素；`client_offset` 是客户区左上角减去边框左上角。上例中图片上的点 (50, 40) 对应屏幕 (357, 310)、窗口 (150, 160)、客户区 (149, 129)。

`--crop x,y,w,h` 相对窗口边框，所以无论窗口在哪里，选中的都是窗口的同一部分。它先于 `--scale` 和 `--max-edge` 应用；`--max-edge 0` 表示不缩小，因此 `--crop ... --max-edge 0` 输出的恰好是 w×h 像素。它可用于 `--hwnd` 和 `--pid`（不能用于 `--region`），`wait` 也接受它。

（以上均为真实运行结果，运行环境是繁体中文 Windows，所以标题是“未命名 - 記事本”；`...` 表示有删减。）

## 配合 AI 智能体使用

- **[AGENTS.md](AGENTS.md)** 是写给智能体的一页说明：命令、观察循环、坐标公式，以及每种错误码该怎么处理。会读 `AGENTS.md` 的工具（Codex、Cursor 等）在这个仓库里会自动读到它；在你自己的项目里，可以复制它，或在智能体说明中引用它。
- **`skills/sightline/SKILL.md`** 用 Agent Skills 格式打包了同样的内容，只在需要时才加载。为 Claude Code 安装，复制整个文件夹：

  ```
  xcopy /E /I skills\sightline %USERPROFILE%\.claude\skills\sightline
  ```

  也可以放到项目的 `.claude\skills\` 下。其他支持 Agent Skills 的工具，把同一个文件夹放进它们各自的 skills 目录即可。

记得让 `sight.exe` 在智能体的 `PATH` 里。

## 参考文档

每个命令的完整参数与输出字段、坐标与 DPI 规则、弹出菜单处理、输出约定与错误码，请见 **[docs/REFERENCE.zh-CN.md](docs/REFERENCE.zh-CN.md)**。

## 常见问题

**支持哪些 Windows 版本？** Windows 10 和 11。

**数据会离开我的电脑吗？** 不会。没有任何网络代码，图像只存在本地文件里。

**为什么 `stat` 的数字和任务管理器不一样？** 内存统计的是*私有字节*（提交内存），不是任务管理器里的“专用工作集”。

## 从源码构建

```powershell
cargo build --release --locked
cargo test
```

## 限制 / 尚未验证

- `--expect-change` / `wait --change` 以捕获到的第一帧（`sight` 启动后约 0.3 秒）为基准，在那之前已完成的变化检测不到；这种情况请用 `--baseline`。
- 窗口公式与 `--crop` 只在 100% DPI 的显示器上验证过；它们只是物理像素的加减运算，但没有在 125%、150% 下测试过。
- `WINDOW_PROTECTED` 尚未在真正禁止截图的窗口上测试过。
- 经典 Win32 `#32768` 菜单尚未测试，对它们的弹出窗口回退尚未验证。
- PID 复用检测依赖精度为 1 秒的进程启动时间（`start_unix_s`），同一秒内被复用的 PID 检测不到。
- 内存统计的是**私有字节**（提交内存，`PrivateUsage`），不是任务管理器里的“专用工作集”，两者数值会不同。

## 许可证

[AGPL-3.0](LICENSE)。
