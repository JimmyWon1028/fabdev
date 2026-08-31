# fabDev P2 Runtime Catalog v1 Specification

> 規劃日期：2026-08-30
>
> 狀態：PHP 線上安裝已完成；Windows x64 Node.js／MariaDB 的 Catalog、Agent、UI 與 Release pipeline 已完成本機實作及產包驗證，待 Windows CI、封裝版 App 與公開 Feed 驗收
>
> 目前目標：Windows x64 的 PHP 8.4.24、Node.js 20.20.2／24.20.0 與 MariaDB 12.3.2 線上安裝及升級

## 1. 目標

P2 讓 fabDev 從固定的官方 GitHub 來源檢查、下載及安裝選用 Runtime。第一版沿用 Unsigned Community Build，不做背景靜默更新，也不自動替換 Site 或全域正在使用的 Runtime。

Runtime 更新與 App 更新保持獨立操作：App 更新仍使用完整 DMG／Setup；Runtime 安裝只處理版本化 Runtime Package，不得修改 App、Agent、Helper、Site Registry、MariaDB 資料或使用者專案。

## 2. 第一版範圍

包含：

- Runtime Catalog v1 的產生、解析與嚴格驗證。
- 固定 GitHub Releases URL、平台原生 TLS、系統 Proxy 與系統信任庫。
- PHP 8.4.24 macOS ARM64／Windows x64，以及 Windows x64 Node.js 20.20.2／24.20.0、MariaDB 12.3.2 Package 的新版偵測、安裝與升級。
- `.part` 下載、檔案大小與 SHA-256、原子改名、staging 解壓及固定健康檢查。
- 使用者確認、下載進度、取消、失敗清理與安全重試。
- 保留既有 PHP 版本、Site PHP 選擇、全域 PHP、`php.ini`、MariaDB 資料／設定，以及未被切換的舊 Node.js／MariaDB Runtime 版本目錄。

不包含：

- 背景靜默下載或自動安裝。
- 自動切換 Site PHP、全域 PHP 或移除舊版本。
- Nginx 與 dnsmasq 線上更新。
- 任意 Catalog URL、任意 Shell、Manifest 指定命令或提升 Helper 權限。
- Apple Developer ID、Windows Code Signing、Tauri Updater 或 Runtime 發布者數位簽章。

## 3. 既有基礎與缺口

目前 `crates/runtime` 已提供：

- Runtime release descriptor 與 Catalog 資料型別。
- SHA-256 驗證。
- `.staging` 解壓、版本目錄安裝及 `current` 原子切換。
- 已安裝版本列舉、啟用、停用、移除及使用者移除標記。

Agent 與 Desktop 已共用固定 Catalog URL、GitHub Asset Host 白名單、Sequence／到期／相容驗證、Package 快取、下載進度、取消／重試、安裝前重新驗證及固定健康檢查。Windows 的 PHP、Node.js 與 MariaDB 都由同一份 Catalog 呈現；Node.js 成功安裝後切換 fabDev 的 active Runtime，MariaDB 僅能在服務停止時安裝或升級，且只替換 Runtime，不修改資料、設定與 Log。

## 4. 發布與下載來源

Community v1 使用目前 App Stable Release 的 Assets：

```text
Catalog:
https://github.com/JimmyWon1028/fabdev/releases/latest/download/fabdev-runtime-v1.json

Artifact:
https://github.com/JimmyWon1028/fabdev/releases/download/v<app-version>/<file-name>
```

Catalog 與 Runtime Packages 必須在同一個 App Draft Release 完成驗證，Publish 後才可由 `releases/latest` 取得。這使第一版發布、撤回與 App Protocol 相容條件保持一致；缺點是 Runtime Catalog 不能脫離 App Stable Release 獨立發布，後續若需要獨立週期，再改用具正式簽章的專用 Feed。

不得接受：

- Catalog 或 UI 提供的自訂 Host。
- `raw.githubusercontent.com`、GitHub Actions Artifact、FTP、SFTP 或未列入程式內建白名單的 Redirect 目的地。
- 非 `JimmyWon1028/fabdev` 的 Release download path。
- Draft、Pre-release 或可變動的非版本 Artifact URL。

第一個請求 URL 必須與 Catalog 固定值完全相符。GitHub Release Asset 的 HTTPS Redirect 只可前往程式內建的 GitHub Release Asset CDN Host 白名單；Redirect 不得改變檔名語意、降級為 HTTP 或前往 Catalog 指定的其他 Host。

## 5. Runtime Catalog v1

固定檔名為 `fabdev-runtime-v1.json`，UTF-8 JSON，最大 1 MiB。第一版範例：

```json
{
  "schemaVersion": 1,
  "product": "fabdev-runtime",
  "channel": "community",
  "catalogSequence": 1,
  "generatedAt": "2026-08-30T00:00:00Z",
  "expiresAt": "2027-02-26T00:00:00Z",
  "unsignedCommunityBuild": true,
  "integrity": "sha256",
  "compatibility": {
    "minimumAppVersion": "0.1.4",
    "minimumAgentProtocolVersion": 33
  },
  "signature": null,
  "runtimes": [
    {
      "name": "php",
      "version": "8.4.24",
      "platform": "macos",
      "architecture": "arm64",
      "minimumOsVersion": "13.0",
      "fileName": "php-8.4.24-macos-arm64-community.tar.gz",
      "url": "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.4/php-8.4.24-macos-arm64-community.tar.gz",
      "size": 1,
      "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
      "signature": null,
      "sourceVerification": {
        "method": "pgp",
        "fingerprint": "9D7F99A0CB8F05C8A6958D6256A97AF7600A39A6",
        "upstreamSha256": "e127be09a8506f4327c5cfa78a614b00d210714484ec215ce0011b4a03c00731"
      },
      "archiveFormat": "tar.gz",
      "installMode": "side-by-side",
      "healthCheckProfile": "php-runtime-v1"
    }
  ]
}
```

`size` 與 `sha256` 範例值必須由 Release 產生器替換，不得直接發布範例內容。

上例保留 PHP-only Catalog 的 Protocol 33 相容範例；包含 Windows Node.js／MariaDB 的正式 Catalog 固定要求 Agent Protocol 36。

### 5.1 Catalog 欄位

| 欄位 | 規則 |
| --- | --- |
| `schemaVersion` | 第一版固定為 `1`；未知版本拒絕。 |
| `product` | 固定為 `fabdev-runtime`。 |
| `channel` | Unsigned Community 固定為 `community`。 |
| `catalogSequence` | 正整數；小於本機已接受 Sequence 時拒絕，相同 Sequence 只接受相同 Catalog SHA-256，較大時更新本機狀態。 |
| `generatedAt` | RFC 3339 UTC；不得明顯晚於系統時間。 |
| `expiresAt` | RFC 3339 UTC；過期 Catalog 拒絕新下載，但不影響已安裝 Runtime；發布流程必須在到期前提供新版 Stable Catalog。 |
| `unsignedCommunityBuild` | 第一版固定為 `true`。 |
| `integrity` | 第一版固定為 `sha256`。 |
| `compatibility` | 驗證 App SemVer 與 Agent Protocol，任一不符即拒絕。 |
| `signature` | 第一版固定為 `null`；不得填入 `community-ad-hoc` 冒充簽章。 |
| `runtimes` | 不得為空，且 `(name, version, platform, architecture)` 不得重複。 |

### 5.2 Runtime 欄位

- `name` 接受 `php`、`node` 與 `mariadb`。
- `version` 必須為不含 prerelease／build metadata 的三段穩定 SemVer；Windows PHP major 至少為 7、Node.js 至少為 20、MariaDB 至少為 10。macOS PHP Community v1 仍固定為 `8.4.24`。
- PHP 平台／架構接受 `macos/arm64` 與 `windows/x64`；Node.js 與 MariaDB 目前只接受 `windows/x64`。
- `fileName` 必須由名稱、版本、平台、架構及 `community` 固定組成。
- `url` 必須與版本化 GitHub Release URL 及 `fileName` 完全一致。
- `size` 必須大於 0，且不得超過 Runtime 類型的固定上限。
- `sha256` 必須是 64 字元小寫十六進位。
- `signature` 第一版固定為 `null`。
- `archiveFormat` 第一版固定為 `tar.gz`。
- `installMode` 第一版固定為 `side-by-side`。
- `healthCheckProfile` 只接受與 Runtime 名稱對應的 `php-runtime-v1`、`node-runtime-v1` 或 `mariadb-runtime-v1`；Catalog 不得指定命令、參數或路徑。

`sourceVerification` 只記錄建置時如何驗證上游 PHP 原始碼，不是 fabDev Runtime Package 的數位簽章，也不能取代 Package SHA-256。`method` 只接受 `pgp` 或 `official-sha256`；只有 `pgp` 可帶允許的完整 Fingerprint。

## 6. 信任與安全邊界

Unsigned Community v1 的 SHA-256 可偵測傳輸中斷或 Catalog 與 Package 不一致，但若 GitHub Account、Catalog 與 Assets 同時被取代，不能證明發布者身分。因此：

- Catalog 與 Package `signature` 都固定為 `null`。
- UI 必須顯示 Unsigned Community 警告、版本、大小與 SHA-256。
- 安裝必須由使用者明確確認，不得背景靜默執行。
- Catalog Sequence 與到期時間只防止操作錯誤及一般回退，不宣稱能抵抗已控制發布來源的攻擊者。
- 建置流程仍必須驗證 PHP 官方 SHA-256、PGP 簽章與允許的完整 Fingerprint。
- Agent 不接受 UI 傳入 URL、Shell、任意健康檢查或任意安裝路徑。

未來正式簽章時新增可驗證的 Catalog detached signature 與 Package signature；不能改用描述字串填入既有 `signature` 欄位。

## 7. Agent Protocol 36

線上 Runtime 更新由 Agent 負責；Desktop 只傳 Runtime 身分與使用者操作，不傳 URL 或檔案路徑。`crates/core/src/protocol.rs` 與 `packages/contracts/src/index.ts` 必須同步修改。

第一版請求：

- `CheckRuntimeUpdates`：取得並驗證 Catalog，回傳目前平台可安裝版本。
- `StartRuntimeDownload { name, version }`：建立背景下載操作並回傳 `operationId`。
- `GetRuntimeUpdateOperation { operationId }`：回傳 queued／downloading／verified／installing／completed／failed／cancelled 與 bytes 進度。
- `CancelRuntimeDownload { operationId }`：取消下載並刪除 `.part`。
- `InstallDownloadedRuntime { operationId }`：在使用者確認後安裝已驗證 Package。

Agent 每次安裝前必須重新讀取快取 Catalog，核對 Runtime identity、Artifact 大小與 SHA-256。App 重啟後不恢復進行中的網路工作；一般平台殘留 `.part` 在下次檢查時清除，已完整驗證的 Package 可再次使用。Windows x64 另外使用 8 MiB `.resume` 分段，最多 4 路並行及 4 次退避重試；網路中斷保留已完成且大小正確的分段供下次續傳，明確取消、Checksum 失敗或完成安裝包重組時會清除。重組後仍必須核對整包大小與 SHA-256，不能以單一分段完成取代整包驗證。

`RuntimeUpdateArtifact` 同時回傳 `installed` 與 `activeVersion`，讓 Desktop 能區分「未安裝」、「已安裝」及「目前 active 版本低於 Catalog，可更新」。

既有本機 `InstallPhpRuntime { artifactPath, releasePath }` 保留給開發 CLI 與人工安裝，但線上 UI 不使用此入口。

## 8. 下載、安裝與失敗回復

```text
取得固定 Catalog
  → 驗證 Schema、Product、Channel、Sequence、時間與相容條件
  → 選擇目前 OS／CPU 與 Runtime 名稱／版本相符的 Package
  → 使用者確認下載
  → 寫入 cache/runtime-updates/pending/*.part
  → 串流核對大小與 SHA-256
  → flush、sync、原子改名
  → 使用者確認安裝
  → 開啟前再次核對快取 Catalog、大小與 SHA-256
  → 解壓至 Runtime .staging
  → 執行 Agent 內建健康檢查
  → 移至 <runtime>/<version> 版本目錄
  → 依 Runtime 契約保留或切換 active 版本
  → 回傳新的 Runtime 狀態
```

PHP 8.4.24 採 Side-by-side 安裝：

- 已有 PHP 7.4／8.2 時，不切換 `current`、全域 PHP 或任何 Site。
- 沒有任何 PHP Runtime 時，仍不得由線上更新自動成為 Site 使用版本；由使用者另行選擇。
- 健康檢查至少包含固定路徑的 PHP CLI 版本、必要 Extension，以及 macOS PHP-FPM 設定測試或 Windows PHP-CGI 啟動檢查。
- 健康檢查失敗時刪除 staging／新版本目錄，不修改既有 Runtime、設定或 Site。
- 相同版本已安裝時回傳明確狀態，不覆蓋現有目錄。

Windows Node.js 與 MariaDB 的升級規則：

- Node.js 在 `node.exe --version` 與 `npm.cmd --version` 通過後原子切換 active 版本；失敗時恢復原 active 版本並刪除本次新增版本。
- MariaDB 必須先停止；`mariadbd.exe --version` 與 `mariadb.exe --version` 通過後切換 active Runtime，立即重新套用 PHP 的 MariaDB 連線設定。失敗時恢復原 Runtime 與連線設定。
- MariaDB 升級不移動、不刪除也不重新初始化既有 data、config 或 log 目錄；不提供自動降版。

## 9. 發布流程

1. 從固定上游 Archive、SHA-256、PGP 簽章與完整 Fingerprint 建置 Windows PHP、Node.js 與 MariaDB Package。
2. 驗證每個 Package 只有單一版本根目錄，且不依賴未封裝的 Homebrew、Herd、nvm 或系統 Runtime。
3. 以三份 Windows Runtime Package 產生 `fabdev-runtime-v1.json`；正式 Catalog 產生器拒絕重複項目、未知平台、錯誤檔名、零大小或非小寫 SHA-256，不發布舊格式 descriptor。
4. 將 Catalog、Package 與個別 checksum 加入下一個 App Draft Release；不得加入 `*-dev` 產物。
5. 從 Draft 重新下載兩平台 Package，核對大小、SHA-256、Catalog 及內容。
6. 在隔離 macOS 與 Windows 完成檢查、下載、取消／重試、安裝、健康檢查及資料保留驗收。
7. Repository Owner 核准後才 Publish；發布後以匿名 URL 再次驗證。
8. 新版 Stable 驗證完成後，依 `AGENTS.md` 刪除已被取代的 Draft Release 與 Assets，保留 Git Tags。

只有使用者明確說「重新打包」時才能建立或覆蓋 Runtime／Community 安裝包；規劃、開始實作、測試或建立 Catalog 都不構成重新打包授權。

## 10. 測試與驗收條件

單元與整合測試至少包含：

- 正確 Catalog、平台選擇與 SemVer／Protocol 相容。
- 未知 Schema、錯誤 Product／Channel、過期時間及 Sequence 回退。
- 重複 Runtime、錯誤檔名、非官方 URL、Redirect 越界、零／超大檔案及無效 SHA-256。
- 中斷下載、超出宣告大小、Checksum 錯誤、取消與安全重試。
- 安裝前再次驗證、錯誤 Archive、staging 清理及相同版本拒絕覆蓋。
- macOS／Windows 固定 PHP，以及 Windows Node.js／npm、MariaDB Server／Client 健康檢查成功與失敗案例。
- 安裝後既有 Site ID、Site PHP、全域 PHP、`php.ini`、Proxy 與 MariaDB 狀態不變。

封裝版端到端驗收：

- macOS ARM64：檢查 → 下載 → 取消 → 重試 → 安裝 → PHP 8.4 CLI／FPM → 指派測試 Site → HTTP 200 → 切回原 PHP。
- Windows x64：檢查 → 下載 → 取消 → 重試 → 安裝 → PHP 8.4 CLI／CGI → 指派測試 Site → HTTP 200 → 切回原 PHP。
- Stop／Start、Quit／Relaunch 後版本與設定仍正確，沒有殘留 `.part`、Port、PID 或 staging 目錄。

## 11. 執行階段

### P2.1：Schema 與產生器

- [x] 擴充 `RuntimeCatalog`／`RuntimeRelease` Typed Model。
- [x] 建立 Catalog 產生器、Parser、嚴格 Validator 與測試。
- [x] 不連接 UI、不下載、不重新打包。

### P2.2：Agent 下載與操作狀態

- [x] 共用 `crates/updater` 的 TLS、`.part`、大小與 SHA-256 流程。
- [x] 升級 Agent Protocol 33，加入背景操作與輪詢。
- [x] 完成取消、重試、快取與錯誤清理。

P2.2 已完成程式與本機 fixture 驗證；公開 GitHub Runtime Catalog／Package 尚未發布，因此固定 `releases/latest` Feed 的匿名實際下載留待 P2.4 Draft 驗收。`InstallDownloadedRuntime` 已保留於 Protocol 33，但在 P2.3 完成健康檢查與 Side-by-side 安裝前固定拒絕執行。

### P2.3：PHP Runtime UI 與安裝

- [x] PHP 頁面顯示可用版本、Unsigned 警告、大小、SHA-256 與進度。
- [x] 使用者分別確認下載及安裝。
- [x] 安裝前重新驗證快取 Catalog、Runtime identity、大小及 SHA-256。
- [x] 解壓 staging 後執行固定 PHP CLI／版本檢查，安裝後驗證必要 MySQL extensions 及 macOS FPM／Windows CGI 設定。
- [x] PHP 8.4.24 採 Side-by-side 安裝，不自動切換 Site、全域 PHP 或 `current`；失敗時清除本次新增的 Runtime、設定及 staging。

P2.3 已完成程式、前端呈現與本機 fixture 驗證。公開 Runtime Catalog／Package 尚未發布，因此實際 GitHub 下載、真實 PHP 8.4.24 binary 健康檢查、Site HTTP 200、重啟持久性及 Windows x64 驗收仍屬 P2.4。

### P2.4：兩平台 Draft 驗收

- 完整執行矩陣、Release tooling 缺口、16 個 Asset 契約、授權關卡與 Publish 後匿名 Feed 閘門見 [`P2_4_RUNTIME_DRAFT_ACCEPTANCE_PLAN.md`](P2_4_RUNTIME_DRAFT_ACCEPTANCE_PLAN.md)。
- [x] 正式 Runtime Catalog v1 產生器、兩平台 Package checksum 納入總表，以及固定 16 個 Draft Asset 契約。
- [x] Windows PHP 8.4.24 專用來源 SHA-256、CLI／CGI／MySQL extensions 與單一 Archive 根目錄驗證腳本。
- [x] Windows 空白使用者 `php.ini` 保留，內部服務設定自動載入 `mysqli`／`pdo_mysql` 的回歸修正。
- 取得明確重新打包授權後才建立 Runtime Packages。
- 建立 App Draft Release，重新下載並驗證所有 Runtime Assets。
- macOS／Windows 端到端通過且 Repository Owner 核准後才 Publish。

### P2.5：Windows Node.js／MariaDB 線上安裝與升級

- [x] Catalog、Agent Protocol 36、Desktop 安裝／更新按鈕、進度、取消與重新驗證。
- [x] Node.js 20／24 side-by-side 安裝、active 版本切換與健康檢查；安裝不改 PATH，明確設為全域時才啟用動態 terminal shim。
- [x] MariaDB 停止閘門、Runtime 切換、資料／設定保留、PHP 連線設定重新套用與失敗回復。
- [x] 固定 SHA-256／PGP Fingerprint 的可重現產包腳本、四 Runtime Catalog 項目與 Draft Release workflow。
- [x] 在 Parallels Windows 11 的 x64 MSVC target 以真實 Archive 完成 Node.js／MariaDB 安裝、active 切換及 binary 健康檢查。
- [ ] 在 Windows x64 CI 執行真實 Node.js／MariaDB Archive 安裝及 binary 健康檢查。
- [ ] 在新版封裝 App 完成未安裝 → 安裝 → 啟動／使用 → 新版 Catalog → 更新 → 重啟持久性驗收。
- [ ] Publish 後由匿名 `releases/latest` 完成 Catalog 與兩套 Runtime 的實際下載驗收。

### P2.6：後續 Runtime

- Nginx／dnsmasq 需先完成停止、原子切換、啟動健康檢查與上一版回復。
- Node.js 多版本與專案感知、MariaDB 資料格式升級／備份／降版契約另行規劃。
