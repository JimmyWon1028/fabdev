# fabDev Online Update and Public Release Plan

> 規劃日期：2026-08-28
>
> 適用階段：macOS ARM64／Windows x64 Unsigned Community Build
> 文件狀態：規劃，尚未授權實作、上傳、打包或變更 GitHub Visibility

## 1. 已確認決策

- 目前先維持 Unsigned Community Build。
- 暫不申請或導入 Apple Developer ID、macOS notarization、Windows Code Signing。
- 暫不做背景完全自動覆蓋安裝。
- App、Runtime 與使用者資料必須使用互相獨立的更新流程。
- 線上下載使用 HTTPS；不讓 App 使用 FTP 或 SFTP 作為下載協定。
- 第一階段以 GitHub Releases 作為安裝包與 Runtime Package 的主要候選來源。
- 是否直接把 `JimmyWon1028/fabdev` 改為 Public，必須在公開前審查完成後再決定。

## 2. 更新範圍

fabDev 的更新分成三條獨立流程，不得合併成單一不可回復操作。

### 2.1 完整安裝包

- macOS：Community DMG。
- Windows：Community NSIS Setup.exe。
- 用於首次安裝、手動覆蓋更新與故障修復。
- 更新時保留 Sites、SQLite、Runtime、`php.ini`、MariaDB 設定與資料。

### 2.2 App 更新

App 更新包含：

- Tauri／Vue Desktop。
- App 內附的 Agent。
- CLI。
- App Bundle 內的 Helper。

App 更新不包含：

- Sites。
- Runtime 已安裝版本。
- `php.ini`。
- MariaDB 設定與資料。
- 使用者匯入的 Proxy／Site 設定。

### 2.3 Runtime 更新

下列元件使用獨立 Runtime Catalog：

- PHP。
- Nginx。
- dnsmasq。
- Node.js。
- MariaDB。

Runtime 更新由 Agent 管理下載、驗證、安裝、健康檢查、切換與回復，不跟 App 安裝包綁在同一次交易。

## 3. Unsigned Community 更新方式

目前 macOS Community 安裝程式需要管理員權限，會把 App 安裝至 `/Applications/fabDev.app`，並把固定功能的 Helper 安裝至 `/Library`。一般使用者權限執行的 Desktop 不應直接覆蓋這兩個位置。

第一階段採用以下流程：

```text
App 啟動或每日檢查版本
  → 顯示新版本、檔案大小與 Release Notes
  → 使用者按「下載更新」
  → 下載完整 DMG／Setup.exe 至暫存目錄
  → 驗證大小、SHA-256 與 fabDev Release 簽章
  → 提示使用者 Quit fabDev
  → 開啟已下載的安裝包
  → 由既有安裝程序執行覆蓋更新
```

更新檢查失敗、離線或下載中斷都不能阻止 App 正常啟動。

### 3.1 設定頁建議

設定頁新增「軟體更新」區塊：

- 目前版本。
- 最新版本。
- Stable／Beta Channel；第一階段只啟用 Stable。
- 自動檢查更新開關，預設開啟。
- 上次檢查時間。
- 「立即檢查」按鈕。
- Release Notes。
- 下載進度。
- 「下載更新」及「開啟安裝包」按鈕。
- Unsigned Community 的 Gatekeeper／SmartScreen 操作說明。

## 4. 下載來源

### 4.1 GitHub Releases

第一階段建議使用 GitHub Releases：

- 具備版本、Release Notes 與附件管理。
- DMG、EXE、SHA-256、Manifest 及 Runtime Package 可綁定同一版本。
- 可由 GitHub Actions 自動建立 Draft Release。
- 不需要自行維護公開下載 Server。

目前 `JimmyWon1028/fabdev` 是 Private Repository。Private Release 需要 GitHub 讀取權限，不能直接作為一般使用者免登入的公開下載來源。

可選擇以下其中一種方式：

#### 方案 A：fabdev Repository 改為 Public

```text
JimmyWon1028/fabdev
  ├─ Public source code
  ├─ Issues／文件
  ├─ GitHub Actions
  └─ GitHub Releases
```

優點：

- 架構最簡單。
- 不需要跨 Repository 發布 Token。
- 原始碼與 Unsigned binary 可供使用者交叉檢查。
- 同一個版本 Tag 可同時對應原始碼與安裝包。

限制：

- 原始碼、Git 歷史、Commit 作者資訊、文件與未來開發內容全部公開。
- 必須先完成授權、內部資料與 Git 歷史審查。
- 改為 Public 不會消除 macOS Gatekeeper 或 Windows SmartScreen 警告。

#### 方案 B：維持 Private，另建 Public Releases Repository

```text
JimmyWon1028/fabdev
  └─ Private source code and CI

JimmyWon1028/fabdev-releases
  └─ Public installers, manifests and release notes
```

適用於只想公開安裝包、不想公開原始碼與企業整合內容的情況。

### 4.2 GitHub Pages

可以使用 GitHub Pages 提供下載頁及小型 Manifest：

```text
https://jimmywon1028.github.io/fabdev/app/stable.json
https://jimmywon1028.github.io/fabdev/runtime/catalog.json
```

GitHub Pages 只放 HTML、版本 Manifest 與 Catalog。大型 DMG、EXE 與 Runtime Package 仍放 GitHub Releases。

未來有正式網域時，可改成：

```text
https://download.fabdev.example/
https://update.fabdev.example/app/stable.json
https://update.fabdev.example/runtime/catalog.json
```

### 4.3 FTP／SFTP

不建議使用 FTP 作為公開下載來源：

- 一般 FTP 沒有完整傳輸加密。
- App 與現代瀏覽器支援不佳。
- 容易被公司防火牆阻擋。
- 缺少 Release、版本說明與 CDN 管理。
- 需要自行維護 Server、安全更新、備份與流量。

若使用自備主機，SFTP 只作為管理者上傳方式，使用者與 App 仍透過 HTTPS 下載：

```text
管理端：SFTP upload
使用者端：HTTPS download
```

### 4.4 未來物件儲存

出現以下需求後，可把 Release 檔案搬到 Cloudflare R2、Amazon S3 或同類物件儲存：

- 需要自訂網域。
- 需要下載統計或更完整的快取控制。
- 需要 Stable／Beta／Private Channel。
- GitHub 下載速度或可用性不符合需求。
- Runtime Package 數量及流量明顯增加。

Manifest URL 應保持穩定，使 App 不需要因儲存來源更換而改版。

## 5. Release 目錄與 Manifest

建議使用不可變的版本路徑：

```text
/app/stable/latest.json
/app/releases/0.2.0/fabDev-Community-0.2.0-macos-arm64.dmg
/app/releases/0.2.0/fabDev-Community-0.2.0-windows-x64-setup.exe

/runtime/stable/catalog-v1.json
/runtime/php/8.4.25/macos-arm64.tar.gz
/runtime/node/24.20.0/windows-x64.tar.gz
```

版本檔案發布後不得覆蓋或重新使用相同版本號。只有 `latest.json` 與 Catalog 指標可以更新。

App Release Manifest 至少包含：

- Schema version。
- App version。
- Channel。
- Publish date。
- Release Notes。
- Minimum OS version。
- 每個平台／架構的 URL、大小、SHA-256 與 Release 簽章。
- Minimum Agent Protocol／Helper version 等相容條件。

Runtime Catalog 至少包含：

- Schema version。
- Catalog sequence。
- Generated time／expires time。
- Channel。
- Runtime name、version、platform、architecture。
- URL、大小、SHA-256 與套件簽章。

## 6. Runtime 線上下載流程

```text
Agent 下載已簽署 Catalog
  → 驗證 Catalog 簽章、時間及 Sequence
  → 選擇符合 OS／CPU／Channel 的版本
  → 下載至 .partial
  → 驗證允許的 HTTPS Host、大小、SHA-256 與套件簽章
  → 解壓至 staging
  → 執行 binary／設定健康檢查
  → 安裝為非使用中版本
  → 停止相關服務
  → 原子切換 current
  → 啟動並驗證
  → 失敗時切回上一版
```

更新政策：

- PHP：只偵測與通知，由使用者確認；不得自動替換使用中的版本。
- Nginx／dnsmasq：可先下載，但切換前要求確認並停止服務。
- Node.js：獨立更新，不跟 Site 自動切換。
- MariaDB：第一階段不做自動更新；先定義資料備份、格式相容與回復流程。
- 每個 Runtime 至少保留上一個可用版本，確認穩定後才允許清理。

目前 Runtime 已具備 SHA-256、staging 與 `current` 原子切換的基礎，但 Catalog 的 `signature` 仍是描述文字，不是正式的數位簽章。

## 7. Repository 改為 Public 的盤點結果

### 7.1 已確認

- 目前 Repository 與 Git 歷史沒有找到被追蹤的 `.env`。
- 沒有找到被追蹤的 PEM private key、P12、PFX、mobile provisioning 或 SSH private key 檔案。
- 程式內搜尋到的 Private Key／密碼內容屬於測試斷言與 fixture，不是真實憑證或密碼。
- Git 歷史目前為 12 個 Commit，完整審查仍在可管理範圍。
- 目前本機 `main` 比 `origin/main` 多一個尚未 Push 的 Commit；變更 GitHub Visibility 不會自動公開本機未 Push 內容。

### 7.2 公開前必須處理

#### License

Cargo workspace 宣告 `MIT OR Apache-2.0`，但 Repository 目前沒有正式 `LICENSE-MIT`、`LICENSE-APACHE` 或統一的 License 文件。公開前必須先確定授權策略並補齊檔案。

#### 客戶與企業整合資料

客戶名稱、內部網路位址、URL、Port、路徑與 Credential 相關設計內容不得進入公開 Repository。公開版本只保留不指向真實環境的中性範例。

#### Proxy 預設資料

公開 Repository 不得包含現有客戶的 Remote Connection 或內部位址。Proxy 文件與測試資料必須使用 `example.test`、`site-one.test` 等不指向真實環境的 fixture。

全新安裝的 Proxy 清單必須為空，不得預載、下載或自動匯入任何 Connection。Site Registry 全新安裝只建立唯一的 `demo.test`，不得加入其他 Site。

#### Commit 作者資訊

Git 歷史中的作者 Email 不是 GitHub noreply 地址。公開後既有 Commit Email 會一併公開。

#### Git 歷史

只刪除目前檔案不會清除舊 Commit。若完整歷史曾包含敏感內容，必須進行歷史清理、撤銷／更換相關 Credential，或建立乾淨的公開歷史。

#### GitHub 外部狀態

公開前還要人工檢查：

- GitHub Actions 歷史 Log。
- Issues、Pull Requests、附件與 Discussions。
- Actions／Dependabot／Environment secrets 的使用方式。
- Branch protection、Release permission 與 Tag 保護。

## 8. 公開前建議流程

```text
凍結公開決策
  → 審查目前檔案與全部 Git 歷史
  → 移除或泛化內部整合、Proxy、URL、IP 與 fixture
  → 確認 Commit 作者資訊
  → 補齊 License、SECURITY.md 與公開 README
  → 執行 secret scan、測試與 lint
  → 建立 Draft Release 並驗證安裝包
  → 人工確認公開內容
  → 最後才變更 Repository Visibility
```

Visibility 改為 Public 後，即使稍後改回 Private，也不能假設先前公開內容尚未被 Clone、Fork 或下載。

## 9. 發布流程

每次 Release：

1. 使用單一版本來源同步 App、Tauri、Cargo 與 JavaScript package 版本。
2. 執行前端、Rust、Helper 測試與 lint。
3. 建立 macOS DMG、Windows Setup.exe 與 Runtime Package。
4. 產生 SHA-256、Release 簽章與 Release Notes。
5. 建立 GitHub Draft Release。
6. 驗證 DMG／EXE 內容及 Runtime 描述檔。
7. 在乾淨 macOS／Windows 執行首次安裝與覆蓋更新測試。
8. 人工核准後發布 Release。
9. 最後才更新 `latest.json` 與 Runtime Catalog。
10. 保留上一版檔案；發生問題時撤回最新版本指標。

## 10. 驗收條件

- 離線或更新伺服器失敗時，App 仍可正常啟動及使用。
- 下載中斷不會留下可被誤安裝的完整檔名。
- 檔案大小、SHA-256 或簽章不符時拒絕安裝。
- 更新不破壞 Sites、Runtime、`php.ini`、SQLite、MariaDB 設定與資料。
- 更新前透過既有 Quit 流程停止 Web、MariaDB 與 Agent，不殘留 Port、PID 或 Socket。
- App、Agent Protocol 或 Helper 不相容時提供明確錯誤及回復方式。
- 新版啟動失敗時可回復至上一個可用 App／Runtime。
- macOS 驗證 Gatekeeper 操作說明及 `/Applications` 權限流程。
- Windows 驗證 SmartScreen 操作說明及執行中檔案替換流程。
- 公開 Repository 前完成 License、內部資料、Git 歷史及作者資訊審查。

## 11. 建議階段

### P0：公開下載基礎

- 決定 `fabdev` Public 或獨立 Public Releases Repository。
- 建立 GitHub Releases 發布規則。
- 建立下載頁、SHA-256、Release Manifest 與 Release Notes。

### P1：App 內檢查與下載

- 自動／手動檢查版本。
- 顯示新版資訊及下載進度。
- 驗證後開啟完整 DMG／Setup.exe。
- 不做背景自動覆蓋安裝。

### P2：Runtime 線上安裝

- Signed Runtime Catalog。
- Agent HTTPS 下載、驗證、健康檢查與回復。
- PHP、Nginx、dnsmasq、Node.js 分類更新政策。
- MariaDB 維持獨立人工更新流程。

### P3：未來正式簽章

- Apple Developer ID、notarization。
- Windows Code Signing。
- Tauri Updater artifact 與簽章。
- Helper 版本協調與 `SMAppService` Signed Distribution。

P3 不影響 P0～P2 的 Manifest、下載來源與 Runtime Catalog 設計，可在產品公開需求成熟後再導入。

## 12. 第一至第四階段執行結果

### 已完成

- 完成目前檔案與 12 個 Git Commit 的初步敏感資料盤點。
- 從目前工作樹移除客戶整合提案，不在公開版本保留其匿名化副本。
- 將程式碼、測試與文件中的客戶識別、內部路徑及特定內網 IP 改成中性範例。
- 修正 Proxy 文件：不再宣稱預載既有連線。
- 確認全新 Site Registry 只建立唯一的 `demo.test`；已有任何 Site 時不新增或覆蓋。
- 確認全新 Proxy Manager 使用空清單，不預載、下載或自動匯入 Connection。
- 補上 `LICENSE-MIT`、`LICENSE-APACHE`、`SECURITY.md` 與 README 的 License／Security 說明。
- 完成前端、Rust workspace、macOS Helper 測試、TypeScript、rustfmt、Clippy、Swift lint 與 `git diff --check`。
- 建立不會上傳的本機 Git bundle 復原檔，保留歷史清理前的 Private Repository 狀態。
- 從已清理且驗證通過的工作樹建立全新公開根 Commit，作者使用 GitHub noreply Email。
- 以 `--force-with-lease` 將 Private Repository 的 `main` 替換為乾淨歷史，並刪除仍指向舊歷史的遠端功能分支。
- 本機 `main` 已同步至乾淨歷史；一般本機與遠端 branch refs 不再指向舊 Commit。
- GitHub 目前只剩乾淨的 `main`；沒有其他 Branch、Tag、Pull Request 或 Release。
- 已刪除 9 次清理前的 GitHub Actions workflow run、相關 Log 與 3 個 Windows artifacts。
- 舊 Repository 已改名為 Private 且唯讀的 `fabdev-private-archive`，避免清理完成前誤改或誤公開。
- 已建立全新的 Private `JimmyWon1028/fabdev`，恢復 Repository 描述與功能開關，並只推送乾淨 `main`。
- 以舊 SHA 查詢全新同名 Repository 時，GitHub API 回覆找不到 Commit；舊物件不再能從新的公開候選網址讀取。

### 尚未執行

- 尚未修改 GitHub Repository Visibility。
- 尚未使用專用 secret scanner；目前僅完成規則式檔案與 Git 歷史掃描。
- `fabdev-private-archive` 仍需由 Repository Owner 在 GitHub 網頁手動永久刪除。

全新同名 Repository 已與舊 Git objects 隔離，但客戶資料仍暫存在 Private archive 與本機復原 bundle。確認 GitHub archive 已永久刪除、復原 bundle 已移至安全位置或依政策銷毀，並完成最後一次公開內容人工審核之前，不得把新 Repository 改為 Public。
