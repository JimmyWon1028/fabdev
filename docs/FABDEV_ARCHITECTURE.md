# fabDev 產品與服務架構

> 狀態：Desktop、Agent 與 53／80／443 本機服務及每 Site HTTPS 已完成整合
> 更新日期：2026-08-25
> 本文件記錄架構決策；即時進度與優先工作見 `docs/FABDEV_PROGRESS.md`。

## 1. 產品定位

fabDev 是協助快速開發 ERP Web 應用的本機開發輔助 App。目標是提供類似 Laravel Herd 的服務架構與操作體驗：開啟一個 App，即可安裝、啟動、停止、監控及設定專案需要的本機開發服務。

fabDev 本身不是 ERP 系統或 ERP Runtime，也不是低程式碼設計器。ERP 專用工具可在核心環境穩定後逐步擴充。

### 1.1 目前產品範圍

目前這個專案只提供單機、單人使用的本機開發環境。資料庫、管理 API、Agent Protocol 與特權 Helper 必須維持本機安全邊界，也不把 Desktop 開發模式宣稱為 Production Server。

為方便開發者用另一台電腦或 VM 驗證瀏覽器相容性，Desktop 可以由使用者明確選擇多個 Site，暫時共用一個高位 Port 的 `LAN Site Share`。這是最多 1–2 台 Client 的無驗證 HTTP 開發預覽，不是多人產品功能；不得公開到網際網路，也不得開放 MariaDB、Agent 或管理介面。

區域網路多人使用與正式 ERP 承載屬於未來獨立產品 `fabDev Server`。其技術架構記錄於第 15 節，僅作為未來產品規劃，不納入目前 fabDev Desktop 的實作範圍。

## 2. 平台範圍

- 最終目標支援 macOS 與 Windows。
- 第一版先完成 macOS。
- 共用核心不得直接依賴單一作業系統功能。
- macOS 使用 dnsmasq、系統 Helper 與 macOS 憑證機制。
- Windows 未來透過 Platform Adapter 使用 `hosts`、Windows Service 與 Certificate Store。

```text
fabDev Desktop
      ↓ Local IPC
fabDev Core Service
      ├─ Site Manager
      ├─ Runtime Manager
      ├─ Service Supervisor
      ├─ Update Manager
      └─ Diagnostics
             ↓
      Platform Adapter
      ├─ macOS
      └─ Windows
```

Desktop 主程式保持一般使用者權限。只有 DNS、系統服務、受保護連接埠及憑證等操作交由最小權限的 System Helper 執行。

## 3. 核心元件

第一階段的必要元件如下：

- fabDev Desktop App
- fabDev Core Service
- macOS System Helper
- 內建 dnsmasq 與 `.test` wildcard DNS
- 內建 Nginx
- 內建 PHP 7.4／8.2 與選裝 PHP Runtime Manager
- PHP-FPM Manager
- Site Manager
- Log、健康檢查與錯誤診斷
- Runtime 更新偵測與通知
- 選用的 LAN Site Share 與 Windows `fabdev-connect`

PHP 8.4、Node.js、MariaDB 及未來的 Redis 等服務都是選用元件，不能成為 fabDev Core 的啟動依賴。

Desktop 首次啟動若 Site Registry 完全空白，必須從 App 內建 fixture 複製 fabDev 自有 Demo 專案並建立唯一的 `demo.test`，預設指定 PHP 8.2。只要 Registry 已有任何 Site，就不得新增 Demo、覆蓋專案或修改既有 Site。

## 4. Site 與本機網域

第一個里程碑只實作單一 linked site；不匯入 Herd Site，也不實作 Park。新增 Site 時由使用者選擇專案資料夾，網域依資料夾名稱自動產生並可修改。若存在 `public/index.php`，自動使用 `public/` 作為 Web Root；否則使用專案根目錄，並允許手動調整。

fabDev 同時管理 Site Home parked path 與 linked sites：

- Site Home：預設使用 `~/Sites`，其中每個第一層非隱藏資料夾自動取得 `<directory>.test`；可在 Sites 畫面變更路徑並重新掃描。
- Link：將指定專案目錄連結至自訂 `.test` 網域。
- Link 設定不因 Site Home 掃描而移除；網域衝突時 Link 優先。
- 每個 Site 保存專案路徑、網域、可選的 PHP 版本與 HTTPS 等設定。

macOS 請求流程：

```text
site.test
  → macOS Resolver
  → dnsmasq
  → 127.0.0.1
  → Nginx
  → 指定版本的 PHP-FPM
  → ERP 專案
```

### 4.1 LAN Site Share 開發預覽

LAN Site Share 只在使用者於 Sites 畫面按下分享時啟動，預設使用主機 `0.0.0.0:18080`，將 HTTP 流量轉送到 fabDev Nginx 的 `127.0.0.1:8080`。多個已選 Site 共用同一 Listener；分享入口先以動態 HTTP `Host` 白名單拒絕未選 Site，再由 Nginx 依原始 `Host` 分流。停止或移除某個 Site 會立即更新白名單，最後一個 Site 停止、Stop All、Agent Shutdown 或 App Quit 才關閉 Listener 並釋放 Port。

Windows Client 使用獨立的 `fabdev-connect.exe`：程式要求 UAC 後只在 Client 的 `127.0.0.1:80` 監聽，建立受標記的 `hosts` 區塊，把使用者輸入的多個 `.test` 網域指向 `127.0.0.1`，再以非同步雙向代理把瀏覽器 TCP 流量送到主機的 `18080`。中斷或正常結束時會移除自己管理的 `hosts` 區塊，且不覆寫既有同名紀錄。

```text
Windows 瀏覽器：http://site-one.test
  → Windows hosts：site-one.test = 127.0.0.1
  → fabdev-connect：127.0.0.1:80
  → LAN：fabDev 主機 IP:18080
  → fabdev-share：127.0.0.1:8080
  → Nginx（依 Host: site-one.test 路由）
  → PHP-FPM／ERP 專案
```

此路徑沒有 TLS、登入、Client 授權、流量限制或 Site 隔離保證，主機高位 Port 也會被區域網路看見。因此它的驗收範圍只包含 Windows 11 瀏覽器可用、正常中斷後清理 `hosts` 與 Port，以及 1–2 台 Client 的短時間開發測試；10 人、長時間穩定運行與正式 ERP 承載仍屬第 15 節的 `fabDev Server`。

## 5. PHP 多版本管理

目前要求可選擇安裝 PHP 7.4、8.2、8.3 與 8.4；是否納入 8.0、8.1 及後續大版本仍待確認。

PHP Manager 必須支援：

- 選擇性下載、安裝、更新及移除。
- 多個版本並存，不互相覆蓋。
- 設定一個全域預設 PHP。
- 每個 Site 可指定獨立 PHP 版本，或不使用 PHP。
- 未指定版本的 Site 使用全域預設版本。
- 相同版本的 Site 共用同一套 PHP Runtime，不重複安裝。
- 各 PHP 版本使用獨立的 CLI、PHP-FPM、Socket、`php.ini`、Extension 與 Log。
- Nginx 依 Site 設定轉送至對應的 PHP-FPM Socket；不使用 PHP 的 Site 只提供靜態檔案。
- 在 Site 目錄執行 PHP、Composer 或 Artisan 時，CLI 使用該 Site 的 PHP；其他目錄使用全域版本。
- 指定版本未安裝時顯示安裝或變更選項，不得靜默改用其他版本。
- 偵測修補版本與新增大版本並通知使用者，由使用者決定是否更新。
- 更新需驗證下載檔案並保留失敗回復能力。

範例：

```text
Global PHP：8.3
site1.test：Default → PHP 8.3
site2.test：Isolated → PHP 7.4
site3.test：Isolated → PHP 8.4
```

## 6. 選用 Node.js

Node.js 不一定安裝。未安裝 Node.js 時，PHP Site 與 fabDev 核心仍須正常運作。Windows x64 Catalog 提供經固定 SHA-256 與上游簽章驗證的 Node.js 20.20.2 與 24.20.0 選裝 Runtime；兩者皆不隨 App 預設安裝，Node.js 20 必須標示為 EOL 相容版本。

目前 Site 模式：

```text
None
fabDev Managed
```

左側 Node.js 頁面獨立負責多版本並存安裝、顯示狀態、切換全域版本及個別移除；Sites 暫不保存 Node.js 選擇。Runtime 安裝於 fabDev Application Support 的 `runtimes/node/<version>`。單純安裝不修改 PATH；使用者明確設為全域時才建立動態 shim 並加入使用者 PATH，且不修改 Homebrew、nvm、Herd 或系統 Node.js。

`.nvmrc`／`fabdev.yml` 的 Site 自動選擇、pnpm／Yarn 的 Site-aware shim，以及由 fabDev 啟動及監控 `npm run dev` 仍屬後續工作；預設不得自動執行專案 script。

Desktop App 若使用內建 Node.js，其 Runtime 只供 fabDev 自身使用，不可當作專案的 Node.js 環境。

### 6.1 獨立 Proxy Manager

Proxy Manager 是 Agent 管理的獨立本機服務，不屬於 Node.js Runtime 或一般 Site。全新安裝的 Proxy 清單必須為空，不得預載任何 Connection；使用者可自行新增及移除 Connection。每個 Connection 保存 Domain、固定 loopback Listen Host、非特權 Listen Port、HTTP Remote Target 與受控 CORS allowlist。UI 不接受任意 Script、路徑、Shell、Listen Host 或 HTTPS Target。

每個 Connection 可單獨啟動、停止及重新啟動，並提供全部啟動／全部停止。所有 Listener 固定綁 `127.0.0.1`；單一 Port 衝突只讓該 Connection 進入 Failed，單一 Remote Target 連線錯誤或定期 TCP Health Check 失敗只讓該 Connection 進入 Degraded，不得影響其他 Connection、Web stack、MariaDB 或 Agent Protocol。

轉送面由 `fabdev-proxy` Rust Runtime 提供 HTTP/1.1 streaming reverse proxy，改寫上游 Host，維持既有允許來源的 Credential CORS 與 OPTIONS preflight 行為，並限制上游建立連線時間。停止時先關閉 Listener，再等待既有連線 drain；逾時才取消剩餘連線，最後確認 Port 可重綁。

完整 Connection 設定及使用者明確啟動的 Connection ID 保存在 SQLite。新增 Connection 預設停止；移除執行中的 Connection 時，Agent 先停止 Listener、drain 既有請求並釋放 Port，再移除設定。Agent 啟動時恢復保存的設定與啟動狀態；Proxy Manager 的單獨停止或全部停止會更新偏好，Web stack 的 Start All／Stop All 不影響 Proxy。App Quit、Agent Shutdown 或協定升級的暫時清理不得將偏好覆寫成停止。若其他程序已占用相同 Port，fabDev 只回報 Failed，不終止或接管外部程序。Desktop 可用版本化 JSON 匯出／匯入 Connection；匯入時 ID、Domain 或 Listen Port 任一重複即略過，且不匯入執行狀態。

Agent Protocol 25 提供 Proxy Manager 查詢、新增／編輯／移除、單獨啟動／停止與全部啟動／停止。新增或編輯時必須驗證小寫且唯一的 ID、唯一的 `.test` Domain、唯一的 1024–65535 Port、完整 HTTP Remote Target，以及精確的 HTTP／HTTPS Credentials Origin；Listen Host 由 Agent 固定為 `127.0.0.1`。HTTPS upstream、WebSocket 與受控 LAN Share 需各自完成安全及相容性驗證後再開放。

## 7. 選用 MariaDB

MariaDB 是選用的 Managed Service，不是 Web stack 的啟動依賴。目前先支援 macOS ARM64 MariaDB 12.3.2；Windows 預留安裝版與 Portable 版，實作前再確認各自的 Runtime、資料與升級策略。

PHP 專案的 MariaDB 連線來源由 Managed Service 是否實際運行自動決定：

```text
fabDev Managed
System / Homebrew
```

目前開發電腦上的 MariaDB 12.3.2 是 Homebrew 管理的外部服務，使用 `127.0.0.1:3306` 與 `/tmp/mysql.sock`。fabDev Managed MariaDB 未運行時，PHP-FPM 設定產生器自動使用 System／Homebrew Socket；Managed 啟動成功且實際 Socket 可用後才改用 fabDev 管理的 Socket，停止後立即切回 System Socket。Desktop 不顯示連線來源或 Socket 選項，也不要求使用者進入 MariaDB 頁面或手動儲存。System Socket 是內部連線細節；Unix Agent 依序檢查已保存的有效 Socket、`/tmp/mysql.sock`、`/opt/homebrew/var/mysql/mysql.sock` 與 `/usr/local/var/mysql/mysql.sock`，Windows 則使用保存的 Named Pipe／TCP 設定。Managed 運行狀態在 Unix 由實際 Socket 判定，在 Windows 由 fabDev PID 檔與 TCP readiness 共同判定。System／Homebrew 模式讓 PHP 專案與 Adminer 可直接用 `localhost` 登入，但不啟動、停止、升級、刪除、接管或修改外部 MariaDB。若 3306 已被占用，fabDev Managed MariaDB 必須明確拒絕啟動。

fabDev Managed MariaDB 使用獨立 Runtime、資料目錄、設定、PID、Socket 與 Log，只綁 `127.0.0.1`。首次初始化提供本機 PHP 專案常用的 root 空密碼連線；左側 MariaDB 頁面可在服務運行時透過一個多帳號 `ALTER USER` 同步設定 `root@127.0.0.1` 與 `root@localhost`，使 TCP 與 Unix Socket 使用相同密碼，但不建立 `root@%`。變更時第一次可留空目前密碼，後續需提供目前 TCP root 密碼；密碼只透過 Agent 的本機 IPC 送入 MariaDB Client stdin，不寫入設定、Log 或命令列參數。

左側 MariaDB 頁面在 Managed Runtime 已安裝時提供 TCP Port、Data Directory、額外 MariaDB 選項及獨立啟動／停止，不提供連線來源或 Socket 選項。結構化 Managed 設定存放於 `config/mariadb.json`；為相容舊版，System Socket 仍另存於 `config/mariadb-connection.json`，避免舊版 App 以舊格式重寫 `mariadb.json` 時破壞 `localhost` 連線。額外選項在 macOS 存放於 `config/mariadb/my.cnf`，Windows 預留 `config/mariadb/my.ini`。啟動時先合併額外選項，再產生 `services/mariadb/my.cnf`；Port、Runtime/Data Directory、Socket、PID、Log 及 `bind-address` 等受管選項由 fabDev 最後覆寫，且不能在額外設定中指定。Managed 設定只能在 MariaDB 停止時儲存，儲存前由安裝的 `mariadbd` 驗證，並拒絕 `!include`／`!includedir`；Data Directory 必須是空目錄或含 `mysql` 系統資料庫的既有 MariaDB 目錄，不搬移或刪除舊資料。`bind-address` 固定為 `127.0.0.1`。

MariaDB 最後一次成功啟動或停止的預期狀態存放於 `state/mariadb.json`。App 啟動時經 Agent Protocol 13 要求恢復該狀態：記錄為啟動且 Runtime 已安裝時才啟動；記錄為停止、尚未產生記錄、或服務已在運行時不執行動作。這個狀態獨立於 Web stack 的自動啟動偏好；Agent 協定升級的暫時關閉不改寫預期狀態。

macOS 首次初始化會從 Runtime 根目錄以相對路徑執行上游安裝腳本，避免 `Application Support` 等含空白的絕對路徑被腳本錯誤拆分。

MariaDB 有獨立的 Install／Start／Stop／Remove。Web 的 Start All／Stop All 只管理 DNS、Nginx 與 PHP，不得連動 MariaDB；停止 MariaDB 只清理程序、Port、PID 與 Socket，保留設定及資料。主控台移除前必須確認 MariaDB 已停止，移除只處理 Runtime 與 `current` 連結；移除 Runtime 和刪除資料必須是兩個不同操作。

Agent 重啟時會以 fabDev 專用的 MariaDB PID 檔及可連線的 Unix Socket 恢復既有程序狀態，使主控台繼續顯示運行中並可單獨停止。PID 已失效或 Socket 無法連線時不得接管程序。

## 8. 資料與服務隔離

fabDev 的 Runtime、設定、狀態、Cache 與 Log 應存放在自己的 Application Support 目錄，不修改 Herd 或 Homebrew 的內容。

```text
fabDev/
├─ config/
├─ runtimes/
├─ services/
├─ sites/
├─ logs/
├─ cache/
└─ state/
```

所有受管服務使用一致的生命週期狀態：

```text
NotInstalled → Installed → Starting → Running
Running → Stopping → Stopped
Installed → Updating
任意狀態 → Failed
```

## 9. 本機測試邊界

正式進行 DNS、Nginx、PHP-FPM 與 `.test` 網域整合測試前，使用者會停用 Herd。測試前仍須先以唯讀方式確認 Herd Helper、dnsmasq、Nginx 及 PHP-FPM 已停止，避免 Port、Socket、DNS、憑證與 PATH 衝突。

fabDev 不得覆蓋或刪除 Herd 設定。現階段也不接管既有 Homebrew MariaDB。

2026-08-22 已確認 Herd App、Helper、dnsmasq、Nginx 與 PHP-FPM 停止，53、80、443 Port 均已釋放。`/etc/resolver/test` 仍存在並指向 `127.0.0.1`；fabDev 必須記錄其為既有設定，不得在解除安裝時誤刪。

## 10. 已確認的技術架構

fabDev 採用 Tauri 2 + Vue 3／TypeScript + Rust Core Agent + 平台原生 System Helper。不可將所有服務管理邏輯放入 Tauri Desktop 單體。

```text
Tauri Desktop（Vue 3／TypeScript）
  → Tauri IPC
Rust Core Agent（一般使用者權限）
  ├─ Site／Runtime／Process／Update／Diagnostics
  ├─ Unix Domain Socket／Named Pipe → fabDev CLI
  └─ 平台安全 IPC
       ├─ macOS：Swift XPC Helper／SMAppService／LaunchDaemon
       └─ Windows：Rust Windows Service／ACL Named Pipe
```

### 10.1 Desktop

- Tauri 2、Vue 3、TypeScript、Vite、Pinia、Vue Router。
- UI 可使用 FabUI／FabGrid，但不得直接修改系統或執行任意 Shell Command。
- WebView 只呼叫明確定義並限制權限的 Tauri Commands。
- fabDev 內部建置用的 Node.js 與使用者選裝的專案 Node.js 必須完全分離。
- macOS 主控台顯示時必須出現在 Dock；關閉主控台只隱藏視窗與 Dock Icon，頂端選單、Agent 及受管服務繼續執行。再次從頂端選單或 Dock 啟用時，喚回既有視窗而不建立重複主控台。

### 10.2 Core Agent 與 CLI

- Rust Core Agent 獨立於 Desktop 視窗運行，負責 Site Registry、設定產生、Runtime、程序、更新與健康檢查。
- Desktop App 內含同版本 Agent；第一次呼叫 Agent 失敗時，自動啟動內建 binary、等待健康檢查並重送原請求。只可移除確認為 Unix Socket 的失效端點，不得覆蓋一般檔案。
- Desktop 主視窗關閉只隱藏 App，不影響受管服務；從 menu bar 明確 Quit 時必須先停止 Web 全部服務與 MariaDB、清理受管孤兒程序並關閉 Agent，確認完成後才退出 Desktop。CLI 不依賴 Desktop 視窗。
- CLI 使用 Rust，與 Agent 共用 Domain Model 及 Typed Protocol。
- Desktop／CLI 對 Agent 使用版本化的 Typed JSON Protocol；macOS 優先使用 Unix Domain Socket，Windows 使用 Named Pipe，不使用公開 TCP Port。

### 10.3 特權 Helper

- macOS Helper 使用 Swift、XPC、SMAppService 與 LaunchDaemon。
- Windows Helper 使用 Rust Windows Service 與 ACL 保護的 Named Pipe。
- Helper 只提供白名單操作，例如 DNS、Nginx、憑證與 `hosts`；禁止提供 `runAsAdmin(command)` 類型的通用接口。
- Desktop 與 Core Agent 維持一般使用者權限。

### 10.4 狀態與專案設定

- 本機狀態使用 SQLite，不使用 macOS 專屬 CoreData。
- 可進版控的專案設定預留 `fabdev.yml`。
- SQLite 保存 Sites、Runtimes、Service Instances、更新狀態、操作紀錄及 App Settings。
- `fabdev.yml` 保存網域、PHP 版本、HTTPS 與選用服務需求。

### 10.5 Runtime 與更新

- Nginx、dnsmasq、PHP、Node.js 與未來 MariaDB 按 OS、CPU 架構及版本分開發布。
- Runtime Catalog 至少包含名稱、版本、平台、架構、下載 URL、大小、SHA-256、nullable Package signature 與上游來源驗證紀錄；Unsigned Community v1 的 Catalog／Package signature 固定為 `null`。
- 安裝採暫存下載、驗證、健康檢查、原子切換及失敗回復。
- P1 Unsigned Community App 更新由 Desktop 經 Tauri Command 呼叫 `crates/updater`，固定讀取 Public GitHub Releases 的 Stable Manifest。網路連線使用平台原生 TLS、系統 Proxy 與系統信任庫，不接受 UI 或 Manifest 指定任意更新來源。
- App Manifest 必須通過 schema、產品、Stable Channel、版本、發布時間、平台、架構、完整安裝包模式、官方 GitHub Release URL、檔名、大小與小寫 SHA-256 驗證。Unsigned Community 的 `signature` 必須為 `null`；正式數位簽章留待 P3。
- 完整 DMG／Setup.exe 下載至 `.part`，完成大小與 SHA-256 驗證後才原子改名；開啟安裝包前必須使用先前快取的 Manifest 再驗證一次。下載或檢查失敗不阻止 App 啟動，也不能留下可被誤認為完整安裝包的檔名。
- App 不在背景直接覆蓋自己。使用者確認安裝後，Desktop 先走既有安全 Quit 流程，停止 Web、MariaDB、受管程序與 Agent，再開啟已驗證的完整安裝包並退出。
- 未來 Tauri signed updater 只更新 fabDev App、Agent、Helper 與 CLI；Runtime 在 P3 導入獨立 signed catalog，P2 Community v1 先使用固定 GitHub Release Catalog 與 SHA-256 完整性驗證。
- App 更新、Runtime 更新及專案設定遷移不得混成單一不可回復操作。
- macOS fabDev App 內建 dnsmasq、Nginx、PHP 7.4 與 PHP 8.2；啟動時只補齊缺少且未被使用者明確移除的內建 Runtime，保留既有版本、設定與 Site。PHP 7.4／8.2 可移除，移除標記會阻止下次啟動自動補回，明確重新安裝成功後才清除。PHP 8.3、PHP 8.4 及其他服務維持獨立選裝。
- 第一個 Runtime 使用 PHP 8.2.33 官方原始碼建置，目標為 macOS ARM64；不得使用 Herd Binary，執行時也不得依賴 Homebrew。
- 開發階段允許 Homebrew 提供編譯工具與相依函式庫，但 Runtime 必須封裝所需動態函式庫並修正 `rpath`。
- PHP 更新只偵測並通知，由使用者確認後執行，不得自動替換使用中的版本。

### 10.6 Site 與 Nginx

- 第一版由 Rust Site Driver 偵測 document root、front controller 與 PHP 版本，再產生明確的 Site Nginx 設定。
- 寫入前必須驗證設定；Reload 不得中斷其他 Site。
- Driver 介面保留 Laravel、一般 PHP、WordPress 及 ERP 專用規則，不要求第一版完整複製 Laravel Valet。

### 10.7 平台支援矩陣

```text
第一階段：macOS 13+／Apple Silicon ARM64
macOS 發布前：補齊 Intel x86_64
第二平台：Windows 10 1803+／Windows 11／x64
暫不納入：Windows ARM64、Linux
```

macOS Intel 的雙機開發、功能分支、原生打包與實機驗收流程見 [`MACOS_INTEL_DEVELOPMENT_TEST_WORKFLOW.md`](MACOS_INTEL_DEVELOPMENT_TEST_WORKFLOW.md)。

macOS 使用 WKWebView，Windows 使用 WebView2。UI 必須避免實驗性 Web API，並在兩平台執行互動與視覺測試。Windows Installer 預設檢查並安裝 WebView2；企業離線需求可另提供內含 WebView2 的安裝包。

## 11. 建議 Monorepo

```text
apps/
  desktop/              # Tauri + Vue
crates/
  core/                 # Domain logic
  agent/                # Background agent
  cli/                  # fabdev CLI
  runtime/              # Runtime manager
  sites/                # Site detection and config
  updater/              # Catalog and updates
  platform/             # Platform interfaces
helpers/
  macos/                # Swift XPC helper
  windows/              # Rust Windows Service
packages/
  ui/
  contracts/            # TypeScript IPC contracts
resources/
  nginx/
  dnsmasq/
docs/
```

前端使用 pnpm workspace；Rust 元件使用 Cargo workspace。

## 12. 尚待確認的實作細節

- PHP 8.3 與 Node.js Binary 的實際建置版本。
- App 程式碼簽章、公證、更新 Channel 與私鑰保管流程。
- Composer／Artisan、Site-aware CLI Shim 與 Shell PATH 的使用者授權流程；全域終端機 PHP 的明確啟用／停用流程已完成。
- Node.js 開發伺服器是否納入程序監控。
- PHP 8.0、8.1、8.5 及後續版本的支援範圍。
- Windows Adapter 與安裝器的詳細實作時程。

## 13. 第一個開發里程碑

第一個垂直切片固定為 macOS ARM64、單一 HTTP Site、dnsmasq、Nginx 與 PHP 8.2.33 FPM。Site 使用自訂 `.test` 網域，不包含 HTTPS、Herd Site 匯入、Node.js、MariaDB 或其他 PHP 版本。

PHP 8.2 Runtime 預設包含 `bcmath`、`curl`、`fileinfo`、`gd`、`imagick`、`imap`、`intl`、`mbstring`、`mysqli`、`opcache`、`openssl`、`pcntl`、`pdo_mysql`、`pdo_sqlite`、`soap`、`sockets`、`sodium`、`tidy`、`xml`、`xsl` 與 `zip`。Redis、LDAP 與 ODBC 後續再作為選配 Extension。

關閉 Desktop 主視窗後，Core Agent 與受管服務繼續運行；從 menu bar 選擇 Quit Desktop 則等同停止 Web 全部服務、MariaDB 與 Agent，不得留下任何 fabDev 受管程序。第一個實際 PHP 專案路徑在 Site 整合階段再指定。

## 14. 目前實作與驗證狀態

已完成 Tauri 2／Vue Desktop、Rust Agent／CLI、Unix Socket Typed JSON Protocol、SQLite Site Registry、Site document root 偵測、Nginx Site Driver、服務設定產生、Start All／Stop All 與真實服務狀態。Runtime Manager 可驗證 SHA-256、解開版本化封裝並原子切換 `current`。

PHP-FPM 服務層已支援多 minor 版本並行。Agent 依 Site 的 `phpVersion` 從 `runtimes/php/<major>.<minor>.<patch>/` 選擇該 minor 最高 patch；各版本使用 `services/php/<major>.<minor>/` 下獨立的 `php.ini`、PID、Socket、Session 與 Log。Start All 只啟動 enabled Sites 實際需要的版本，Nginx Site 設定指向對應 Socket；運行中新增第一個使用新版本的 Site 時會先啟動該 FPM，再驗證並 reload Nginx，移除最後一個使用該版本的 Site 後則停止該 FPM。設定或 reload 失敗時會回復 Site 設定與 Registry。

macOS ARM64 開發 Runtime 已由官方原始碼建置：PHP 7.4.33、PHP 8.2.33、PHP 8.4.24、Nginx 1.30.4、dnsmasq 2.93。所有封裝皆驗證上游 SHA-256 與簽章，收納執行期需要的 Homebrew dylib 並使用 ad-hoc code signing；封裝後已確認 Mach-O 不再引用 `/opt/homebrew`。PHP 7.4 使用版本專屬相容性 Patch，處理現代 Clang、libxml2、OpenSSL 3 與 ICU，建置固定 GNU C11／C++17。三套 PHP Runtime 均已內建 Tidy，並封裝 Imagick、IMAP、ImageMagick 設定與 Coder Module；已通過 CLI、PNG 輸出、搬移後載入、FPM 設定及實際 HTTP Site 驗證。已在 8080 同時以獨立 FPM Socket 驗證 `php74.test` 回應 PHP 7.4.33、`site1.test` 回應 PHP 8.2.33；測試 Site 與程序均已清除，PHP 7.4.33 Runtime 保留為已安裝。

macOS System Helper 已完成第一版 Swift 實作：root Helper 只持有 53／80／443，固定轉送至一般使用者權限的 dnsmasq 53535 與 Nginx 8080／8443；不執行 Runtime binary、憑證操作或任意命令。XPC 僅接受 `start`、`stop`、`status`，並以 App identifier 與相同 Team ID 的 Code Signing Requirement 驗證呼叫者。`/etc/resolver/test` 採固定內容、原子寫入與 symbolic link 防護；正式模式不覆蓋或刪除非 fabDev 檔案。fabDev CA 由 Desktop／CLI 驗證固定 Application Support 路徑後加入目前使用者的 Login Keychain，不交給無互動 Session 的 root Helper。非特權整合測試已確認 15353／18080／18443 入口；正式 443 已以 `demo.test` 驗證 HTTPS 200。

在沒有 Developer ID 的本機開發階段，另提供明確標記的 local-test LaunchDaemon 安裝腳本。它只啟動同一組固定 53／80／443 Proxy，不能接受外部 Port、路徑或命令；可沿用內容相容的既有 resolver，但不修改也不在移除時刪除它。Agent 預設只啟動 53535／8080／8443 後端，並同時探測 53／80／443 入口；入口未就緒時，主控台必須回報 Failed，而不是只依程序 PID 顯示 Running。

Unsigned Community Build 將這個安全模型整理為可發佈的 arm64 DMG，不使用 Apple Developer ID 或 notarization。DMG 建置輸入含 ad-hoc 簽署的 App 與 Helper、固定 Community LaunchDaemon、繁體中文安裝指南、安裝／移除 `.command`、SHA-256 manifest；App 內建 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33 與 8.2.33。PHP 8.4.24 與 MariaDB 12.3.2 維持獨立 Community Runtime Package，不放入基礎 DMG。初始 Registry 只在全新安裝且沒有任何 Site 時建立 `demo.test`，不得封裝開發電腦既有的 Site、SQLite、路徑或其他使用者資料。

Community 安裝必須由使用者明確執行並授權 `sudo`；更新保留使用者 Registry、Runtime、`php.ini` 與 TLS 資料。移除程序先停止服務、從 Login Keychain 撤銷受管 CA 信任及移除受管 Helper，再把 App 移到垃圾桶，資料移除則需要第二次明確確認且同樣使用可復原的垃圾桶。Helper 只接受固定 53／80／443 代理設定；Community 安裝腳本只能替換帶有 fabDev 管理標記的 Community 或 local-test LaunchDaemon，並相容舊版標記。

2026-08-23 已在本機安裝 local-test LaunchDaemon，確認 Agent 將 DNS、Nginx 與 PHP-FPM 回報為 Running；`site1.test` 解析至 `127.0.0.1`，HTTP 回傳 200、Nginx 1.30.4 與 PHP 8.2.33，且既有 resolver 的 SHA-256 安裝前後一致。Chrome 控制擴充功能會以 `ERR_BLOCKED_BY_CLIENT` 阻擋本機 HTTP URL，因此自動化瀏覽器畫面驗證未完成，仍需由使用者在一般 Chrome 頁籤手動重新整理確認。

Xcode 26.6／Swift 6.3.3 工具鏈與 macOS 13 deployment target 已可編譯 Helper，Tauri bundle 亦已驗證 `Contents/Resources/fabdev-system-helper` 與 `Contents/Library/LaunchDaemons/com.fabdev.system-helper.plist` 的嵌入位置。`FABDEV_DATA_DIR` 可讓 Desktop 與 Agent 共用隔離資料；已從 Desktop 實際驗證 Agent IPC、Sites 顯示、Stop All／Start All、DNS 轉送與 PHP 8.2.33 HTTP 回應。Runtimes 畫面已在後續 Protocol 3 接上實際安裝狀態；PHP 7.4.33、8.2.33 與 8.4.24 已可安裝，PHP 8.3 的 Runtime Package 尚未建立。

2026-08-23 完成 PHP 7.4 並行驗證後，local-test Helper 曾發生 UDP 53 未轉送至正常回應的 dnsmasq 53535 Backend；Agent 因入口健康檢查失敗而正確回復為 Installed。管理員重新啟動 Helper 後，完整 Start All 已恢復，DNS、Nginx 與 PHP-FPM 均回報 Running。經正式 53／80 入口在運行中新增 `php74.test`，已確認 PHP 7.4.33 與 8.2.33 並行；移除最後一個 7.4 Site 後，7.4 FPM 與 Socket 自動停止，8.2 Sites 持續正常回應。

2026-08-23 Desktop bundle 已內含 `fabdev-agent`，並完成自動啟動與失效 Socket 復原的黑箱驗證。後續生命週期規格已改為：主視窗關閉只隱藏，menu bar Quit 則先停止所有服務與 Agent。正式資料仍沿用既有 Application Support 目錄；Nginx 全域設定及 PHP-FPM Socket 會正確引用含空白的路徑。以 App 自動啟動 Agent 後，已確認 DNS、Nginx、PHP-FPM 全部 Running，三個範例 `.test` Site 均回傳 HTTP 200，其中預設 Site 使用 PHP 8.2.33。

Runtimes 主控台已接上 Agent Protocol 3，可列出實際 PHP patch、全域版本及使用該 minor 的 Sites。使用者可選擇本機 Release JSON 與 `.tar.gz` 安裝套件；Agent 會驗證 PHP 類型、支援系列、平台、架構、檔案大小及 SHA-256。首次安裝才自動設為全域版本，後續安裝不改變選擇；切換使用原子 `current` symlink。全域版本或仍被任何 Site 使用的系列禁止移除，且 Runtime 名稱與版本拒絕路徑元件。Sites 新增表單只提供已安裝的 PHP minor。

隔離端到端測試已確認 PHP 8.2.33 首次安裝、PHP 7.4.33 並存、全域版本雙向切換、使用中移除遭拒，以及移除 Site 後安全刪除 Runtime。PHP 8.4.24 已建立正式 Runtime Package 與 Community Release；PHP 8.3 仍待建立。

Agent Protocol 4 與 Sites 主控台已支援運行中切換 Site PHP minor。切換會先建立並驗證目標 PHP-FPM、更新 Site Nginx 設定與 reload，成功後才停止無人使用的舊 FPM；失敗時回復 Registry 與設定。範例 Site 已從 PHP 8.2 實際切換至 7.4，Nginx 指向 7.4 Socket 且 HTTP 回傳 200；其他範例 Site 持續使用 8.2。

Agent Protocol 5 將 Site 的 PHP 版本改為可選。Sites 主控台的 `-` 代表不使用 PHP；Agent 會保存此狀態並產生不含 FastCGI 規則的 Nginx 靜態 Site 設定，切換後也會停止已無其他 Site 使用的舊 PHP-FPM。

每個 PHP minor 的 `php.ini` 已改為持久保存在 `config/php/<major>.<minor>/php.ini`，服務設定重建不會覆蓋。`config/php/default/php.ini` 是由目前 PHP 8.2 設定去除 Runtime／Service 絕對路徑後建立的範本，只在新 minor 尚未有專屬設定時使用。Runtimes 主控台可讀取、編輯、驗證並套用各 minor 設定，也可編輯預設範本；使用中的 FPM 會重啟，驗證或啟動失敗則回復原設定。PHP 7.4 已完成原內容儲存、FPM 驗證與重啟測試。

目前仍沒有有效 Apple Code Signing Identity，因此尚不能安全驗證 SMAppService 的正式簽署、管理員核准與註冊流程，也尚未接上 Desktop 的 Helper 狀態與控制命令。第一階段先交付 Unsigned Community DMG；它使用使用者主動執行的管理員安裝程序，不把 unsigned XPC 冒充為可信任的 Team ID 通道。此電腦仍有 Herd 留下且相容的 `/etc/resolver/test`；fabDev Community 模式只沿用它，不接管。未來 Signed Distribution Runtime 仍須由 macOS 13 相容的乾淨建置環境產生並完成 Developer ID 簽署與 notarization。

2026-08-23 已產生並唯讀驗證 `fabDev-Community-0.1.0-macos-arm64.dmg`：App signature 為 ad-hoc 且沒有 Team ID，DMG 內部 SHA-256 全數通過；PHP 7.4.33、PHP 8.2.33、Nginx 1.30.4、dnsmasq 2.93、繁體中文指南及安裝／移除程序均存在。隔離 Registry 連續執行兩次 Demo 初始化後仍只有一個 `demo.test`，發佈內容沒有 SQLite、既有 Site domain 或建置者專案路徑。完整 root 安裝／移除流程尚未在目前工作環境執行，以避免取代正在運行的既有 Helper 與使用者服務。

2026-08-24 依內建 Runtime 與服務生命週期新規格重新打包同版 DMG；總覽的 Web 服務控制改為單一「全部啟動／全部停止」狀態按鈕。只讀掛載確認 App 內只有四個內建 Runtime，沒有 PHP 8.4 或 MariaDB；App、Agent 與 CLI 均為 arm64，ad-hoc 簽章、內外層 SHA-256 與每個 Runtime Descriptor 雜湊皆通過。DMG SHA-256 為 `e44f8e93aadf370e4ecc33987bdcba7fc7ee2260810aa3c6fe683f94c1b819d3`。

2026-08-23 Desktop 啟動流程改為自動確保全部開發服務可用：服務已全數運行時不重啟，部分運行或 Failed 時先 Stop All 再啟動，完整停止時直接啟動；錯誤會保留在主控台供診斷。Community Runtime 同時改用 `*-macos-arm64-community` 正式命名、`community-ad-hoc` 描述及獨立 `channel: community` Catalog，開發用 `*-dev` 產物仍維持獨立。

Agent Protocol 7 將 MariaDB Runtime 安裝與移除接上主控台。安裝會驗證 MariaDB 類型、12.3.2 版本、macOS ARM64、檔案大小及 SHA-256；執行中禁止安裝或移除。移除會先解除 `current`，只刪除 Runtime 並保留 `services/mariadb` 下的設定、資料與 Log；刪除失敗時嘗試恢復啟用連結。隔離測試已確認未安裝 → 安裝 → 啟動、執行中拒絕移除、停止 → 移除，以及重新安裝後讀回原資料。

Agent Protocol 8 新增 MariaDB 設定讀寫。主控台可持久修改非特權 TCP Port 與 Data Directory；Agent 驗證目錄為空或既有 MariaDB 資料目錄，並拒絕在 MariaDB 運行時變更。

Agent Protocol 9 新增 MariaDB `my.cnf`／`my.ini` 額外設定讀寫及 root 密碼變更。額外設定由對應 Runtime 驗證且不得覆寫 fabDev 的安全與程序選項；root 密碼不持久化於 fabDev。

Agent Protocol 10 修正 MariaDB 12 的 root 密碼 SQL 語法，並清除失敗回應中的 SQL 與密碼內容。

Agent Protocol 11 將 root 密碼操作固定連至 `127.0.0.1` 的設定 Port，管理 PHP 專案實際使用的 `root@127.0.0.1`，不再誤用 Unix Socket 所匹配的 `root@localhost`。

Agent Protocol 12 將 root 密碼操作改為同步更新 `root@127.0.0.1` 與 `root@localhost`，讓本機 TCP 與 Unix Socket 共用密碼。

Agent Protocol 13 新增 MariaDB 最後啟停狀態恢復。明確的 Start／Stop 會持久更新預期狀態，App 啟動時要求 Agent 恢復；Agent 協定升級期間的暫時停止不會覆寫該狀態。

Agent Protocol 14 強制 Desktop 汰換仍在記憶體中的舊 Agent binary，確保 menubar Quit、受管孤兒程序清理及空 Site 啟動規則使用同一版服務生命週期邏輯；協定升級只替換 Agent 程序，不刪除 Runtime、Site 或設定。

Agent Protocol 15 將 Start All 改為冪等的服務狀態收斂：所需服務已全部運行時直接成功，部分運行或 Failed 時先完整停止再啟動，全部停止時直接啟動。UI 的暫時舊狀態不得造成「already running」錯誤。

Agent Protocol 18 新增單一 LAN Site Share 的查詢、啟動與停止；Protocol 19 將狀態擴充為多 Site 清單，並支援逐一停止 Site。它只控制 Web 轉送，不公開 Agent Protocol 或 MariaDB。

Agent Protocol 20 新增本機 CA 建立與每 Site HTTPS 切換。CA 與 Site 私鑰只存於 fabDev Application Support，Site 憑證 SAN 僅包含標準化 `.test` 網域；Nginx 以 8443 提供 TLS，System Helper 固定代理 443，HTTP Site 則回傳 301。Registry、Nginx reload 或憑證產生失敗時會回復 Site HTTPS 狀態與設定；停用 HTTPS 或移除 Site 只刪除該 Site leaf certificate，保留共用 CA。

Agent Protocol 31 新增 MariaDB 的 PHP 連線來源與 System Socket 設定。Managed 模式維持 fabDev 專用 Socket；System／Homebrew 模式讓 PHP 的 `localhost` 連線使用指定 Socket。

Agent Protocol 32 把 MariaDB 連線資訊獨立持久化，避免舊版設定覆寫造成回歸；Managed Service 未運行時，PHP-FPM 設定產生器會自動改用 System Socket，啟動與停止後立即重新套用。

Agent Protocol 33 提供 Runtime Catalog 檢查、背景下載、操作輪詢、取消及已驗證 Package 安裝。Agent 只接受 Runtime 名稱、版本與 `operationId`，固定從 GitHub Release Catalog 解析 URL 與檔名；下載採系統 TLS／Proxy、Redirect Host 白名單、`.part`、大小／SHA-256 及原子完成。安裝前重新讀取並驗證快取 Catalog 與 Package，PHP 使用固定 `php-runtime-v1` 健康檢查及 Side-by-side 安裝，不切換 `current`、全域 PHP 或 Site。操作不跨 Agent 重啟恢復，啟動時只清除殘留 `.part`。

Desktop 更換 Agent Protocol 時會分別記錄 Web stack 與 MariaDB 的運行狀態；舊 Agent 安全關閉、新 Agent 就緒後，只恢復升級前正在運行的服務。

## 15. 未來獨立產品：fabDev Server

`fabDev Server` 是未來供區域網路多人使用、承載正式 ERP 工作負載的獨立產品，不是 fabDev Desktop 的 Server Mode。兩者可以共用 Domain Model、Runtime、Site 設定產生器、Release Catalog 與 Typed Contracts，但必須使用不同的執行程序、網路安全模型、安裝包、更新流程及驗收標準。

### 15.1 技術架構

```text
局網 ERP 使用者
  → HTTPS 443
  → Nginx
  → PHP-FPM／PHP FastCGI Process Pool
  → ERP Application
  → MariaDB（只接受 Server 本機連線）

系統管理員瀏覽器
  → HTTPS + Authentication + RBAC
  → Vue 3 Server Console
  → REST API／WebSocket
  → fabdev-serverd（Rust）
       ├─ Site Manager
       ├─ Runtime Manager
       ├─ Service Supervisor
       ├─ Backup Manager
       ├─ Update Manager
       ├─ Health Monitor
       └─ Diagnostics／Audit Log
```

建議技術組合：

- Server Daemon：Rust、Tokio；獨立 binary `fabdev-serverd`。
- 管理 API：Axum REST API，WebSocket 或 Server-Sent Events 傳送即時狀態。
- 管理介面：Vue 3、TypeScript、Vite，透過一般瀏覽器使用，不依賴 Tauri。
- Web Data Plane：Nginx；Linux 使用獨立 PHP-FPM Pool，Windows 使用 fabDev 管理的多程序 PHP FastCGI Pool。
- ERP 資料：MariaDB；fabDev 控制狀態維持 SQLite，兩者不得混用。
- 系統服務：Linux 使用 systemd，Windows 使用 Windows Service；特權操作仍透過最小權限、白名單式平台 Helper。
- 本機控制通道：Unix Socket／ACL Named Pipe，不把底層 Agent Protocol 直接公開到區域網路。

### 15.2 Control Plane 與 Data Plane

`fabdev-serverd` 是 Control Plane，負責 Site、Runtime、設定、服務、備份、更新、健康檢查與操作歷程。Nginx、PHP、ERP Application 與 MariaDB 是 Data Plane。管理 Web UI 或 Control Plane 暫時不可用時，既有 ERP Data Plane 必須繼續提供服務；設定更新需先驗證，再以原子寫入及 Graceful Reload 套用。

可優先重用目前的 `crates/core`、`crates/services`、`crates/runtime`、`crates/sites`、`crates/platform` 與 `packages/contracts`。Server 專用功能應獨立放入例如 `crates/server`、`crates/server-api`、`crates/auth`、`crates/backup`、`crates/observability` 與 `apps/server-console`，不得把多人驗證、LAN Listener 或 Server 權限散入 Desktop Domain Logic。

### 15.3 網路與安全邊界

- ERP 對外只開放 HTTPS；MariaDB、Agent、Runtime 管理及平台 Helper 不得直接暴露給 Client。
- 管理介面必須有登入驗證、Session Timeout、CSRF 防護、登入失敗限制、RBAC 與完整 Audit Log，並可限制管理網段。
- ERP Site 與管理介面使用不同網域或明確隔離的路由；不得把 Tauri Command 直接轉成未驗證 HTTP API。
- MariaDB 預設只綁 Server loopback；每個 ERP 使用獨立 Database User，不得使用 `root` 作為應用程式帳號。
- Runtime、設定、Site 檔案及備份使用獨立服務帳號與最小檔案權限；秘密資料不得寫入一般 Log 或命令列參數。
- 第一版以內部 CA 或企業憑證提供 TLS；對外網際網路 Hosting 不在第一版範圍。

### 15.4 程序、更新與復原

- 每個 PHP minor 至少使用獨立 Process Pool；重要 Site 可選擇獨立 Pool，避免單一 Site 耗盡共用 Worker。
- Supervisor 必須提供 Health Check、有限次數重啟、指數退避與失敗告警，不得無限快速重啟。
- App、Agent、Helper 與 Runtime 使用簽署 Catalog、版本目錄、健康檢查及原子 `current` 切換；失敗時回復前一個可用版本。
- 程式更新與資料庫 Migration 分離；不可逆 Migration 前必須建立並驗證備份。
- MariaDB 提供排程備份、外部 NAS／儲存目的地、還原驗證、磁碟容量告警、Slow Query Log，並預留 Binary Log 與 Point-in-Time Recovery。
- 監控至少涵蓋 HTTP 狀態與延遲、PHP Worker、MariaDB SQL Health、CPU、記憶體、磁碟、備份時間、服務重啟與 TLS 到期日。

### 15.5 第一版產品範圍與驗收

第一版採單一 Server，不導入 Kubernetes、多節點 Cluster、自動水平擴充、MariaDB HA 或跨機房部署。建議平台順序為 Linux x64 Server、Windows x64 Server；macOS 保留給 fabDev Desktop，不作為第一版 Server 平台。

第一版目標為一台 Server、最多 20 個 ERP Site、10 個區域網路同時使用者，並至少完成以下驗收：

- 固定 ERP Fixture 同時處理至少 50 個 HTTP 請求，不得出現非預期 5xx 或受管程序異常退出。
- Nginx、PHP 與 MariaDB 連續運行 72 小時，不得出現程序遺失、持續性記憶體增長或 Port、PID、Socket 殘留。
- 完成斷電重啟、Agent／Nginx／PHP／MariaDB 崩潰、磁碟空間不足及網路中斷的診斷與安全恢復驗證。
- 完成資料庫自動備份、異機保存及實際還原驗證。
- 更新失敗時不得破壞 ERP 程式、設定或資料，並能回復至更新前可用狀態。
