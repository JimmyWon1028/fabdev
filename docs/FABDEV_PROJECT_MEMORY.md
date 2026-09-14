# fabDev 長期專案記憶

> 最後整理：2026-09-14

本文件保存只屬於 fabDev、值得跨電腦延續的決策與實作經驗。它不保存聊天逐字稿、個人資料、憑證、Token、私鑰、真實客戶資料、本機絕對路徑、一次性 Artifact，或已被新版取代的暫時狀態。

## 使用方式與資訊優先順序

開始工作時依序確認：

1. `AGENTS.md`：目前必須遵守的工作、測試、安全及發布規則。
2. `docs/FABDEV_ARCHITECTURE.md`：產品架構與資料契約。
3. `docs/FABDEV_PROGRESS.md`：目前版本、已完成項目與優先 TODO。
4. `docs/PUBLIC_RELEASE_SPEC.md`：App 與 Runtime 的公開發布契約。
5. 本文件：可重用的判斷原則、歷史教訓與診斷捷徑。

本文件不是即時狀態來源。版本、Protocol、測試數量、Release Asset、SHA-256、CI Run、Tag 與發布狀態必須重新從正式文件及實際 Repository／GitHub 狀態確認。

## 長期產品邊界

- fabDev 是同一個跨平台產品。Windows-first 是開發與發布順序，不是 Windows-only，也不能作為隱藏或省略 macOS 功能的理由。
- 共用 Domain Logic、Agent Protocol、TypeScript Contracts、設定格式、UI 狀態、取消／重試與錯誤處理應保持跨平台一致。無法一致時，先記錄技術原因、影響、替代操作與預計版本，再取得 Repository Owner 確認。
- Desktop 透過明確的 Tauri Command 呼叫 Core Agent；MCP 只能是既有版本化 Agent Protocol 的薄型轉接層，不另建服務管理邏輯。
- 平台差異集中於 `crates/platform/` 或 `helpers/`。特權 Helper 只提供固定白名單能力，不接受任意 Shell、路徑或 Port。
- Desktop 與 Agent 維持一般使用者權限。53／80／443、固定 resolver 與 LaunchDaemon 等特權操作只能經過既有白名單 Helper。
- 所有 Proxy listener 預設只綁 loopback；不得為了方便測試擴大暴露範圍或讓 Helper 接受可變 listener 參數。
- Runtime、設定、狀態、Cache 與 Log 屬於 fabDev 自己的 Application Support 範圍；不得覆蓋 Herd、接管 Homebrew MariaDB，或依賴 Herd 的 NVM／binary。

## 已確認的 UI 契約

- Sites 與 Proxy 的既有清單排版維持不變；需要更動時先取得 Repository Owner 明確指示。
- Proxy 頂部維持資料操作與服務操作分組，不把兩類動作混在同一組。
- Runtime 卡片維持緊湊且一致；PHP 只顯示使用中的 Site 數量，Agent 狀態維持在設定下方。
- Sites／Proxy 編輯流程必須保護未儲存修改，且文字選取、拖曳、單行名稱與識別碼顯示不可因後續樣式調整而退步。

## 工作與授權邊界

- 每次開始先執行 `git status -sb`，保留既有未提交內容，避免無關重構、全檔格式化及 `git add -A`。
- 以可重現問題與直接證據決定修改範圍。規格有疑問、衝突或可能改變未要求的既有行為時，先向 Repository Owner 確認。
- 可以進行必要的唯讀調查與本機驗證；安裝／移除 Helper、提升權限、覆蓋資料、發布、簽章、公證及其他重大外部變更仍需符合 `AGENTS.md` 的明確授權規則。
- 未收到「推送」、「上傳」或「同步到 GitHub」等明確要求時，不執行 `git push`。需要提交時只 stage 本次核准的明確路徑。
- 未收到「重新打包」時，不執行 Community macOS 打包、不覆蓋 DMG，也不因 App 功能修正重建線上 Runtime Package。
- Community macOS 發行維持既有 Unsigned Community 模式。未收到「發行簽章版」時，不新增或修改 Developer ID、notarization、stapling、Hardened Runtime、憑證或 CI Secret，也不為了消除 Gatekeeper 提示改變安裝／移除流程。
- 發布時間與人類可讀報告使用 `Asia/Taipei`（UTC+8）。外部協定要求 UTC 時保留機器格式，但對使用者顯示時轉換並標示時區。

## 服務生命週期經驗

- `Start All`／`Stop All` 採單一、依狀態切換的控制。全部已啟動時再次 Start 必須成功且不重啟；部分失敗或部分運行時應先清理，再一致地恢復。
- `Stop All → Quit fabDev` 的驗收標準是沒有殘留受管 Runtime 程序、Port、PID 或 Socket。Helper 本身是否常駐要依產品契約判斷，不能把受管 Runtime 殘留誤認為正常 Helper 行為。
- 發現 `Address already in use`、DNS 異常或孤兒程序時，先檢查 Agent、listener、PID／parent process、Socket、resolver 與 log。不要先要求重裝或把問題歸因於操作順序。
- 服務修改除單元測試外，至少驗證 Start → HTTP／PHP → Stop，並確認清理後 Port 可重新綁定。

## DNS、HTTP 與 HTTPS 診斷

- macOS 的 scoped `/etc/resolver/test` 可能不會反映在一般 `dig` 結果；`dig` 出現 router NXDOMAIN 不能單獨證明 fabDev DNS 失效。
- 依序確認 DNS、HTTP listener、Nginx route、PHP-FPM 與 MariaDB。可使用 `curl http://demo.test` 或直接查詢 fabDev resolver 交叉驗證。
- HTTPS 必須逐層確認 HTTP 301、443 listener、Nginx SNI、leaf certificate SAN、CA chain 與 Login Keychain trust；瀏覽器錯誤頁不能取代 `curl`、TLS 與憑證檢查。
- fabDev CA 由目前互動使用者信任到 Login Keychain。root Helper 不產生、不信任也不搬移憑證；Site 私鑰只留在 fabDev Application Support。
- 修改已安裝 Helper 的固定 Proxy、plist、簽章或 bundle identifier 後，只重啟 App／Agent 不會更新 LaunchDaemon。必須重新建置並透過專案安裝流程替換，再驗證實際載入版本與 listener。

## Agent Protocol 與程序診斷

- 修改 request／response 時，同步更新 `crates/core/src/protocol.rs` 與 `packages/contracts/src/index.ts`，並確認 Desktop、Agent、CLI 與測試使用同一版本契約。
- 新 Protocol 行為看似沒有生效時，先確認是否仍有舊版 Agent 或 dev process 留在記憶體中。結束舊程序樹並重新啟動後，再判斷程式碼是否有問題。
- 診斷輸出必須遮罩密碼、Token、私鑰及敏感 `.env`；MCP 預設唯讀並限制在明確 Site。
- Laravel Query／Job／Dump／outgoing request tracing 是選用的框架層能力。一般 ERP／Legacy PHP 先完成 DNS → HTTP／HTTPS → Nginx → PHP-FPM → MariaDB 的通用診斷，再加入框架 instrumentation。

## MariaDB 與 PHP-FPM 不變條件

- Managed MariaDB 同時支援 `127.0.0.1` TCP 與 `localhost` 平台對應連線，且 root 密碼語意一致。
- Managed MariaDB 實際運行時，PHP-FPM 自動使用 fabDev 管理的連線端點；未安裝或停止時，自動切回可用的 System／Homebrew 連線端點。切換後立即重新產生並套用 PHP-FPM 設定。
- App Quit 或 Agent 升級為清理程序而暫停 MariaDB 時，不得覆寫使用者上次明確選擇的啟動偏好。
- 修改 MariaDB 設定契約、Runtime 安裝／移除、PHP-FPM 模板或設定產生器時，加入 Managed 與 System 自動切換的回歸測試。
- System／Homebrew Socket、Windows Named Pipe 與 TCP readiness 是內部細節，不在一般 UI 提供手動來源切換。

## App 與 Runtime 發布模型

- App Release 與線上 Runtime Distribution 完全分離。`fabdev` Repository 只發布 App Installer、fabDev Connect、App Manifest 與 checksum；選裝 Runtime Package／Catalog 只由 `fabdev-runtimes` 管理。
- App SemVer、App Tag 與 Runtime Catalog sequence 各自獨立。一般 App／Agent／Desktop 修正不構成 Runtime 重打包或 Catalog 更新授權。
- bundled Runtime 未改變時，不因 App 發布重新建置線上 Runtime Package；macOS 同版補發布若封裝契約未變，優先重用已驗證且 SHA-256 相符的 bundled Runtime Archive。
- Runtime Archive 缺乏可信來源、descriptor 不符或內容雜湊不同時，停止重用，只重建實際受影響的 Runtime。
- 專案用 Node.js Runtime 與 fabDev 建置用 Node、Homebrew、nvm、Herd 及系統 Node.js 分離。安裝選裝 Node.js 不自動修改 PATH；只有使用者明確設為全域時才管理 shim。

## Windows-first 與分段 Gate

- Windows 修正先完成重現與證據，再做針對性修正與直接測試；同一批問題完成前，不因每個小問題重跑完整 Windows CI。
- 同一批修正只建立一次 Windows x64 候選。CI 成功只代表候選建置與靜態驗證通過，不能宣稱已通過實機啟動。
- 候選 Artifact 直接交給 Repository Owner 做 Windows 實機 Gate。除非被要求或正在診斷 Artifact／CI 完整性，不把 Windows Installer 下載回 macOS 重複檢查。
- Repository Owner 明確回報目前 Gate 通過後，才進入下一 Gate；失敗時停止後續完整 Runtime、跨平台或發布驗證。
- Windows Publish 完成後可以開始下一個 App 版本；尚未補 macOS Asset 時，Release Notes 與進度文件必須如實標示 Windows-first 狀態。

## macOS 同版補發布

- 只補當時最新 Windows Stable 的相同版本，並使用既有 Annotated Tag 的相同 Tag Commit；不移動 Tag、不增加版本號、不重建或覆蓋已發布的 Windows Binary。
- 若 macOS 建置需要修改任何程式碼、共用設定或既有 Binary，停止同版補發布。修正必須進入下一個 Windows Patch 並完成 Windows-first Publish 後，macOS 再補該新版本。
- 同版補發布只新增 macOS DMG 與個別 checksum，並依 Release 規格更新跨平台 checksum、App／Stable Manifest 與 Release Notes。
- 補入前後都重新驗證既有 Asset；Windows Setup 與 fabDev Connect 的 Asset ID、大小及 digest 必須保持不變。
- macOS DMG 驗證包含 `hdiutil verify`、外部與內部 checksum、ad-hoc codesign、主要 binary 架構、App 版本、Runtime manifest 與 Archive 一致性。

## 可重用的失敗判斷

- `hdiutil: create failed - Device not configured` 若發生在受限 sandbox，先以適當權限重跑同一打包命令，不要因此修改打包腳本。
- `rust-objcopy` 缺少 `libLLVM.dylib` 可能只是 debug-symbol stripping 警告；以最終 binary、簽章與包裝驗證是否失敗作判斷。
- 受限 macOS shell 可能無法讀取 Keychain，導致 `gh auth status` 誤報。需要 GitHub 寫入且已獲授權時，先在能讀取系統 Keychain 的環境確認，再決定是否重新登入。
- GitHub 認證確實失效且使用者已明確要求推送／發布時，使用官方 Web device login，完成後再設定 Git credential；不要把登入責任只丟回給使用者。

## 驗證與回報原則

- 新功能測試成功路徑及至少一個錯誤或邊界案例；缺陷修正加入能重現原問題的回歸測試。
- 驗證範圍依實際影響調整。未變更安裝／更新流程時沿用已核准的人工驗收，不無理由重跑耗時的跨平台完整流程。
- `node --check`、typecheck、靜態分析、CI 建置與實機測試代表不同層級；回報時清楚說明實際完成哪一層，不以其中一種代替另一種。
- 目前測試數量、ignored tests、Release Asset 數量及 digest 都是可變資料，必須即時確認，不從本文件推斷。
- 完成 Publish 與匿名公開下載驗證後，才依規則刪除已被取代且仍為 Draft 的 Release；保留其 Git Tag。不得把此規則套用到已發布 Stable／Pre-release。

## 跨電腦維護方式

- 本文件、`AGENTS.md` 與正式專案文件均使用 Repository 相對路徑並納入 Git，讓 macOS 與 Windows Clone 後得到相同專案上下文。
- 不把 `~/.codex/memories/`、Codex 聊天紀錄或整個 Codex Home 提交到 Repository。這些內容可能包含其他專案、個人資訊、舊絕對路徑與 generated state。
- 長期有效且必須遵守的新規則寫入 `AGENTS.md`；架構決策寫入 `FABDEV_ARCHITECTURE.md`；當前進度寫入 `FABDEV_PROGRESS.md`；可重用的診斷經驗才寫入本文件。
- 新增記憶前先刪除一次性版本號、舊 Artifact、暫停狀態及重複敘述。若記憶與現況衝突，先查實際程式碼與正式文件，再更新或移除舊記憶。
