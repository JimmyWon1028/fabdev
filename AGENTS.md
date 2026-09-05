# Repository Guidelines

## 專案結構與模組配置

fabDev 是 macOS 優先、最終支援 Windows 的 ERP Web 本機開發工具。Tauri／Vue Desktop 位於 `apps/desktop/`；Rust Core、Agent、CLI、Runtime 與服務管理位於 `crates/`；共用 TypeScript 契約及 UI 位於 `packages/`。Nginx、dnsmasq、PHP 設定模板放在 `resources/`，可重現的 Runtime 建置腳本放在 `scripts/`，端到端 PHP fixture 放在 `tests/fixtures/`。架構決策以 `docs/FABDEV_ARCHITECTURE.md` 為準；目前進度與優先 TODO 以 `docs/FABDEV_PROGRESS.md` 為準。不要提交 `target/`、`.build/`、`artifacts/`、`dist/` 或 Runtime binary。

## 建置、測試與本機開發

- `pnpm dev`：啟動 Tauri Desktop 開發模式。
- `pnpm build`：建置 Vue、Tauri 及完整 Cargo workspace。
- `pnpm test`：執行 Vue/Vitest 與 Rust 測試。
- `pnpm lint`：執行前端 lint、`cargo fmt --check` 與 Clippy。
- `cargo run -p fabdev-agent -- --dns-port 53535 --http-port 8080`：以非特權埠執行 Agent。
- `FABDEV_DATA_DIR=/tmp/fabdev-local-test pnpm dev`：讓 Desktop 與 Agent 共用隔離的本機測試資料。
- `sudo ./scripts/install-local-test-helper.sh`：本機測試固定的 53／80 入口；使用後以對應的 `uninstall-local-test-helper.sh` 移除。
- `./scripts/build-php-runtime.sh`、`build-nginx-runtime.sh`、`build-dnsmasq-runtime.sh`：從已固定雜湊及簽署金鑰的官方原始碼建立 macOS ARM64 開發封裝。

開發工具使用獨立的 Node.js 24、pnpm 11 與 Rust stable；不得依賴 Herd 的 NVM 或 binary。
只有使用者明確說「重新打包」時，才能執行 `pnpm run build:community:macos` 或覆蓋 Community DMG；開始、繼續、測試或完成細節都不構成打包授權。
macOS Community 發行目前維持既有 Unsigned Community DMG。除非 Repository Owner 明確要求「發行簽章版」，不得加入或變更 Apple Developer ID、notarization、stapling、Hardened Runtime、簽章憑證、CI Secret 或其他簽章／公證設定，也不得為了消除 Gatekeeper 警告擅自修改安裝／移除腳本或 DMG 包裝。GitHub Release 經瀏覽器下載的 DMG 若帶有 quarantine，本機測試只能在 SHA-256 驗證通過後，先退出所有同名舊掛載、只移除該 DMG 的 `com.apple.quarantine`，再重新掛載；這只屬本機檔案 metadata 處理，不得混入 Release Asset 或專案設定。

fabDev App Release 與線上 Runtime Distribution 自 `v0.1.21` 起完全分離。`JimmyWon1028/fabdev` 的 App Release 只包含 Windows／macOS App Installer、fabDev Connect、App Manifest 及 checksum；不得加入、重建、複製或上傳線上 PHP、MariaDB、Node.js Runtime Package、Runtime Catalog 或 Runtime `.tar.gz`。選裝 Runtime Package 與 Catalog 只由獨立的 `JimmyWon1028/fabdev-runtimes` 管理，使用自己的 Catalog sequence、最低相容版本與發布生命週期，不跟隨 App SemVer 或 App Tag。一般 App／Agent／Desktop 功能修正及 App 進版不構成 Runtime 重新打包、Catalog 更新或 Runtime Release 授權；只有 Runtime 內容或 Catalog 本身確實變更且 Repository Owner 明確要求時才處理。此分離不改變 App Installer 內既有 bundled Runtime 的產品契約，但 bundled Runtime 內容未變時不得因 App 發布而另外重打選裝 Runtime Package。

## 目前未發布穩定性基線（2026-09-05）

目前 `main` 工作目錄包含尚未進版、提交或發布的穩定性修正與既有 UI 整理，完整問題、修正、回歸測試及驗證邊界以 `docs/STABILITY_CODE_AUDIT_2026-09-05.md` 為準。這批工作維持 App `0.1.22`／Agent Protocol `38`，不得描述為已發布版本內容，也不得因整理文件而觸發 Windows CI、打包、進版、Tag、Draft 或 Publish。

後續修改必須以可重現問題為依據，優先保留現有功能、Agent Protocol、資料格式、服務範圍與操作流程，不做無關重構。Sites 與 Proxy 的既有清單排版已由 Repository Owner 指定保留；Proxy 頂部維持資料操作與服務操作分組，Runtime 卡片維持緊湊一致、PHP 只顯示使用中的 Site 數量，Agent 狀態維持在設定下方。若需要改動這些已確認的 UI，必須先取得 Repository Owner 明確指示。

此基線最近一次完整驗證為 Desktop 88、Release 規則 18、Rust 281、macOS Helper 9 項測試通過，另有 7 項需外部環境的 Rust 測試維持 ignored；`pnpm lint` 與 `git diff --check` 通過。隔離 UI 預覽只驗證頁面渲染與導覽，不等同 Windows／macOS 安裝、更新、服務或實機驗收。

## 架構與設定原則

Desktop 只透過明確定義的 Tauri Command 呼叫 Core Agent。Agent 使用版本化 JSON Protocol 與 Unix Socket；變更 request 或 response 時，必須同步修改 `crates/core/src/protocol.rs` 與 `packages/contracts/src/index.ts`。本機狀態使用 SQLite，可進版控的 Site 設定預留 `fabdev.yml`。平台差異應收斂在 `crates/platform/` 或 `helpers/`，不得散落於共用 Domain Logic。Runtime 安裝到 fabDev Application Support，使用版本目錄與 `current` 連結；封裝內的 Mach-O 不得保留 `/opt/homebrew` 執行期依賴。

fabDev 是同一個跨平台產品。除非作業系統或底層工具有明確且無法合理克服的技術限制，Windows 與 macOS 的功能範圍、使用者操作、狀態提示、取消／重試、錯誤處理及資料契約都必須保持一致。先完成某一平台只代表開發順序，不得成為另一平台省略功能的理由；開始後續平台版本前，必須逐項比對先完成平台的既有功能，禁止以未說明的 `platform` 條件靜默隱藏功能。若確實做不到或必須延後，需先記錄技術原因、影響、替代操作及預計處理版本，明確告知 Repository Owner 並取得確認，同時在 UI、測試與 Release Notes 標示平台差異。

fabDev Managed MariaDB 必須同時支援本機 PHP 專案以 `127.0.0.1` TCP 與 `localhost` Unix Socket 登入；兩種連線的 `root` 密碼必須同步，PHP-FPM 的 `mysqli`／`PDO MySQL` 預設 Socket 必須指向 fabDev 管理的 MariaDB Socket，不得依賴或覆蓋系統的 `/tmp/mysql.sock`。App 啟動時必須恢復使用者上次明確選擇的 MariaDB 啟動／停止狀態；Quit 或 Agent 升級為了清理程序而暫時停止 MariaDB 時，不得把偏好覆寫為停止。

MariaDB 連線來源不提供手動選項。fabDev Managed MariaDB 實際啟動時，PHP-FPM 的 `mysqli`／`PDO MySQL` 預設 Socket 必須自動指向 fabDev 管理的 MariaDB Socket；未安裝或已安裝但停止時，自動使用 System／Homebrew MariaDB Socket，並確保 PHP 專案與 Adminer 可直接以 `localhost` 登入。Managed MariaDB 啟動或停止後必須立即重新產生並套用 PHP-FPM 設定，不得依賴使用者開啟 MariaDB 頁面或手動儲存設定。

不得因重新產生 `www.conf`、Runtime 更新或 App 重啟而破壞 `localhost` 登入。修改 MariaDB 設定契約、Runtime 安裝／移除流程、PHP-FPM 模板或設定產生器時，必須加入 Managed 與 System Socket 自動切換的回歸測試。

System／Homebrew MariaDB Socket 屬於內部連線細節，不顯示於一般 MariaDB 設定畫面；Unix 平台依序偵測已保存的有效 Socket、`/tmp/mysql.sock`、Apple Silicon Homebrew 與 Intel Homebrew 的常見 Socket 路徑，Windows 使用保存的 Named Pipe／TCP 設定。Managed 運行狀態在 Unix 由實際 Socket 判定，在 Windows 由 fabDev PID 檔與 TCP readiness 共同判定。

## 程式風格與命名規範

所有程式碼使用兩格空白縮排，不使用 Tab；程式碼注釋一律使用英文。Rust 遵循 `rustfmt.toml`，TypeScript／Vue 檔名使用 `kebab-case`，變數與函式使用 `camelCase`，型別與元件使用 `PascalCase`，常數使用 `UPPER_SNAKE_CASE`。避免在功能提交中混入無關的全檔格式化。

## 測試規範

新功能至少測試成功路徑與一個錯誤或邊界案例；缺陷修正需加入回歸測試。前端測試命名為 `*.test.ts`，Rust 測試放在所屬模組的 `#[cfg(test)]`。提交前執行 `pnpm test`、`pnpm lint` 及 `git diff --check`；服務改動還需驗證 Start → HTTP/PHP → Stop，並確認沒有殘留 Port、PID 或 Socket。macOS Helper 位於 `helpers/macos/`，以 `pnpm run test:helper:macos` 測試；所有 Proxy listener 必須只綁 loopback，不得新增可由 XPC 傳入的 Port、路徑或任意命令。

後續版本若安裝與更新程序沒有改變，發布驗收沿用先前已通過的程序結果：

1. 不重新執行 macOS 安裝／啟動／移除。
2. 不重新執行 Windows 安裝／啟動／移除。
3. 不重跑 PHP、MariaDB、Node.js、HTTPS 完整人工流程。
4. Windows 不做實機 smoke test，沿用先前安裝程序驗收結果。

此規則只免除上述重複人工流程；版本、建置、自動測試、Manifest、Runtime Catalog、Release Assets 與 SHA-256 完整性仍須依發布流程驗證。若安裝或更新程序後續有變更，則必須恢復相應的人工回歸驗收。

Windows 修正與發布採分段 Gate，不得因每個小問題重跑整套流程：

1. 問題重現與證據：先確認錯誤來源、影響範圍及必要 log，不修改、不打包。
2. 針對性修正：相關小修改只跑直接單元／靜態測試、typecheck、format 與 `git diff --check`；同一批問題完成前不觸發完整 Windows CI。
3. Windows 候選：同一批修正完成後只跑一次 Windows x64 CI 與 NSIS 靜態驗證；CI 成功不得宣稱已通過實機啟動。
4. Repository Owner 實機 Gate：預設由 Repository Owner 測試安裝、啟動、移除、更新等耗時 Windows 流程，Codex 提供候選、步驟及通過標準；除非 Repository Owner 明確要求，Codex 不自行重跑這些實機流程。
5. 只有 Repository Owner 明確回報目前 Gate 通過後，才進入下一 Gate。任一 Gate 失敗即停止，不執行後續完整 Runtime、跨平台或發布驗證。
6. Stable 發布前再集中執行一次實際受影響的必要驗證；Windows 專屬修正不得因此打包或重測 macOS，也不得重跑未受影響的 PHP、MariaDB、Node.js、HTTPS 完整人工流程。

## 固定的跨平台進版與發布順序

後續 App 版本固定由 Windows x64 推進版本號：先完成 Windows 實作、候選、CI 與 Repository Owner 實機 Gate；取得明確 Publish 授權後發布 Windows-first Stable Release。macOS ARM64 不獨立進版，也不阻擋 Windows 繼續下一個 App 版本；Repository Owner 可選擇暫時忽略 macOS，較舊且尚未包含 macOS Asset 的 Windows-first 版本不必逐版補齊。

Windows-first 只代表開發、推送與發布順序，不代表產品或版本是 Windows-only。共用 Domain Logic、Agent Protocol、TypeScript Contracts、設定格式與 UI 契約必須從實作開始就保留兩平台一致性；不得因 macOS 延後發布而加入未說明的 `platform` 條件或破壞既有 macOS 能力。Windows Publish 完成後即可結束該次 Windows Release 並開始下一個 Windows App 版本；Release Notes 與 `docs/FABDEV_PROGRESS.md` 必須使用 Windows-first，並如實標示當下只先提供 Windows x64 Asset 及是否已包含 macOS，不得把尚未補入的 macOS 描述為已發布。

Repository Owner 要求補 macOS 時，只補當時最新 Windows Stable 的相同版本，並使用該版本已發布 Annotated Tag 的相同程式碼與 Tag Commit；不得替較舊且尚未包含 macOS Asset 的 Windows-first Release 補版後把它重新設為 Latest。補版不增加版本號、不建立或移動 Tag，也不重新建置或覆蓋已發布的 Windows Binary。若 macOS 建置或驗證發現必須修改任何程式碼、共用設定或既有 Binary，禁止把不同 Commit 或 dirty-worktree 產物補入原 Release；必須停止並告知 Repository Owner，先把修正納入下一個 Windows Patch 版本並完成 Windows-first Publish，之後 macOS 再補該最新版本。已發布 Release 的同版補齊只能新增 macOS DMG 與其個別 checksum，並依 `docs/PUBLIC_RELEASE_SPEC.md` 替換跨平台 `SHA256SUMS`、App Manifest、Stable Manifest 與 Release Notes；Windows Setup、fabDev Connect、其個別 checksum、版本、Tag、Commit、`publishedAt` 與 Release ID 必須保持不變。

macOS 同版補發布固定沿用 `v0.1.22` 已驗證的快速流程。只有 Repository Owner 明確說「重新打包」後，才從目標 Tag Commit 建立隔離的 detached worktree 並建置 App／DMG，不修改程式碼。建置前先比較上一個已驗證 macOS Stable 與目標版本的 bundled Runtime manifest、descriptor、建置腳本及封裝腳本；若 bundled Runtime 內容與封裝契約均未變，必須重用已驗證且 SHA-256 相符的 dnsmasq、Nginx、PHP 等內建 Runtime Archive，不得重新執行耗時的 Runtime 原始碼建置。重用前仍須同時驗證 descriptor SHA-256，並與上一個已驗證 DMG 內的 Runtime Archive 做逐位元或 SHA-256 比對；若內容有變、缺少可信來源或任何雜湊不符，立即停止重用，只重新建置實際受影響的 bundled Runtime。App Release 仍不得包含已分離的線上 Runtime Package、Catalog 或 Runtime `.tar.gz`。

DMG 完成後必須驗證 `hdiutil verify`、外部 checksum、掛載後內部 `SHA256SUMS`、App／CLI／Helper 的 ad-hoc codesign、所有主要 Binary 的 macOS ARM64 架構、App 版本、bundled Runtime manifest 與內建 Runtime Archive 一致性。補入既有 Stable Release 前，先下載並驗證全部既有 Asset，使用原本 Manifest 的 `publishedAt` 產生同時包含 Windows x64 與 macOS ARM64 的 App Manifest；上傳範圍只能是新增的 macOS DMG、其個別 checksum，以及需要替換的跨平台 `SHA256SUMS`、App Manifest、Stable Manifest。上傳後重新下載全部 Release Assets，逐檔比對、驗證個別與總 SHA-256、確認 Windows Binary 的 Asset ID／大小／digest 未變，再以未登入公開 URL 驗證 Latest Release、Stable Manifest 與 macOS DMG 均可下載。Release Notes 與中英文 README 必須同步改為已補齊 macOS；除非 Repository Owner 另行要求，README 只保留本機修改，不自行 commit 或 push。安裝、啟動、更新與移除等人工驗收仍由 Repository Owner 執行。

Windows-first Stable 尚只提供 Windows x64 Asset 的期間，macOS App 可能因 Latest Manifest 尚無對應 Installer 而回報更新檢查錯誤；這是延後或忽略 macOS 發布時必須接受並如實記錄的影響，不得誤稱 macOS 已有該版本。macOS 補齊完成後必須從公開 URL 重新下載並驗證全部 App Assets、大小、GitHub digest、個別與總 SHA-256、兩份 Manifest 的逐位元一致性，以及 Latest Manifest 同時且只包含 Windows x64 與 macOS ARM64 Installer；完成這些檢查後才可宣稱最新 Stable 已補齊 macOS。

## HTTPS、Helper 與 MCP 開發經驗

- 修改 macOS Helper 的固定 Proxy、plist、簽章或 bundle identifier 後，只重啟 App／Agent 不會更新已安裝的 LaunchDaemon；必須先重新建置，再使用專案安裝程序替換 Helper，並驗證實際載入版本與 53／80／443 listener。
- HTTPS 驗證需逐層確認 DNS、HTTP 301、443 listener、Nginx SNI、leaf certificate SAN、CA chain 與 Login Keychain 信任；瀏覽器錯誤不能取代 `curl`／TLS 與憑證檢查。正式驗收至少包含 `demo.test` 的 HTTP redirect 與 HTTPS 200。
- fabDev CA 應由互動中的目前使用者信任至 Login Keychain；root Helper 不負責產生、信任或搬移憑證。Site 私鑰只能保存在 fabDev Application Support，leaf certificate SAN 只能包含正規化後的目標 `.test` 網域。
- MCP 應是既有版本化 Agent Protocol 的薄型轉接層，不可另建一套服務管理邏輯。預設唯讀並限制在明確 Site；輸出必須遮罩密碼、Token、私鑰與敏感 `.env`，所有變更工具採白名單及明確確認，且不得暴露任意 Shell、路徑、Port 或提升 Helper 權限。
- Laravel 專用的 Query、Job、Dump 與 outgoing request tracing 不可直接假設適用一般 ERP／Legacy PHP；先完成 DNS → HTTP／HTTPS → Nginx → PHP-FPM → MariaDB 的通用診斷，再以選用 instrumentation 擴充框架層追蹤。
- 專案用 Node.js 是預設未安裝的獨立選裝 Runtime，必須與 fabDev 建置用 Node、Homebrew、nvm、Herd 及系統 Node.js 分離。Windows x64 Catalog 固定提供 Node.js 20 與 24 並存安裝；安裝本身不得改變 PATH，只有使用者明確按「設為全域」時才建立 fabDev 的 `node`／`npm`／`npx`／`corepack` shim 並加入使用者 PATH。切換全域版本必須同步更新 shim 指向；完全未安裝時不得建立 Node shim 或修改 PATH。
- Node.js Runtime 建置必須同時驗證固定的官方 Archive SHA-256、Node.js 發布者簽署的 `SHASUMS256.txt.asc` 與允許的完整 Key Fingerprint，再封裝成以版本為單一根目錄的 fabDev Runtime Package。未明確要求「重新打包」時，不得因此把 Node.js 納入 Community DMG。

## Commit、PR 與安全邊界

採用 Conventional Commits，例如 `feat: add runtime installer`。PR 應說明目的、驗證方式、相關 issue，UI 變更需附截圖。Desktop 與 Agent 維持一般使用者權限；53／80／443、固定的 `/etc/resolver/test` 及 LaunchDaemon 只能經白名單 System Helper 操作，CA 信任則由目前使用者的互動 Session 經固定路徑與內容驗證後執行。不得覆蓋 Herd 設定、接管既有 Homebrew MariaDB，或提交 Token、私鑰與真實環境資料。

新版 Stable Release 完成 Publish 與公開下載驗證後，已被新版取代且不再使用的 Draft Release 必須刪除，包含其 Draft Assets，避免 Repository Owner 的 Releases 頁被廢棄 Draft 排在正式版本前方。刪除前必須確認目標仍為 `draft=true` 且新版 Stable 已驗證成功；只刪除 Release 紀錄與 Assets，預設保留 Git Tag。已發布的 Stable／Pre-release 不得套用此規則，除非使用者另行明確要求刪除。
