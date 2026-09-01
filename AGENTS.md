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
6. Stable 發布前再集中執行一次實際受影響的必要驗證；Windows-only 修正不得因此打包或重測 macOS，也不得重跑未受影響的 PHP、MariaDB、Node.js、HTTPS 完整人工流程。

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
