# P2.4 Runtime Draft Acceptance Plan

> 狀態：`v0.1.5` Draft 的 14 個 Assets 與 macOS 覆蓋／隔離 Runtime 安裝已通過；Windows 覆蓋安裝與既有 `demo.test` 也通過，但 PHP 8.4.24 真實安裝發現 Rust `tar` 在套用 Windows 目錄 mtime 時回傳 `Access is denied`。`v0.1.5` 不發布；已加入 Windows 不保留 Archive mtime 的修正與真實 Release Package 安裝回歸，未標記 Windows CI 已通過，正式候選版為 `0.1.6`。

## 1. 目標與固定範圍

P2.4 只交付 PHP 8.4.24 的兩平台線上安裝驗收：

- macOS ARM64：`php-8.4.24-macos-arm64-community.tar.gz`
- Windows x64：`php-8.4.24-windows-x64-community.tar.gz`
- Runtime Catalog：`fabdev-runtime-v1.json`
- 安裝模式固定為 `side-by-side`，不切換 `current`、全域 PHP 或任何 Site。
- Catalog／Package signature 在 Unsigned Community v1 固定為 `null`。
- 不在本階段加入 Node.js、Nginx、dnsmasq 或 MariaDB 線上更新。

`v0.1.4` 與 `v0.1.5` Tag 均保留為失敗候選稽核，不移動、不重用；`v0.1.5` Draft 維持未發布，待新版 Stable 完成公開驗證後依 `AGENTS.md` 刪除 Release、保留 Tag。

## 2. 已確認的前置缺口

P2.4 不能直接執行現有 Draft workflow，必須先完成下列 Release tooling：

1. `.github/workflows/release-draft.yml` 目前只產生既有 App／Connect 的 9 個 Assets，尚未建置或上傳 PHP 8.4 Runtime Assets。
2. `scripts/generate-runtime-catalog.sh` 與 `scripts/prepare-community-runtimes.sh` 仍輸出舊 `*-dev`／`catalog.json` 格式及非 `null` signature，不符合 Runtime Catalog v1，實作時應刪除或由正式產生器完整取代，不保留兩套發布流程。
3. `scripts/build-windows-runtime-packages.sh` 同時產生 PHP、MariaDB 與 Node.js，且使用 macOS `stat` 語法，不適合直接放入 Windows Release Job；需改成只處理 PHP 8.4.24 的可重現 Windows x64 流程。
4. Windows 官方 NTS PHP 套件的 `mysqli`／`pdo_mysql` 由 `php.ini` 載入；目前新版本的使用者設定初始為空。正式打包前必須用真實 Windows package 驗證「使用者 php.ini 保持空白」與「內部服務設定仍載入必要 extensions」可同時成立，並加入回歸測試。
5. GitHub Draft Asset 不提供匿名 `releases/latest` 下載；Production App 也不應加入 Token、任意 URL 或測試 Feed。Draft 階段可驗證 Asset bytes 與隔離快取安裝，但真正的匿名 App 內檢查／下載只能在 Repository Owner 核准 Publish 後立即執行。

## 3. P2.4a：Release pipeline hardening

此階段只修改程式、腳本、workflow 與測試，不建立套件或 Release。

### 3.1 正式 Runtime 產生器

- 以 `fabdev-runtime` 的 Typed Model 與 Validator 作為唯一契約，建立可由 CI 呼叫的正式 Catalog／descriptor 產生入口。
- 輸入固定 Release 版本、Catalog Sequence、產生／到期時間，以及兩平台 Package 路徑；URL、檔名、平台、架構與健康檢查 profile 由程式產生，不接受任意值。
- 第一份公開 Catalog 使用 `catalogSequence: 1`、`minimumAppVersion: 0.1.5`、`minimumAgentProtocolVersion: 33`。
- Catalog 到期日採明確輸入並通過 UTC／未過期驗證；建議為 Publish 日期後 180 天，不在 workflow 中隱含無限期限。
- 產生後立即以同一 Rust Validator 重新解析；拒絕零大小、錯誤 SHA-256、重複 Runtime、錯誤 URL、未知平台及非 `null` signature。

### 3.2 macOS ARM64 Package

- 在 macOS hosted runner 以固定 PHP 8.4.24 source SHA-256、PHP 發布者 PGP Fingerprint 與既有固定 extension sources 建置。
- 封裝前驗證 PHP CLI、FPM、`mysqli`、`pdo_mysql`、Imagick、IMAP、Tidy、OPcache、ARM64 Mach-O、ad-hoc code signing，以及沒有 `/opt/homebrew` 執行期依賴。
- Archive 只能有單一 `8.4.24/` 根目錄；正式檔名不得包含 `-dev`。

### 3.3 Windows x64 Package

- 在 Windows hosted runner 下載固定 PHP 8.4.24 NTS x64 官方 Archive，核對固定 SHA-256。
- 只封裝 PHP 8.4.24，不同時發布 MariaDB 或 Node.js。
- 驗證 `php.exe`、`php-cgi.exe`、必要 DLL／extensions、CLI 版本與 CGI 啟動；Archive 只能有單一 `8.4.24/` 根目錄。
- 使用真實 package 執行空白使用者 php.ini、內部基礎設定、`mysqli`／`pdo_mysql` 與 Adminer／Site `localhost` 連線回歸。

### 3.4 Draft workflow

- macOS／Windows Jobs 各自上傳 Runtime Package 與不可變的來源驗證資料，最後只有 `create-draft` Job 具 `contents: write`。
- `create-draft` 合併兩平台輸出，產生 Runtime Catalog、兩份 Package checksum、既有 App manifests 與總 `SHA256SUMS`。
- Release Assets 預期由 9 個增加為 14 個：既有 9 個，加上 2 個 Runtime Packages、2 個個別 checksum 與 1 個 Runtime Catalog。
- Workflow 維持手動 `REPACKAGE v0.1.6` 與 `DRAFT v0.1.6` 雙重確認，只接受既有 annotated Tag，且永不自動 Publish。

## 4. P2.4b：候選版與 Draft 靜態驗收

需要依序取得版本修改、提交／推送、Tag、重新打包及 Draft Release 授權。

1. 將專案版本升級為 `0.1.6`，執行完整 `pnpm test`、`pnpm lint`、`pnpm build` 與 `git diff --check`。
2. 提交並推送 Release tooling／版本變更，確認 macOS 與 Windows CI 均通過。
3. 建立並推送 annotated `v0.1.6` Tag。
4. 經使用者明確說「重新打包」後，手動啟動 Draft workflow。
5. 從 Draft 重新下載全部 14 個 Assets，不使用 runner 工作目錄中的原始檔驗收。
6. 核對總表、個別 checksum、Catalog、實際大小、SHA-256、Archive 單一根目錄、來源驗證資料與公開內容邊界。
7. 將 Draft Catalog／Package 放入隔離資料目錄，執行與 Production Agent 相同的重新驗證、Side-by-side 安裝、健康檢查及回滾測試；不在 App 加入 Draft Token 或 URL override。

## 5. P2.4c：兩平台實機驗收

macOS ARM64 與 Windows x64 都必須保存驗收前後快照，至少包含 Site ID／PHP、全域 PHP、php.ini、Proxy、MariaDB 狀態及 Runtime 清單。

每平台執行：

1. 從 `0.1.3` 覆蓋安裝 `0.1.6`，確認既有資料與服務狀態不變。
2. 檢查 Catalog 呈現的版本、Unsigned Community 警告、大小與 SHA-256。
3. 下載途中取消，確認 `.part` 清除；重新下載並確認進度與 verified 狀態。
4. 安裝前第二次確認；安裝 PHP 8.4.24 後確認全域 PHP、既有 Sites 與 `current` 未切換。
5. 驗證 PHP 8.4 CLI，以及 macOS FPM 或 Windows CGI 與必要 MySQL extensions。
6. 將唯一測試 Site `demo.test` 明確切換到 PHP 8.4，驗證 HTTP／HTTPS 200、MariaDB `127.0.0.1` 與 `localhost`。
7. 切回原 PHP，執行 Stop／Start、Quit／Relaunch，確認設定持久且沒有殘留 Port、PID、Socket、`.part` 或 staging。
8. 注入錯誤 checksum／Archive／健康檢查失敗案例，確認只刪除本次新增版本並恢復驗收前狀態。

## 6. Publish 與公開 Feed 閘門

Draft Asset 靜態驗證與兩平台隔離安裝通過後，Repository Owner 才能另行核准 Publish。由於 GitHub Draft 無法透過匿名 `releases/latest` 存取，下列項目是 Publish 後的立即阻擋驗收：

- 未登入請求 `fabdev-runtime-v1.json` 與兩平台 Package 均為 HTTP 200。
- `releases/latest` 指向 `v0.1.6`，Catalog bytes 與 Draft 驗收版本逐位元一致。
- 封裝版 App 完成真正的匿名「檢查 → 取消 → 重試 → 下載 → 安裝」。
- 公開 Assets 的大小與 SHA-256 與 Draft 驗收紀錄一致。

若 Publish 後匿名流程失敗，先停止對外宣告並回報；刪除已發布 Stable、改動 Tag 或回復 `latest` 都需要使用者另行明確授權。新版 Stable 完整驗證成功後，才依 `AGENTS.md` 刪除被取代且仍為 Draft 的舊 Release，保留其 Git Tag。

## 7. 授權關卡

| 關卡 | 需要的明確授權 | 不包含 |
| --- | --- | --- |
| P2.4a | 開始 Release pipeline hardening | 重新打包、版本修改、Tag、Release |
| P2.4b-1 | 允許升級版本並提交／推送 `0.1.6` | Tag、重新打包、Release |
| P2.4b-2 | 允許建立並推送 `v0.1.6` Tag | 重新打包、Release |
| P2.4b-3 | 明確說「重新打包」，並允許建立 Draft Release | Publish |
| P2.4c | 允許安裝 Draft 候選版與 PHP 8.4 Runtime 驗收 | Publish |
| Publish | 允許更新 Release Notes 並 Publish `v0.1.6` | 刪除已發布 Stable 或 Tag |

## 8. 完成條件

P2.4 只有在兩平台 Package、Draft bytes、隔離安裝、Publish 後匿名 Feed、Side-by-side 行為、Site HTTP／HTTPS、MariaDB Socket、自動回復與重啟持久性全部有證據且無阻擋問題時才完成。Windows 原生驗收不能由 macOS cross-check 取代。
