<p align="center">
  <a href="../../releases"><img alt="Release" src="https://img.shields.io/github/v/release/sao-coding/sightline?include_prereleases"></a>
  <a href="../../actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/sao-coding/sightline/ci.yml?branch=main&label=CI"></a>
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4">
  <a href="LICENSE"><img alt="License: AGPL-3.0" src="https://img.shields.io/badge/license-AGPL--3.0-blue"></a>
</p>

<p align="center"><a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · <b>繁體中文</b></p>

# sightline

讓 Windows 上的 AI 程式設計代理有眼睛、也有碼錶。單一小型執行檔，指令是 `sight`。

sightline 能找出視窗、被擋住也照樣截圖、等畫面先變化再穩定才出圖，並告訴代理影像像素要怎麼換算成螢幕、視窗和用戶區座標；也能量測整棵程序樹的 CPU 與記憶體。它**只看、只量**：從不點擊、不輸入、不移動滑鼠。操作交給代理自己的工具，sightline 負責確認結果。

[為什麼](#為什麼需要它) • [安裝](#安裝) • [快速開始](#快速開始) • [搭配代理](#搭配-ai-代理使用) • [參考文件](docs/REFERENCE.zh-TW.md) • [常見問題](#常見問題)

```
sight list --exe notepad                  # 哪個視窗？
sight shot --hwnd 15603054 --settle 300   # 等畫面穩定後長什麼樣？
sight wait --hwnd 15603054 --change --settle 300   # 我點完之後，UI 變了並穩定了嗎？
sight stat --pid 19668 --duration 2000    # 用了多少 CPU 和記憶體？
```

## 為什麼需要它

叫 Windows 上的代理「看一下對話框顯示得對不對」，它多半會臨時寫一段 PowerShell：`Start-Sleep 3`、`CopyFromScreen`、寫死的 PID。sightline 就是用來取代這些。

| 現在的腳本 | 問題 | sightline 的做法 |
|---|---|---|
| `Start-Sleep 3` | 等太短不穩定，等太長浪費時間 | `--settle 300` 等到像素不再變動；有逾時，沒穩定就老實回報 `settled:false` |
| 點擊後 `Start-Sleep 0.5` | UI 可能還沒開始變，單用 settle 會回傳舊畫面 | `--expect-change` 先等像素與基準畫面出現差異，再等穩定 |
| `CopyFromScreen` | 疊在上面的其他視窗也被截進去 | `shot --hwnd` 直接擷取目標視窗，被遮住也沒關係 |
| 視窗擷取 API | 右鍵選單是獨立的頂層視窗，會被漏掉 | 自動偵測並帶上該程式自己的彈出選單 |
| 模型自動縮放影像 | 圖上的位置對不上螢幕 | 每張影像都附上從影像像素到螢幕像素的 `formula` |
| 手工 `Crop`、先把視窗移到固定位置 | 視窗在別處就失效 | `--crop x,y,w,h` 相對於視窗、與視窗位置無關；`formula_window`、`formula_client` 把像素換算到視窗與用戶區座標 |
| DPI 縮放 | 150% 縮放下手算座標全錯 | 以 Per-Monitor DPI 感知執行，並提供 `dpi_scale` 與 `monitor_dpi_scale` |
| `Get-Process` | 只看一個程序，PID 還是寫死的 | `list` 找 PID；`stat` 量整棵程序樹，提供 avg/p95/max |
| 自由格式的文字輸出 | 代理只能讀文字猜哪裡出錯 | 一行 JSON、固定的錯誤碼，`hint` 直接說下一步 |

影像寫成檔案，只回傳**路徑**，不回傳 base64，所以一張截圖只花一次讀檔，而不是幾千個 token。

## 指令

| 指令 | 回答的問題 | 主要好處 |
|---|---|---|
| `list` | 有哪些視窗？該鎖定哪個 PID / HWND？ | 不必寫死 PID 或標題；提供位置、所在螢幕、DPI、是否最小化 |
| `shot` | 這個視窗現在長什麼樣子？ | 被遮住也能擷取、帶上彈出選單、等變化再等穩定、相對於視窗的 `--crop`、多種座標公式 |
| `wait` | 我操作之後，UI 變了並穩定了嗎？ | 與 `shot` 相同的等待邏輯，但不出圖；回傳 `changed`、`change_ms`、`settled` |
| `stat` | 這個程式在一段時間內用了多少 CPU 和記憶體？ | 量整棵程序樹（Electron、瀏覽器、遊戲），avg/p95/max，以私有位元組計算，共用分頁不會重複計入 |
| `doctor` | 這台電腦有幾個螢幕、各自縮放多少？ | 結果怪怪的時候先跑它 |

截圖寫入檔案，只回傳**路徑**，不回傳 base64。代理讀一次檔案就好，不必為一張圖花掉幾千個 token。`--max-edge`（預設 1568）讓影像維持在視覺模型不會再縮放的尺寸。

## 安裝

**PowerShell 一行安裝**（安裝到 `%LOCALAPPDATA%\Programs\sight`，並加入使用者 `PATH`）：

```powershell
irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex
```

裝完請**開新的終端機**，讓 `PATH` 生效，再執行 `sight doctor` 確認。指定版本、改安裝資料夾或解除安裝：

```powershell
$env:SIGHT_VERSION = "0.1.0"; irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex
$env:SIGHT_INSTALL_DIR = 'D:\tools\sight'; irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex
$env:SIGHT_UNINSTALL = 1; irm https://raw.githubusercontent.com/sao-coding/sightline/main/install.ps1 | iex
```

腳本會用每個版本附帶的 `sight.exe.sha256` 驗證下載內容。

**手動安裝。** 從 [Releases](../../releases) 下載 `sight.exe`（`sight-windows-x64.zip` 另含文件與 skill），放進某個資料夾，再把該資料夾加入 `PATH`：

```powershell
# 範例：C:\tools\sight（使用者 PATH，不需要系統管理員）
[Environment]::SetEnvironmentVariable('Path', "$([Environment]::GetEnvironmentVariable('Path','User'));C:\tools\sight", 'User')
```

或用「設定 → 系統 → 關於 → 進階系統設定 → 環境變數 → `Path` → 新增」。完成後請重新開啟終端機（以及代理）。代理必須能在它的 `PATH` 中找到 `sight`。

**從原始碼建置**（Windows 10/11，需要 Rust 工具鏈）：

```powershell
cargo build --release     # target\release\sight.exe
```

## 快速開始

```
sight doctor                              # 螢幕與 DPI
sight list --exe notepad                  # 找出目標視窗
sight shot --hwnd 15603054 --settle 300   # 等畫面穩定後擷取
```

代理循環中的一輪：

1. **找到目標。**

   ```
   > sight list --exe notepad
   {"v":1,"ok":true,"data":{"count":1,"windows":[{"hwnd":15603054,"pid":19668,"exe":"Notepad.exe","title":"未命名 - 記事本","rect":{"x":3010,"y":362,"w":1178,"h":637},"monitor":1,"dpi_scale":1.0,"monitor_dpi_scale":1.0,"minimized":false}]},"warnings":[]}
   ```

2. **看。** 用代理的看圖工具開啟 `data.path`。

   ```
   > sight shot --hwnd 15603054 --settle 300
   {"v":1,"ok":true,"data":{"path":"C:\\Users\\me\\AppData\\Local\\Temp\\sightline\\shot-20261123-214402-405.jpg","image":{"w":1178,"h":637},"maps_to":{"space":"screen_physical","x":3010,"y":362,"w":1178,"h":637},"scale":1.0,"dpi_scale":1.0,"monitor_dpi_scale":1.0,"formula":"screen_x = 3010 + px / 1.0; screen_y = 362 + py / 1.0","settled":true,"elapsed_ms":945,"popups":[]},"warnings":[]}
   ```

3. **操作。** 用代理自己的輸入工具，依下面的公式算出座標後點擊或輸入。
4. **等 UI 就緒**，再去看。外部點擊剛發生時，UI 可能還沒開始變，單用 `--settle` 會回傳舊畫面。`--expect-change`（或 `wait --change`）先等像素與基準畫面出現差異，再等穩定，兩個階段共用 `--timeout`：

   ```
   > sight wait --hwnd 15603054 --change --settle 300
   {"v":1,"ok":true,"data":{"changed":true,"change_ms":326,"settled":true,"elapsed_ms":492},"warnings":[]}
   > sight shot --hwnd 15603054 --expect-change --settle 300     # 同樣的等待，之後儲存影像
   ```

   若逾時前沒有任何變化，`shot` 仍會存下最新一幀，兩個指令都以 `TIMEOUT`（結束碼 5）失敗，並帶 `data.changed:false`。閃爍游標之類的微小雜訊不算變化：需要至少 `--min-change-px` 個像素不同（預設 16）。基準是擷取到的第一幀，它在 `sight` 啟動後約 0.3 秒才抵達，所以在那之前已完成的變化不會被看到（`TIMEOUT` 且 `changed:false`：用一般的 `shot` 看一眼）。

5. **再驗證。** 再跑一次相同的 `shot` 來比對。要看效能，就在程式工作時跑 `stat`：

   ```
   > sight stat --pid 17400 --duration 2000
   {"v":1,"ok":true,"data":{"pid":17400,"exe":"powershell.exe","duration_ms":2038,"samples":4,"logical_cores":16,"cpu_core_pct":{"avg":100.42,"p95":104.98,"max":104.98},"cpu_total_pct":{"avg":6.28,"p95":6.56,"max":6.56},"mem_mb":{"avg":52.87,"p95":52.87,"max":52.87},"mem_measure":"private_bytes: ...","processes":[...]},"warnings":[]}
   ```

   這是一個 PowerShell 無窮迴圈：吃滿一個核心（`cpu_core_pct` ≈ 100），佔 16 核心的約 6.3%（`cpu_total_pct`）。

### 座標：選與你的輸入工具一致的原點

對視窗擷取，`shot` 會為每種原點各給一條公式。請選與你自己的輸入工具一致的那一條，其中 `px, py` 是存下影像裡的像素：

```
> sight shot --hwnd 11736012 --crop 100,120,300,200 --max-edge 0
{"v":1,"ok":true,"data":{"image":{"w":300,"h":200},"maps_to":{"space":"screen_physical","x":307,"y":270,"w":300,"h":200},"scale":1.0,"formula":"screen_x = 307 + px / 1.0; screen_y = 270 + py / 1.0","window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},"formula_window":"win_x = 100 + px / 1.0; win_y = 120 + py / 1.0","formula_client":"client_x = 99 + px / 1.0; client_y = 89 + py / 1.0","settled":true,"changed":false,"change_ms":null,"elapsed_ms":351,"popups":[]},"warnings":[]}
```

| 欄位 | 原點 | 適用於 |
|---|---|---|
| `formula` | 螢幕（虛擬桌面），實體像素 | 以絕對螢幕座標點擊的工具 |
| `formula_window` | 視窗 DWM 邊框的左上角 | 相對於視窗取座標的工具；與 `--crop` 同一原點 |
| `formula_client` | 用戶區左上角（標題列之下） | 相對於用戶區取座標的工具 |

`window.frame` 與 `window.client` 是螢幕像素；`client_offset` 是用戶區左上角減去邊框左上角。上例中影像上的點 (50, 40) 對應螢幕 (357, 310)、視窗 (150, 160)、用戶區 (149, 129)。

`--crop x,y,w,h` 相對於視窗邊框，所以無論視窗在哪裡，選到的都是視窗的同一部分。它先於 `--scale` 與 `--max-edge` 套用；`--max-edge 0` 表示不縮小，因此 `--crop ... --max-edge 0` 輸出的恰好是 w×h 像素。它可用於 `--hwnd` 與 `--pid`（不能用於 `--region`），`wait` 也接受它。

（以上都是實際執行結果；`...` 表示有刪減。）

## 搭配 AI 代理使用

- **[AGENTS.md](AGENTS.md)** 是寫給代理的一頁說明：指令、觀察循環、座標公式，以及每種錯誤碼該怎麼處理。會讀 `AGENTS.md` 的工具（Codex、Cursor 等）在這個儲存庫中會自動讀到；在你自己的專案裡，可以複製它，或在代理說明中引用它。
- **`skills/sightline/SKILL.md`** 以 Agent Skills 格式包裝同樣的內容，只在需要時才載入。要安裝到 Claude Code，複製整個資料夾：

  ```
  xcopy /E /I skills\sightline %USERPROFILE%\.claude\skills\sightline
  ```

  也可以放在專案的 `.claude\skills\` 底下。其他支援 Agent Skills 的工具，把同一個資料夾放進它們各自的 skills 目錄即可。

記得讓 `sight.exe` 位於代理的 `PATH` 中。

## 參考文件

每個指令的完整參數與輸出欄位、座標與 DPI 規則、彈出選單處理、輸出約定與錯誤碼，請見 **[docs/REFERENCE.zh-TW.md](docs/REFERENCE.zh-TW.md)**。

## 常見問題

**支援哪些 Windows 版本？** Windows 10 與 11。

**資料會離開我的電腦嗎？** 不會。沒有任何網路程式碼，影像只存在本機檔案中。

**為什麼 `stat` 的數字和工作管理員不一樣？** 記憶體量測的是*私有位元組*（認可記憶體），不是工作管理員中的「私人工作集」。

## 從原始碼建置

```powershell
cargo build --release --locked
cargo test
```

## 限制 / 尚未驗證

- `--expect-change` / `wait --change` 以擷取到的第一幀（`sight` 啟動後約 0.3 秒）為基準，在那之前已完成的變化偵測不到。
- 視窗公式與 `--crop` 只在 100% DPI 的螢幕上驗證過；它們只是實體像素的加減運算，但沒有在 125%、150% 下測試過。
- `WINDOW_PROTECTED` 尚未在真正禁止擷取的視窗上測試。
- 傳統 Win32 `#32768` 選單尚未測試，針對它們的彈出視窗備援機制尚未驗證。
- PID 重複使用的檢查依賴精度為 1 秒的程序啟動時間（`start_unix_s`），同一秒內被重複使用的 PID 偵測不到。
- 記憶體量測的是**私有位元組**（認可記憶體，`PrivateUsage`），不是工作管理員中的「私人工作集」，兩者數值會不同。

## 授權

[AGPL-3.0](LICENSE)。
