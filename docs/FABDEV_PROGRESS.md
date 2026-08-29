# fabDev 工作進度與 TODO

> 更新日期：2026-08-29
> 目前階段：macOS ARM64／Windows x64 Unsigned Community Build 0.1.1 最終 Publish 檢查

## 已完成

- Tauri／Vue Desktop、Rust Agent／CLI、Unix Socket Protocol 32 與 SQLite Site Registry。
- macOS App 與 `pnpm dev` 內建 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33、PHP 8.2.33；首次啟動只補缺少版本，保留既有開發資料。
- macOS 與 Windows 在 Site Registry 完全空白時建立唯一的 `demo.test`；Community 首次初始化會把 Site Home 固定在範例專案的父目錄，避免掃描其他本機專案，已有任何 Site 時不新增或覆蓋。
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
- menu bar、macOS App 選單及 `Command+Q` 的 `Quit fabDev` 會走同一套退出流程，先停止 Web 全部服務與 MariaDB、清理受管孤兒程序，再關閉 Agent 與 Desktop。
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

## 2026-08-29 工作日誌

- Repository Owner 已明確授權提交並推送發布前後驗收文件、更新 `v0.1.1` Release Notes、Publish 與公開下載驗證。
- `v0.1.1` Windows x64 Setup 已在 Parallels Windows 11 ARM 的 x64 模擬層完成 `0.1.0 → 0.1.1` 覆蓋更新、資料保留、Start／Stop／Start、PHP 7.4／8.2 切換、HTTP 200、解除安裝與乾淨資料基線首次安裝。首次啟動只有 `demo.test`，Proxy 為空；解除安裝清除 App、登錄、Hosts、程序與 Port，並依政策保留使用者資料。
- Draft Connect 已確認 Shared Folder 啟動後轉存相同 SHA-256 的本機 Runtime 並進入 UAC `--elevated`；同時驗證它會拒絕接管本機 fabDev 已存在的同名 Hosts 紀錄。多 Site 實際轉送與中斷清理維持 P2，不列為 P0 NSIS Publish 阻擋條件。
- quarantine DMG 副本保持原 SHA-256，Gatekeeper 對 ad-hoc、無 Team ID 的 App 如預期回報 rejected；管理員安裝已在完整生命週期驗收通過。53／80／443 檢查位於 Helper 寫入前，實際特權 Port 衝突因 sudo 授權已失效未再次重跑。
- macOS hosted release 的 `rust-objcopy` 警告來自 runner Rust 工具缺少 `libLLVM.dylib`；已讓 Tauri release build 與 Community CLI 明確使用 `CARGO_PROFILE_RELEASE_STRIP=none`。完整測試、lint 與無 stripping 警告的 release App build 通過，修正已推送至 main，但不移動固定的 `v0.1.1` Tag 或變更 Draft Assets。
- 專案正式版本來源已由 `0.1.0` 更新為 `0.1.1` 候選版；annotated `v0.1.1` Tag 固定在 Release Commit `8d70808`。GitHub Actions 已從該 Tag 重新建置 macOS ARM64／Windows x64 產物並建立 Draft Release，沒有 Publish。
- 本機重新打包的 `fabDev-Community-0.1.1-macos-arm64.dmg` 為 98,158,623 bytes，SHA-256 為 `fba390ef39b0fe6e0542a64448c4af954423bc2ea8a3e3ca47777397565a22fc`；DMG、27 個內層檔案、App／Build 版本、ad-hoc 簽章與 Desktop／Agent／CLI ARM64 均驗證通過，新版 Uninstaller 與來源一致。此 Hash 只記錄本機候選包，不取代未來 Draft Assets 的重新下載驗證。
- `v0.1.1` Draft 的 9 個 Assets 已全部重新下載；總表與三份個別 SHA-256、兩份逐位元一致的 Manifest、實際檔案大小及 DMG 內部 27 個校驗項目全數通過。DMG 為 99,295,774 bytes、SHA-256 `24849fd966de2f61c4641056f9ab1c6b0b0ed59308f2e9b3cb6388cdf60ddb28`；Windows Setup 為 48,332,278 bytes、SHA-256 `5bd0c91c8885e855c03865aba9909b02c26ee2e73503450c1af27fa3fd310319`；Connect 為 749,568 bytes、SHA-256 `2082d724e809a04111a78a74fe7f0aadd021218569a4589a8c6b7b9fd0a4710f`。
- 從 `v0.1.1` Draft 重新下載的 DMG 已在恢復至 fabDev 未安裝基線的 Mac 完成管理員首次安裝；App／Helper 正確安裝，首次初始化只有 `demo.test`、Proxy 清單為空，Site Home 已保存為 Demo 父目錄，重啟後沒有掃描其他本機專案。外部 Resolver 與 System／Homebrew MariaDB 全程保持不變。
- macOS App 選單的 `Quit fabDev` 已以實際 UI 操作驗證，Desktop、Agent、dnsmasq、Nginx、PHP-FPM、Proxy 與內部 Port 全部清理。`demo.test` 啟用 HTTPS 後，Login Keychain CA 信任、HTTP 301、HTTPS 200、leaf SAN 與私鑰 600 權限均通過。
- 同版覆蓋更新保留 Site ID、Site Home、HTTPS、CA／leaf certificate、Demo、空白 Proxy 與 Resolver；更新後手動開啟 App 可正常恢復服務。完整移除則清除 App、Helper、使用者資料、Demo、CA、程序與 listener，三個本次項目移至垃圾桶且可復原，外部 Resolver 與 MariaDB 仍未受影響。
- 從 GitHub Draft 重新下載的 `v0.1.0` DMG 已通過管理員安裝、Helper／Resolver 建立、唯一 `demo.test` 的 DNS、HTTP、HTTPS、憑證 SAN 與 Login Keychain 信任驗證；Proxy 首次安裝清單為空。
- 乾淨初始化發現 Site Home 未持久化，導致預設掃描其他本機專案；已改為建立 `demo.test` 後同步保存其父目錄，並加入不匯入同層無關資料夾的回歸測試。
- macOS App 選單的原生 Quit 項目會直接結束 Desktop，沒有停止 Agent 與 Web 服務；已換成具有 `Command+Q` 的 fabDev 自訂 Quit 項目，統一交由既有的安全退出流程處理。
- Community 移除程序原本只依目前資料目錄的 CA Fingerprint 撤銷信任，無法清除舊資料留下的 fabDev CA；已改為逐張核對精確 Subject、Issuer 及 Fingerprint，再移除所有符合的 fabDev 自簽 CA，且不依賴使用者資料仍存在。
- `v0.1.0` 原始移除程序已清除 App、Helper、資料與 Demo；殘留的舊 fabDev CA 已依精確 Fingerprint 人工移除，安裝前保留的外部 `/etc/resolver/test` 也已恢復。這項人工補救不算原始安裝包通過移除驗收。
- 以上三項為 `v0.1.0` Draft 的 P0 阻擋問題；原 Tag 與 Draft 保持不變且不得 Publish。修正需使用新的 Patch 版本重新打包、建立 Draft 並重跑完整驗收。

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

- `v0.1.0` 的首次 Site Home、App 選單 Quit 與舊 CA 清理三項阻擋問題，已由 `v0.1.1` Draft 在恢復至 fabDev 未安裝基線的 Mac 完成首次安裝、覆蓋更新與完整移除回歸。
- 覆蓋安裝程序結束後未觀察到 App 保持運行；手動開啟後所有服務與保留資料驗證通過。若後續要把「更新後自動重新開啟 App」列為發佈條件，仍需在另一個 macOS Session 重現確認。
- Gatekeeper quarantine 已驗證會拒絕 ad-hoc App；53／80／443 衝突腳本已確認先檢查再寫入，但本次未在 sudo 授權失效後重新建立實際特權 Port 衝突。
- release stripping 工具鏈警告已在 main 修正並以無警告 release App build 驗證；固定的 `v0.1.1` Tag 與既有 Draft Assets 不回寫此未來建置修正。
- Windows x64 Setup 已在 Parallels Windows 11 ARM 的 x64 模擬層完成生命週期驗收；乾淨實體 Windows x64、SmartScreen 簽章信譽與 IIS／Herd 共存尚未驗證。

## TODO

Laravel Herd 可借鏡但尚未完成的完整盤點與優先順序，見 [`HERD_REFERENCE_BACKLOG.md`](HERD_REFERENCE_BACKLOG.md)。

### P0：Community Beta

- [x] 完成 Public Repository、Release Asset 命名、Stable Channel、App Manifest v1、Draft／Publish 與回復契約；見 [`PUBLIC_RELEASE_SPEC.md`](PUBLIC_RELEASE_SPEC.md)。
- [x] 建立 Release Asset／Manifest／Checksum 產生器；驗證四個版本來源與 Agent Protocol，不覆蓋既有輸出，也不執行打包或發布。
- [x] 建立只接受手動雙重確認、既有 Tag 且只會建立 Draft 的 GitHub Actions Release workflow；只有最後 Job 具寫入權限，已用 `v0.1.0` 與 `v0.1.1` 完成兩平台建置與 Draft 建立。
- [x] 在恢復至 fabDev 未安裝基線的 Mac 驗證安裝 → 自動啟動 → `demo.test` → 更新 → 完整移除；`v0.1.1` 已通過原三項阻擋問題回歸與外部 Resolver／MariaDB 共存檢查。
- [x] 驗證 Gatekeeper、quarantine 與管理員授權；實際 53／80／443 特權 Port 衝突保留為後續補充驗收。
- [x] 修正 release stripping 工具鏈警告。
- [x] 建立第一個 `v0.1.0` Draft Release，重新下載 9 個 Assets，核對實際大小、Manifest 與 SHA-256；目前仍未 Publish。
- [x] 建立 `v0.1.1` Draft Release，重新下載 9 個 Assets 並驗證大小、Manifest、SHA-256、DMG 內容與公開內容邊界；目前仍未 Publish。
- [x] Repository Owner 已在 Mac／Windows 驗收完成後人工核准 `v0.1.1` Publish。

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
- [x] 在 Parallels Windows 11 ARM 的 x64 模擬層以乾淨資料基線驗證安裝 → UAC Helper／Hosts → `demo.test` → PHP 切換 → 完整移除；實體 Windows x64 仍待補測。
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
