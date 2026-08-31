# macOS Intel Development and Test Workflow

> 狀態：已確認流程，尚未開始實作 macOS Intel 支援。

## 1. 目的

本文件定義 fabDev macOS Intel（x86_64）的開發、Git 分支、原生打包與實機驗收流程。主要開發先在 Apple Silicon Mac 完成，Intel Mac 負責原生 Runtime、App、DMG 建置及完整安裝測試，最後再回到 Apple Silicon Mac 做 ARM64 回歸並合併 `main`。

這個流程的目標是：

- 保持既有 macOS ARM64 行為及 Community 打包契約不變。
- 避免在 Apple Silicon 上混用 ARM64 Homebrew 與 Intel Runtime 依賴。
- 讓 Intel 環境發現的修正保留在同一功能分支，完成雙架構驗證後才進入 `main`。
- 將程式碼完成、打包、Push、合併、Release 與 Publish 視為不同授權階段。

## 2. 平台與命名

| 環境 | 原生架構 | Rust Target | Manifest 架構 |
| --- | --- | --- | --- |
| Apple Silicon Mac | arm64 | `aarch64-apple-darwin` | `arm64` |
| Intel Mac | x86_64 | `x86_64-apple-darwin` | `x64` |

所有 App、Agent、CLI、System Helper 與內建 Runtime 必須使用相同目標架構。Intel Package 不得包含只能在 Apple Silicon 執行的 ARM64 binary，也不得把 x86_64 內容標記成 `arm64`。

## 3. 授權邊界

以下動作各自需要使用者明確授權，前一階段完成不會自動授權下一階段：

1. 開始修改 macOS Intel 支援。
2. 建立或切換功能分支。
3. Commit 及 Push 功能分支。
4. 在 Intel Mac「重新打包 Intel macOS 版本」。
5. 將功能分支合併到 `main`。
6. Push `main`。
7. 升級版本、建立或 Push Tag、建立 Draft Release、Publish Release。

只有使用者明確說「重新打包 Intel macOS 版本」後，才能建立或覆蓋 Intel Runtime Package、App 或 Community DMG。「開始」、「繼續」、「測試」或「完成 Intel 支援」都不構成打包授權。

本流程不會恢復目前暫停的 Release 工作，也不授權版本升級、Tag、Draft Release 或 Publish。

## 4. 階段 A：Apple Silicon Mac 開發

### 4.1 開始前

- 執行 `git status -sb`，保留並避開既有未提交修改。
- 經使用者確認後，建立獨立功能分支；建議名稱為 `codex/macos-intel-support`。
- 不修改版本、不建立 Tag、不建立或覆蓋安裝包。

### 4.2 實作範圍

Intel 支援至少包含：

- Desktop、Agent、CLI 與 Swift System Helper 的 x86_64 建置目標。
- macOS 架構正規化，以及 `arm64`／`x64` 的檔名、Descriptor 與 Catalog 契約。
- dnsmasq、Nginx、PHP 與選裝 MariaDB Runtime 的原生架構建置及封裝流程。
- Tauri sidecar、Bundle Resource、codesign 與 DMG 路徑的架構感知處理。
- Community 安裝／移除程序的 Intel 平台檢查。
- 內建 Runtime 的平台、架構、大小與 SHA-256 驗證。
- App Update、Runtime Update 與 Release Manifest 對 `macos/x64` 的處理。
- ARM64 與 Intel 命名、驗證、錯誤路徑及相容性的回歸測試。

平台差異應集中在共用的架構解析或平台層，不得在不同腳本與 Domain Logic 中散落互相矛盾的 `uname -m` 判斷。

### 4.3 Apple Silicon 端驗證

Apple Silicon Mac 可執行：

- 前端與 Node.js 單元測試。
- Rust workspace 測試、Clippy、rustfmt 與 `x86_64-apple-darwin` cross-check。
- Swift Helper 測試及 x86_64 編譯檢查。
- 架構命名、Descriptor、Catalog、Manifest 與安裝腳本的靜態／單元測試。
- `pnpm test`、`pnpm lint`、`pnpm build` 與 `git diff --check`。

未取得重新打包授權時，不執行 `pnpm run build:community:macos`，也不建立 Runtime Package、App Bundle 或 DMG。Apple Silicon 的 cross-check 不能取代 Intel 原生打包與實機驗收。

## 5. 階段 B：Push 功能分支

Apple Silicon 端完成實作及可執行的驗證後：

1. 彙整修改檔案、測試結果、ARM64 相容性及 Intel 待驗證項目。
2. 由使用者檢查差異並明確授權 Commit／Push。
3. 只 Push 功能分支，不直接更新 `main`。
4. 不提交 `target/`、`.build/`、`artifacts/`、`dist/`、Runtime binary、DMG、憑證、Token、SQLite 或真實環境資料。

建議使用 GitHub Pull Request 彙整兩台 Mac 的 Commit、測試證據及最終差異，但建立 PR 不等於授權合併或 Release。

## 6. 階段 C：Intel Mac 原生打包與測試

### 6.1 Intel 測試機

Intel Mac 建議透過 Tailscale 私網使用 macOS 原生 SSH，螢幕共享只用於 App、Installer、Keychain、管理員確認及 UI 驗收。建置與 log 蒐集使用 SSH。

建議使用獨立測試帳號及工作目錄，避免改動既有專案、Herd、Homebrew MariaDB 或系統網站。開始前確認：

```bash
uname -m
sw_vers
whoami
xcode-select -p
node --version
pnpm --version
rustc -vV
brew --prefix
```

預期 `uname -m` 為 `x86_64`，Intel Homebrew prefix 通常為 `/usr/local`。專案使用完整 Xcode 26、Node.js 24、pnpm 11.22.0 及 Rust stable；不得依賴 Herd 的 NVM 或 binary。

### 6.2 取得程式碼

Intel Mac 從 GitHub Clone Repository，Checkout 同一個功能分支。不得在 Intel Mac 的 `main` 上直接修改 Intel 問題。

安裝依賴後，先執行不產生 Community 安裝包的測試與 lint。只有取得「重新打包 Intel macOS 版本」授權後，才執行 Intel Runtime、App 與 DMG 建置。

### 6.3 原生產物檢查

打包後至少驗證：

- Desktop、Agent、CLI、System Helper 均為 x86_64 Mach-O。
- dnsmasq、Nginx、PHP 及 DMG 內所有可執行 Runtime 均為 x86_64。
- Descriptor、Catalog、檔名與 Manifest 均標記 `macos/x64`。
- 封裝後不存在未收斂的 `/opt/homebrew` 或 `/usr/local` 執行期依賴。
- App、CLI、Helper 的 ad-hoc codesign 與 DMG 內外層 SHA-256 均通過。
- 安裝包不包含建置機路徑、Site、SQLite、憑證私鑰、環境檔或客戶資料。

### 6.4 Intel 實機驗收

至少完成：

1. Community DMG 首次安裝與 App 啟動。
2. 唯一 `demo.test` 初始化及 HTTP 200。
3. HTTP redirect、HTTPS 200、443 listener、leaf SAN、CA chain 與 Login Keychain 信任。
4. Start All、HTTP/PHP、Stop All、Quit，並確認沒有殘留 Port、PID 或 Socket。
5. Managed MariaDB 的 TCP、Unix Socket、密碼同步及 App 重啟狀態恢復。
6. Managed MariaDB 停止時，自動切換到有效的 System／Homebrew MariaDB Socket。
7. PHP CLI、PHP-FPM、`mysqli`、`PDO MySQL` 及內建 Extension。
8. 覆蓋安裝、資料保留及完整移除。
9. 不接管既有 Herd、Homebrew MariaDB、`/tmp/mysql.sock` 或非 fabDev 建立的 `/etc/resolver/test`。

## 7. Intel 問題回修迴圈

Intel 實測發現問題時：

1. 在 Intel Mac 的同一功能分支修改。
2. 執行相關 Intel 回歸測試並記錄結果。
3. 經使用者授權後 Commit／Push 回同一功能分支。
4. 不直接 Push 或合併到 `main`。
5. Apple Silicon Mac Fetch 最新功能分支，檢查 Intel Commit 與完整差異。

必要時重複此流程，直到 Intel 原生打包與驗收通過。

## 8. 階段 D：回到 Apple Silicon 驗證與合併

Intel 測試通過後，Apple Silicon Mac 必須：

1. Fetch 功能分支的最新 Commit。
2. Review Intel Mac 新增的程式碼及文件變更。
3. 重新執行 ARM64 測試、lint、build 與 `git diff --check`。
4. 確認 ARM64 Runtime、App、安裝程序、Manifest 及更新流程沒有回歸。
5. 彙整 ARM64 與 Intel 的驗收證據及尚未完成項目。
6. 經使用者明確授權後，才將功能分支合併到 `main`。
7. 合併完成不代表可以 Push `main`；Push 仍需獨立明確授權。

不得在 Intel 驗收尚未通過時，為了先佔版本或進入 Release 流程而合併 `main`。

## 9. 完成條件

macOS Intel 支援只有在以下條件全部成立後，才能標記為完成：

- Apple Silicon 端程式測試與 x86_64 cross-check 通過。
- Intel 原生 Runtime、App 與 DMG 建置通過。
- Intel 實機安裝、服務、HTTPS、MariaDB、更新／覆蓋及移除驗收通過。
- Intel 回修 Commit 已回到功能分支。
- Apple Silicon 重新取得分支後，ARM64 完整回歸通過。
- 使用者已 Review 並明確核准合併 `main`。

版本升級、Tag、Draft Release、Publish 及公開下載驗證不屬於本開發測試流程的完成條件，必須依 Release 規格另外取得授權並執行。
