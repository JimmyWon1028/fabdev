# fabDev P0 Public Release Specification

> 建立日期：2026-08-28
>
> 適用範圍：macOS ARM64／Windows x64 Unsigned Community Build
>
> 狀態：`v0.1.0` Draft 因 macOS 驗收阻擋問題不得 Publish；`v0.1.1` 建立公開發布基線；`v0.1.3` 已完成兩平台驗收、Stable Publish、匿名公開下載與 App 內更新驗收

## 1. 目標

P0 建立可供人工下載、驗證及覆蓋安裝的公開發布基礎。第一階段只處理完整安裝包，不實作 App 內下載器、背景自動更新或 Runtime 線上安裝。

公開下載來源固定為：

```text
Repository: https://github.com/JimmyWon1028/fabdev
Download page: https://github.com/JimmyWon1028/fabdev/releases
```

大型安裝包、Checksum 與版本 Manifest 放在 GitHub Releases，不使用 FTP。未來即使改用 GitHub Pages、物件儲存或自訂網域，也必須保留相同的 Manifest 資料契約。

## 2. P0 範圍與非目標

### 本階段包含

- Stable Channel 的版本與 Tag 規則。
- macOS ARM64 DMG 與 Windows x64 NSIS 安裝包命名。
- 公開 SHA-256、檔案大小與 App Release Manifest。
- Draft Release、人工驗證、Publish 與撤回流程。
- GitHub Release 頁面的人工下載流程。

### 本階段不包含

- App 內檢查、下載或啟動安裝包；此項屬於 P1。
- Tauri Updater 的差分或背景自動安裝。
- Apple Developer ID、notarization 或 Windows Code Signing。
- Runtime Catalog 線上發布與 Runtime 自動切換。
- GitHub Pages、自訂下載網站或自備 FTP／SFTP Server。
- 未經明確「重新打包」授權的 DMG／EXE 建置。

## 3. 發布 Channel

第一階段只啟用 `stable`。

| Channel | P0 狀態 | GitHub Release 類型 | 用途 |
| --- | --- | --- | --- |
| `stable` | 啟用 | 一般 Release | 經人工驗證的公開版本 |
| `beta` | 保留 | Pre-release | P1 以後的預覽版本 |

`stable` Manifest 不得指向 Draft 或 Pre-release。Beta 版不得取代 Stable 下載入口。

## 4. 版本與 Git 契約

- App 版本使用 SemVer，例如 `0.1.0`、`0.1.1`、`0.2.0`。
- Git Tag 使用 `v<version>`，例如 `v0.1.0`。
- Tag 必須建立在已通過發布檢查的 `main` Commit。
- 公開過的 Tag、安裝包與版本 Manifest 不得覆蓋或重用。
- 修正任何已發布內容都必須增加版本號並建立新 Release。
- Draft 可刪除重建；一旦 Publish，就視為外部可能已下載。

目前版本必須在下列來源完全一致：

```text
package.json
apps/desktop/package.json
apps/desktop/src-tauri/tauri.conf.json
Cargo.toml [workspace.package]
```

發布前需從安裝包內再次讀取實際 App 版本，不得只相信檔名。

## 5. Release Asset 契約

### 5.1 App 安裝包

| 平台 | Release Asset 名稱 | 安裝方式 |
| --- | --- | --- |
| macOS ARM64 | `fabDev-Community-<version>-macos-arm64.dmg` | 下載後開啟 DMG，執行既有安裝程序 |
| Windows x64 | `fabDev-Community-<version>-windows-x64-setup.exe` | Quit fabDev 後執行 Current User NSIS |

每個公開安裝包都必須同時提供：

```text
<asset-name>.sha256
```

Release 另外提供彙總檔：

```text
SHA256SUMS
fabdev-app-v1.json
fabdev-stable-v1.json
```

`fabdev-app-v1.json` 是該版本不可變的 Manifest。`fabdev-stable-v1.json` 內容與它相同，但使用固定檔名，供 GitHub Latest Release URL 取得目前 Stable 版本。

`SHA256SUMS` 收錄所有安裝包與選用工具，不收錄 Manifest 或 `.sha256` 檔，避免產生循環 Checksum。

### 5.2 選用工具

`fabdev-connect.exe` 不屬於 Desktop App 更新包，可在同一 Release 作為獨立工具發布：

```text
fabDev-Connect-<version>-windows-x64.exe
fabDev-Connect-<version>-windows-x64.exe.sha256
```

選用工具不得出現在 App Manifest 的 `artifacts` 清單中，避免 App 誤把它當成安裝包。

### 5.3 Runtime Package

PHP、Nginx、dnsmasq、MariaDB 與 Node.js 使用獨立 Runtime Catalog。即使 Runtime Package 附在同一 GitHub Release，也不得放入 `fabdev-app-v1.json`；App 與 Runtime 更新生命週期必須保持分離。

## 6. App Release Manifest v1

每個 Stable Release 都必須附上 UTF-8 JSON：

```json
{
  "schemaVersion": 1,
  "product": "fabdev",
  "channel": "stable",
  "version": "0.1.0",
  "tag": "v0.1.0",
  "publishedAt": "2026-08-28T00:00:00Z",
  "releaseUrl": "https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.0",
  "releaseNotesUrl": "https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.0",
  "unsignedCommunityBuild": true,
  "integrity": "sha256",
  "compatibility": {
    "agentProtocolVersion": 32,
    "requiresFullInstaller": true
  },
  "artifacts": [
    {
      "platform": "macos",
      "architecture": "arm64",
      "minimumOsVersion": "13.0",
      "fileName": "fabDev-Community-0.1.0-macos-arm64.dmg",
      "url": "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.0/fabDev-Community-0.1.0-macos-arm64.dmg",
      "size": 123456789,
      "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      "signature": null,
      "installMode": "open-dmg"
    },
    {
      "platform": "windows",
      "architecture": "x64",
      "minimumOsVersion": "11",
      "fileName": "fabDev-Community-0.1.0-windows-x64-setup.exe",
      "url": "https://github.com/JimmyWon1028/fabdev/releases/download/v0.1.0/fabDev-Community-0.1.0-windows-x64-setup.exe",
      "size": 48324392,
      "sha256": "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
      "signature": null,
      "installMode": "run-installer-after-quit"
    }
  ]
}
```

以上數值只是格式範例，不得直接用於實際 Release。實際 Manifest 必須由該次建置產物計算。

### 6.1 欄位規則

- `schemaVersion`：固定為整數 `1`；不相容變更必須增加版本。
- `product`：固定為 `fabdev`。
- `channel`：P0 固定為 `stable`。
- `version`：不含 `v` 的 SemVer，必須與 App、Tag 及檔名一致。
- `tag`：必須等於 `v<version>`。
- `publishedAt`：UTC RFC 3339 時間。
- `releaseUrl`、`releaseNotesUrl`：必須使用 HTTPS。
- `unsignedCommunityBuild`：P0 固定為 `true`。
- `integrity`：P0 固定為 `sha256`。
- `agentProtocolVersion`：安裝包內 Agent Protocol 的實際版本。
- `requiresFullInstaller`：P0 固定為 `true`。
- `artifacts`：只列出已實際建置及完成平台驗收的安裝包。
- `size`：實際 byte 數，必須大於 0。
- `sha256`：64 個小寫十六進位字元。
- `signature`：P0 固定為 `null`，不得填入 `community-ad-hoc` 冒充發布簽章。
- `installMode`：macOS 使用 `open-dmg`；Windows 使用 `run-installer-after-quit`。

P1 實作 Manifest Parser 時，未知欄位可以忽略；未知 `schemaVersion`、Platform、Architecture 或 `installMode` 必須拒絕自動處理並退回人工下載頁。

### 6.2 固定與版本 URL

人工下載頁：

```text
https://github.com/JimmyWon1028/fabdev/releases
```

目前 Stable Manifest 的候選固定 URL：

```text
https://github.com/JimmyWon1028/fabdev/releases/latest/download/fabdev-stable-v1.json
```

版本不可變 Manifest：

```text
https://github.com/JimmyWon1028/fabdev/releases/download/v<version>/fabdev-app-v1.json
```

固定 URL 只有在 P1 實作前完成未登入 `200`、Redirect、Cache 與內容驗證後，才可寫入 App。P0 先將 GitHub Releases 頁面視為正式的人工作業入口。

### 6.3 Release Asset 產生器

`scripts/generate-app-release-manifest.mjs` 會驗證四個版本來源及 Rust／TypeScript Agent Protocol 一致，將已存在的安裝包複製成標準 Release Asset 名稱，並產生：

```text
<asset-name>.sha256
SHA256SUMS
fabdev-app-v1.json
fabdev-stable-v1.json
```

使用方式：

```bash
pnpm run release:prepare -- \
  --version 0.1.0 \
  --published-at 2026-08-28T12:34:56Z \
  --output-dir artifacts/releases/v0.1.0 \
  --macos-arm64 artifacts/fabDev-Community-0.1.0-macos-arm64.dmg \
  --windows-x64 artifacts/windows-x64/FabDev_0.1.0_x64-setup.exe \
  --windows-connect-x64 target/x86_64-pc-windows-msvc/release/fabdev-connect.exe
```

macOS 或 Windows 安裝包至少提供一個；`fabdev-connect.exe` 為選用。`--published-at` 必須是已決定的 UTC RFC 3339 秒數，確保使用相同輸入可得到相同 Manifest。輸出目錄必須尚不存在，工具不會覆蓋先前整理的 Release Assets。

此工具不會執行 Tauri／Cargo 建置、不會建立 Tag 或 GitHub Release，也不會上傳或 Publish。只有使用者明確授權「重新打包」後，才能先產生新的 DMG／EXE 再交給此工具整理。

聚焦測試：

```bash
pnpm run test:release
```

## 7. 完整性與安全邊界

P0 的 SHA-256 可以偵測下載中斷或檔案內容不一致，但不是正式的發布者身分簽章。若 GitHub Account 或 Release 同時遭修改，SHA-256 與安裝包可能一起被替換。

因此 P0 必須遵守：

- 只使用 `https://github.com/JimmyWon1028/fabdev` 及其 GitHub Release Redirect。
- 發布者 GitHub Account 啟用強式登入保護。
- Publish 後不替換同一版本 Asset；修正時建立新版本。
- Release Notes 明確標示 `Unsigned Community Build`。
- macOS 指示使用者核對 SHA-256，並說明 Gatekeeper 警告。
- Windows 指示使用者核對 SHA-256，並說明 SmartScreen 警告。
- 不把 ad-hoc code signing、`community-ad-hoc` 或 HTTPS 當成正式 Code Signing。
- 安裝包不得包含 Token、私鑰、真實客戶資料或建置電腦絕對路徑。

正式 Ed25519 Release Manifest 簽章、Apple Developer ID 與 Windows Code Signing 留到 P3。

## 8. Draft Release 流程

1. 指定候選版本與 Release Commit；凍結版本範圍。
2. 同步四個版本來源並確認 `main` 工作樹乾淨。
3. 執行 `pnpm test`、`pnpm lint` 與 `git diff --check`。
4. 使用者明確核准「重新打包」後，才建立 DMG／EXE。
5. 驗證安裝包內容、實際版本、架構、大小、內外層 SHA-256 與 Runtime 清單。
6. 產生每檔 `.sha256`、`SHA256SUMS`、`fabdev-app-v1.json` 與 `fabdev-stable-v1.json`。
7. 建立 `v<version>` Tag 及 GitHub Draft Release；不得直接 Publish。
8. 上傳符合第 5 節命名的 Assets。
9. 從 GitHub Draft Release 重新下載所有 Assets，重新計算大小與 SHA-256。
10. 在乾淨 macOS／Windows 執行安裝、啟動、`demo.test`、覆蓋更新及移除驗收。
11. 人工核對 Release Notes、支援平台、Unsigned 警告與已知限制。
12. Repository Owner 明確核准後才 Publish。

Draft Release 建立、Tag Push、Asset Upload 與 Publish 都屬於外部狀態變更，必須分別在使用者授權範圍內執行。

### 8.1 Draft-only GitHub Actions workflow

`.github/workflows/release-draft.yml` 只接受 `workflow_dispatch` 手動觸發，不接受 Push、Pull Request、排程或 Release 事件。執行前必須先由人工建立並推送已核准的 `v<version>` Tag；workflow 使用 `--verify-tag`，不會自行建立或移動 Tag。

手動執行時必須提供 `release_scope`（`all` 或 `windows`）、Stable SemVer、固定的 UTC `publishedAt`，並分別輸入完全相符的：

```text
REPACKAGE v<version>
DRAFT v<version>
```

前者代表這次執行已取得重新打包授權，後者只授權建立 Draft。`release_scope=all` 會在 GitHub Hosted `macos-15` ARM64 與 `windows-latest` 建置、測試及整理跨平台 Assets；`release_scope=windows` 必須略過整個 macOS Job、macOS Artifact 下載與 DMG／Runtime 打包，只建立 Windows x64 Installer、Connect、六個 Windows Runtime、Windows-only App Manifest 與 Runtime Catalog。只有最後的 `create-draft` Job 具有 `contents: write`；其餘 Job 都是 `contents: read`。所有第三方 Action 固定到完整 Commit SHA。

最後一步固定使用 `gh release create --draft --verify-tag --latest=false`，不包含 Publish 指令。建立後會從 GitHub Releases 清單確認 Release 仍為 Draft；不能使用 Published Release 的 Tag 查詢端點驗證未發布 Draft。

`v0.1.0` 已在取得 Tag Push、重新打包與 Draft Release 授權後實際執行。macOS ARM64 與 Windows x64 建置、測試及 Artifact 上傳成功；Draft 內 9 個 Assets 已重新下載，總表與個別 SHA-256、Manifest 記錄的大小與 Hash、兩份 Manifest 的逐位元一致性，以及 DMG 內部 checksum 均通過。此結果只代表 Draft Asset 完整，不代表已完成乾淨機安裝驗收或 Publish。

### 8.2 `v0.1.0` macOS 驗收紀錄

從 Draft 重新下載的 DMG 已完成管理員安裝，並確認 Helper、Resolver、`demo.test` DNS／HTTP／HTTPS、憑證 SAN、Login Keychain 信任與空白 Proxy 清單。驗收同時發現三項 P0 阻擋問題：Community 首次初始化沒有保存範例 Site Home，可能掃描其他本機專案；macOS App 選單的原生 Quit 會繞過 Agent 與服務清理；移除程序無法撤銷舊資料留下的 fabDev CA。

原始移除程序已清除 App、Helper、資料與 Demo；殘留 CA 以精確 Fingerprint 人工清除，並恢復安裝前保留的外部 Resolver。人工補救只能讓本機回到安裝前狀態，不代表 `v0.1.0` 安裝包通過移除驗收。

這三項問題的修正只進入後續程式碼，不得移動或重用 `v0.1.0` Tag，也不得覆蓋既有 Draft Assets。必須增加 Patch 版本、重新取得打包與 Draft 授權，再從首次安裝開始重跑 macOS 覆蓋更新及移除驗收。`v0.1.0` Draft 維持未發布。

### 8.3 `v0.1.1` Draft 驗證

`v0.1.1` 的正式版本來源與 Cargo workspace lock 已同步，完整測試與 lint 通過；annotated `v0.1.1` Tag 固定指向 Release Commit `8d70808`。取得重新打包授權後，本機 macOS ARM64 Community DMG 已建立，外層 SHA-256、Disk Image checksum、27 個內層檔案、App／Build 版本、ad-hoc 簽章、ARM64 Desktop／Agent／CLI、四個固定內建 Runtime 與新版移除程序均通過檢查。

取得 Draft Release 授權後，GitHub Actions Run `33222168031` 已從固定 Tag Commit `8d70808` 完成 macOS ARM64、Windows x64 與 Draft 建立 Job。建立當時 Release 保持 `draft=true`、`published_at=null`；完成後續平台驗收及 Repository Owner 核准後，`v0.1.1` 已另行 Publish。原 `v0.1.0` Draft 仍保持不變。

Draft 內 9 個 Assets 已全部重新下載驗證。`SHA256SUMS` 與三份個別校驗檔均通過，兩份 Manifest 逐位元一致，並確認 `requiresFullInstaller=true`、簽章欄位為 `null`、Connect 未混入 App 安裝包清單。DMG 為 99,295,774 bytes、SHA-256 `24849fd966de2f61c4641056f9ab1c6b0b0ed59308f2e9b3cb6388cdf60ddb28`；Windows Setup 為 48,332,278 bytes、SHA-256 `5bd0c91c8885e855c03865aba9909b02c26ee2e73503450c1af27fa3fd310319`；Connect 為 749,568 bytes、SHA-256 `2082d724e809a04111a78a74fe7f0aadd021218569a4589a8c6b7b9fd0a4710f`。DMG Disk Image checksum、內部 27 個校驗項目、App／Build `0.1.1`、ad-hoc codesign、ARM64 Desktop／Agent／CLI、新版移除程序及公開內容邊界均通過。

這一階段只代表 Draft Assets 與封裝內容完整；Windows 安裝與 Publish 驗收結果分別記錄於第 8.5 與 9.1 節。

### 8.4 `v0.1.1` macOS 驗收紀錄

從 Draft 重新下載並驗證的 DMG 已在恢復至 fabDev 未安裝基線的 Mac 完成管理員首次安裝。App、Community Helper、Protocol 32 Agent、DNS、Nginx 與 PHP-FPM 正常啟動；初始化只有 `demo.test`，Proxy 清單為空，Site Home 已保存為 Demo 父目錄，App 重啟後仍未匯入其他本機專案。既有的相容 Resolver 與外部 System／Homebrew MariaDB 在安裝、運行及移除期間均未被修改或接管。

以實際 macOS App 選單執行 `Quit fabDev` 後，Desktop、Agent、dnsmasq、Nginx、PHP-FPM、Proxy 與內部 53535／8080／8443 listener 全部清理。替 `demo.test` 啟用 HTTPS 後，Login Keychain CA 信任、HTTP 301、HTTPS 200、leaf certificate SAN 與 CA chain 均通過。

同版覆蓋更新保留原 Site ID、Site Home、HTTPS、CA／leaf certificate、Demo、空白 Proxy 與 Resolver 指紋；更新後手動開啟 App 可正常恢復服務。完整移除已清除 App、Helper、資料、Demo、CA、受管程序及 53／80／443／53535／8080／8443 listener，本次 App、資料與 Demo 分別移至垃圾桶且可復原。覆蓋安裝結束後未觀察到 App 保持運行，若將更新後自動重新開啟列為發佈條件，仍需在另一個 macOS Session 重現確認。

加入 quarantine 屬性的 Draft DMG 副本保持原 SHA-256；Gatekeeper 對內部 ad-hoc、無 Team ID 的 App 回報 rejected，符合 Unsigned Community Build 的已知限制。實際管理員首次安裝與 Helper 更新已在前述生命週期驗收通過；53／80／443 衝突腳本確認會在寫入 Helper 前停止，但因驗收結束後 sudo 授權已失效，本次未再次建立實際特權 Port 衝突。

### 8.5 `v0.1.1` Windows 驗收紀錄

從 Draft 重新下載的 Windows x64 Setup 已在 Parallels Windows 11 ARM VM 以 x64 模擬執行，檔案大小為 48,332,278 bytes，SHA-256 為 `5bd0c91c8885e855c03865aba9909b02c26ee2e73503450c1af27fa3fd310319`。先從保留資料的 `0.1.0` 執行 Current User 靜默覆蓋更新，Installer exit code 為 0，登錄版本更新為 `0.1.1`，SQLite SHA-256 保持 `525a44e957b1c1f0b6c3c103ff7d8f8ee6b1baea2ff4def4d6ec8492a9ccce8b`，原 `demo.test` Site ID、Site Home、PHP 8.2 與空白 Proxy 均保留。

更新後 Agent Protocol 32、Nginx 1.30.4、PHP 7.4.33／8.2.33 均正常。Start → Stop → Start 完成，Stop 後 fabDev 管理的 Nginx／PHP 與 80／443 listener 全部清除；兩個 PHP 版本切換後 `http://demo.test` 都回傳 HTTP 200 與對應版本。Desktop 使用 Windows GUI subsystem，背景 Nginx／PHP 行程皆位於 fabDev Runtime，沒有 fabDev 衍生的額外 Terminal 行程。

解除安裝 exit code 為 0，移除登錄、Desktop、Agent、Helper、Uninstaller、受管 Hosts、程序與 listener；使用者 `data` 與既有 Connect 設定依保留政策留在原目錄。為建立乾淨基線，保留資料整包移至 VM 內具名可復原備份後，再次安裝同一 Setup。首次啟動前沒有資料目錄或 Connect 設定；首次啟動後唯一 Site 為 `demo.test`、Proxy 清單為空、Site Home 為 `C:\Users\jimmywon\Sites`，PHP 7.4.33／8.2.33 與 HTTP 200 均通過。

Draft Connect 為 749,568 bytes，SHA-256 `2082d724e809a04111a78a74fe7f0aadd021218569a4589a8c6b7b9fd0a4710f`；從 Parallels Shared Folder 啟動後成功轉存相同雜湊的本機 Runtime，並以 `runas --elevated` 執行。Connect 正確拒絕接管本機 fabDev 已存在的同名 `demo.test` Hosts 紀錄。Connect 的多 Site 實際轉送與中斷清理仍屬 P2，不作為本次 P0 NSIS 發布阻擋條件。

此環境證明 GitHub Actions 產生的 Windows x64 binary 可在 Windows 11 ARM 的 x64 模擬層完成安裝與生命週期驗收；乾淨實體 Windows x64、SmartScreen 簽章信譽及 IIS／Herd 共存仍是後續驗收邊界。

### 8.6 `v0.1.2` 本機 macOS 候選包驗證

取得 `0.1.2` 重新打包授權後，六個專案版本來源與 Cargo workspace lock 已同步。完整測試、Release Script、lint、rustfmt、Clippy 與 Swift lint 通過；本次只建立本機 macOS ARM64 Community 候選包，不包含 Commit、Push、Tag、Draft Release 或 Publish。

`fabDev-Community-0.1.2-macos-arm64.dmg` 為 98,639,468 bytes，SHA-256 為 `4b718f1f639347e93531ea192c5064883620f9fd09f509f0185fb0df2a754c2b`。Disk Image checksum、27 個內層 SHA-256、App／Build `0.1.2`、ad-hoc codesign、ARM64 Desktop／Agent／CLI、dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33／8.2.33、安裝／移除程序來源一致性與公開內容邊界均通過；包內沒有 PHP 8.4、MariaDB、SQLite、憑證私鑰、環境檔或客戶識別內容。

本機候選包 Hash 不取代未來 GitHub Actions Draft Asset 的重新下載與逐位元驗證，也不代表 `0.1.1 → 0.1.2` App 內線上更新、macOS 覆蓋安裝或 Windows x64 生命週期已完成驗收。

### 8.7 `v0.1.3` Draft 與平台驗收

`0.1.2` 封裝驗收在 macOS 大小寫不敏感磁碟發現安裝器會把同一 App 的新舊大小寫名稱誤判為兩份；`0.1.3` 以 inode 判斷同一實體 App 並保留真正重複安裝的拒絕保護。六個版本來源與 Cargo workspace lock 已同步，Tag 固定在 Commit `1d6625d42e16e65e2b188a5da2c4c4774f784f74`。

取得重新打包、Tag、Draft 與驗收授權後，GitHub Actions 完成 macOS ARM64、Windows x64 與 Draft Release Jobs。Draft 內 9 個 Assets 已重新下載，大小、`SHA256SUMS`、三份個別 checksum、兩份 Manifest、DMG 內部 27 項 checksum、版本、架構、ad-hoc 簽章與公開內容邊界全數通過：

- macOS DMG：99,976,348 bytes，SHA-256 `96d6e49f363cd257b97e83dda0d4ada8793b6cf8bffcb93de49576b66d318a9e`。
- Windows Setup：48,728,655 bytes，SHA-256 `fdb9fe3830791be471311f701d7ba1c4e8877e4ae3d7fa3a3e7b03b66aec4254`。
- Windows Connect：749,568 bytes，SHA-256 `4d18c1578d6c33649ced95417d4503ab7ddd08538f478fc5ce0fcbe8a97540a8`。

macOS ARM64 完成 `0.1.1 → 0.1.3` 覆蓋更新、資料保留、`demo.test`、PHP 8.2.33 與安全 Quit 清理。Parallels Windows 11 ARM64 的 x64 App 相容環境完成 `0.1.1 → 0.1.3` 人工覆蓋、解除安裝資料保留與重新安裝，並另以封裝版 `0.1.2` 實際走 App 內新版偵測、下載、大小／SHA-256 驗證、安全 Quit、開啟 Setup、覆蓋安裝及重新啟動。更新後 Desktop／Agent 皆為 `0.1.3`、Protocol 32，原 Site ID、Site Home、空白 Proxy 與 HTTP 200／PHP 8.2.33 均保留。

## 9. Publish 後驗證

- 以未登入狀態開啟 Release 頁面及每個 Asset，狀態必須成功。
- 從公開 URL 重新下載安裝包並核對大小與 SHA-256。
- 驗證 `SHA256SUMS`、版本 Manifest 與 Release Assets 完全一致。
- 驗證 Stable 固定 Manifest URL 指向同一版本；若固定 URL 尚未啟用，P0 只公布 Release 頁面。
- 驗證 Source Tag 指向 Release Commit，且沒有重用或移動 Tag。
- 確認沒有意外發布 GitHub Actions 暫存 Artifact、Log 或內部 Runtime 開發包。
- 記錄最終 Release URL、Commit、Tag、Asset 大小及 SHA-256。

### 9.1 `v0.1.1` Stable Publish 執行結果

Repository Owner 明確核准後，GitHub Release `378823889` 已於 `2026-08-29T01:54:37Z` 發布為 Stable；狀態為 `draft=false`、`prerelease=false`，並已成為 GitHub 最新正式 Release：

```text
https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.1
```

- 未登入開啟 Release 頁面回傳 HTTP `200`。
- 9 個公開 Assets 已由版本公開 URL 全部重新下載；檔案大小與發布前 Draft 驗收版本一致。
- `SHA256SUMS` 與三份個別 `.sha256` 全數通過；DMG、Windows Setup 與 Connect 的 SHA-256 分別為 `24849fd966de2f61c4641056f9ab1c6b0b0ed59308f2e9b3cb6388cdf60ddb28`、`5bd0c91c8885e855c03865aba9909b02c26ee2e73503450c1af27fa3fd310319`、`2082d724e809a04111a78a74fe7f0aadd021218569a4589a8c6b7b9fd0a4710f`。
- `fabdev-app-v1.json` 與 `fabdev-stable-v1.json` 逐位元一致；內容為 Stable `0.1.1`、`requiresFullInstaller=true`，兩個 App artifact 的 `signature` 均為 `null`。
- 9 個發布後公開檔案均與發布前 Draft 驗收版本逐位元一致，沒有在 Publish 過程替換 Asset。
- 遠端 annotated `v0.1.1` Tag 的 peeled Commit 仍為 `8d70808a43fb3f2f5406c0e572c2b6e4e51f0350`，沒有移動或重用 Tag。
- Release 只有預期的 Community DMG、Windows Setup、Connect、Checksum 與兩份 Manifest，未包含 Actions 暫存 Artifact、Log 或內部 Runtime 開發包。

### 9.2 `v0.1.3` Stable Publish 執行結果

Repository Owner 明確核准後，GitHub Release `379130930` 已於 `2026-08-30T00:54:23Z` 發布為 Stable；狀態為 `draft=false`、`prerelease=false`：

```text
https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.3
```

- 9 個公開 Assets 已由匿名 URL 重新下載，三個 Binary 的大小與 SHA-256、總表及個別 checksum 全數通過。
- `fabdev-app-v1.json` 與 `fabdev-stable-v1.json` 逐位元一致；內容為 Stable `0.1.3`、Agent Protocol 32、`requiresFullInstaller=true`，兩個 App artifact 的 `signature` 均為 `null`。
- 遠端 annotated `v0.1.3` Tag 的 peeled Commit、`main` 與 `origin/main` 均為 `1d6625d42e16e65e2b188a5da2c4c4774f784f74`，沒有移動或重用 Tag。
- Release 只有預期的 Community DMG、Windows Setup、Connect、Checksum 與兩份 Manifest，沒有 Actions 暫存 Artifact、Log、內部 Runtime 開發包或客戶資料。
- 發布後已由 Windows 封裝版 App 完成 `0.1.2 → 0.1.3` Stable 線上更新；下載檔 SHA-256 與公開 Asset 一致，安全 Quit 會先結束 Desktop、Agent、Nginx 與 PHP，再開啟完整 Setup。

### 9.3 `v0.1.11` Windows Stable Publish 執行結果

Repository Owner 明確核准後，Commit `83f2ba9d88bed940000aaefb68a81de61b1b315e`、annotated Tag `v0.1.11` 與 Windows-only Draft Release 已建立，Windows x64 Workflow 與 Draft Release Workflow 全數通過。16 個 Draft Assets 重新下載後通過大小、總表、個別 SHA-256、Manifest、Runtime Catalog 與 Archive 完整性驗證。

```text
https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.11
```

- Release 已發布為 `draft=false`、`prerelease=false` 的最新 Stable；未登入頁面回傳 HTTP 200。
- 公開 Stable Manifest 與 Runtime Catalog 回傳 HTTP 200，分別固定為 App `0.1.11`／Agent Protocol 36，以及 Runtime Catalog sequence 5／minimum App `0.1.11`。
- 公開 Windows Setup 為 49,305,659 bytes，SHA-256 `3c12f1b24ffbd7675bc325b87c41f20459924a1ba14e6e3f58e9a41cbfb0c3ee`；匿名完整下載與兩個 8 MiB Range `206` 均通過。
- Windows VM 以 Tag 內相同的 `fabdev-updater` 程式碼完成公開 Feed 版本判斷、四路分段下載、整包 SHA-256 與 pending installer 驗證；本機候選的 `/UPDATE /P /R` 原地覆蓋、App 自動重啟、資料保留與 `demo.test` HTTP 200 已先行通過。
- 這是 Windows x64 Unsigned Community Release；Windows 11 ARM x64 相容模式與 GitHub native x64 runner 已驗證，實體 Windows x64、SmartScreen 信譽及 IIS／Herd 共存仍列為已知邊界。

### 9.4 `v0.1.12` Windows PHP Runtime Stable Publish 執行結果

Repository Owner 明確核准後，Commit `9c86342dd81e006991bae49b612cac32dc1beb0d`、annotated Tag `v0.1.12` 與 Windows-only Draft Release 已建立。GitHub Actions Run `33389051643` 全數通過，20 個 Draft Assets 約 321 MiB；全部重新下載後通過總表、八份個別 SHA-256、App Manifest、Runtime Catalog sequence 6 與六個 Runtime Archive 驗證。

```text
https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.12
```

- Release 已發布為 `draft=false`、`prerelease=false` 的 Latest Stable；發布後沒有殘留 Draft Release。
- 公開 Stable Manifest 與 Runtime Catalog 固定為 App `0.1.12`／Agent Protocol 36，以及 Catalog sequence 6／minimum App `0.1.12`；匿名下載與 Draft 驗收檔逐位元一致。
- 公開 Windows Setup 為 49,305,664 bytes，SHA-256 `0287677c041ed4556db6d93cab99777d90aa2f0baecfec4fd5aa7a65d7a63173`；Release 頁、Setup、PHP 7.4.33 與 PHP 8.2.33 端點的匿名 Range 請求均通過。
- Windows VM 由 `0.1.11` 使用 `/UPDATE /P /R` 原地更新至 `0.1.12` 並自動重新啟動；Site、全域 PHP 與設定雜湊保留，Agent `0.1.12`、Protocol 36 與 `demo.test` HTTP 200 通過。
- Publish 後在 VM 依序移除 PHP 7.4.33／8.2.33，再由公開 Catalog 重新下載安裝；兩版 Archive 大小與 SHA-256、CLI、`mysqli`、`pdo_mysql`、移除標記清除與最後恢復 PHP 8.2 均通過。

## 10. 撤回與回復

發布後發現問題時：

1. 立即在 Release Notes 標示已知問題，停止把該版本列為建議下載。
2. 保留問題版本的稽核紀錄；不能假設刪除後外部副本也消失。
3. 下載頁恢復顯示上一個已驗證版本，或暫停 Stable 固定 Manifest。
4. 不覆蓋問題版本的 Assets 或移動原 Tag。
5. 修正後增加 Patch 版本，重新走完整 Draft 與驗收流程。

P0 不做自動降版。已安裝使用者依 Release Notes 人工下載上一個完整安裝包；Sites、SQLite、Runtime、`php.ini` 與 MariaDB 資料仍必須保留。

## 11. Release Notes 最低內容

每份公開 Release Notes 至少包含：

- fabDev 版本與發布日期。
- 支援的 OS／Architecture。
- 新功能、修正及已知限制。
- `Unsigned Community Build` 警告。
- SHA-256 驗證方式。
- macOS Gatekeeper／Windows SmartScreen 操作說明連結。
- 覆蓋更新前 Quit fabDev 的提醒。
- 使用者資料、Site、Runtime 與 MariaDB 保留政策。
- 安全問題使用 Private Vulnerability Reporting 的連結。

## 12. P0 驗收條件

P0 發布基礎需依序完成：

- [x] Public Repository 與公開安全回報管道。
- [x] Release Asset、版本、Channel、Manifest、Draft、Publish 與回復契約。
- [x] 產生 Release Manifest 與 Checksum 的可重現腳本。
- [x] 建立只接受手動雙重確認、只會產生 Draft、不會自動 Publish 的 GitHub Actions Release workflow，並以 `v0.1.0`、`v0.1.1`、`v0.1.2` 與 `v0.1.3` 實際執行候選版流程。
- [x] 在恢復至 fabDev 未安裝基線的 Mac 完成 `v0.1.1` Community DMG 首次安裝、覆蓋更新與完整移除驗收；原三項阻擋問題均通過回歸。
- [x] 在乾淨 Windows 資料基線完成 x64 NSIS 首次安裝、覆蓋更新與移除驗收；本次環境為 Parallels Windows 11 ARM 的 x64 模擬層，實體 Windows x64 仍列為後續邊界。
- [x] 建立第一個 `v0.1.0` Draft Release，並從 GitHub 重新下載 9 個 Assets 驗證大小、Manifest 與 SHA-256。
- [x] 建立 `v0.1.1` Draft Release，並從 GitHub 重新下載 9 個 Assets 驗證大小、Manifest、SHA-256 與 DMG 內容。
- [x] Repository Owner 已明確核准 `v0.1.1` Stable Publish。
- [x] Publish `v0.1.1`，並以未登入公開 URL 重新下載 9 個 Assets，完成大小、SHA-256、Manifest、Draft 位元組與固定 Tag 驗證。
- [x] 建立 `v0.1.3` Draft、完成 macOS／Windows 覆蓋與資料保留驗收，並由 Repository Owner 明確核准 Stable Publish。
- [x] Publish `v0.1.3`，完成 9 個公開 Assets、Stable／App Manifest、匿名下載、固定 Tag 與 Windows App 內 `0.1.2 → 0.1.3` 更新驗收。
- [x] Publish Windows-only `v0.1.11`，完成 16 個公開 Assets、App／Runtime Manifest、匿名完整下載、Range 分段與 Windows VM 原生 Updater 驗收。
- [x] Publish Windows-only `v0.1.12`，完成 20 個公開 Assets、PHP 7.4／8.2／8.4 Runtime Catalog、匿名下載，以及 Windows VM 的 PHP 7.4／8.2 移除後公開線上重裝驗收。

`v0.1.12` 是目前可供公開下載的 Windows x64 正式 Stable Release；P0 公開發布基礎、P1 App 內更新，以及 Windows x64 的 P2 PHP／MariaDB／Node.js Runtime Catalog 與線上下載均已完成。macOS 發布流程與正式發布者簽章依目前範圍保留後續處理。
