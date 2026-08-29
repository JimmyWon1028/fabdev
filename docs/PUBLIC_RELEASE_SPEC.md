# fabDev P0 Public Release Specification

> 建立日期：2026-08-28
>
> 適用範圍：macOS ARM64／Windows x64 Unsigned Community Build
>
> 狀態：`v0.1.0` Draft 因 macOS 驗收阻擋問題不得 Publish；`v0.1.1` Draft 已建立並完成 Assets 完整性驗證，尚待平台驗收

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

手動執行時必須提供 Stable SemVer、固定的 UTC `publishedAt`，並分別輸入完全相符的：

```text
REPACKAGE v<version>
DRAFT v<version>
```

前者代表這次執行已取得重新打包授權，後者只授權建立 Draft。流程在 GitHub Hosted `macos-15` ARM64 與 `windows-latest` 建置、測試及整理 Assets，只有最後的 `create-draft` Job 具有 `contents: write`；其餘 Job 都是 `contents: read`。所有第三方 Action 固定到完整 Commit SHA。

最後一步固定使用 `gh release create --draft --verify-tag --latest=false`，不包含 Publish 指令。建立後會從 GitHub Releases 清單確認 Release 仍為 Draft；不能使用 Published Release 的 Tag 查詢端點驗證未發布 Draft。

`v0.1.0` 已在取得 Tag Push、重新打包與 Draft Release 授權後實際執行。macOS ARM64 與 Windows x64 建置、測試及 Artifact 上傳成功；Draft 內 9 個 Assets 已重新下載，總表與個別 SHA-256、Manifest 記錄的大小與 Hash、兩份 Manifest 的逐位元一致性，以及 DMG 內部 checksum 均通過。此結果只代表 Draft Asset 完整，不代表已完成乾淨機安裝驗收或 Publish。

### 8.2 `v0.1.0` macOS 驗收紀錄

從 Draft 重新下載的 DMG 已完成管理員安裝，並確認 Helper、Resolver、`demo.test` DNS／HTTP／HTTPS、憑證 SAN、Login Keychain 信任與空白 Proxy 清單。驗收同時發現三項 P0 阻擋問題：Community 首次初始化沒有保存範例 Site Home，可能掃描其他本機專案；macOS App 選單的原生 Quit 會繞過 Agent 與服務清理；移除程序無法撤銷舊資料留下的 fabDev CA。

原始移除程序已清除 App、Helper、資料與 Demo；殘留 CA 以精確 Fingerprint 人工清除，並恢復安裝前保留的外部 Resolver。人工補救只能讓本機回到安裝前狀態，不代表 `v0.1.0` 安裝包通過移除驗收。

這三項問題的修正只進入後續程式碼，不得移動或重用 `v0.1.0` Tag，也不得覆蓋既有 Draft Assets。必須增加 Patch 版本、重新取得打包與 Draft 授權，再從首次安裝開始重跑 macOS 覆蓋更新及移除驗收。`v0.1.0` Draft 維持未發布。

### 8.3 `v0.1.1` Draft 驗證

`v0.1.1` 的正式版本來源與 Cargo workspace lock 已同步，完整測試與 lint 通過；annotated `v0.1.1` Tag 固定指向 Release Commit `8d70808`。取得重新打包授權後，本機 macOS ARM64 Community DMG 已建立，外層 SHA-256、Disk Image checksum、27 個內層檔案、App／Build 版本、ad-hoc 簽章、ARM64 Desktop／Agent／CLI、四個固定內建 Runtime 與新版移除程序均通過檢查。

取得 Draft Release 授權後，GitHub Actions Run `33222168031` 已從固定 Tag Commit `8d70808` 完成 macOS ARM64、Windows x64 與 Draft 建立 Job。Release 保持 `draft=true`、`published_at=null`，沒有 Publish；原 `v0.1.0` Draft 亦保持不變。

Draft 內 9 個 Assets 已全部重新下載驗證。`SHA256SUMS` 與三份個別校驗檔均通過，兩份 Manifest 逐位元一致，並確認 `requiresFullInstaller=true`、簽章欄位為 `null`、Connect 未混入 App 安裝包清單。DMG 為 99,295,774 bytes、SHA-256 `24849fd966de2f61c4641056f9ab1c6b0b0ed59308f2e9b3cb6388cdf60ddb28`；Windows Setup 為 48,332,278 bytes、SHA-256 `5bd0c91c8885e855c03865aba9909b02c26ee2e73503450c1af27fa3fd310319`；Connect 為 749,568 bytes、SHA-256 `2082d724e809a04111a78a74fe7f0aadd021218569a4589a8c6b7b9fd0a4710f`。DMG Disk Image checksum、內部 27 個校驗項目、App／Build `0.1.1`、ad-hoc codesign、ARM64 Desktop／Agent／CLI、新版移除程序及公開內容邊界均通過。

這只代表 Draft Assets 與封裝內容完整，仍不代表 macOS／Windows 乾淨機安裝、覆蓋更新、完整移除或 Publish 驗收完成。

## 9. Publish 後驗證

- 以未登入狀態開啟 Release 頁面及每個 Asset，狀態必須成功。
- 從公開 URL 重新下載安裝包並核對大小與 SHA-256。
- 驗證 `SHA256SUMS`、版本 Manifest 與 Release Assets 完全一致。
- 驗證 Stable 固定 Manifest URL 指向同一版本；若固定 URL 尚未啟用，P0 只公布 Release 頁面。
- 驗證 Source Tag 指向 Release Commit，且沒有重用或移動 Tag。
- 確認沒有意外發布 GitHub Actions 暫存 Artifact、Log 或內部 Runtime 開發包。
- 記錄最終 Release URL、Commit、Tag、Asset 大小及 SHA-256。

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
- [x] 建立只接受手動雙重確認、只會產生 Draft、不會自動 Publish 的 GitHub Actions Release workflow，並以 `v0.1.0` 與 `v0.1.1` 實際執行。
- [ ] 在乾淨 Mac 完成 Community DMG 首次安裝、覆蓋更新與移除驗收；`v0.1.0` 已因三項阻擋問題失敗，待新 Patch 版本重跑。
- [ ] 在乾淨 Windows x64 完成 NSIS 首次安裝、覆蓋更新與移除驗收。
- [x] 建立第一個 `v0.1.0` Draft Release，並從 GitHub 重新下載 9 個 Assets 驗證大小、Manifest 與 SHA-256。
- [x] 建立 `v0.1.1` Draft Release，並從 GitHub 重新下載 9 個 Assets 驗證大小、Manifest、SHA-256 與 DMG 內容。
- [ ] Repository Owner 人工核准第一個 Stable Publish。

目前只完成發布契約，不代表已有可供下載的正式 Stable Release。
