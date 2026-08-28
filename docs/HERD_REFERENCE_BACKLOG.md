# Laravel Herd 可借鏡功能與 fabDev 未完成清單

> 盤點日期：2026-08-26
> 狀態：產品參考與 Backlog，不代表已承諾全部實作
> 來源：Laravel Herd 官方文件，實作前仍須重新確認版本、授權與平台差異

## 1. 目的與判斷原則

本文件只記錄 Laravel Herd 已提供、fabDev 尚未完成，而且對 ERP／Legacy PHP 本機開發有借鏡價值的功能。排序以 fabDev 的產品定位為準，不追求完整複製 Herd。

納入原則：

- 優先改善 PHP／ERP 專案的可重現環境、診斷能力與日常操作效率。
- macOS 與 Windows 必須共用 Domain Logic，平台差異收斂於 Platform Adapter 或 Helper。
- Desktop、Agent 與 Runtime 維持一般使用者權限；特權 Helper 不增加任意命令、路徑或 Port。
- 任何會修改 Site、Runtime、服務或系統狀態的功能，都必須有白名單、明確確認與失敗回復。
- Laravel 專用能力只能作為選用 Adapter，不得讓一般 PHP／Legacy ERP 依賴 Laravel。

## 2. 已完成，不列入本 Backlog

以下能力已在 fabDev 完成或已有對應方案：

- Site Home 與 linked site、`.test` DNS、document root 偵測。
- 每 Site PHP 版本切換與全域 PHP。
- 每 Site HTTPS、本機 CA、SAN 憑證、HTTP 轉址及 443 固定入口。
- macOS ARM64 MariaDB 選裝、啟停、設定、密碼同步及狀態恢復。
- `php.ini` 編輯、驗證與安全套用。
- Start All／Stop All、menu bar 狀態與受管程序清理。
- 短時間、區域網路用的 LAN Site Share 與 Windows `fabdev-connect`。
- 單一 Node.js 24.19.0 LTS 獨立選裝／移除；預設不安裝、不與 Site 綁定，且不接管外部 Node.js。

## 3. 優先摘要

| 優先級 | 功能 | fabDev 現況 | 建議方向 |
| --- | --- | --- | --- |
| P0 | `fabdev.yml` | 尚未實作 | 先支援 Site、PHP、HTTPS、alias 與服務需求 |
| P0 | PHP／Composer／Artisan shim | 尚未實作 | 依目前目錄選擇 Site PHP，不污染系統 PATH |
| P0 | 通用 Site 診斷與 Log Viewer | 尚未實作 | DNS → HTTP／HTTPS → Nginx → PHP-FPM → MariaDB |
| P0 | `fabdev-mcp` 唯讀工具 | 尚未實作 | 作為 Agent Protocol 薄型轉接層 |
| P0 | Runtime 更新偵測 | 部分 Runtime 管理已完成 | 加入 PHP／服務版本通知與安全更新流程 |
| P1 | PHP Extension 管理 | 部分 Extension 內建 | 優先 Redis、LDAP、ODBC、SQL Server／PostgreSQL 驅動 |
| P1 | Node.js 多版本 | 單一獨立 LTS Runtime 已完成 | 補多版本、`.nvmrc` 與選用的專案感知 CLI shim |
| P1 | 選裝服務目錄 | 只有 MariaDB | 優先 Redis／Valkey，再評估其他資料庫與搜尋服務 |
| P1 | Xdebug | 尚未整合 | 各 PHP 版本選裝、按需啟用及 IDE 設定指引 |
| P1 | 本機 Mail Catcher | 尚未實作 | SMTP 攔截、Site inbox、HTML／raw／附件檢視 |
| P1 | Site 操作捷徑與整理 | 部分具備 | Favorites、Groups、Terminal／Editor／DB／Log action |
| P2 | Dumps／請求追蹤 | 尚未實作 | 通用事件格式，加選用 Laravel Adapter |
| P2 | PHP Profiler | 尚未實作 | 評估 SPX，相容 Web、CLI 與長時間程序 |
| P2 | 公開安全分享 | 只有無驗證 LAN Share | 使用者自帶 Tunnel、短效 Token、Basic Auth、明確警告 |
| P2 | Framework Driver | 尚未實作 | 框架偵測、資訊、Log 路徑及啟動命令 Adapter |
| P3 | 部署／雲端整合 | 尚未實作 | 維持供應商中立，不優先綁定 Forge |

## 4. P0：核心開發體驗

### 4.1 可進版控的 `fabdev.yml`

Herd 的 `herd.yml` 可保存專案名稱、alias、PHP 版本、TLS 與服務版本，並透過初始化命令套用環境。參考：[Herd.yml](https://herd.laravel.com/docs/macos/sites/herd-yaml)。

fabDev 建議：

- [ ] 定義並版本化 `fabdev.yml` schema。
- [ ] 第一版只包含 `name`、`domain`／`aliases`、`documentRoot`、`php`、`secured`。
- [ ] 第二版加入 MariaDB、Redis／Valkey等選用服務與版本需求。
- [ ] 提供 `fabdev init`、`fabdev apply` 與 dry-run。
- [ ] 不把密碼、Token、私鑰或實際 `.env` 值寫入檔案。
- [ ] 套用前顯示差異；部分失敗時回復 Registry、Nginx 與服務狀態。

驗收重點：新電腦 clone 專案後，可以從版本庫設定重建相同的 PHP、HTTPS 與服務需求，但不複製任何秘密。

### 4.2 Site-aware CLI 與 Toolchain Shim

Herd 將 PHP、Composer、Laravel Installer 與常見 Artisan 操作整合到 CLI，並讓 isolated site 使用指定 PHP。參考：[Command Line](https://herd.laravel.com/docs/macos/advanced-usage/herd-cli)、[Updates](https://herd.laravel.com/docs/macos/getting-started/updates)。

fabDev 建議：

- [ ] 提供 `php`、`composer`、`artisan` shim。
- [ ] 從目前工作目錄向上尋找 `fabdev.yml` 或 Site Registry 對應路徑。
- [ ] Site 目錄使用該 Site PHP，其他目錄使用全域 PHP。
- [ ] `composer` 使用 fabDev 管理的 PHAR，不依賴 Herd 或 Homebrew PHP。
- [ ] 提供 `fabdev php`、`fabdev composer`、`fabdev artisan` 的顯式形式，方便 CI 與 AI Agent。
- [ ] 顯示實際 PHP 路徑、版本與 Site，避免靜默選錯 Runtime。

### 4.3 通用診斷中心與 Log Viewer

Herd Log Viewer 能依專案選擇、持續讀取及搜尋 Log；Dumps 則可聚合最近請求的 Log。參考：[Log Viewer](https://herd.laravel.com/docs/macos/debugging/logs)。

fabDev 建議先做框架無關能力：

- [ ] `diagnose_site` 依序檢查 DNS、53／80／443、Nginx、憑證、PHP-FPM、document root 與 MariaDB。
- [ ] 聚合 Agent、Nginx access/error、PHP-FPM、PHP error 與 MariaDB Log。
- [ ] 依 Site、時間、層級及關鍵字篩選；支援複製已遮罩的診斷報告。
- [ ] 清楚區分「未啟動」、「Port 衝突」、「設定錯誤」、「Runtime 遺失」與「應用程式 5xx」。
- [ ] 自動遮罩密碼、Token、Cookie、Authorization header、DSN 與使用者路徑中的敏感片段。

驗收重點：遇到 `demo.test` 無法開啟時，使用者不必手動執行多條命令，就能看到故障層級與安全的修復建議。

### 4.4 `fabdev-mcp`

Herd 透過 MCP 向外部 AI Client 提供 `site_information`、`debug_site`、Site／PHP／Service／HTTPS 管理工具，並建議採 per-project 設定。參考：[AI Integrations](https://herd.laravel.com/docs/macos/advanced-usage/ai-integrations)。

第一階段唯讀：

- [ ] Resource：`site_information`、`service_status`、`runtime_information`。
- [ ] Prompt：`diagnose_site`。
- [ ] Tool：`list_sites`、`get_site_status`、`get_php_versions`、`get_service_status`、`tail_logs`。
- [ ] 以 `SITE_PATH` 或 Site ID 限制範圍，預設不得跨專案列出資料。
- [ ] 所有資料由既有 Agent Protocol 取得，不直接讀取任意路徑或執行 Shell。

第二階段受控變更：

- [ ] `set_site_https`、`set_site_php`、`start_service`、`stop_service`。
- [ ] 安裝 Runtime／Service、刪除資料或信任 CA 不列入無確認自動操作。
- [ ] 每個變更回傳前後狀態、可回復資訊與遮罩後的錯誤。

### 4.5 Runtime 更新偵測與支援矩陣

Herd 會檢查 App、PHP 與 Node.js 更新，並支援多個 PHP 主版本。參考：[Updates](https://herd.laravel.com/docs/macos/getting-started/updates)。

fabDev 建議：

- [ ] 補齊 PHP 8.3 Community Runtime。
- [ ] 評估 PHP 8.5，並明確記錄 ERP 專案相容性。
- [ ] Runtime Catalog 提供可用更新、安全更新、平台、架構與雜湊狀態。
- [ ] 更新前保留舊版本；成功驗證後才切換 `current`。
- [ ] Site 使用中的版本不得被直接移除。
- [ ] 通知可關閉，但安全更新需清楚標示風險。

## 5. P1：選裝工具與服務

### 5.1 PHP Extension 管理

Herd 提供廣泛的 PHP Extension，並允許額外安裝與啟用。參考：[PHP Extensions](https://herd.laravel.com/docs/macos/technology/php-extensions)。

fabDev 優先順序：

1. Redis。
2. LDAP。
3. ODBC、`sqlsrv`、`pdo_sqlsrv`。
4. `pgsql`、`pdo_pgsql`。
5. MongoDB。
6. Xdebug 與 Profiler Extension。

要求：

- [ ] Extension Package 必須依 OS、CPU、PHP API 與 minor version 分開。
- [ ] 安裝前驗證來源、簽章／SHA-256 與 Mach-O／DLL 依賴。
- [ ] 透過受管 `php.ini` 啟用或停用，先驗證 CLI 與 FPM，再重啟服務。
- [ ] 不允許封裝殘留建置機的 `/opt/homebrew` 或使用者絕對路徑。

### 5.2 Node.js 多版本

Herd 能管理多個 Node.js 版本，並在 Site 選擇版本時建立 `.nvmrc`。參考：[Manage Node.js](https://herd.laravel.com/docs/macos/technology/node-versions)、[Managing Sites](https://herd.laravel.com/docs/macos/sites/managing-sites)。

fabDev 建議：

- [x] Node Runtime 與 Desktop 建置用 Node 完全隔離。
- [x] 提供單一穩定 LTS 的選裝、移除及每 Site 啟用／停用。
- [ ] 支援多版本、全域 Node 與每 Site `.nvmrc`／`fabdev.yml` 版本。
- [ ] 提供 `node`、`npm`、`npx`、Corepack、pnpm、Yarn shim。
- [ ] 評估是否由 Agent 管理 `npm run dev`；預設不自動執行專案 script。
- [ ] Windows 與 macOS 使用相同版本選擇規則。

### 5.3 選裝服務目錄

Herd 的服務管理涵蓋資料庫、Cache／Queue、Broadcast、搜尋與 Object Storage，並提供版本、Port、啟停、clone 與資料目錄操作。參考：[Services](https://herd.laravel.com/docs/macos/herd-pro-services/services)、[Version Matrix](https://herd.laravel.com/docs/macos/herd-pro-services/service-versions)。

fabDev 建議順序：

- [ ] Redis 或 Valkey：ERP Session、Cache、Queue，優先度最高。
- [ ] PostgreSQL：跨資料庫專案需求。
- [ ] MySQL：與 MariaDB 並存但資料、Port、Socket 完全隔離。
- [ ] MongoDB：只在實際專案需求確認後加入。
- [ ] Reverb／相容 WebSocket service：作為即時功能選配。
- [ ] Meilisearch／Typesense：搜尋服務選配。
- [ ] MinIO／RustFS：S3 相容開發儲存選配。

每個服務必須具備版本化 Runtime、獨立資料目錄、Port 衝突檢查、狀態恢復、更新與移除時保留資料的明確選項。

### 5.4 本機 Mail Catcher

Herd Mail 以本機 SMTP 攔截郵件，依 Site 分組，並檢視 Header、HTML、raw content 與附件。參考：[Mail](https://herd.laravel.com/docs/macos/herd-pro-services/mail)。

fabDev 建議：

- [ ] 只綁 loopback 的 SMTP listener，預設不得向外投遞。
- [ ] 依 Site ID 或 SMTP username 分 inbox。
- [ ] 顯示 HTML、純文字、Header、raw source 與附件。
- [ ] 提供容量、保留天數、全部清除及敏感內容警告。
- [ ] 產生 Laravel、一般 PHP 與 Legacy ERP 可直接使用的設定片段。

### 5.5 Xdebug 與 IDE 整合

Herd 內含各 PHP 版本的 Xdebug，並能按需啟用；Pro 整合可依 breakpoint 自動偵測。參考：[Xdebug](https://herd.laravel.com/docs/macos/debugging/xdebug)。

fabDev 建議：

- [ ] Xdebug 作為每 PHP minor 的選裝 Extension，不預設常駐啟用。
- [ ] 提供 Debug／Develop／Coverage 模式與 `start_with_request` 設定。
- [ ] 產生 VS Code、PhpStorm 的 Site path mapping 範例。
- [ ] UI 明確顯示效能影響，並可一鍵停用及安全重啟 FPM。

### 5.6 Site Manager 操作效率

Herd Site Manager 提供 Favorites、Groups、Terminal、Editor、Database、Log、Tinker、Profiler 與 framework information。參考：[Managing Sites](https://herd.laravel.com/docs/macos/sites/managing-sites)。

fabDev 建議：

- [ ] Favorites 與 Groups；只影響 UI，不改變磁碟路徑。
- [ ] Open Terminal、Open Editor、Open Web Root、Open Logs。
- [ ] 從 Site 的受控 DB 設定開啟 AdminerEvo 或使用者指定的 DB Client。
- [ ] 顯示 PHP、framework、document root、HTTPS、MariaDB 與最近健康檢查。
- [ ] 支援 Site aliases，但不得改用公共 TLD。

## 6. P2：進階診斷與整合

### 6.1 Dumps 與請求事件追蹤

Herd Dumps 可攔截 `dump()`，並顯示 Query、Job、View、outgoing HTTP 與 Log。參考：[Dumps](https://herd.laravel.com/docs/macos/debugging/dumps)。

fabDev 不應直接複製 Laravel 注入方式，建議：

- [ ] 先定義框架無關的診斷事件格式與 per-request correlation ID。
- [ ] 一般 PHP 先支援 PHP error、request metadata、slow request 與 DB slow query。
- [ ] Laravel Adapter 再加入 Query、Job、View、Dump 與 outgoing HTTP。
- [ ] Legacy ERP Adapter 應明確 opt-in，不得修改正式專案原始碼。
- [ ] 所有攔截可完全停用，並量測對請求時間與記憶體的影響。

### 6.2 PHP Profiler

Herd 使用客製 SPX 支援 Web、CLI 與長時間 CLI profiling。參考：[Profiler](https://herd.laravel.com/docs/macos/debugging/profiler)。

- [ ] 評估 SPX 授權、PHP 7.4／8.2／8.3／8.4／8.5 相容性與跨平台建置。
- [ ] 預設不收集資料，只在使用者明確啟用時 profile。
- [ ] Profile 儲存在 fabDev 資料目錄，具容量限制與清除功能。
- [ ] 不把固定診斷路由注入所有 Site；使用 per-Site、短效授權入口。

### 6.3 公開安全分享

Herd 可透過 Expose 或 ngrok 將本機 Site 暫時公開，支援 HTTPS 與 Basic Auth。參考：[Sharing Sites](https://herd.laravel.com/docs/macos/sites/sharing-sites)。

fabDev 現有 LAN Site Share 不是公開 Tunnel，兩者不得混用：

- [ ] 僅支援使用者自行安裝並登入的 Tunnel Provider。
- [ ] 預設關閉；啟用時顯示資料外流、Webhook 與客戶資料風險。
- [ ] 強制短效 Session、明確 Site allowlist、隨機 URL，並建議 Basic Auth。
- [ ] Stop All、Quit 或 Agent upgrade 必須終止 Tunnel。
- [ ] 不保存 Provider Token 明文，也不把 Token 暴露給 MCP。

### 6.4 Framework Driver／Adapter

Herd 支援 framework-specific information、Log path 與 Custom Driver。參考：[Supported Frameworks](https://herd.laravel.com/docs/macos/extending-herd/supported-frameworks)、[Custom Drivers](https://herd.laravel.com/docs/macos/extending-herd/custom-drivers)。

fabDev 建議先定義只描述能力的 Adapter：

- [ ] `detect`：Laravel、WordPress、一般 PHP、指定 ERP 類型。
- [ ] `information`：版本、入口、設定與健康檢查，但不輸出秘密。
- [ ] `logs`：標準與自訂 Log 路徑白名單。
- [ ] `commands`：只允許宣告過的 Composer／Artisan／framework command。
- [ ] Adapter 不得直接取得 Helper、任意 Shell 或 Agent 管理權限。

## 7. P3：暫緩或需獨立產品決策

### 7.1 部署平台整合

Herd 可連結 Laravel Forge，並透過 MCP 取得最近部署資訊。參考：[Laravel Forge](https://herd.laravel.com/docs/macos/integrations/laravel-forge)、[AI Integrations](https://herd.laravel.com/docs/macos/advanced-usage/ai-integrations)。

fabDev 目前應專注本機開發環境。若未來實作：

- 採 provider-neutral deployment adapter，不讓 Core 依賴單一平台。
- Token 只進 OS Credential Store，不進 SQLite、Log、`fabdev.yml` 或 MCP resource。
- 預設唯讀顯示部署狀態；部署、重啟或修改環境變數需另行授權。

### 7.2 Social Auth Callback Relay

Herd 以 `fwd.host` 將公共 callback 轉回 `.test` Site。參考：[Social Authentication](https://herd.laravel.com/docs/macos/advanced-usage/social-auth)。

fabDev 不應直接依賴 Herd 的公共服務。若有需求，優先使用使用者自帶 Tunnel 或另行設計受控 Relay，並完成 callback allowlist、短效 Token、稽核與隱私評估。

## 8. 明確不照搬

- 不使用或重新封裝 Herd 的 NVM、PHP、Nginx、dnsmasq、Extension 或其他 binary。
- 不覆蓋、刪除或匯入 Herd 設定；共存檢查維持唯讀。
- 不提供會刪除 parked project 資料夾的 UI；移除 Site 預設只刪除 fabDev 設定。
- 不讓 MCP 讀取完整 `.env`、任意檔案、資料庫內容、私鑰或 Credential Store。
- 不讓 MCP、Framework Adapter 或 Desktop 直接執行任意 root／Administrator 命令。
- 不把 Laravel-specific instrumentation 變成一般 PHP Site 的必要依賴。
- 不把 LAN Site Share 宣稱為安全的公開分享或正式 ERP Hosting。
- 不因參考 Herd Pro 功能而直接沿用其產品分級或商業模式。

## 9. 建議實作順序

1. `fabdev.yml` schema、dry-run 與安全套用。
2. PHP／Composer／Artisan shim。
3. 通用 `diagnose_site` 與 Log Viewer。
4. 唯讀 `fabdev-mcp`。
5. Runtime 更新偵測、PHP 8.3／8.5 評估。
6. Redis／Valkey 與 PHP Extension 管理。
7. Node.js 多版本與選用的專案感知 shim。
8. Xdebug、本機 Mail Catcher、Site 操作捷徑。
9. MCP 受控變更工具。
10. Dumps／Profiler／Framework Adapter。
11. 公開 Tunnel 與部署整合只在明確產品需求出現後評估。

完成每一項後，應同步更新 `docs/FABDEV_PROGRESS.md`，並把已完成項目從本文件移至「已完成，不列入本 Backlog」。
