# sightline 參考文件

回到 [README](../README.zh-TW.md)。

## 指令參考

全域參數：`--pretty`（縮排的 JSON，給人看），所有指令都能用。

### `list`

列出頂層視窗。預設只列出可見且有標題的視窗，桌面、cloaked 視窗與小於 10 px 的視窗會被隱藏。

| 參數 | 意義 |
|---|---|
| `--pid <PID>` | 只看這個程序的視窗 |
| `--exe <TEXT>` | 執行檔名稱包含此文字（不分大小寫） |
| `--title <TEXT>` | 標題包含此文字（不分大小寫） |
| `--all` | 關閉所有預設過濾（不可見、cloaked、無標題、桌面 `Progman`/`WorkerW`、小於 10 px 的視窗） |
| `--full-title` | 不截斷標題（預設前 40 個字元 + `…`） |
| `--limit <N>` | 最多回傳的視窗數（預設 50，`--all` 也適用） |

每個視窗包含：`hwnd`、`pid`、`exe`、`title`、`rect`（螢幕實體像素）、`monitor`（從 1 起算）、`dpi_scale`、`monitor_dpi_scale`、`minimized`。依 Z 順序排列，最上層在前。最小化的視窗不含 `rect` 與 `monitor`。標題被截斷時多一個 `title_truncated:true`，`--title` 仍以完整標題比對。超過 `--limit` 時，`data` 多出 `total` 與 `truncated:true`，並給出 `list_truncated` 警告，提示用 `--exe`、`--title` 或 `--pid` 縮小範圍。

### `shot`

擷取畫面並存成檔案。`--pid`、`--hwnd`、`--region` 必須且只能指定一個。

| 參數 | 預設值 | 意義 |
|---|---|---|
| `--pid <PID>` | | 擷取該程序最大的可見視窗（其他視窗以 `other_candidate_window` 警告列出） |
| `--hwnd <HWND>` | | 擷取這個視窗（十進位，與 `list` 輸出相同） |
| `--region x,y,w,h` | | 螢幕區域，實體像素；真正的螢幕擷取，蓋在上面的東西都會被截進去 |
| `--crop x,y,w,h` | | 只保留這個矩形，座標相對於視窗 DWM 邊框的左上角，實體像素。僅限 `--pid` / `--hwnd`（與 `--region` 併用，或超出視窗，都會回傳 `INVALID_ARGS`，`hint` 會給出視窗大小）。先於 `--scale` 與 `--max-edge` 套用 |
| `--baseline <image>` | | 與這張更早的截圖（它的 `data.path`）比較，而不是與第一幀比較。隱含 `--expect-change`。見[基準圖](#基準圖) |
| `--scale <F>` | `1.0` | 縮放倍率，範圍 (0, 16]。大於 1 時用最近鄰內插，並輸出 PNG |
| `--max-edge <N>` | `1568` | 縮小到長邊不超過 N 像素。**`0` 表示不縮小**（1:1 像素） |
| `--expect-change` | 關閉 | 以第一幀為基準，等畫面與它出現差異，之後再套用 `--settle`。適合在外部點擊或按鍵之後立刻使用 |
| `--min-change-px <N>` | 使用 `--expect-change` 時為 `16`，否則為 `1` | 差異像素達到多少才算「變化」（>= 1）；`1` 即精確比對。可忽略閃爍游標之類的雜訊 |
| `--settle <ms>` | 關閉 | 等畫面持續這麼久沒有變動（受 `--timeout` 限制） |
| `--timeout <ms>` | `5000` | 總逾時，「等變化」與「等穩定」兩個階段共用 |
| `--out <path>` | 暫存資料夾 | 副檔名 `.png` / `.jpg` / `.jpeg` 決定格式。預設 `%TEMP%\sightline\shot-<時間戳記>.jpg`（`--scale` > 1 時為 PNG） |

JPEG 品質為 80。視窗擷取（`--pid` / `--hwnd`）另外會回傳 `popups`（可能為空）。

警告：`other_candidate_window: ...`、`popups_included_via_screen_capture`、`region_partially_offscreen`、`region_spans_mixed_dpi`、`window_dpi_unaware_or_virtualized`、`client_rect_unavailable`。

輸出欄位：`path`、`image`、`maps_to`、`scale`、`dpi_scale`、`monitor_dpi_scale`、`formula`；視窗目標另有 `window`、`formula_window`、`formula_client`（見[座標與 DPI](#座標與-dpi)）；以及一律存在的 `settled`、`changed`、`change_ms`、`elapsed_ms`。

**感知變化的等待。** 單用 `--settle` 是等畫面不再變動，但外部點擊剛發生時，UI 可能還沒開始變，於是它會把舊畫面當成「已穩定」回傳。使用 `--expect-change` 時：

1. 第一幀作為基準。
2. 等到某一幀與上一個被接受的幀至少有 `--min-change-px` 個像素不同（第一個這樣的幀就是首次變化），再照常套用 `--settle`。
3. 兩個階段共用 `--timeout`。若逾時前沒有任何變化，仍會存下最新一幀，並以 `TIMEOUT`（結束碼 5）失敗，`data.changed:false`、`data.settled:false`。

兩種模式下，`changed` 表示是否看到與基準不同的幀，`change_ms` 是從指令啟動到該幀的時間（沒有則為 `null`）。視窗被調整大小也算變化。`--min-change-px` 大於 1 時，小於門檻的差異會被丟棄，因此可能不會出現在存下的影像裡。

基準是擷取到的第一幀，它在 `sight` 啟動後約 0.3 秒才抵達（擷取工作階段啟動耗時）。在那之前就已完成的變化不會被看到：`TIMEOUT` 且 `changed:false` 可能代表 UI 其實已經更新，這時用一般的 `shot` 看一眼即可。對反應是瞬間的 UI，只用 `--settle`，或在操作之前一刻再啟動指令。

#### 基準圖

`--baseline <image>` 用你之前截的一張圖取代「第一幀」，這樣即使變化在 `sight` 啟動之前就已完成，也能偵測到。符合實際的順序：

```
> sight shot --hwnd 11736012 --out before.png
# 操作：UI 瞬間就變了
> sight wait --hwnd 11736012 --baseline before.png --settle 300
{"v":1,"ok":true,"data":{"changed":true,"change_ms":318,"settled":true,"elapsed_ms":502},"warnings":[]}
> sight wait --hwnd 11736012 --change --timeout 1500      # 同樣的順序，沒有基準圖
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"no change detected within --timeout 1500ms (no image saved)",...},"data":{"changed":false,"change_ms":null,"settled":false,"elapsed_ms":1534}}
```

擷取到的第一幀會與這張圖比較；若至少有 `--min-change-px` 個像素不同，就立刻算作變化（此時 `change_ms` 約等於第一幀抵達的時間），然後照常套用 `--settle`。若一致，則像 `--expect-change` 那樣繼續等待之後的變化。

- **必須可比較。** 每次 `shot` 都會在圖片旁寫一個旁置檔 `<name>.sight.json`（取代副檔名），記錄目標（`window` + `hwnd`，或 `region` + 矩形）、相對於視窗的 `crop`、裁切後縮放前的像素大小（`source`）、儲存大小（`image`）與 `format`。`--baseline` 會拿它與新的擷取核對：目標類型或 `--crop` 不同、圖片與旁置檔不再一致、或檔案不存在時，回傳帶 `hint` 的 `INVALID_ARGS`。移動視窗沒問題，因為裁切和比較都是相對於視窗的。不是 sight 截的圖（沒有旁置檔）會被當作未縮放的 1:1 影像，並給出警告 `baseline_without_metadata`；它的大小與本次擷取不同時是 `INVALID_ARGS`。
- **視窗大小變了算作變化。** 視窗（或 `--crop` 的結果）的像素大小（裁切後、縮放前）與基準圖不同時，會回報為已變化（`changed:true`，`change_ms` 約等於第一幀抵達的時間），並帶警告 `baseline_size_changed: {"baseline":{"w":642,"h":452},"now":{"w":742,"h":512}}`，之後照常套用 `--settle`。點擊經常會讓視窗變大或展開，代理需要的是這個結果，而不是一個錯誤。若新的大小使 `--crop` 超出視窗，仍照常回傳 `INVALID_ARGS`，`hint` 給出視窗大小。
- **JPEG 與縮放過的基準圖不需要容差。** `shot` 預設存 JPEG（品質 80），預設的 `--max-edge` 也會縮小大視窗。這裡不做模糊比對，而是把即時幀先經過與 `shot` 相同的縮放、相同的 JPEG 編碼/解碼再比較，所以相同的像素必然完全相等。`--max-edge 0` 的 `.png` 仍然最省（不用逐幀轉換）。唯一的例外：縮放過或 JPEG 的基準圖不能用於跨多個螢幕或超出螢幕邊緣的 `--region`（或彈出視窗的螢幕擷取後備），那會回傳 `INVALID_ARGS`，請改用 `--max-edge 0` 的 `.png`。
- 其他警告：`baseline_size_changed`、`baseline_metadata_unreadable`、`baseline_metadata_not_written`（旁置檔寫不出來）。

### `wait`

目標、`--crop`、`--settle`、`--timeout`、`--min-change-px`、`--baseline` 參數與 `shot` 相同，等待邏輯也相同，但**不儲存影像**。`--change` 即 `--expect-change`。代理需要在下一次點擊前確認 UI 已就緒時使用它。

```
> sight wait --hwnd 11736012 --change --settle 300
{"v":1,"ok":true,"data":{"changed":true,"change_ms":326,"settled":true,"elapsed_ms":492},"warnings":[]}
> sight wait --hwnd 11736012 --change --timeout 1000
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"no change detected within --timeout 1000ms (no image saved)","hint":"increase --timeout or --settle; with --expect-change/--change, make sure the action really changes pixels in the target"},"data":{"changed":false,"change_ms":null,"settled":false,"elapsed_ms":1020}}
```

輸出：`changed`、`change_ms`、`settled`、`elapsed_ms`。用 `--crop` 可以只盯視窗裡應該發生變化的那一塊。

### `rec`

把視窗或螢幕區域錄成 GIF 動圖，用於 README 展示和測試證據。與 `shot` 一樣，`--pid`、`--hwnd`、`--region` 必須且只能指定一個。

| 參數 | 預設值 | 意義 |
|---|---|---|
| `--duration <ms>` | 必填 | 錄製時間長度，從擷取到第一幀時開始算 |
| `--fps <N>` | `10` | 取樣幀率，1 到 30 |
| `--max-edge <N>` | `800` | 縮小到長邊不超過 N 像素（`0` 表示原尺寸） |
| `--until-settled <ms>` | 關閉 | 影像發生變化後又連續這麼久沒變，就提前結束（不會超過 `--duration`） |
| `--keyframes <N>` | `0` | 另外儲存 N 張變化最大時刻的 PNG 靜態幀 |
| `--crop x,y,w,h` | | 與 `shot` 相同的相對於視窗的裁切（僅限 `--pid` / `--hwnd`） |
| `--out <path>` | 暫存資料夾 | GIF 路徑。預設 `%TEMP%\sightline\rec-<時間戳記>.gif`；靜態幀存為旁邊的 `<name>-key1.png` 等 |

`rec` **會阻塞**到錄製結束，所以代理應當在操作的同時讓它在背景執行（見下）。它使用與 `shot` 相同的 Windows Graphics Capture 工作階段，包括彈出視窗的螢幕擷取後備和跨螢幕區域。

- **幀。** 依 `--fps` 取樣；只有像素變化時才寫入新幀，沒變化的時段只是延長上一幀的延遲。靜止的視窗只錄出一幀，延遲很長。延遲以百分之一秒（GIF 的單位）為單位，取自連續的時間線；機器忙碌時實際幀數可能少於 `--fps`。
- **計時。** 從擷取到第一幀時開始計時（擷取啟動約需 0.3 秒），所以 `elapsed_ms` 比 `duration_ms` 大約多 0.7 秒。
- **`--until-settled`** 在第一幀之後的第一次變化時才開始計時。錄製開始時視窗是靜止的，不會導致它結束；如果始終沒有變化，就錄滿 `--duration`。
- **輸出。** `path`、`frames`、`fps`、`duration_ms`（GIF 本身的長度）、`image`、`size_bytes`、`stopped`（`duration` 或 `settled`）、`elapsed_ms`，以及與 `shot` 相同的 `maps_to`、`scale`、`dpi_scale`、`monitor_dpi_scale`、`formula`，視窗還有 `window`、`formula_window`、`formula_client`、`popups`。它們把 GIF 像素（和靜態幀像素）換算成錄製開始時的座標。指定 `--keyframes N` 時，`keyframes` 列出 `{t_ms, path}`：第一幀、最後一幀，以及中間變化最大的幀（共 N 張，不同的幀不夠時更少），依時間排序；`t_ms` 從 GIF 開頭算起。
- **警告。** `gif_large`（超過約 10 MB）、`window_resized_during_recording`（之後的幀會依第一幀的大小裁切或補邊），以及 `shot` 的那些警告。
- **限制。** 彈出視窗只在錄製開始時偵測一次：之後才開啟的選單不會出現在視窗錄影裡（這種情況請錄 `--region`）。公式裡的視窗位置是開始時的位置。

```
> sight rec --hwnd 11736012 --duration 4000 --keyframes 3 --out demo.gif
{"v":1,"ok":true,"data":{"path":"C:\\Users\\me\\AppData\\Local\\Temp\\sightline\\rec-20261124-161200-100.gif","frames":22,"fps":10,"duration_ms":4000,"image":{"w":642,"h":452},"size_bytes":597570,"maps_to":{"space":"screen_physical","x":207,"y":150,"w":642,"h":452},"scale":1.0,"dpi_scale":1.0,"monitor_dpi_scale":1.0,"formula":"screen_x = 207 + px / 1.0; screen_y = 150 + py / 1.0","window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},"formula_window":"win_x = 0 + px / 1.0; win_y = 0 + py / 1.0","formula_client":"client_x = -1 + px / 1.0; client_y = -31 + py / 1.0","stopped":"duration","elapsed_ms":4504,"popups":[],"keyframes":[{"t_ms":1,"path":"...\\rec-20261124-161200-100-key1.png"},{"t_ms":1500,"path":"...\\rec-20261124-161200-100-key2.png"},{"t_ms":2700,"path":"...\\rec-20261124-161200-100-key3.png"}]},"warnings":[]}
```

在操作的同時放到背景執行（它啟動約需 0.3 秒；第一次操作前先等約 0.5 秒）：

```powershell
$job = Start-Job { sight rec --hwnd 15603054 --duration 10000 --until-settled 500 --keyframes 3 --out demo.gif }
Start-Sleep -Milliseconds 500                       # 0.3 s
# ... act with your own tool ...
$rec = Receive-Job $job -Wait -AutoRemoveJob | ConvertFrom-Json
```

### `stat`

取樣某個程序（預設連同所有子孫程序）的 CPU 與記憶體。

| 參數 | 預設值 | 意義 |
|---|---|---|
| `--pid <PID>` | 必填 | 根程序 ID |
| `--duration <ms>` | 必填 | 取樣時間長度（> 0） |
| `--interval <ms>` | `500` | 取樣間隔（> 0） |
| `--no-tree` | 關閉 | 只量根程序 |

- `cpu_core_pct`：100 代表吃滿一個邏輯核心。`cpu_total_pct` = `cpu_core_pct` / `logical_cores`。
- `mem_mb`：私有位元組，單位 MiB，整棵樹加總。`processes` 列出所有被取樣的程序。
- 為什麼要看整棵樹：在一台電腦上對 Edge 瀏覽器的 PID 跑 `stat`，得到 46 個程序、`mem_mb` 6293 MiB；加上 `--no-tree` 只剩 561 MiB。
- 警告：`target_exited`、`processes_exited_during_sampling: ...`、`access_denied_for_some_processes: ...`。

### `doctor`

列出螢幕：`index`、`rect`（虛擬桌面實體像素）、`dpi_scale`、`primary`。

```
> sight doctor
{"v":1,"ok":true,"data":{"monitors":[{"index":2,"rect":{"x":0,"y":0,"w":2560,"h":1440},"dpi_scale":1.5,"primary":true},{"index":1,"rect":{"x":2560,"y":0,"w":2560,"h":1440},"dpi_scale":1.0,"primary":false}]},"warnings":[]}
```

## 座標與 DPI

sightline 回報的所有座標都是虛擬桌面上的**螢幕實體像素**。它以 Per-Monitor V2 DPI 感知模式執行，不會混入被縮放過的「邏輯座標」。多螢幕環境下座標可能是負數。

存下來的影像通常比視窗小。`shot` 會提供縮放比例（`scale`，每個螢幕像素對應多少影像像素）和現成的換算公式，代理不用自己算。

**算例。** 一個 150% DPI 下的檔案總管視窗，實體尺寸 1691×953，使用預設的 `--max-edge 1568` 擷取：

```
> sight shot --hwnd 3870832 --settle 300
{"v":1,"ok":true,"data":{"path":"...\\shot-20261123-214403-422.jpg","image":{"w":1568,"h":884},"maps_to":{"space":"screen_physical","x":798,"y":278,"w":1691,"h":953},"scale":0.9273,"dpi_scale":1.5,"monitor_dpi_scale":1.5,"formula":"screen_x = 798 + px / 0.9273; screen_y = 278 + py / 0.9273","settled":true,"elapsed_ms":726,"popups":[]},"warnings":[]}
```

模型在影像的 (400, 200) 看到一個按鈕，那麼：

```
screen_x = 798 + 400 / 0.9273 ≈ 798 + 431.4 = 1229
screen_y = 278 + 200 / 0.9273 ≈ 278 + 215.7 = 494
```

就在螢幕實體座標 (1229, 494) 點擊。`maps_to` 是影像涵蓋的螢幕矩形。視窗移動或改變大小後要重新 `shot`，`formula` 只對該次的影像有效。

**視窗原點與用戶區原點。** `formula` 換算到螢幕。對 `--hwnd` / `--pid` 的擷取，還會再給兩條公式換算到視窗，因為很多輸入工具是相對於視窗取座標的。請選擇與你自己的輸入工具一致的原點：

| 欄位 | 原點 |
|---|---|
| `formula` | 螢幕（虛擬桌面），實體像素 |
| `formula_window` | 視窗 DWM 延伸邊框的左上角（可見視窗，含標題列） |
| `formula_client` | 用戶區左上角（標題列與邊框之下） |

`data.window` 包含 `frame {x,y,w,h}`（DWM 延伸邊框範圍，螢幕像素）、`client {x,y,w,h}`（GetClientRect + ClientToScreen，螢幕像素）和 `client_offset {x,y}`（用戶區左上角減去邊框左上角）。`--region` 模式下這三個欄位與兩條公式都不會出現。帶標題列視窗的真實輸出：

```
"maps_to":{"x":307,"y":270,"w":300,"h":200},"scale":1.0,
"formula":"screen_x = 307 + px / 1.0; screen_y = 270 + py / 1.0",
"window":{"frame":{"x":207,"y":150,"w":642,"h":452},"client":{"x":208,"y":181,"w":640,"h":420},"client_offset":{"x":1,"y":31}},
"formula_window":"win_x = 100 + px / 1.0; win_y = 120 + py / 1.0",
"formula_client":"client_x = 99 + px / 1.0; client_y = 89 + py / 1.0"
```

影像上的點 (50, 40) 對應螢幕 (307+50, 270+40) = (357, 310)、視窗 (150, 160)、用戶區 (149, 129)。偏移量是 `maps_to` 減去邊框或用戶區原點，因此搭配 `--crop`、`--scale`、`--max-edge` 以及彈出視窗後備時依然正確。

**`--crop`。** `--crop x,y,w,h` 相對於 DWM 邊框左上角（即 `formula_window` 的原點），單位為實體像素，所以無論視窗在哪裡，選到的都是視窗的同一部分，不必先把視窗移到固定位置。它先於 `--scale` / `--max-edge` 套用：`--crop 100,120,300,200 --max-edge 0` 輸出的恰好是 300×200 像素，與從完整 1:1 擷取裡切出同一矩形的結果逐像素相同。裁切範圍超出邊框會回傳 `INVALID_ARGS`，`hint` 給出視窗大小。彈出視窗與裁切範圍重疊時，螢幕擷取後備使用的恰好是換算後的裁切矩形，所以意義仍是視窗的同一部分。

**`dpi_scale` 與 `monitor_dpi_scale`。** `dpi_scale` 是視窗本身的 DPI / 96；`monitor_dpi_scale` 是視窗所在螢幕的縮放。不支援 DPI 的舊程式在 150% 螢幕上會回報 `1.0`，因為 Windows 用點陣圖把它拉伸。兩者不同時，`shot` 會發出 `window_dpi_unaware_or_virtualized` 警告。公式依然是實體像素，但要確認你的輸入工具也使用實體像素。`--region` 模式下，兩個值都取自區域左上角所在的螢幕。

**彈出視窗。** 視窗擷取（Windows Graphics Capture）抓不到選單、下拉清單和工具提示，因為它們是獨立的頂層視窗。若同一程序有可見的彈出視窗與目標重疊（由目標擁有，或類別為 `#32768`、`tooltips_class32`、`ComboLBox`、`Xaml_WindowedPopupClass`、`Microsoft.UI.Content.PopupWindowSiteBridge`），`shot` 會改為擷取涵蓋視窗與彈出視窗的螢幕區域，加上 `popups_included_via_screen_capture` 警告，在 `data.popups` 列出它們（`hwnd`、`class`、`via`），並把 `maps_to` 設為兩者的聯集，所有公式都已經算進去（使用 `--crop` 時區域就是裁切矩形）。此模式下，蓋在這塊區域上的其他程式也會出現在影像中。

## 輸出約定

每個指令都只在 stdout 輸出一行 JSON。

```json
{"v":1,"ok":true,"data":{},"warnings":[]}
{"v":1,"ok":false,"error":{"code":"TARGET_NOT_FOUND","message":"no visible window found for pid 99999999","hint":"run `sight list` to see available targets"}}
```

先看 `ok`，再看 `warnings`。`hint` 是寫給代理看的，直接指出下一步。失敗時也可能帶有 `data`：`--settle` 或 `--expect-change` 造成的 `TIMEOUT` 仍會存下最後一幀，可以直接使用：

```
> sight shot --hwnd 15603054 --settle 300 --timeout 200
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"image did not settle within --timeout 200ms (saved the latest frame anyway)","hint":"increase --timeout or --settle"},"data":{"path":"...\\shot-20261123-214420-128.jpg",...,"settled":false,"elapsed_ms":365,"popups":[]}}
```

| 錯誤碼 | 結束代碼 | 意義 | `hint` |
|---|---|---|---|
| （成功） | 0 | | |
| `INVALID_ARGS` | 1 | 參數錯誤或互相衝突（含解析錯誤）、`--crop` 與 `--region` 併用、`--crop` 超出視窗、`--baseline` 讀不出、目標或 `--crop` 不同，或是大小不同的外來圖片 | run `sight --help` for usage（`--crop` 出錯時 `hint` 給出視窗大小） |
| `TARGET_NOT_FOUND` | 2 | 找不到該 pid / hwnd / 可見視窗 | run `sight list` to see available targets |
| `WINDOW_MINIMIZED` | 3 | 目標視窗已最小化 | restore the window and retry |
| `WINDOW_PROTECTED` | 3 | 視窗禁止被擷取，不要重試 | the window opted out of capture (display affinity) |
| `PERMISSION_DENIED` | 4 | 無法讀取該程序（例如它以系統管理員身分執行） | run with sufficient privileges for the target process |
| `TIMEOUT` | 5 | 沒拿到畫面、畫面沒能及時穩定，或用 `--expect-change` / `--change` 時沒看到變化（`data.changed:false`） | increase --timeout or --settle; with --expect-change/--change, make sure the action really changes pixels in the target |
| `INTERNAL` | 6 | 非預期的失敗（擷取、IO、系統資訊） | run `sight doctor` to check the environment |

