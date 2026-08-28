# fabDev 工作進度與 TODO

> 更新日期：2026-08-28
> 目前階段：macOS ARM64／Windows x64 Unsigned Community Build 0.1.0

## 已完成

- Tauri／Vue Desktop、Rust Agent／CLI、Unix Socket Protocol 32 與 SQLite Site Registry。
- macOS App 與 `pnpm dev` 內建 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33、PHP 8.2.33；首次啟動只補缺少版本，保留既有開發資料。
- macOS 與 Windows 在 Site Registry 完全空白時建立唯一的 `demo.test`；已有任何 Site 時不新增或覆蓋。
- `.test` DNS、Nginx、53／80／443 固定 Helper，以及 Start All／Stop All 與 menu bar 狀態。
- 每 Site HTTPS 啟用／停用、本機 CA 與 SAN 憑證、macOS Login Keychain／Windows Current User Root 信任，以及 HTTP 自動轉址 HTTPS。
- 多 Site、新增／移除、document root 偵測、每 Site PHP 7.4／8.2／8.4 切換，以及不使用 PHP 的純靜態 Site。
- Site Home 預設為 `~/Sites`；第一層非隱藏資料夾自動成為同名 `.test` Site，並保留原有 linked site。
- Sites 與 Proxy 主控台支援版本化 JSON 匯出／匯入；Sites 依網域略過重複，Proxy 依 ID、網域或 Listener Port 略過重複。
- PHP 7.4.33、8.2.33、8.4.24 並行 FPM、全域 PHP、Runtime 安裝／移除與持久 `php.ini`；上傳限制為 64M。
- PHP 設定提供由目前 PHP 8.2 設定初始化的預設 `php.ini` 範本，只套用到尚未建立專屬設定的 PHP minor。
- PHP 7.4 與 8.2 內建 Runtime 可安全移除；仍保留全域版本與 Site 使用中保護，明確移除後不會在下次啟動自動補回。
- 左側倒數第二項 Node.js 頁面、預設不安裝的 Node.js 24.19.0 LTS 選裝 Runtime，以及 Agent 安裝、狀態與移除；Node.js 與 Sites 分離，也不接管 Homebrew、nvm、Herd 或系統 Node.js。
- 左側 Proxy Manager、Agent／CLI 的新增／移除、全部與單獨啟動／停止；全新安裝的 Proxy 清單為空，使用者設定與啟動狀態保存在 SQLite，所有 Listener 只綁 loopback，Port 衝突與上游故障互相隔離。
- 設定頁可持久開關「App 開啟時自動啟動服務」；預設開啟，已運行不重啟，部分異常會先清理再啟動。
- Community DMG 讓 App 內建 DNS、Nginx、PHP 7.4／8.2，並含 Helper、安裝／移除程序與唯一 `demo.test`；PHP 8.4、MariaDB 維持獨立選裝套件。
- 總覽的 Web 服務控制使用單一狀態按鈕：全部運行時顯示「全部停止」，其他狀態顯示「全部啟動」。
- 總覽的 MariaDB 卡片只顯示連線與運行狀態；啟動、停止及設定操作統一放在 MariaDB 頁面。
- menu bar `Quit fabDev` 會先停止 Web 全部服務與 MariaDB、清理受管孤兒程序，再關閉 Agent 與 Desktop。
- Community Runtime 使用 `*-macos-arm64-community`、`community-ad-hoc` 描述及獨立 Catalog；開發套件維持 `*-dev`。
- Windows Named Pipe Agent、Nginx／PHP-CGI Platform Adapter、白名單 Hosts Helper 與單一使用者 NSIS 安裝程式。
- Windows 首次啟動會安裝內附 Nginx 1.30.4、PHP 7.4.33／8.2.33，並建立唯一的 `demo.test`。
- macOS ARM64 MariaDB 12.3.2 Runtime、主控台／menu bar／CLI 的獨立 Install／Start／Stop／Remove、3306 衝突檢查及隔離資料目錄。
- Sites 畫面的多 Site `LAN Site Share`：多個 Site 共用主機高位 Port 並由 Nginx 依 Host 分流；可逐一停止，最後一個 Site、Stop All、Agent Shutdown 或 App Quit 會釋放 Listener。
- Windows `fabdev-connect.exe`：UAC 後自動管理多個有明確標記的 `.test` hosts，以非同步 Client `127.0.0.1:80` 代理轉送到主機，保存最後使用的主機與 Sites，並在從 Parallels Shared Folders 啟動時自動轉存本機 Runtime，再要求 UAC。

## 最近驗證

- 完整測試：前端 28、Rust 116、macOS Helper 9 項一般測試全數通過；另有 1 項需指定實際 MariaDB Runtime 的測試維持忽略。
- 隔離 HTTPS 流程已確認 CA chain、`tls-e2e.test` SAN、Nginx 1.30.4 `-t`、18444 高位 TLS 與實際 HTTPS 靜態檔回應；`demo.test` 已完成 Login Keychain 信任、HTTP 301 與正式 443 HTTPS 200 驗證。
- `pnpm lint`：TypeScript、rustfmt、Clippy 與 Swift lint 通過。
- Community Runtime Catalog 清單含 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33、8.2.33、8.4.24 與 MariaDB 12.3.2；基礎 DMG 已依內建四個 Runtime 的規格重新打包。
- PHP 8.4.24 官方 SHA-256／PGP 驗證、CLI／FPM 設定、必要 Extension 與封裝通過；Mach-O 不依賴 `/opt/homebrew`。
- PHP 8.4.24 已透過 Agent 安裝至目前 Application Support；全域 PHP 維持 8.2.33。受管 `php.ini` 已驗證 64M 上傳限制與 Asia/Taipei，實際安裝 Binary 載入成功。
- DMG 唯讀掛載後，內外層 SHA-256、App 簽章與內建 Runtime Descriptor 雜湊全數通過；App 內只有 dnsmasq、Nginx、PHP 7.4／8.2，沒有 `-dev`、PHP 8.4 或 MariaDB Runtime。
- GitHub Actions Windows MSVC 完整檢查與 NSIS 建置成功；產物為 45 MiB 的單一自解壓安裝程式。
- MariaDB 12.3.2 Runtime 官方 SHA-256／PGP 驗證、Server／Client 版本與封裝通過；隔離流程 Start → TCP SQL → Stop → Start → 資料讀回通過。
- MariaDB 首次初始化已用實際 `Application Support` Runtime 與含空白的隔離資料路徑驗證；12.3.2 TCP SQL 查詢通過。
- Agent 重啟後會由 fabDev 專用 PID 與 Unix Socket 恢復 MariaDB 運行狀態，仍可從主控台單獨停止；失效 PID 不會被接管。
- App 啟動時會依 `state/mariadb.json` 恢復 MariaDB 最後一次成功啟動／停止的狀態，並與 Web stack 自動啟動設定分離。
- 左側 MariaDB 頁面可獨立啟停並持久設定非特權 TCP Port 與 Data Directory；運行中、非絕對路徑及非 MariaDB 的非空目錄會被拒絕。
- MariaDB 連線來源完全自動：Managed Service 實際運行時使用 fabDev Socket，未安裝或已安裝但停止時使用 System／Homebrew 連線；啟動或停止後立即重建使用中的 PHP-FPM 設定。Unix 依 Socket、Windows 依 fabDev PID 與 TCP readiness 判定 Managed 狀態。左側 MariaDB 頁面不顯示來源或 Socket 選項，未安裝時也不顯示設定卡片；已以 Adminer 驗證 `localhost` 登入 Homebrew MariaDB。
- 左側 MariaDB 頁面新增 macOS `my.cnf`／Windows `my.ini` 額外選項編輯器及 root 密碼設定；額外設定會由 MariaDB 驗證，受管連線與程序選項不能覆寫，密碼不持久化。
- 主控台安裝／移除隔離流程通過：執行中拒絕移除，停止後只移除 Runtime，重裝後可讀回保留資料。
- `fabdev-share` 雙向 TCP 轉送及停止後 Port 釋放測試通過；`fabdev-connect` 的 hosts 新增／移除、衝突拒絕、網域驗證、雙向轉送與 Port 釋放共 4 項測試通過。
- `fabdev-connect` 通過 `x86_64-pc-windows-msvc` 交叉編譯檢查；Windows GUI、UAC、實際 hosts 與瀏覽器流程待 Parallels Windows 11 驗收。
- Node.js 24.19.0 LTS 官方 macOS ARM64 Archive SHA-256 與發布者 PGP 簽章驗證通過；選裝套件已產生並確認 Node v24.19.0、npm 11.17.0、描述檔與單一 `24.19.0/` 封裝根目錄。
- Proxy 聚焦測試確認自訂新增／移除與驗證、設定持久化、HTTP Host 改寫、Credential CORS、實際 streaming response、單一 Port 衝突隔離及停止後 Port 釋放。

## 2026-08-26 工作日誌

- 完成 Agent Protocol 25 Proxy Manager；新增獨立 `fabdev-proxy` Rust Runtime、新增／編輯／移除、Credentials Origin、全部／單獨啟動停止、CLI、Desktop 頁面與 SQLite 設定及啟動狀態持久化。
- 隔離 Agent 流程確認自訂 Proxy 新增後可跨重啟保存；執行中的 Connection 移除時會停止並釋放 Port，第二次重啟後不會恢復已移除設定。
- Proxy Listener 固定 loopback，其他程序占用 Port 時只標記該 Connection Failed；上游請求或 15 秒 TCP Health Check 失敗標記 Degraded，不影響其他連線。
- 完成單一穩定 Node.js LTS Runtime 的狀態、安裝與移除；既有 SQLite `node_version` 欄位保留供舊資料相容，但不再由 Site 使用。
- 新增左側倒數第二項 Node.js 頁面；預設顯示未安裝，安裝後可由同一頁獨立移除。
- 建立 Node.js 官方 Archive／SHA-256／PGP 驗證與 fabDev Runtime 封裝腳本；Runtime 不修改 Homebrew、nvm、Herd、系統 Node.js 或使用者 PATH。
- 使用隔離 Agent 與實際選裝套件完成預設未安裝 → 安裝 → Node v24.19.0／npm 11.17.0 執行 → 移除 → 回到未安裝的完整流程。
- 完整前端、Rust workspace、macOS Helper 測試與 lint 通過；macOS 缺少 Windows MSVC C Header／Library 工具鏈，因此本機 Windows workspace 交叉檢查未完成，仍以 GitHub Actions Windows MSVC 為正式驗證環境。

## 2026-08-25 工作日誌

- 完成 Agent Protocol 20 的每 Site HTTPS 流程、本機 CA、`.test` SAN leaf certificate、Nginx 8443 TLS listener、HTTP 轉址及 System Helper 固定 `443→8443` 代理。
- 處理已安裝舊版 Helper 未包含 HTTPS 入口的狀況；更新並重新安裝 Helper 後，同意信任目前使用者 Login Keychain 內的 fabDev CA，再重新啟用 `demo.test` HTTPS。
- 最終以 `demo.test` 驗證 DNS、HTTP 301、正式 443 HTTPS 200、憑證 SAN 與 CA chain；先前瀏覽器的 `ERR_SSL_UNRECOGNIZED_NAME_ALERT` 已排除。
- 查閱 Laravel Herd 官方 AI／MCP 功能：Herd 讓外部 AI Client 透過 MCP 取得 Site／Runtime／Service 資訊，執行 Site 診斷、HTTPS／PHP 切換及服務管理；它不是內建生成式 AI 對話功能。
- fabDev 後續 MCP 方向定為既有 Agent Protocol 的薄型轉接層。第一階段優先提供唯讀的 `site_information`、`site_status`、`diagnose_site` 與 Log／服務狀態，再逐步開放需確認的 HTTPS、PHP 與服務啟停操作；不得提供任意 Shell 或擴大 Helper 權限。

## 驗證邊界

- 尚未在乾淨 Mac 執行完整管理員安裝、更新及移除流程。
- 尚未驗證 Gatekeeper quarantine 與 Herd／Valet Port 衝突指引。
- Release 建置仍警告 `rust-objcopy` 找不到 `libLLVM.dylib`；不影響產物生成，但 stripping 尚待修正。
- Windows 安裝程式尚未在實體 Windows x64 驗證安裝、UAC Hosts 修改、啟動服務及完整解除安裝。

## TODO

Laravel Herd 可借鏡但尚未完成的完整盤點與優先順序，見 [`HERD_REFERENCE_BACKLOG.md`](HERD_REFERENCE_BACKLOG.md)。

### P0：Community Beta

- [x] 完成 Public Repository、Release Asset 命名、Stable Channel、App Manifest v1、Draft／Publish 與回復契約；見 [`PUBLIC_RELEASE_SPEC.md`](PUBLIC_RELEASE_SPEC.md)。
- [x] 建立 Release Asset／Manifest／Checksum 產生器；驗證四個版本來源與 Agent Protocol，不覆蓋既有輸出，也不執行打包或發布。
- [x] 建立只接受手動雙重確認、既有 Tag 且只會建立 Draft 的 GitHub Actions Release workflow；只有最後 Job 具寫入權限，目前尚未執行。
- [ ] 在乾淨 Mac 驗證安裝 → 自動啟動 → `demo.test` → 更新 → 完整移除。
- [ ] 驗證 Gatekeeper、quarantine、管理員授權及 53／80／443 衝突錯誤訊息。
- [ ] 修正 release stripping 工具鏈警告。
- [ ] 建立第一個 Draft Release，重新下載驗證後由 Repository Owner 人工核准 Publish。

### P1：核心開發體驗

- [ ] 提供可由一般本機瀏覽器操作的 Web UI；新增只綁定 loopback、具身分驗證與權限限制的 HTTP／WebSocket API，並讓前端在非 Tauri 環境改走該 API。
- [ ] 建立 PHP 8.3 Community Runtime 與升級偵測通知。
- [ ] 提供 shell PHP／Composer／Artisan shim，支援全域及 Site 版本。
- [ ] 加入可進版控的 `fabdev.yml` Site 設定。
- [ ] 提供 Redis、LDAP、ODBC 等選配 PHP Extension 管理。
- [ ] 建立 `fabdev-mcp` 薄型轉接層；先提供每 Site 範圍的資訊、狀態與 DNS → HTTP／HTTPS → Nginx → PHP → MariaDB 診斷，再加入具確認、白名單與敏感資訊遮罩的變更工具。

### P2：選裝與跨平台

- [x] 單一穩定版 Node.js LTS 獨立選裝、顯示狀態及移除。
- [ ] Node.js 多版本、全域版本、`.nvmrc`／`fabdev.yml` 與選用的專案感知 CLI shim。
- [x] macOS ARM64 MariaDB 選裝服務。
- [ ] Windows MariaDB 安裝版與 Portable 版的 Runtime、資料及升級策略。
- [x] Windows Platform Adapter 與 Unsigned Community NSIS 安裝包。
- [ ] 在乾淨 Windows x64 驗證安裝 → UAC → `demo.test` → PHP 切換 → 完整移除。
- [ ] 在 Parallels Windows 11 驗證 `fabdev-connect.exe` → UAC → 多 Site hosts → `http://site-one.test`／`http://site-two.test` → 並行載入 → 中斷清理。
- [ ] Developer ID、notarization 與 `SMAppService` Signed Distribution。

### P3：正式服務產品線

- [ ] 未來另立 `fabDev Server` 產品；不得直接沿用 Desktop 本機開發模式，其 Control Plane、Data Plane、網路安全、備份、更新、監控及第一版驗收架構記錄於 `docs/FABDEV_ARCHITECTURE.md` 第 15 節，不納入目前單機、單人 fabDev Desktop 的實作範圍。

### fabDev Desktop 產品化驗收目標

- [ ] 可管理至少 100 個 Site，並同時啟用 20 個 Site，不得出現 UI、Registry 或服務狀態錯亂。
- [ ] 使用固定 ERP 測試 Fixture 驗證同時處理至少 50 個本機 HTTP 請求，過程不得出現請求錯誤或受管程序異常退出。
- [ ] Web Stack 與 MariaDB 連續運行 72 小時，不得出現程序遺失、持續性記憶體增長、Port、PID 或 Socket 殘留。
- [ ] Start All → Stop All → Start All、Quit → Relaunch 及 Agent Upgrade 等生命週期流程累計執行至少 500 次，不得殘留受管程序或破壞服務狀態。
- [ ] 強制終止 Nginx、PHP-FPM／PHP-CGI、MariaDB 或 Agent 後，必須能明確診斷並安全恢復，不得接管非 fabDev 程序。
- [ ] App、Agent、Helper 或 Runtime 更新失敗時，不得破壞既有 Site、Runtime、`php.ini`、MariaDB 設定或資料，並可回復至更新前的可用狀態。
- [ ] 在乾淨 macOS 與實體 Windows x64，以及安裝 Herd、Valet 或 IIS 的共存環境，完成安裝、啟動、PHP 切換、更新、衝突處理及完整移除驗證。
