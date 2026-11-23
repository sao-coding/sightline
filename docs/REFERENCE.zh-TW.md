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
| `--scale <F>` | `1.0` | 縮放倍率，範圍 (0, 16]。大於 1 時用最近鄰內插，並輸出 PNG |
| `--max-edge <N>` | `1568` | 縮小到長邊不超過 N 像素（`0` 表示不限制） |
| `--settle <ms>` | 關閉 | 等畫面持續這麼久沒有變動（受 `--timeout` 限制） |
| `--timeout <ms>` | `5000` | 擷取的總逾時 |
| `--out <path>` | 暫存資料夾 | 副檔名 `.png` / `.jpg` / `.jpeg` 決定格式。預設 `%TEMP%\sightline\shot-<時間戳記>.jpg`（`--scale` > 1 時為 PNG） |

JPEG 品質為 80。視窗擷取（`--pid` / `--hwnd`）另外會回傳 `popups`（可能為空）。

警告：`other_candidate_window: ...`、`popups_included_via_screen_capture`、`region_partially_offscreen`、`region_spans_mixed_dpi`、`window_dpi_unaware_or_virtualized`。

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

**`dpi_scale` 與 `monitor_dpi_scale`。** `dpi_scale` 是視窗本身的 DPI / 96；`monitor_dpi_scale` 是視窗所在螢幕的縮放。不支援 DPI 的舊程式在 150% 螢幕上會回報 `1.0`，因為 Windows 用點陣圖把它拉伸。兩者不同時，`shot` 會發出 `window_dpi_unaware_or_virtualized` 警告。公式依然是實體像素，但要確認你的輸入工具也使用實體像素。`--region` 模式下，兩個值都取自區域左上角所在的螢幕。

**彈出視窗。** 視窗擷取（Windows Graphics Capture）抓不到選單、下拉清單和工具提示，因為它們是獨立的頂層視窗。若同一程序有可見的彈出視窗與目標重疊（由目標擁有，或類別為 `#32768`、`tooltips_class32`、`ComboLBox`、`Xaml_WindowedPopupClass`、`Microsoft.UI.Content.PopupWindowSiteBridge`），`shot` 會改為擷取涵蓋視窗與彈出視窗的螢幕區域，加上 `popups_included_via_screen_capture` 警告，在 `data.popups` 列出它們（`hwnd`、`class`、`via`），並把 `maps_to` 設為兩者的聯集，`formula` 已經算進去。此模式下，蓋在這塊區域上的其他程式也會出現在影像中。

## 輸出約定

每個指令都只在 stdout 輸出一行 JSON。

```json
{"v":1,"ok":true,"data":{},"warnings":[]}
{"v":1,"ok":false,"error":{"code":"TARGET_NOT_FOUND","message":"no visible window found for pid 99999999","hint":"run `sight list` to see available targets"}}
```

先看 `ok`，再看 `warnings`。`hint` 是寫給代理看的，直接指出下一步。失敗時也可能帶有 `data`：`--settle` 造成的 `TIMEOUT` 仍會存下最後一幀，可以直接使用：

```
> sight shot --hwnd 15603054 --settle 300 --timeout 200
{"v":1,"ok":false,"error":{"code":"TIMEOUT","message":"image did not settle within --timeout 200ms (saved the latest frame anyway)","hint":"increase --timeout or --settle"},"data":{"path":"...\\shot-20261123-214420-128.jpg",...,"settled":false,"elapsed_ms":365,"popups":[]}}
```

| 錯誤碼 | 結束代碼 | 意義 | `hint` |
|---|---|---|---|
| （成功） | 0 | | |
| `INVALID_ARGS` | 1 | 參數錯誤或互相衝突（含解析錯誤） | run `sight --help` for usage |
| `TARGET_NOT_FOUND` | 2 | 找不到該 pid / hwnd / 可見視窗 | run `sight list` to see available targets |
| `WINDOW_MINIMIZED` | 3 | 目標視窗已最小化 | restore the window and retry |
| `WINDOW_PROTECTED` | 3 | 視窗禁止被擷取，不要重試 | the window opted out of capture (display affinity) |
| `PERMISSION_DENIED` | 4 | 無法讀取該程序（例如它以系統管理員身分執行） | run with sufficient privileges for the target process |
| `TIMEOUT` | 5 | 沒拿到畫面，或畫面沒能及時穩定 | increase --timeout or --settle |
| `INTERNAL` | 6 | 非預期的失敗（擷取、IO、系統資訊） | run `sight doctor` to check the environment |

