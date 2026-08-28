# fabDev P0 Public Release Specification

> 建立日期：2026-08-28
>
> 適用範圍：macOS ARM64／Windows x64 Unsigned Community Build
>
> 狀態：P0 發布契約已定義，尚未建立 GitHub Release 或重新打包安裝程式

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
- [ ] 產生 Release Manifest 與 Checksum 的可重現腳本。
- [ ] 建立只會產生 Draft、不會自動 Publish 的 GitHub Actions Release workflow。
- [ ] 在乾淨 Mac 完成 Community DMG 首次安裝、覆蓋更新與移除驗收。
- [ ] 在乾淨 Windows x64 完成 NSIS 首次安裝、覆蓋更新與移除驗收。
- [ ] 建立第一個 Draft Release 並從 GitHub 重新下載驗證。
- [ ] Repository Owner 人工核准第一個 Stable Publish。

目前只完成發布契約，不代表已有可供下載的正式 Stable Release。
