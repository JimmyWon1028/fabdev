# Windows 0.1.13～0.1.16 實機測試紀錄

## 目前狀態

- `v0.1.15` 仍是 GitHub `latest` Stable。
- `v0.1.16` 是 Windows x64 only Pre-release，仍在 Repository Owner 測試中。
- Repository Owner 曾回報 0.1.16 安裝後一開始會當掉，後續重新測試已回報「用起來可以」且 Windows 測試通過；另發現手動刪除 MariaDB 服務目錄後無法自動恢復的邊界案例，排入下一版。
- `v0.1.16` 不包含 macOS 產物，尚未轉為 Stable；依 Repository Owner 要求已補上 Windows Runtime Assets，但測試確認前不得轉 Stable 或重新打包 App。
- 本文件只記錄已觀察到的事實、已確認根因、已完成修正及仍待確認項目；不得把推測寫成已修正。

## 事件摘要

| 版本／環境 | 現象 | 結論 | 狀態 |
| --- | --- | --- | --- |
| 0.1.13，乾淨 Windows 11 VM | 啟動時出現 `VCRUNTIME140.dll` 遺失 | Windows x64 MSVC binary 需要 Microsoft Visual C++ 2015–2022 x64 Redistributable；舊 NSIS 未先檢查 prerequisite | 0.1.15 已加入安裝前檢查與官方下載引導 |
| Windows ARM VM 與另一台 Windows x64／Intel VM | ARM 可開 Proxy 網址，Intel 曾無法連線；另一台 VM 直接連 `api.lysm.com.tw` 正常 | 問題不是遠端 API 全面故障；Windows Proxy `.test` 網域未可靠同步到 Hosts | 0.1.15 已由白名單 Helper 同步獨立 Proxy Hosts 區塊，Repository Owner 測試通過 |
| 0.1.15 App 內更新 | NSIS 無法寫入 `%LOCALAPPDATA%\fabDev\fabdev-agent.exe`，按「重試」無效 | `fabdev-agent.exe` 仍在記憶體並鎖住執行檔；更新流程只等 Desktop PID 與 Named Pipe 失效，沒有確認 Agent 程序退出及檔案解除鎖定 | 0.1.16 已修正後續 Windows 更新交接；0.1.15 本身不回溯替換 |
| Windows x64／Intel | 開啟 Site 偶爾顯示 `Forbidden` | 尚未取得發生當下 URL、response headers 與 Nginx log，不能判定為 Proxy、Nginx 或上游應用程式 | 未修正，等待新 log 證據 |
| Windows | fabDev 偶爾被描述為「當掉」，之後 Agent 仍留在記憶體 | 按視窗 `X` 只隱藏到系統匣是既定行為；若 Desktop 程序實際消失或無回應，舊版缺少足夠 Desktop crash log，不能判定根因 | 0.1.16 已補 Desktop／frontend／Agent 錯誤記錄及可恢復啟動錯誤處理；根因仍以新 log 為準 |
| 0.1.16 Windows x64 Pre-release | Repository Owner 曾回報安裝後一開始就會當掉 | CI 只證明程式可編譯、測試與 NSIS 建置通過；必須以真實 Windows x64 補做最小啟動 smoke test | 後續重新測試回報「用起來可以」；最小啟動 Gate 通過，繼續 Runtime 安裝測試 |
| 0.1.16 Windows x64，手動刪除 MariaDB 服務目錄後重新安裝 Runtime | 啟動前顯示 `MariaDB data directory does not exist: \\?\C:\Users\...\services\mariadb\data` | Runtime、設定與資料分開管理；重新安裝只恢復 Runtime，保存的 `config\mariadb.json` 仍指向已刪除的 Data Directory。載入設定時先要求目錄存在，因此尚未執行後面的建立目錄與資料庫初始化 | 已確認根因，尚未修改；排入下一版，且只允許自動恢復 fabDev 預設 Managed Data Directory |

## `VCRUNTIME140.dll` 經驗

Rust／Tauri 的 Windows x64 MSVC 產物可能依賴 Microsoft Visual C++ Runtime。乾淨 VM 沒有該 Runtime 時，App 會在自身程式碼執行前由 Windows Loader 阻擋，因此不能只在 App 啟動後檢查。

正確處理位置是 NSIS 複製檔案前：

1. 同時檢查 x64 VC Runtime Registry 狀態及 `VCRUNTIME140.dll`。
2. 缺少時顯示明確的繁體中文／英文訊息。
3. 只開啟 Microsoft 官方 x64 Redistributable 下載頁。
4. 中止本次 fabDev 安裝，要求使用者完成 prerequisite 後重新執行 Setup。
5. 不在 fabDev 安裝器內靜默下載或執行第三方 prerequisite。

## Proxy `.test` 網域經驗

排查順序必須拆開：

1. 直接測試遠端 Target，例如 `api.lysm.com.tw`，先排除遠端服務全面故障。
2. 檢查 Proxy listener 是否真的綁在 `127.0.0.1:<port>`。
3. 檢查 Proxy `.test` 網域是否存在於 fabDev 自己管理的 Hosts 區塊。
4. 用指定 `Host` 的請求確認本機 Proxy，而不是只用瀏覽器畫面判斷。
5. 比對 ARM VM、Intel VM 與另一台乾淨 VM，避免把單一 VM 的 DNS／Hosts 狀態誤判成架構問題。

Proxy 網域的新增、修改、移除、恢復啟動與解除安裝都必須同步 Hosts。Helper 只能操作有明確標記的 fabDev Proxy 區塊，不得覆蓋其他軟體或使用者的 Hosts 紀錄。

## Agent 鎖定與更新經驗

Windows 會鎖住正在執行的 `.exe`。Agent 回覆 `Stopped` 或 Named Pipe 已無法連線，不代表 `fabdev-agent.exe` 已經完全退出，也不代表檔案 handle 已釋放。

0.1.15 的失敗流程是：

1. Desktop 要求 Agent Stop All／Shutdown。
2. Desktop 以 Named Pipe 無法連線作為停止完成條件。
3. PowerShell launcher 只等待 Desktop PID 結束。
4. NSIS 立即開始覆蓋安裝。
5. Agent 程序仍在退出尾端，NSIS 因檔案鎖定顯示「中止／重試／略過」。

按「重試」無效時不得按「略過」，否則可能留下新版 Desktop 搭配舊版 Agent。現場恢復方式是中止安裝、重新啟動 Windows，且不要先開啟 fabDev，再手動執行完整 Setup。

0.1.16 的後續更新交接加入：

- 以完整 Agent 執行檔路徑比對程序，不只靠 process name。
- Desktop 結束後最多等待 Agent 10 秒自然退出。
- 超時時只強制停止同一路徑的殘留 Agent。
- 再等待並確認 Agent 程序消失。
- 以 exclusive open 確認 `fabdev-agent.exe` 已解除鎖定，最多再等待 5 秒。
- 任何步驟失敗時寫入 updater launcher log，且不啟動 NSIS。

這項修正位於 0.1.16，因此保護的是 0.1.16 發起的後續更新。0.1.15 的舊 launcher 不會因下載新版 Setup 而自動取得新邏輯。

## 啟動與錯誤記錄經驗

舊版 Windows Desktop 在 Tauri `setup` 階段安裝 bundled Runtime 或建立 Tray 失敗時，錯誤會向上傳到 `.expect(...)`，可能讓 Desktop 直接退出，而且沒有專用 Desktop log。

0.1.16 程式碼已嘗試調整為：

- bundled Windows Runtime 初始化失敗不再直接中止 Desktop。
- Windows Tray 初始化失敗不再直接中止 Desktop。
- 可恢復啟動錯誤會顯示到 UI，並寫入 Desktop process log。
- Rust panic、Agent request／response error、前端 `error`、未處理 Promise rejection 與 UI 全域錯誤都會留下記錄。

上述內容先由程式碼變更及編譯／靜態測試確認，之後 Repository Owner 重新測試已回報 0.1.16「用起來可以」，因此目前最小啟動 Gate 視為通過。若問題再次發生，仍需先讀取新產生的 `desktop-process.log` 與 Windows Application Error／Windows Error Reporting，再決定修正範圍。

Windows 診斷檔案：

```text
%LOCALAPPDATA%\FabDev\logs\desktop-process.log
%LOCALAPPDATA%\FabDev\logs\agent-process.log
%LOCALAPPDATA%\FabDev\logs\nginx-access.log
%LOCALAPPDATA%\FabDev\logs\nginx-error.log
```

若再次發生「當掉」或 `Forbidden`，必須同時記錄發生時間、完整 URL、畫面、`fabDev.exe`／`fabdev-agent.exe` 是否仍存在，以及上述 log。沒有這些資料時只記為未確認，不做猜測性修正。

## MariaDB 服務目錄遭手動刪除後的恢復經驗

MariaDB Runtime、Data Directory、設定與 Log 使用不同位置。使用者手動刪除 `%LOCALAPPDATA%\fabDev\data\services\mariadb` 時，會刪除預設 Data Directory，但不會刪除保存在 `config\mariadb.json` 的設定。重新安裝 MariaDB Runtime 只恢復 `runtimes\mariadb\<version>`，依既有資料保留契約不得自行覆蓋 Data、設定或 Log。

目前失敗順序：

1. 重新安裝 Runtime 成功。
2. Agent 讀取仍存在的 `config\mariadb.json`。
3. `validate_mariadb_settings()` 先對保存的 Data Directory 執行 `canonicalize()`。
4. 目錄已被手動刪除，因此回報 `MariaDB data directory does not exist`。
5. 啟動流程尚未走到 `generate_mariadb_config_with_settings()` 的建立目錄，也未走到 `initialize_mariadb()` 的空資料庫初始化。

畫面中的 `\\?\C:\...` 是 Windows canonical／verbatim path 前綴，不是另一個目錄；一般 UI 錯誤不應直接顯示這個內部格式。

0.1.17 修正邊界：

- 只在保存路徑確定等於 fabDev 預設 Managed Data Directory，且該目錄完全不存在時，自動重建空目錄，之後沿用既有初始化流程建立新資料庫。
- 自訂、外接磁碟或網路 Data Directory 不存在時仍回報錯誤，不得靜默建立空資料庫，以免把暫時離線或路徑錯誤誤判成資料已刪除。
- 非空目錄但缺少 `mysql` 系統資料夾時仍拒絕啟動，不得覆蓋未知檔案。
- 一般錯誤訊息移除 Windows `\\?\` 前綴；內部仍可保留 canonical path。
- 加入「保存預設路徑 → 手動刪除整個服務目錄 → 載入設定重建空目錄 → 啟動時進入初始化」回歸測試，以及「缺少自訂 Data Directory 仍拒絕」邊界測試。

0.1.17 Windows Gate 2 已完成原始碼修正：載入已保存的 Managed MariaDB 設定時，若遺失路徑確定等於 fabDev 預設 Data Directory，會先重建空目錄，再交由既有啟動流程初始化；自訂遺失路徑及含未知檔案的非資料庫目錄仍會拒絕。使用者可見錯誤會移除 Windows `\\?\` 前綴。

針對性驗證結果：

- 預設 Data Directory 遺失後自動復原：通過。
- 自訂 Data Directory 遺失時維持拒絕且不建立目錄：通過。
- 非空且不是 MariaDB 資料庫的目錄維持拒絕：通過。
- Windows verbatim／UNC 路徑顯示清理：通過。
- ERP MariaDB preset 的 `collation-server = utf8_unicode_ci`：通過。
- Rust format 與 `git diff --check`：通過。
- 本機 macOS 對 Windows MSVC 的交叉檢查停在第三方 `ring`／bundled SQLite 缺少 Windows C SDK 的 `assert.h`／`stdlib.h`，尚未進到本次 fabDev 程式碼；保留到 Windows 候選 CI 驗證。

上述 Gate 2 未打包、未執行完整 Windows CI、未建立 Tag、未上傳或發布，也未處理 macOS。確定舊 MariaDB 資料已不需要時，舊版現場暫時恢復方式仍是手動建立原本的空 `services\mariadb\data` 目錄，再啟動 MariaDB；若舊資料仍重要，不得用此方式取代備份或資料復原。

## 0.1.17 Windows 候選證據

- Commit：`cc07b09a6cc81f6de91a0fbe51f54567c1c25657`。
- GitHub Actions Windows x64 Run：`33493487090`，單次 push 觸發，全部步驟成功，執行時間 6 分 8 秒。
- Installer artifact：`fabDev_0.1.17_x64-setup.exe`，49,337,351 bytes，SHA-256 `60bf2030d81f7b8b484054171478b398cae502e693a11cac022656dac075eeaa`。
- Connect artifact：`fabdev-connect.exe`，749,568 bytes，SHA-256 `9c026d60e26672e46f9e489199ecca3da9c98424df7ef3e99018a61df99d8447`。
- NSIS 靜態驗證：Nullsoft Installer、214 個封裝項目；Desktop、Agent、Windows Helper 與 Connect 均為 Windows x64 PE。
- 內建 Runtime Manifest：Windows x64、Nginx 1.30.4、PHP 7.4.33／8.2.33；Desktop、Agent 與 Helper binary 皆可找到 `0.1.17` 版本字串。
- CI 唯一提示是 GitHub Actions 將使用 Node.js 20 runtime 的既有 actions 強制改以 Node.js 24 執行，不影響 fabDev 建置或候選內容。
- 候選只存在於 Actions artifact；未建立 Tag、Draft、Pre-release 或 Stable Release，也未處理 macOS。
- CI 與靜態驗證通過不等於 Windows 實機通過。Gate 4 仍由 Repository Owner 驗證安裝／啟動及本次 MariaDB 修正，未回報通過前不得進入後續發布 Gate。

## 0.1.16 Windows 候選證據

- Commit：`b854d23d9ebb144fff3a6780d1c35f0f4685421f`
- Tag：`v0.1.16`
- GitHub Actions Windows x64 Run：`33483193767`
- Setup：`fabDev-Community-0.1.16-windows-x64-setup.exe`
- 大小：`49,339,290` bytes
- SHA-256：`becd6063106d926627052438de8a7bfe4c0a7323a87fbe3090942dfb8629d7c0`
- 靜態驗證：NSIS 3 Unicode、214 個檔案；Desktop／Agent／Helper 均為 Windows x64 PE。
- Release 狀態：公開 Pre-release，`draft=false`、`prerelease=true`；GitHub `latest` 仍指向 `v0.1.15`。
- 實機結果：Repository Owner 後續重新測試回報「用起來可以」；最小啟動 Gate 通過，Runtime 安裝仍在測試。
- Runtime Catalog：Windows-only sequence `10`、6 個項目、SHA-256 `3d89341ac78e29dfe84d7eaf82ceb2fc0abc8d9338099581df529ed02646fee1`。
- Runtime Assets：PHP 7.4.33／8.2.33／8.4.24、MariaDB 12.3.2、Node.js 20.20.2／24.20.0，共 6 個 Windows x64 Archive 及個別 checksum。
- Runtime Archive SHA-256：PHP 7.4 `6b737d3b87d54e7d94ef857c388fc96912377893b53992d49261ee606d7fece4`、PHP 8.2 `525a9f43bc276584dc5b53dd11afa9b8e488944f4be83c83c1f6cfccce4236d8`、PHP 8.4 `f605ffa185598cd238607f1b9acddcba205ce81e0feefda7476bf3564d305dd2`、MariaDB `12ec4747cbbe4027458f46bf45c29c877fd8c2e678432cb72f32f604f4b6bb7a`、Node.js 20 `c433ce2e3f1b833af18c038eedb41147879e2f08e705fceb74cac18b4413eda8`、Node.js 24 `c1d397c5cbb74c53091c99a1e09a78e404f7446aefff8183fdcfa0673884a3cf`。
- Release Assets：共 18 個，GitHub 遠端大小與 digest 已核對；Catalog 與 `SHA256SUMS` 已由未登入公開 URL 重新下載並逐位元比對一致。

Windows CI 成功不等於安裝後可啟動。這次 CI 沒有啟動已安裝的 GUI App，因此最早期 Tauri setup、WebView、Runtime 初始化、Tray 初始化或 Windows Loader 階段仍由 Repository Owner 的實機 Gate 驗證。

## 後續驗收與節省時間原則

後續採分層驗證，不在每個小修改後重跑全部流程：

1. 每個小修改只跑直接相關的單元／靜態測試、typecheck、format 與 `git diff --check`。
2. 相關小修正累積成一個 Windows 候選後，只跑一次 Windows x64 CI；不因每個小問題各自重跑完整 CI。
3. CI 產物先交給 Repository Owner 做一次最小 Windows x64 smoke test：安裝、啟動、主視窗保持運行、Agent ready、關閉與重新啟動。Codex 預設只提供候選、操作步驟與通過標準，除非 Repository Owner 明確要求，不自行執行耗時實機流程。
4. 最小啟動 smoke test 未通過時立即停止，不執行完整 PHP、MariaDB、Node.js、HTTPS、Proxy 或發布驗證。
5. 最小啟動通過後，才依實際改動選擇受影響流程；更新交接改動才測原地更新與 Agent 檔案解鎖，Proxy 改動才測 Proxy／Hosts。
6. 發布候選的 Windows 驗收集中執行一次；不重跑 macOS 安裝／打包，也不重跑未受影響的 Runtime 完整人工流程。
7. `Forbidden` 只在取得新證據後修正，不把它混入 Agent 鎖定或啟動問題。

每一階段都必須停在 Gate 等待 Repository Owner 回報。只有收到目前 Gate「通過／沒問題」的明確結果才往下；不能因程式碼完成、CI 綠燈、安裝包已產生或先前版本曾通過就自行略過 Gate。

本次流程失誤是把「MSVC check、測試及 NSIS 建置成功」誤當成「Windows App 可正常啟動」，並在缺少實機啟動 smoke test 時宣稱 0.1.16 已改善啟動穩定性。後續 Release 報告必須分開寫明：靜態／CI 已通過、實機尚未測試、實機已通過，三者不可互相替代。

Repository Owner 已確認 0.1.16 Windows 測試通過；0.1.16 仍保留為 Windows Pre-release，macOS 跳過 0.1.16。後續共同版本進入 0.1.17，先完成 Windows Gate，再另行處理 macOS。
