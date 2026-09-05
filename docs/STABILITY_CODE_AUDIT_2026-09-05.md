# fabDev 穩定性、結構與程式碼查驗

日期：2026-09-05（Asia/Taipei）
查驗基線：`main`／`fffd9ac`，App `0.1.22`、Agent Protocol `38`

## 範圍與結論

依 Repository Owner 要求，維持現在的功能與操作方式，只修正能重現的既有缺陷。此次檢視 Desktop Store／Tauri IPC、Agent 請求與停止流程、Site 驗證、Runtime 安裝與回復、Updater、Proxy、LAN Share，以及現有測試與架構文件；這是重點路徑查驗，不代表全專案逐行審計、完整壓力測試或兩平台實機驗收。

初輪修正 LAN Share 白名單繞過、停止後殘留既有連線、Site 接受一般檔案作為 document root 三項缺陷。Repository Owner 後續要求「開始處理穩定性」，再修正停滯下載取消、停止流程的錯誤清理及前端輪詢競態，並要求注意保留原有功能。後續追查再修正 Runtime 取消後立即重試的清理競態、任務歷史清理誤刪安裝記錄、LAN Share 停止失敗後的殘留狀態，接續修正 Runtime 移除偏好回滾、MariaDB 回滾前停止保護、Catalog 換包後任務誤用、重疊線上安裝、版本切換接受目錄連結及終端整合驗證失敗仍寫入啟動檔、Proxy 完成任務未回收、背景 task 結束後無法重試及設定保存部分寫入及 Site 刪除失敗回復卡住，以及四個 Site 清單讀取失敗分支鎖死，共二十項。主要缺陷皆先以針對性測試確認失敗，再修正與驗證；大型結構調整與平台功能差異只記錄，沒有擴充功能。

## 已重現並修正

### A1／P1：LAN Share 只檢查第一個 HTTP 請求

- 原程式在 TCP 連線開始時檢查一次 Host，接著直接執行 `copy_bidirectional`；後續請求不再經過白名單。
- 重現：先請求已分享的 `demo.test`，再於同一 TCP 連線請求未分享的 `private.test`。舊程式將第二個請求送到上游，測試收到 200，而非預期的 403。
- 同樣可重現「從白名單移除 Site 後，舊 keep-alive 連線仍可繼續請求」，以及 absolute-form URL 指向未分享 Site、Host 卻填入已分享 Site 的情況。
- 修正：[restricted_http.rs](../crates/share/src/restricted_http.rs) 在每個 HTTP 請求轉送前讀取目前白名單，並核對 URL authority 與 Host。一條連線中的 HTTP 訊息由既有 workspace 使用的 Hyper 解析，避免把後續請求混在原始 TCP 串流裡放行。
- 維持正常 keep-alive、合法 absolute-form URL、POST 串流、回應串流與 HTTP Upgrade。已完成的 Upgrade 仍以串流傳輸；本輪不新增對既有長連線的逐幀授權或強制撤銷規則。
- 驗證：[http-boundary.rs](../crates/share/tests/http-boundary.rs)。三個拒絕案例於修正前失敗；修正後連同三個正常相容案例共六項通過。

### A2／P1：停止 LAN Share 沒有停止既有連線

- 原程式使用未保存 Handle 的 `tokio::spawn` 處理各連線。停止分享會結束 accept loop，但已建立的 client／upstream 連線繼續存活。
- 重現：建立雙端連線並成功傳送 `ping` 後，呼叫 `stop()` 或 Drop ShareServer；兩端在兩秒內都沒有關閉，兩項測試失敗。
- 修正：[ShareServer](../crates/share/src/lib.rs) 使用 `JoinSet` 保存連線任務，停止時中止並等待清理；Drop 也會連帶取消任務。HTTP 上游 driver 歸屬於各連線，避免新增另一層無人管理的背景任務。
- 驗證：[lifecycle.rs](../crates/share/tests/lifecycle.rs) 的 Stop／Drop 兩項回歸，以及 Upgrade 後停止的雙端清理。確認既有連線關閉，分享埠可重新綁定。

### A3／P2：document root 可以是一般檔案

- 原程式只驗證自訂 document root 存在且位於專案內，沒有確認它是目錄。
- 重現：傳入相對路徑 `public/index.php` 或該檔案的絕對路徑，都會被 `create_site` 接受；後續 Nginx 無法將一般檔案當成網站根目錄使用。
- 修正：[Site 驗證](../crates/core/src/site.rs) 在 canonicalize 與原本的專案範圍檢查後，加上目錄型別檢查，沿用既有 Agent 錯誤回應流程。
- 驗證：`rejects_a_file_as_the_document_root` 修正前失敗、修正後通過；Core 39 項測試通過。
- 不變動 document root 自動偵測、既有資料、Site Home 或 symlink 規則。

### A4／P2：下載停滯時無法及時取消

- 既有 Windows App 分段下載及 Runtime 下載在等待 response／下一個 chunk 時不檢查取消。以 loopback fixture 暫停回應標頭或 body，原始程式在兩秒內沒有結束；Windows 的 Range 探測也可重現。
- 修正：[cancellation.rs](../crates/updater/src/cancellation.rs) 在等待期間每 50 毫秒檢查既有取消旗標；Windows 的 Manifest 取得與下載傳輸、Runtime 的 response／body 等待都能中斷。驗證後的檔案最終 rename 保持原有流程。
- 取消後先讓下載 future 結束／釋放連線，再移除本次 partial 與 Windows resume segment；一般斷線仍保留原有續傳行為。沒有替 macOS App 新增取消介面，也沒有更改 URL、Catalog、並行分段數、重試次數或 SHA-256 規則。
- 回歸包含 Range 探測停滯、兩類下載的 headers／body 停滯。body 測試必須等第一個 byte 寫入檔案才取消，驗證連線關閉、暫存清除，接著在相同路徑重新下載並比對完整內容。
- 相容性驗證：Windows 已完成分段續傳與 SHA-256、Runtime 斷線 Range 續傳、伺服器忽略 Range 時安全重啟、快取驗證均通過；取消前不啟動 operation、未取消時保留原成功／失敗結果也有測試。

### A5／P2：LAN Share 停止失敗會中斷後續服務清理

- 原始 Agent 先以 `?` 停止 LAN Share；一旦失敗，Stop All 不會繼續清理 Web services，Shutdown 也不會進行後續 Proxy／Web／MariaDB 清理。
- 修正：[Agent 停止流程](../crates/agent/src/main.rs) 仍依原順序停止，先保存 Share 錯誤、繼續原有服務清理，最後彙整錯誤回報。Stop All 與 Shutdown 原本不同的服務範圍保持不變；MariaDB 啟停偏好仍由原有服務方法管理。
- 回歸驗證 Share 單獨失敗、Share 與 service 同時失敗、service 單獨失敗及正常成功，確認後續清理一定執行且錯誤不遺失。修正前前兩個案例失敗，修正後通過。
- 完整 Services 測試另確認 untracked managed DNS fixture 的實際子程序會被停止，並維持停止 MariaDB 時 Web services 繼續運行等原有契約。

### A6／P2：前端輪詢重疊及舊回應覆寫最新狀態

- 原 Store 允許同類背景輪詢持續累積；舊 `getStatus` 回應可在 Stop All 完成後將畫面改回 running，舊請求失敗可覆寫新的連線狀態；兩個前景請求重疊時，先完成的一個會提前解除 `busy`。Proxy 舊查詢同樣可能覆寫操作後的結果。
- 修正：[RequestGate](../apps/desktop/src/utils/request-gate.ts) 為每個 Store 實例分別合併尚未完成的 Status／Proxy 查詢，僅套用目前有效請求的結果；前景操作使舊 Status 查詢失效，Proxy 操作成功使舊查詢失效。`busy` 以尚未結束的前景操作數計算，避免巢狀 refresh 提前解除。
- 保持原有 Agent Command、UI、輪詢間隔、服務操作與錯誤顯示方式。沒有移動 Store 的領域邏輯或更動資料契約。
- [Store 回歸測試](../apps/desktop/src/stores/fabdev.test.ts) 使用真實 Pinia Store 與可控制完成順序的 Tauri mock，涵蓋查詢合併、Stop 後舊回應、舊錯誤、重疊 busy、Proxy 操作結果、busy 期間不輪詢、失敗後恢復，以及舊 Promise 不得釋放新請求。最初五項在原程式失敗；最終八項皆通過。

### A7／P2：Runtime 取消後立即重試可能與舊任務清理衝突

- Agent 原本在收到取消時立刻將公開狀態改為 Cancelled，但實際 downloader／檔案清理仍在背景進行；同版重試只排除 Queued／Downloading，因此可能在舊 task 清理共享 partial／verified cache 前先啟動新下載。
- 以受控的未完成清理狀態測試 `RuntimeUpdateManager::start`，原程式立即接受同版重試；修正後同版必須等待原 worker 結束。這補的是 Agent 任務邊界；A4 測試已驗證 downloader 完整返回之後的取消／重試。
- 修正以內部 completion signal 追蹤 worker 真正結束，正常返回、取消前尚未開始、task 被丟棄時都釋放等待者；等待期間不持有全域 operations lock，其他 Runtime 版本仍能啟動。同時把取消與下載完成的狀態判斷放進同一把鎖，避免取消結果又被 Verified 覆寫。
- 公開 Cancelled 回應、Protocol、下載路徑與快取格式保持不變。回歸涵蓋同版等待、清理後可重試、其他版本不受阻塞、worker 失敗後可再次嘗試及 task abort 釋放等待者。
- 此處 Agent 重試測試使用受控 worker 狀態與缺少 Catalog 的失敗路徑，沒有把它描述為公開網路下載；真正下載、續傳與取消後成功重試由 A4 的 loopback fixture 驗證。

### A8／P2：清理 Runtime 歷史會刪除待安裝／安裝中的任務

- operations 達 64 筆時，原程式只保留 Queued／Downloading；Verified 與 Installing 一起被移除，後續查詢或安裝完成回報會找不到 operation。
- 修正只淘汰已結束且可清理的記錄，保留 Verified、Installing 及尚未完成實際清理的取消 task；原有 64 筆上限仍保留，避免無限制增長。
- 以 64 筆包含待安裝／安裝中記錄的 fixture 重現原程式失敗；修正後兩種記錄可繼續查詢。另確認取消中 worker 不會被歷史清理移除，以維持 A7 的同版重試等待。

### A9／P2：LAN Share 停止錯誤後留下分享狀態

- `LanShareState::stop` 原本先取走 server，再以 `?` 等待停止；若失敗便略過 `info = None`。此時已沒有 server，卻仍保留 Site／Port 資訊，後續 start 可能誤認分享仍在運行。
- 修正保留停止錯誤回報，同時清除已取走 server 的分享資訊；不改原有分享網站、埠或啟停操作。
- 受控停止失敗測試在原程式失敗，修正後清除 info 且再次 Stop 可正常完成；另使用真實 loopback ShareServer 經 Agent 狀態層停止，確認 server／info 清空及 listener 可重新綁定。

### A10／P2：Runtime 安裝回滾沒有恢復原本的移除偏好

- `install_or_replace_tar_gz_with_health_check` 在返回待確認的 transaction 前清除 `.removed` 記錄；後續 Agent 驗證／設定套用失敗時雖回滾 Runtime 與 current，卻沒有恢復這筆記錄。
- 回歸測試確認：原本明確移除 PHP 8.2.33，再安裝並回滾後，`is_runtime_marked_removed` 由 true 變成 false。依 Desktop 的 `should_install_bundled_runtime` 邏輯，缺少目錄又沒有移除標記時，重啟可能再次補裝原本不想安裝的 bundled Runtime；本輪未操作正式 App 重啟重現。
- 修正：transaction 保存既有移除 marker 的路徑與原始內容，回滾恢復 Runtime／current 後一併還原。成功 commit 仍清除 marker，維持明確重新安裝的原有行為。
- 新增三項測試，涵蓋有／無移除標記、安裝是否切換 current、健康檢查失敗、成功 commit。確認舊版本檔案、current、測試設定與移除偏好保留，新版本與 staging 清理；原本同版本 Package 替換／回滾測試亦通過。
- fixture 只在暫存目錄生成含文字檔的測試 archive，沒有重建、更新或發布任何產品 Runtime Package／Catalog。

### A11／P2：MariaDB 更新回復可能在新程序仍運行時替換 Runtime

- `restart_active_mariadb_runtime` 先啟動 MariaDB，再套用 PHP 連線設定。後一步失敗時 MariaDB 可能仍在運行，原更新錯誤分支卻直接還原 Runtime 目錄，可能遇到 Windows 檔案占用或再次啟動時已運行的錯誤。
- 修正：失敗回復固定先呼叫既有 `stop_mariadb`，停止成功才回滾檔案並重啟舊 Runtime。停止失敗時保留現有 Runtime 與 backup、回報原啟動／設定錯誤及停止原因；回滾失敗時也不嘗試從未完成回復的目錄啟動。
- 使用受控的運行狀態及失敗步驟重現原順序的三項測試失敗，修正後通過：停止後才替換、停止失敗不替換／重啟、回滾失敗不重啟。沒有藉由實際破壞資料庫或 Windows 執行檔製造錯誤。
- 沿用不改寫 MariaDB desired state 的 stop／start 方法，未修改 root 密碼、資料目錄或連線來源契約。擴充 Services 回歸，讓 PHP 8.2／8.4 未啟用版本的設定經過 Managed Socket → 停止 → 保存的 System Socket，確認 mysqli 與 PDO 預設 Socket 同步切換。
- 尚未實機驗證 MariaDB Runtime 更新與失敗回復，也未驗證跨 MariaDB 版本的資料格式回復；以上是程序順序、檔案交易及 Socket 設定的自動測試。

### A12／P2：Catalog 同版本換包後可能沿用舊下載任務安裝另一個套件

- Agent 以名稱、版本與平台重新查 Catalog／快取，但原本未比較回傳套件與下載任務的 SHA-256、大小、檔名。Catalog 同版替換後，只要新套件已存在有效快取，舊任務也能通過安裝前檢查；下載開始與實際讀取 Catalog 之間換包時，也可能把不相符的任務標為 Verified。
- 修正：下載完成與安裝前都核對名稱、版本、平台、架構、檔名、大小及 SHA-256。不同套件回報失敗並要求重新整理／下載；套件未變而僅 Catalog sequence 增加，仍可正常使用。
- PHP、Node.js、MariaDB 畫面共用任務比對函式，避免同版本不同套件誤顯示為已下載；保留進行中任務的進度與取消入口。既有安裝確認視窗改從實際下載任務取得版本、大小與 SHA-256，沒有增加操作或修改版面。
- 三項 Agent 測試使用隔離 Catalog／快取 fixture，覆蓋兩個拒絕案例及內容未變時的成功路徑；前端四項測試覆蓋套件一致性與進行中任務相容性。兩個 Agent 拒絕案例及前端換包關聯案例皆先確認原程式失敗，再修正通過。
- 沒有修改或下載公開 Catalog／產品 Runtime Package，也沒有用測試 payload 執行實際安裝。前端驗證為資料函式測試及 typecheck，尚未做 Desktop 確認視窗人工驗收。

### A13／P2：兩個線上 Runtime 任務可以同時進入安裝階段

- 原 `RuntimeUpdateManager::begin_install` 只檢查單一任務，兩個 Verified 任務都能成為 Installing。不同線上安裝會使用共用 Runtime 目錄或服務設定；重疊執行可能干擾 current、備份與設定套用。
- 修正：在同一把 operations lock 內檢查是否已有 Installing，再切換目標狀態。忙碌時沿用既有錯誤回應，另一個任務保持 Verified，可在前一個完成或失敗後重試；未新增佇列或改變下載並行功能。
- 三項回歸覆蓋 PHP／MariaDB／Node.js 任務互斥、同時請求只能接受一個、前次完成或失敗後可安裝。原程式的前兩類案例失敗，修正後皆通過。
- 此處驗證的是線上安裝任務的准入與狀態，不宣稱已重現真實檔案損壞，也不代表所有手動匯入／移除命令已完成並行壓力驗收。

### A14／P2：版本切換接受 current 或目錄連結，可能破壞全域 Runtime 選擇

- CLI 的 `SetGlobalPhp`／`SetGlobalNode` 接受版本字串，底層 `set_active_version` 原本使用會跟隨 symlink 的 `is_dir()`。Unix 傳入 `current` 時，既有有效連結被改為 `current -> current`；也能選到不在安裝版本列表中的目錄別名或外部目錄連結。
- 修正：切換前以 `symlink_metadata` 確認目標是實體版本目錄，與既有 `list_installed_versions`／`remove_installed_version` 的目錄判定一致；沿用 NotInstalled 錯誤，不改 CLI／Protocol 或正常版本切換操作。
- 暫存 fixture 在原程式重現接受 `current`、內部 alias 與外部目錄連結；修正後拒絕四種連結（含斷裂連結）且 current 保留。另一項跨平台測試涵蓋一般檔案、缺少版本、路徑越界及後續正常版本切換。
- 新增兩項 Runtime 回歸；只操作測試暫存目錄，沒有更動使用者的 Runtime、PATH、shell profile 或正式 App。Unix symlink 案例於 macOS 執行，Windows 分支未做實機驗收。

### A15／P2：macOS 終端整合設定無效時仍會新增或覆寫啟動檔

- PHP／Node.js 的 enable 流程原本先寫入 bin 內的 shim，再讀取及驗證 `.zprofile`／`.zshrc`。設定含不完整的 fabDev 管理區塊時，既有驗證會回報錯誤，但 shim 已被改動；若共享 bin 已在 PATH 中，這些檔案仍可能影響命令解析。
- 修正只調整順序：先讀取設定、驗證管理區塊與產生預定內容，成功後才建立 bin 並寫入 shim。沿用原本的啟用、修復、停用、PATH 與設定內容規則，沒有增加功能或變更 Windows 實作。
- 兩項新增回歸皆先在原程式失敗；修正後覆蓋 profile／rc 設定錯誤，以及 shim 原本不存在／已存在的四種組合，確認設定內容不變且不新增或覆寫 shim。既有正常啟用、修復及停用測試一併通過。
- 測試完全使用暫存目錄，不讀寫使用者真正的 shell profile、PATH 或 Herd 設定。此修正處理寫入前的設定驗證失敗；不宣稱已處理所有寫入途中 I/O 失敗的跨檔回滾或程序突然終止。

### A16／P2：Proxy 運行期間沒有回收已完成的連線任務

- Proxy accept loop 將每條連線加入 JoinSet，但原本只在 Stop 的 drain 階段呼叫 `join_next`。連線已關閉時，完成的 task 記錄仍持續留在集合中，長時間運行會累積。
- 修正：在既有 select loop 加入非空集合的 `join_next` 分支，運行期間即回收完成記錄；保留原有 listener、健康檢查、HTTP／CORS、串流與停止等待時間。
- 新增 loopback 回歸，連續三次建立／關閉 client，確認每次任務數回到零且 listener 仍運行。原程式兩秒內未清理，修正後通過；停止後埠可再次綁定。
- 任務數透過僅在 `cfg(test)` 編譯的觀測欄位讀取；本輪未量測長時間 RSS 或壓力負載，不把集合殘留描述為仍有活躍 Socket。

### A17／P2：Proxy 背景任務結束後，啟動重試仍被忽略

- 狀態查詢已能將結束的背景 task 標為 Failed，但 `start` 原本只檢查 running map 有無記錄，直接返回，無法由啟動操作恢復 listener。
- 修正：仍在執行的 task 保持原本冪等行為；已結束的 task 先經既有 stop 清理記錄，再依原流程重新綁定及啟動。不新增自動重啟或重試排程。
- 回歸以受控 abort 模擬背景任務意外結束，原程式重試後仍為 Failed；修正後恢復 Running、可建立 TCP 連線，Stop 後釋放埠。另新增正常 task 重複啟動案例，確認維持同一 task 狀態，既有 client 仍收到 HTTP 204。
- Proxy 全模組測試同時驗證 HTTP 上游轉送、Host／CORS／Cookie、response timeout、串流 body 與 Stop 釋放 listener。本輪沒有重現自然發生的背景 panic，也未操作使用者正式 Proxy 或做 Windows 實機驗收。

### A18／P2：Proxy 設定與啟動清單分開保存，失敗時可能留下部分寫入

- Agent 的 Proxy 編輯／移除流程先保存 connections，再保存 running IDs。兩次寫入各自提交；第二次失敗時第一筆已變更，原流程仰賴後續再次寫回補救，但補救寫入也可能失敗。
- 修正：Core 新增 `save_proxy_state`，沿用既有兩個序列化／排序方法，在單一 SQLite transaction 內一起保存。Agent 的編輯、移除及對應回復寫入改用這個方法。保留資料表、key、JSON／清單格式與既有回應契約。
- 回歸使用記憶體 SQLite trigger，刻意拒絕第二筆寫入。原有順序留下 replacement connections，測試失敗；交易修正後保留原本兩筆資料，涵蓋初始空資料與已保存資料，移除失敗條件後仍可重試並保存空清單。
- 另一項成功路徑確認連線依埠排序、running IDs 排序與去重維持不變。Core 41 項測試通過；沒有操作正式資料庫，也未把 SQL 失敗注入測試描述為磁碟故障、斷電或兩平台實機驗收。

### A19／P2：Site 刪除失敗後，回復流程重複取得服務鎖而卡住

- `RemoveSite` 在 `if let Err(...) = state.services.lock()...remove_site_config().await` 中處理失敗。該鎖的暫存 guard 在錯誤分支仍存活；恢復 registry 後再次取得 services lock 同步網域，因此等待自己釋放鎖，請求無法完成。
- 修正只將設定刪除結果先存入區域變數，讓 guard 在進入錯誤回復前釋放。維持原有設定刪除、registry 回復、網域同步及錯誤回應內容，不改刪除功能或資料契約。
- 回歸直接呼叫 Agent handler，以隔離 Site 的 `.conf` 路徑為目錄製造真實檔案讀取失敗。原程式兩秒內沒有返回；修正後回報 Error、Site 記錄保留且 GetStatus 可正常完成。移除測試阻擋條件後再次刪除成功，設定檔及 registry 記錄清除。
- 此測試於 Unix 使用暫存目錄與記憶體 SQLite，沒有啟動正式服務或刪除使用者 Site，也未執行 Windows Helper／實機流程。驗證的是失敗回復可結束及後續請求可用，不宣稱所有錯誤分支都已完成審計。

### A20／P2：四個 Site 清單讀取失敗分支在回復時重複取得資料庫鎖

- 新增、編輯、刪除與切換 PHP 在資料更新後，以 `match state.sites.lock().await.list()` 讀取清單。讀取失敗時，暫存 guard 仍持有 sites lock，分支又取得同一把鎖嘗試回復，導致請求卡住。
- 修正四處相同模式：先保存清單讀取結果，釋放 guard，再進入成功／錯誤分支。未更改 SQL、資料格式、Site 功能、回復操作或 Agent Protocol。
- 新增一項 Agent 回歸，逐一測試四種操作；在隔離 SQLite 以 trigger 讓操作後清單含無效 PHP 欄位，確定走到指定讀取錯誤。原程式四種操作皆逾時；修正後皆返回對應資料解析錯誤。測試移除注入條件後確認 repository 可重新讀取，最後刪除暫存資料。
- Agent 的 dev-dependency 加入 workspace 已有且已鎖版的 rusqlite，只供測試建立 SQLite trigger；沒有新增套件版本或產品執行期依賴。
- 這裡驗證的是錯誤分支不再鎖死，不是損壞資料庫的自動修復。既有回復操作若也無法讀取異常資料，仍可能失敗；本輪不擴充資料修復功能，不宣稱資料損壞時一定回復成功。未操作正式資料庫，也未做 Windows 實機驗收。

## 項目結構評估

目前 monorepo 的大方向恰當：`apps/desktop` 負責 Desktop，`crates` 區分 Agent、Core、Services、Runtime、Updater、Proxy、Share，`packages/contracts` 保存前端契約，`helpers` 承擔特權操作。沒有必要為本輪缺陷修正重建目錄結構。

較大的維護問題是檔案內部的責任集中，而非頂層目錄配置：

| 檔案 | 查驗基線總行數 | 維護風險與後續建議 |
| --- | ---: | --- |
| `crates/services/src/lib.rs` | 5,729 | PHP、MariaDB、程序、設定產生、日誌與平台條件交錯；後續可依現有職責拆成內部模組，保持 public API。 |
| `crates/agent/src/main.rs` | 4,424 | 啟動、IPC dispatch、Runtime operation、Site 同步及回復流程集中；後續可先分離 request handlers 與 operation 狀態管理。 |
| `apps/desktop/src-tauri/src/lib.rs` | 2,540 | IPC、App 更新、Runtime 準備、退出與 tray 功能集中；後續可依 Tauri Command 群組分檔。 |
| `apps/desktop/src/stores/fabdev.ts` | 1,096 | 多個領域共用 Store；查驗時測試主要針對 utils，Store 請求競態與錯誤回復缺少直接測試；後續 A6 已補上。 |

行數含測試，不直接等同程式品質；例如 Services 的主要測試模組自第 3,473 行開始。需要關注的是責任交錯與修改影響範圍。Services 仍有大量 Unix／Windows 分支，平台差異尚未完全收斂至 Platform／Helper。

以上只提出維護建議，本輪不改動既有架構、不拆 Store，也不搬移服務管理邏輯。新增的產品內部模組限於 LAN Share HTTP 驗證／轉送、下載取消等待與 Store 請求管理，用於上述缺陷；沒有更動領域架構。

## 記錄項目

| 項目 | 證據與狀態 |
| --- | --- |
| macOS App 更新取消差異 | `SettingsView.vue:22` 與 Tauri `cancel_app_update_download` 明確只開放 Windows。依「保持現在功能」指示只記錄，沒有替 macOS 新增取消功能。 |
| 發布進度文件不一致 | 後續已由 GitHub Releases 確認 `v0.1.22` 為 Latest Stable，並同步修正 `FABDEV_PROGRESS.md`、架構文件、發布規格首頁及兩項已完成的 `0.1.22` 候選待辦；此項不再是待處理問題。 |

## 驗證結果

最後一輪相容性複核沒有再確認新的缺陷，也未修改產品邏輯。新增 Unix Agent 流程回歸，依序新增靜態 Site、編輯名稱／網域、嘗試切換至未安裝的 PHP、查詢狀態及刪除 Site；確認 PHP 切換失敗後 registry 與 Nginx Site 設定逐位元保持原狀，成功刪除後專案的 `index.html` 內容仍保留。fixture 使用記憶體 SQLite 與暫存路徑，沒有啟動 Nginx／PHP 或操作正式 Site；這是 Agent／設定檔層的相容性驗證，不代表瀏覽器或兩平台實機驗收。

| 檢查 | 結果 |
| --- | --- |
| `pnpm test` | 完整命令成功：Desktop 88、Release 規則 18、Rust 281、macOS Helper 9 項通過；7 項 Rust 測試預設 ignored（含需外部 PHP binary 的 Share 流程）。 |
| `pnpm lint` | 完整命令成功：TypeScript typecheck、rustfmt、workspace Clippy、macOS Helper lint 通過。 |
| Updater 測試 | `cargo test -p fabdev-updater`：29 項通過、3 項既有外部下載測試 ignored。涵蓋既有下載／續傳／校驗與新增停滯取消測試；後續修正包含 Agent 任務管理與 Runtime 安裝失敗回復，下載成功／續傳行為未變。 |
| Proxy 回歸 | `cargo test -p fabdev-proxy`：12 項通過，包含新增的連線清理、背景 task 結束後重試及正常重複啟動三項回歸。 |
| Agent 回歸 | `cargo test -p fabdev-agent`：47 項通過、2 項需要外部 Runtime Archive 的測試 ignored；包含任務管理新增 8 項、MariaDB 回復新增 3 項、套件一致性新增 3 項、安裝互斥新增 3 項及 Site 刪除失敗回復新增 1 項，以及清單讀取失敗新增 1 項（含四種操作）及正常 Site 操作／PHP 切換拒絕新增 1 項回歸。 |
| Windows x64 CI | 修正推送後由既有 push trigger 自動執行 Run [`33955789378`](https://github.com/JimmyWon1028/fabdev/actions/runs/33955789378)；MSVC、前端測試、Windows 發布契約、Rust workspace、fabDev Connect、Unsigned NSIS 與 Artifact 上傳皆成功。這是 Commit `75e09cc` 的 CI／封裝靜態驗證，不是 Windows 實機啟動或 Release 驗收。 |
| 真實 PHP 流程 | [php-flow.rs](../crates/share/tests/php-flow.rs) 另行執行通過，使用既有 fabDev PHP 8.2.33 binary、`-n`、PHP 內建 HTTP server、獨立暫存目錄及隨機 loopback 埠，完成 Share Start → PHP 執行回應 → 未分享 Host 拒絕 → Stop。PHP 子程序已等待結束，兩個 listener 可重新綁定，暫存目錄已移除。 |
| Desktop UI 導覽 | 以目前工作區另開 `127.0.0.1:1421` 隔離 Vite 預覽，逐頁確認總覽、Sites、PHP 設定、Proxy 與設定可正常導覽及渲染；確認 Agent 狀態位於設定下方、總覽服務範圍說明及 Proxy 頂部操作分組生效。純瀏覽器預覽無 Tauri IPC，因此預期顯示 Agent 未連線與 `invoke` 錯誤，不計為產品缺陷。原生 Tauri smoke 因電腦上已有相同 App 識別碼與開發服務埠，無法可靠區分新舊視窗；隔離資料目錄自動啟動的 PHP-FPM 已正常停止，相關測試程序與暫存資料均已清除。 |
| `git diff --check` | 通過。 |

最終全測試與另行執行的 PHP 測試分開計數，不把 ignored 當成已通過。Frontend 的 Store 測試是受控非同步測試，不宣稱等同 Desktop 實機互動驗收。

初輪測試曾遇到沙箱禁止 loopback 與舊 Runtime 測試 binary 的 `CARGO_MANIFEST_DIR` 指向已刪除 worktree；調整測試執行環境／清除該 crate 舊建置產物後通過，沒有重打 Runtime。PHP fixture 起初誤把 HTTP chunked framing 當 body，改由 Hyper 解碼後通過。本輪 Clippy 首次指出新增 fixture 未處理 read 回傳長度；補上讀取非空 assertion 並重跑 Updater 測試與完整 lint 後通過，未因此變更產品程式。

## 驗證邊界與變更範圍

- 查驗執行期間沒有觸發 Windows CI、NSIS、Windows 實機流程或安裝／更新／移除驗收；後續 push 自動觸發的 Windows x64 CI 與 NSIS 已成功，但仍不代表 Windows 實機通過。
- 真實 PHP 測試驗證的是 PHP 內建 HTTP server 經 LAN Share 的流程，不宣稱已重跑完整 Nginx → PHP-FPM、DNS、HTTPS、MariaDB 或 Desktop UI 人工驗收。
- 尚未做長時間連線壓力測試、HTTP fuzzing 或完整瀏覽器相容性測試。
- UI 變更限於既有資訊與排版：Agent 狀態移至設定下方；總覽的全部啟停明確標示為 Web 服務並補充控制範圍；MariaDB 連線來源改為自動切換說明；PHP、Node.js、MariaDB Runtime 卡片統一為較緊湊的樣式，PHP 僅顯示使用中的 Site 數量；Proxy 頂部將資料操作與服務操作分組。Sites 與 Proxy 的既有清單排版已保留，沒有增加產品操作或改變服務行為。Agent Protocol、TypeScript Contracts、App 版本、Runtime Package／Catalog、打包與發布設定未變。查驗完成後已依 Repository Owner 指示 commit／push，但仍未進版、Tag、Draft 或發布；原本未追蹤的 `note.txt` 保留原狀。
- 程式變更限於 Site 驗證／錯誤回復鎖範圍、LAN Share、Proxy 任務清理／啟動重試／設定保存交易、Updater 取消、Agent 停止錯誤清理／Runtime 任務生命週期與安裝一致性／MariaDB 更新回復、Runtime 安裝移除偏好回滾／版本切換目錄檢查、Desktop Store 請求競態、Runtime 畫面任務資料及 macOS 終端整合寫入前檢查；Share 只引用 workspace 已使用且已鎖版的 `bytes`、`http-body-util`、`hyper`、`hyper-util`，Agent 測試另引用既有 rusqlite；`Cargo.lock` 僅增加這些依賴關聯，沒有升級套件。

本機查驗紀錄位於 `/tmp/fabdev-audit-final-test.log`、`/tmp/fabdev-audit-lint.log`、`/tmp/fabdev-audit-share-final.log` 與 `/tmp/fabdev-audit-php-flow.log`；修正前的失敗證據保存在對應的 `share-before`、`lifecycle-before`、`site-before` log。這些是本機暫存證據，不納入發布產物。

本輪穩定性驗證紀錄：`/tmp/fabdev-stability-final-test.log`、`/tmp/fabdev-stability-final-lint.log`、`/tmp/fabdev-stability-updater-final.log`、`/tmp/fabdev-stability-ui-final.log`、`/tmp/fabdev-stability-php-flow.log`。修正前失敗證據：`/tmp/fabdev-stability-cancel-before.log`、`/tmp/fabdev-stability-body-before.log`、`/tmp/fabdev-stability-stop-before.log` 與 `/tmp/fabdev-stability-ui-before.log`。

本次後續 Agent 修正的全測試與 lint 紀錄：`/tmp/fabdev-stability-followup-full-test.log`、`/tmp/fabdev-stability-followup-full-lint.log`；針對性結果：`/tmp/fabdev-stability-followup-regressions.log`；本批修改後另行重跑的 PHP 分享啟停測試通過，紀錄為 `/tmp/fabdev-stability-followup-php.log`；三個主要缺陷的修正前失敗證據：`/tmp/fabdev-stability-followup-before.log`。

Repository Owner 已明確表示上一版可用、不急著發布新版。本批修改沿用 App 0.1.22／Protocol 38；查驗完成後依明確指示以 Commit `75e09cc` 推送，既有 push trigger 自動執行的 Windows x64 Run `33955789378` 已成功。未進版、建立 Tag、Draft 或 Release，也未變更已發布的 `v0.1.22`。

安裝／更新回復這一批的驗證紀錄：`/tmp/fabdev-stability-rollback-full-test.log`、`/tmp/fabdev-stability-rollback-full-lint.log`、`/tmp/fabdev-stability-rollback-focused.log`。Runtime marker 原缺陷重現於 `/tmp/fabdev-stability-rollback-before.log`，MariaDB 原回復順序的失敗證據於 `/tmp/fabdev-stability-mariadb-rollback-before.log`。新增 6 項測試並擴充既有 Socket 切換測試；正式 Runtime Archive、安裝目錄與資料庫均未操作。由於本批涉及安裝／更新回復程式，未來若準備發布，仍需按專案規範完成受影響的人工回歸，不能把此次單元測試視為實機驗收。

套件一致性與線上安裝互斥這一批的最終紀錄：`/tmp/fabdev-stability-identity-full-test.log`、`/tmp/fabdev-stability-identity-full-lint.log`、`/tmp/fabdev-stability-identity-agent.log`。修正前證據：`/tmp/fabdev-stability-identity-before.log`、`/tmp/fabdev-stability-identity-ui-before.log`、`/tmp/fabdev-stability-install-concurrency-before.log`。此批新增 6 項 Agent 與 4 項前端測試；全套測試包含既有 Managed／System Socket 切換回歸。

版本切換這一批的最終紀錄：`/tmp/fabdev-stability-switch-full-test.log`、`/tmp/fabdev-stability-switch-full-lint.log`、`/tmp/fabdev-stability-switch-runtime.log`；修正前失敗證據：`/tmp/fabdev-stability-switch-before.log`。Runtime 29 項測試通過；完整測試、lint 及 `git diff --check` 通過，未觸發 CI 或打包。

終端整合這一批的最終紀錄：`/tmp/fabdev-stability-terminal-full-test.log`、`/tmp/fabdev-stability-terminal-full-lint.log`、`/tmp/fabdev-stability-terminal-platform.log`；修正前失敗證據：`/tmp/fabdev-stability-terminal-before.log`。Platform 13 項測試通過；完整測試、lint 及 `git diff --check` 通過，未觸發 CI、打包或發布。

Proxy 這一批的最終紀錄：`/tmp/fabdev-stability-proxy-full-test.log`、`/tmp/fabdev-stability-proxy-full-lint.log`、`/tmp/fabdev-stability-proxy-after.log`；修正前兩項失敗證據：`/tmp/fabdev-stability-proxy-before.log`。完整測試、lint 及 `git diff --check` 通過；未觸發 CI、打包、提交或發布。

Proxy 設定保存這一批的最終紀錄：`/tmp/fabdev-stability-proxy-storage-full-test.log`、`/tmp/fabdev-stability-proxy-storage-full-lint.log`、`/tmp/fabdev-stability-proxy-storage-core.log`；修正前失敗證據：`/tmp/fabdev-stability-proxy-storage-before.log`。新增兩項回歸，完整測試、lint 及 `git diff --check` 通過。Repository Owner 再次重申穩定為目標，本輪維持最小缺陷修正與既有功能，不擴充、不做無關重構，也未觸發 CI、打包或發布。

Site 刪除回復這一批的最終紀錄：`/tmp/fabdev-stability-site-lock-full-test.log`、`/tmp/fabdev-stability-site-lock-full-lint.log`、`/tmp/fabdev-stability-site-lock-after.log`；修正前失敗證據：`/tmp/fabdev-stability-site-lock-before.log`。新增一項包含錯誤回復、狀態查詢與成功重試的 Agent 回歸；完整測試、lint 及 `git diff --check` 通過。未觸發 CI、打包或發布。

Site 清單讀取回復這一批的最終紀錄：`/tmp/fabdev-stability-registry-lock-full-test.log`、`/tmp/fabdev-stability-registry-lock-full-lint.log`、`/tmp/fabdev-stability-registry-lock-after.log`；修正前四個分支均卡住的證據：`/tmp/fabdev-stability-registry-lock-before.log`。完整測試通過；lint 曾指出新增測試的 unnecessary unwrap，改用 match 後針對性測試與完整 lint 通過。`git diff --check` 通過，未觸發 CI、打包或發布。

相容性複核最終紀錄：`/tmp/fabdev-stability-consolidation-full-test.log`、`/tmp/fabdev-stability-consolidation-full-lint.log`；新增 Agent 流程測試：`/tmp/fabdev-stability-site-flow.log`。完整測試、lint 及 `git diff --check` 通過；本輪僅新增回歸測試與查驗紀錄，沒有新增產品修正、打包、CI 或發布。未把此次有界查驗描述為全專案無缺陷。
