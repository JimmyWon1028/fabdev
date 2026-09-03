# fabDev

fabDev 是面向 ERP Web 開發的跨平台本機環境，目前支援 macOS 13+／Apple Silicon ARM64 與 Windows 11／x64。基礎 App 內建 Nginx、PHP 7.4 與 PHP 8.2，macOS 另內建 dnsmasq；兩平台均提供多個 `.test` Sites、依 Site PHP 版本啟動的獨立 PHP 服務，以及選裝的 PHP 8.4、MariaDB 12.3.2 與 Node.js 20／24。

## 開發需求

- Node.js 24（建議 `/opt/homebrew/opt/node@24/bin`）
- pnpm 11.22+
- Rust stable
- Xcode 26（完整安裝，用於 macOS System Helper）

## 開發與驗證

```bash
pnpm install
pnpm test
pnpm lint
pnpm dev
pnpm run build:helper:macos
```

以非特權埠測試 Core Agent：

```bash
cargo run -p fabdev-agent -- --dns-port 53535 --http-port 8080 --https-port 8443
cargo run -p fabdev-cli -- status
```

以隔離資料目錄啟動 Desktop 時，Agent 與 Desktop 必須共用同一個覆寫值：

```bash
FABDEV_DATA_DIR=/tmp/fabdev-local-test pnpm dev
```

Desktop 開啟時會先補齊內建的 Nginx 1.30.4、PHP 7.4.33 與 PHP 8.2.33，macOS 另補齊 dnsmasq 2.93；只安裝缺少的版本，不覆蓋既有 Runtime、Site、`php.ini` 或其他開發資料。Site Registry 完全空白時會建立唯一的 `demo.test` 與 fabDev 自有 Demo 專案；只要已有任何 Site 就不新增或覆蓋。接著 Desktop 自行啟動同版本的內建 Agent 與全部開發服務。可在「設定」關閉「App 開啟時自動啟動服務」。若服務已全數運行則保持原狀，部分運行或異常時會先清理再啟動。若發現失效的 Agent IPC 端點，會先安全清理再重連。正式資料仍沿用原本的 Application Support 目錄；Agent 啟動記錄位於其中的 `logs/agent-process.log`。關閉主視窗只會隱藏 Desktop，不會停止服務；從 menu bar 或系統匣選擇 `Quit fabDev` 時會依序停止 Web 全部服務與 MariaDB、清理受管孤兒程序、關閉 Agent，確認 IPC 端點消失後才退出 Desktop。

建立內含 Agent 的本機 macOS App：

```bash
./scripts/run-tauri.sh build --debug --bundles app
codesign --force --deep --sign - target/debug/bundle/macos/fabDev.app
open target/debug/bundle/macos/fabDev.app
```

此流程產生供本機測試使用的 ad-hoc 簽署 App；未來的 Signed Distribution 仍須 Developer ID 與 notarization。

## Unsigned Community Build

不使用 Apple Developer ID 的 Community DMG 可由以下指令建立：

```bash
pnpm run build:community:macos
```

產物位於 `artifacts/fabDev-Community-<version>-macos-arm64.dmg`，並附有同名 `.sha256`。Community DMG 內建 PHP 7.4.33、PHP 8.2.33、Nginx 1.30.4 與 dnsmasq 2.93，另包含唯一的 `demo.test` 範例及可雙擊的安裝與移除程序。PHP 8.4.24、MariaDB 12.3.2 與 Node.js 20／24 是由 [`fabdev-runtimes`](https://github.com/JimmyWon1028/fabdev-runtimes) 獨立發布的選裝 Runtime Package，需由主控台另外安裝，不放入 App Release。內建 PHP 以目前確認的設定作為初始 `php.ini`，其中 `upload_max_filesize` 與 `post_max_size` 均為 64M；首次啟動會依使用者目錄產生正確的 Runtime、Log 與 Session 路徑，不包含建置電腦的絕對路徑。本機候選包與 GitHub Actions 從固定 Tag 建置的 Release Assets 都會依發布流程完成完整性驗證。

Community 安裝程式會驗證 DMG 內的 `SHA256SUMS`，再要求一次管理員權限安裝 `/Applications/fabDev.app` 與固定功能的 LaunchDaemon。更新會保留 Sites、Runtime 與 `php.ini`；移除程序預設保留資料，只有使用者再次確認才會把資料移到垃圾桶。完整操作說明在 [`distribution/macos/community/INSTALL.zh-TW.md`](distribution/macos/community/INSTALL.zh-TW.md)。

App 公開下載使用 [fabdev GitHub Releases](https://github.com/JimmyWon1028/fabdev/releases)；目前 `v0.1.22` 是 Windows-first Latest Stable，已提供 Windows x64 App 與 fabDev Connect。本版已明確略過 macOS 且不回補，macOS 現有公開版本維持 `v0.1.21`；這只代表發布順序與本版維護決定，fabDev 仍是同一個跨平台產品。`v0.1.21` 起 App Release 只提供 App Installer、fabDev Connect、App Manifest 與 SHA-256；Runtime Catalog 與 Package 改由 [fabdev-runtimes Releases](https://github.com/JimmyWon1028/fabdev-runtimes/releases) 獨立管理。Stable 版的版本、Asset 命名、Manifest、SHA-256、Draft／Publish、同版平台補齊與回復契約見 [`docs/PUBLIC_RELEASE_SPEC.md`](docs/PUBLIC_RELEASE_SPEC.md)。`pnpm run release:prepare -- ...` 只整理已存在的 App 安裝包並產生 Manifest／Checksum，不會觸發打包或發布，也拒絕 Runtime Package 輸入。`.github/workflows/release-draft.yml` 只接受手動雙重確認與既有 Tag，且只能建立或補齊 Draft；Stable Publish 仍需 Repository Owner 另行明確核准。

公開發布分成兩個互不綁定版本的儲存庫：

| 儲存庫 | 管理內容 | 版本生命週期 |
| --- | --- | --- |
| [`JimmyWon1028/fabdev`](https://github.com/JimmyWon1028/fabdev) | Desktop App、macOS DMG、Windows Setup、fabDev Connect、App Manifest | 使用 App SemVer 與 `v<version>` Tag |
| [`JimmyWon1028/fabdev-runtimes`](https://github.com/JimmyWon1028/fabdev-runtimes) | Runtime Catalog、選裝 Runtime Package 及 checksum | 使用單調遞增的 `catalog-vN`，不跟隨 App 版號 |

目前 Runtime Latest 是 `catalog-v3`：`fabdev-runtime-v2.json` 的 `catalogSequence=3`、最低 App `0.1.21`、最低 Agent Protocol `37`，共列出 Windows x64 7 項與 macOS ARM64 4 項。Catalog 換版只更新可安裝清單，不會把所有 Package 重新打包；`catalog-v2` 曾移除 Node.js 20.20.2，`catalog-v3` 直接恢復原本 `catalog-v1` 的相同 Package URL、大小及 SHA-256，沒有重新上傳 Package。

Windows x64 使用 Current User NSIS 單檔安裝程式；完整的建置環境、Runtime／sidecar 準備、Windows 11 驗收及除錯經驗整理在 [`docs/WINDOWS_X64_PACKAGING.md`](docs/WINDOWS_X64_PACKAGING.md)。

Windows x64 的 PHP 7.4.33／8.2.33／8.4.24／8.5.10、MariaDB 12.3.2 與 Node.js 20.20.2／24.20.0，以及 macOS ARM64 的 PHP 8.4.24、MariaDB 12.3.2 與 Node.js 20.20.2／24.20.0，均由 Runtime Catalog 管理為獨立選裝套件。Windows 套件可由 `./scripts/build-windows-runtime-packages.sh` 建立，輸出為 `artifacts/windows-x64/runtimes/` 下配對的 Release JSON 與 `.tar.gz`。新建或替換 Package 必須發布到 `fabdev-runtimes` 的新 Release，再由下一個 `catalog-vN` 引用，不得混入 App Release；單純修改安裝清單時只發布新 Catalog，不重打未變更的 Package。MariaDB 與 Node.js 來源除了固定 SHA-256，也會驗證官方 PGP 簽章與完整 Fingerprint。

## Runtime 建置與安裝

```bash
./scripts/build-php-runtime.sh
PHP_VERSION=7.4.33 ./scripts/build-php-runtime.sh
PHP_VERSION=8.4.24 ./scripts/build-php-runtime.sh
./scripts/build-nginx-runtime.sh
./scripts/build-dnsmasq-runtime.sh
./scripts/build-mariadb-runtime.sh
./scripts/build-node-runtime.sh
./scripts/generate-runtime-catalog.sh

cargo run -p fabdev-cli -- install-runtime \
  artifacts/php-8.2.33-macos-arm64-dev.tar.gz \
  artifacts/php-8.2.33-macos-arm64-dev.json

cargo run -p fabdev-cli -- install-runtime \
  artifacts/php-7.4.33-macos-arm64-dev.tar.gz \
  artifacts/php-7.4.33-macos-arm64-dev.json

cargo run -p fabdev-cli -- install-runtime \
  artifacts/php-8.4.24-macos-arm64-dev.tar.gz \
  artifacts/php-8.4.24-macos-arm64-dev.json

cargo run -p fabdev-cli -- install-runtime \
  artifacts/mariadb-12.3.2-macos-arm64-dev.tar.gz \
  artifacts/mariadb-12.3.2-macos-arm64-dev.json
```

Runtime 由官方原始碼建置並驗證 SHA-256／上游簽章；安裝時再次驗證 SHA-256，再原子切換 `<runtime>/current`。Unsigned Community 以 SHA-256 提供完整性驗證；未來 Signed Distribution 才導入 Developer ID、notarization 與已簽署的 Runtime Catalog。

macOS MariaDB 12.3.2 Runtime 與 Homebrew 完全隔離；設定、資料、PID、Socket 與 Log 預設都位於 fabDev 資料目錄。MariaDB 預設只監聽 `127.0.0.1:3306`，首次初始化建立適合本機 PHP 開發的 `root` 空密碼帳號。主控台的 MariaDB 卡片可選擇 Release JSON 與 `.tar.gz` 安裝套件，並提供獨立的安裝、啟動、停止及移除操作；Web 的 Start All／Stop All 不會控制 MariaDB。移除 MariaDB Runtime 前必須先停止服務，且只刪除 Runtime，設定、資料與 Log 都會保留。

左側 MariaDB 頁面可持久修改 TCP Port、Data Directory 與平台對應的額外選項；主控台與 menu bar 均可單獨啟動或停止 MariaDB。最後一次成功啟動或停止的狀態保存在 `state/mariadb.json`；App 下次啟動時會獨立恢復 MariaDB，且不受 Web 服務自動啟動設定影響。結構化設定保存在 `config/mariadb.json`，macOS 的額外選項保存在 `config/mariadb/my.cnf`，Windows 則使用 `config/mariadb/my.ini`，只在下次啟動 MariaDB 時套用。儲存前必須停止 MariaDB，設定會先由安裝的 MariaDB 驗證；Port、路徑、Socket／PID、Log 與 loopback listener 仍由 fabDev 管理。Data Directory 只能選擇空目錄或既有 MariaDB 資料目錄，fabDev 不會自動搬移或刪除舊資料。

MariaDB 運行時可在同一頁同步設定 `root@127.0.0.1` 與 `root@localhost` 的密碼，讓 PHP 專案透過 TCP 或 Adminer 使用 localhost Socket 都能登入。第一次設定時目前密碼可留空；後續變更必須輸入目前密碼。fabDev 不會保存或回填密碼，也不會把密碼放進 MariaDB Client 的命令列參數。

```bash
cargo run -p fabdev-cli -- install-maria-db-runtime \
  artifacts/mariadb-12.3.2-macos-arm64-dev.tar.gz \
  artifacts/mariadb-12.3.2-macos-arm64-dev.json
cargo run -p fabdev-cli -- start-maria-db
cargo run -p fabdev-cli -- stop-maria-db
cargo run -p fabdev-cli -- remove-maria-db-runtime
```

若 3306 已被 Homebrew MariaDB 或其他程式使用，fabDev 會拒絕啟動自己的 MariaDB，不會停止或接管既有服務。停止 fabDev MariaDB 只會釋放 Port、PID 與 Socket，資料目錄會保留；移除後重新安裝相同 Runtime 可繼續使用原資料。

PHP Runtime 安裝於 `runtimes/php/<major>.<minor>.<patch>/`。Agent 會為每個 Site 選擇相同 minor 的最高已安裝 patch，並使用 `services/php/<major>.<minor>/php-fpm.sock`；指定版本未安裝時會明確失敗，不會靜默改用其他 PHP 版本。macOS PHP 7.4、8.2 與 8.4 Runtime 預設包含 Imagick、IMAP 與 Tidy，ImageMagick 的設定、Coder Module 及執行期 dylib 均隨 Runtime 封裝。

主控台的 Runtimes 畫面會讀取實際安裝目錄；PHP 7.4 與 8.2 仍標示為內建，但可和其他版本一樣移除。內建版本被明確移除後會留下輕量標記，App 下次啟動不會自動補回；重新安裝對應 Runtime Package 才會清除標記。所有 PHP 版本若是全域版本或仍被 Site 使用，都必須先切換全域版本或調整 Site 才能移除。其他 PHP 版本可選擇 Runtime 描述檔及對應的 `.tar.gz` 套件安裝，並核對平台、架構、大小與 SHA-256。第一個安裝版本會成為全域 PHP；後續安裝不會自動改變全域版本。

Sites 畫面可直接切換各 Site 的 PHP minor，或選擇 `-` 將 Site 設為不使用 PHP。純靜態 Site 只產生 Nginx 靜態檔案規則，不會啟動 PHP-FPM。切換 PHP 版本時，Agent 會先啟動目標 PHP-FPM、驗證並 reload Nginx，成功後才停止不再使用的舊版本；失敗時回復 Registry 與 Site 設定。Runtimes 畫面的 `php.ini` 按鈕可編輯各 minor 的持久設定，檔案位於既有 Application Support 資料目錄的 `config/php/<major>.<minor>/php.ini`。另有 `config/php/default/php.ini` 預設範本；首次由目前 PHP 8.2 設定建立，之後只供尚未建立專屬設定的 PHP minor 使用，不覆蓋既有設定。各 minor 儲存時會使用對應 PHP-FPM 驗證並安全重啟，無效設定不會取代原檔。

左側倒數第二項的 Node.js 頁面在 Windows x64 提供 Node.js 20.20.2 與 24.20.0，預設均不安裝，並以 `runtimes/node/<version>/` 並存。Catalog 套件會核對平台、架構、大小、SHA-256 與上游發布者簽章；安裝不會自動改變 PATH。只有使用者按「設為全域」時，fabDev 才在使用者 PATH 啟用會動態讀取 `current.version` 的 `node`、`npm`、`npx` 與 `corepack` shim；切換版本無需 nvm，也不修改 Homebrew、Herd、系統 Node.js 或既有 nvm 安裝。Node.js 20 僅供舊專案相容，畫面會標示其已 EOL。

每個 Site 可在 Sites 畫面獨立啟用 HTTPS。fabDev 會在 `config/tls` 建立自己的本機 CA，在 `config/tls/sites` 產生只包含該 `.test` 網域 SAN 的憑證，私鑰只保存在使用者的 fabDev Application Support。首次啟用會將固定名稱的 fabDev CA 加入目前使用者的 Login Keychain 信任；停用 Site HTTPS 會移除該 Site 憑證並恢復 HTTP，不會刪除仍供其他 Sites 使用的 CA。啟用後 Port 80 只做 HTTPS redirect，Nginx 的一般使用者 TLS listener 為 8443，System Helper 固定代理 `443→8443`。

Sites 的 Site Home 預設為使用者目錄下的 `~/Sites`。其中每個第一層非隱藏資料夾會自動建立同名 `.test` 站台，例如 `~/Sites/site1` 對應 `site1.test`；Sites 畫面可另選 Home 路徑並重新掃描。原有手動 linked site 完整保留，網域衝突時以 linked site 為準；移除 Home Site 必須移除或移出對應資料夾，不會由 fabDev 刪除專案檔案。Sites 畫面也可用版本化 JSON 匯出／匯入設定；匯入時相同網域會直接略過，專案檔案不會被複製或修改。

相同操作也可透過 Agent CLI 驗證：

```bash
cargo run -p fabdev-cli -- runtimes
cargo run -p fabdev-cli -- set-global-php 8.2.33
cargo run -p fabdev-cli -- remove-php-runtime 7.4.33
cargo run -p fabdev-cli -- set-site-php <site-id> 7.4
cargo run -p fabdev-cli -- node-runtime
cargo run -p fabdev-cli -- set-global-node 24.20.0
cargo run -p fabdev-cli -- enable-terminal-node
cargo run -p fabdev-cli -- disable-terminal-node
cargo run -p fabdev-cli -- remove-node-runtime 20.20.2
cargo run -p fabdev-cli -- secure <site-id>
cargo run -p fabdev-cli -- unsecure <site-id>
cargo run -p fabdev-cli -- php-ini 7.4
```

## Proxy Manager

全新安裝的 Proxy 清單為空，不預載任何 Connection。左側 Proxy 頁面可新增及移除使用者設定的 Remote Connection。每個連線有獨立的 loopback Listener、遠端 Target、狀態與錯誤；可全部啟動／停止，也可單獨啟動、停止及重新啟動。一個 Port 衝突或遠端中斷只影響該連線。Proxy 設定可用版本化 JSON 匯出／匯入；匯入時若 ID、`.test` 網域或 Listener Port 任一重複就直接略過，匯入後預設保持停止。

所有 Listener 固定綁定 `127.0.0.1`，不直接對區域網路開放。明確的啟動／停止狀態保存於 fabDev SQLite；App Quit 或 Agent 升級期間的暫時停止不覆寫使用者偏好。若其他程序占用設定的 Listener Port，對應連線會顯示 Failed，fabDev 不會終止或接管既有程序。

CLI 可使用相同的 Agent Protocol：

```bash
cargo run -p fabdev-cli -- proxies
cargo run -p fabdev-cli -- add-proxy custom --domain custom.test --port 3020 --target http://api.example.com
cargo run -p fabdev-cli -- remove-proxy custom
cargo run -p fabdev-cli -- start-proxy example
cargo run -p fabdev-cli -- stop-proxy example
cargo run -p fabdev-cli -- start-all-proxies
cargo run -p fabdev-cli -- stop-all-proxies
```

## LAN Site Share

需要讓同一局網的另一台 Windows 電腦或 Parallels VM 用瀏覽器開啟 `http://site-one.test` 時，可先啟動 Web 服務，再到 Sites 畫面對需要的各個 Site 按「局網分享」。所有已選 Site 共用例如 `192.168.1.10:18080`，由 Nginx 依瀏覽器的 `.test` 網域分流；停止某個 Site 不影響其他分享，Stop All、Agent Shutdown 或 App Quit 則停止全部分享。

Windows 執行獨立的 `fabdev-connect.exe`，輸入畫面顯示的主機 `IP:Port`，並以空白或逗號分隔輸入 `site-one.test, site-two.test` 後按「連線」。程式會要求 UAC，自動新增及清除自己管理的 Windows `hosts` 區塊，並只監聽 Client 的 `127.0.0.1:80`，因此不需手動修改 hosts。若 IIS 或其他程式占用 Client Port 80，程式會拒絕啟動並顯示錯誤。

CLI 也可控制主機分享：

```bash
cargo run -p fabdev-cli -- share <site-id> --port 18080
cargo run -p fabdev-cli -- unshare <site-id>
cargo run -p fabdev-cli -- lan-share
cargo run -p fabdev-cli -- stop-share
```

這是無 TLS、無登入、只供 1–2 台 Client 短時間瀏覽器測試的開發便利功能，不適用於網際網路、10 人使用或正式 ERP。正式產品用途的 `fabDev Server` 架構及驗收目標記錄於 [`docs/FABDEV_ARCHITECTURE.md`](docs/FABDEV_ARCHITECTURE.md) 第 15 節。

53/80/443 埠由 macOS System Helper 固定代理至一般使用者權限的 53535/8080/8443；所有 listener 只綁 `127.0.0.1`。Helper 只接受固定服務控制，不執行 Runtime binary、憑證操作或任意命令。Helper 僅管理固定的 `/etc/resolver/test`，如果檔案不是 fabDev 建立便拒絕覆蓋或刪除。開發測試可執行 `helpers/macos/.build/debug/fabdev-system-helper --development`，使用 15353/18080/18443 作為入口，不修改系統設定。

需要用 Chrome 直接開啟 `http://site1.test` 時，可安裝本機測試 Helper：

```bash
pnpm run build:helper:macos
sudo ./scripts/install-local-test-helper.sh
```

這個暫時性 LaunchDaemon 只提供固定的 `53→53535`、`80→8080` 與 `443→8443`。它會接受相容的既有 `/etc/resolver/test`，但不會修改或取得該檔案的所有權；移除時也只刪除由 fabDev 建立的 resolver：

```bash
sudo ./scripts/uninstall-local-test-helper.sh
```

Unsigned Community Build 使用明確的管理員安裝程序註冊固定 LaunchDaemon，不依賴 Apple-issued Code Signing Identity。未來的無終端正式安裝路線仍使用 Developer ID、notarization 與 `SMAppService`。請勿手動覆蓋 Herd 設定；安全邊界與完整決策請參閱 [`docs/FABDEV_ARCHITECTURE.md`](docs/FABDEV_ARCHITECTURE.md)，目前進度則見 [`docs/FABDEV_PROGRESS.md`](docs/FABDEV_PROGRESS.md)。

## Security

請勿在公開 Issue 揭露憑證、私鑰、客戶資料、內部網域、IP 或漏洞細節。安全問題請依 [`SECURITY.md`](SECURITY.md) 使用 GitHub Private Vulnerability Reporting 回報。Unsigned Community Build 沒有 Apple Developer ID 或 Windows 正式簽章，安裝前必須核對 Release 一併提供的 SHA-256。

## License

fabDev 以 [MIT](LICENSE-MIT) 或 [Apache License 2.0](LICENSE-APACHE) 雙重授權。使用者可自行選擇其中一種授權條款。
