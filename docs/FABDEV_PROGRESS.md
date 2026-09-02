# fabDev 工作進度與 TODO

> 更新日期：2026-09-02
> 目前階段：[`v0.1.19`](https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.19) 已發布為 Latest Stable，Tag 固定在 Commit `be971bed1aa44ab22b6eeeb672420f4284fa4311`，不包含後續的 Windows 安裝語言與單一實例修正。Repository Owner 已通過 Commit `a1e83db7b6c71d692c1eddc4ccbcaea4ca9897a9` 的 Windows 功能實機 Gate；版本統一為 `0.1.20` 的 Commit `9f505906731402f610f8ff731e602f5a24b44b3d` 已由 Windows x64 Run `33612756679` 成功產出 `fabDev_0.1.20_x64-setup.exe` 候選，Setup.exe 本體 SHA-256 已由 Windows 端記錄，目前只待確認安裝程式檔案內容或安裝後 App 顯示版本為 `0.1.20`。不打包 macOS，也尚未建立 Tag、Draft 或 Publish

## 已完成

- Tauri／Vue Desktop、Rust Agent／CLI、Unix Socket Protocol 33 與 SQLite Site Registry。
- macOS App 與 `pnpm dev` 內建 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33、PHP 8.2.33；首次啟動只補缺少版本，保留既有開發資料。
- macOS 與 Windows 在 Site Registry 完全空白時建立唯一的 `demo.test`；Community 首次初始化會把 Site Home 固定在範例專案的父目錄，避免掃描其他本機專案，已有任何 Site 時不新增或覆蓋。
- `.test` DNS、Nginx、53／80／443 固定 Helper，以及 Start All／Stop All 與 menu bar 狀態。
- 每 Site HTTPS 啟用／停用、本機 CA 與 SAN 憑證、macOS Login Keychain／Windows Current User Root 信任，以及 HTTP 自動轉址 HTTPS。
- 多 Site、新增／移除、document root 偵測、每 Site PHP 7.4／8.2／8.4 切換，以及不使用 PHP 的純靜態 Site。
- Site Home 預設為 `~/Sites`；第一層非隱藏資料夾自動成為同名 `.test` Site，並保留原有 linked site。
- Sites 與 Proxy 主控台支援版本化 JSON 匯出／匯入；Sites 依網域略過重複，Proxy 依 ID、網域或 Listener Port 略過重複。
- PHP 7.4.33、8.2.33、8.4.24 並行 FPM、全域 PHP、Runtime 安裝／移除與持久 `php.ini`；上傳限制為 64M。
- PHP 設定提供由目前 PHP 8.2 設定初始化的預設 `php.ini` 範本，只套用到尚未建立專屬設定的 PHP minor。
- PHP 7.4 與 8.2 內建 Runtime 可安全移除；仍保留全域版本與 Site 使用中保護，明確移除後不會在下次啟動自動補回。
- 左側倒數第二項 Node.js 頁面提供 Windows x64 Node.js 20.20.2／24.20.0 並存選裝；預設均未安裝，支援每個版本安裝／更新／移除、明確設為全域及動態 terminal shim，不使用 nvm，也不接管外部 Node.js。
- 左側 Proxy Manager、Agent／CLI 的新增／移除、全部與單獨啟動／停止；全新安裝的 Proxy 清單為空，使用者設定與啟動狀態保存在 SQLite，所有 Listener 只綁 loopback，Port 衝突與上游故障互相隔離。
- 設定頁可持久開關「App 開啟時自動啟動服務」；預設開啟，已運行不重啟，部分異常會先清理再啟動。
- Community DMG 讓 App 內建 DNS、Nginx、PHP 7.4／8.2，並含 Helper、安裝／移除程序與唯一 `demo.test`；PHP 8.4、MariaDB 維持獨立選裝套件。
- 總覽的 Web 服務控制使用單一狀態按鈕：全部運行時顯示「全部停止」，其他狀態顯示「全部啟動」。
- 總覽的 MariaDB 卡片只顯示連線與運行狀態；啟動、停止及設定操作統一放在 MariaDB 頁面。
- menu bar、macOS App 選單及 `Command+Q` 的 `Quit fabDev` 會走同一套退出流程，先停止 Web 全部服務與 MariaDB、清理受管孤兒程序，再關閉 Agent 與 Desktop。
- Community Runtime 使用 `*-macos-arm64-community`、`community-ad-hoc` 描述及獨立 Catalog；開發套件維持 `*-dev`。
- Windows Named Pipe Agent、Nginx／PHP-CGI Platform Adapter、白名單 Hosts Helper 與單一使用者 NSIS 安裝程式。
- Windows 首次啟動會安裝內附 Nginx 1.30.4、PHP 7.4.33／8.2.33，並建立唯一的 `demo.test`。
- macOS ARM64 MariaDB 12.3.2 Runtime、主控台／menu bar／CLI 的獨立 Install／Start／Stop／Remove、3306 衝突檢查及隔離資料目錄。
- Sites 畫面的多 Site `LAN Site Share`：多個 Site 共用主機高位 Port 並由 Nginx 依 Host 分流；可逐一停止，最後一個 Site、Stop All、Agent Shutdown 或 App Quit 會釋放 Listener。
- Windows `fabdev-connect.exe`：UAC 後自動管理多個有明確標記的 `.test` hosts，以非同步 Client `127.0.0.1:80` 代理轉送到主機，保存最後使用的主機與 Sites，並在從 Parallels Shared Folders 啟動時自動轉存本機 Runtime，再要求 UAC。

## 最近驗證

- 2026-09-01：[`v0.1.17`](https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.17) 已發布為 Latest Stable Release，Release ID `380491300`，`draft=false`、`prerelease=false`、發布時間 `2026-09-01T13:26:54Z`；Tag 固定在 Commit `533877cc4b3f05d5d8df94aa99758fefaf16735c`。GitHub Actions [Run 33504988884](https://github.com/JimmyWon1028/fabdev/actions/runs/33504988884) 的 Windows x64 Runtime／NSIS、macOS ARM64 Runtime／DMG 與 Draft 組裝 Jobs 全數通過。Release 共 30 個 Assets、685,454,665 bytes；Publish 前全部重新下載後，GitHub API digest `30/30` 逐筆一致，`SHA256SUMS` 內 13 個主要 Assets 與 13 份個別 checksum 全數通過。App／Stable Manifest 逐位元一致，版本 0.1.17、Stable channel、Windows x64 與 macOS ARM64 兩個安裝包；Runtime Catalog sequence 11、minimum App 0.1.17、Protocol 36，含 Windows 6＋macOS 4 共 10 個 Runtime，12 個 Manifest 實體引用的大小、SHA-256 與正式 `v0.1.17` URL 全數一致。`SHA256SUMS` SHA-256 為 `8126ffe9d0a26cb40e6fbda2436d8539fb591f16dd1b13c8edd1aa7f5f146c9f`，App／Stable Manifest 為 `7f6262570c37580bcf0facd4709e5f61f63e6b92cba07fe229fcc208812c978f`，Runtime Catalog 為 `10ec4acbb51e484a3dbf03f19350121c337d5e6feae32ca774586913c657da8e`，DMG 為 `880c912939a6bef47f22f4e71bcdb59b571ad78348d8fd786ea5c267b4724f56`，Windows Setup 為 `40cdd199305d10649279e068058e55c78088d15aeea05917405443ac65589c3d`。DMG `hdiutil verify`、內部 27 筆 checksum、App／Agent／Helper／CLI 簽章與 0.1.17 版本均通過；四份內建 Runtime descriptor 大小／SHA-256 一致，366 個 Runtime Mach-O 全為 ARM64、簽章有效且無 Homebrew／`/usr/local` 執行期依賴。Windows NSIS 可解出 214 個項目，Desktop／Agent／Helper 與 PHP 均為 x64；VC Runtime 安裝前檢查回歸測試通過。依既定分工不重跑 Windows 或 macOS 實機安裝／更新／移除。Publish 後 Release、Latest、App／Stable／Runtime Manifest 均由未帶 Token 的公開 URL 回傳 HTTP 200，Windows Setup／macOS DMG Range 回傳 HTTP 206 且總大小分別為 49,340,299／100,969,057 bytes；公開 Manifest Hash 與 Publish 前驗證值一致，Latest 指向 `v0.1.17`。Repository 目前沒有待清理的 Draft。
- 2026-09-01：依明確重新打包授權，以 Commit `80033bb971015a88eeb95d08e5fcf54c58a43cec` 建立本機 `fabDev-Community-0.1.17-macos-arm64.dmg`，99,680,458 bytes，SHA-256 `7a5ec3c2ab0e9d290518c251aecd5945e8134c905bd1a22fbfd4135f23ef732f`。`pnpm test`、`pnpm lint`、`hdiutil verify`、DMG 內 27 個 `SHA256SUMS`、App／CLI／Helper 簽章、App／Build／CLI／Agent 0.1.17 及 Desktop／Agent／Helper／CLI ARM64 均通過。內建 Runtime 僅含 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33 與 PHP 8.2.33；四份 descriptor 大小／SHA-256 逐筆相符，共 369 個 Runtime Mach-O 皆為 ARM64，未發現 Homebrew／`/usr/local` 執行期依賴。本候選未安裝、未上傳，也未建立 Tag 或 Release。
- 2026-09-01：`0.1.17` Windows 替代候選 Commit `cfd9f22f024f29960ed1416c06af0e8af4f5f745` 的 Windows x64 [Run 33500683364](https://github.com/JimmyWon1028/fabdev/actions/runs/33500683364) 全數成功；NSIS 含 214 個封裝項目，Desktop、Agent、Windows Helper 與 Connect 均為 x64 PE，版本與 Runtime Manifest 通過靜態驗證。Repository Owner 後續保留原始 190-byte `my.ini` 失敗現場安裝替代候選，MariaDB 已能自動清理已知半成品並完成重新初始化，Gate 4 實機測試回報通過。候選仍僅存在 Actions artifact，尚未建立 Tag 或 Release、未發布。
- 2026-09-01：[`v0.1.15`](https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.15) 已發布為 latest Stable Release，Release ID `380254567`，Tag 固定在 Commit `e121d44c27d671097bd44c7806ee37f085988d42`。GitHub Actions [Run 33473901421](https://github.com/JimmyWon1028/fabdev/actions/runs/33473901421) 的 Windows x64 Installer／Connect／六個 Runtime、macOS ARM64 DMG／四個 Runtime、驗證與 Draft Jobs 全數通過。Release 共 30 個 Assets、685,584,720 bytes；全部重新下載後通過 `SHA256SUMS`、13 份個別 checksum、兩份逐位元一致 App Manifest、Runtime Catalog sequence 9／minimum App 0.1.15、10 個 Runtime gzip 與 Manifest 內 12 個實體資產的大小／SHA-256／URL 逐筆比對。`SHA256SUMS` SHA-256 為 `532bb14641b7b5a443a48622edce58266b97107040da1b1068389e503e1ed7e2`，App／Stable Manifest 為 `5a95b8b8d17e685bf03dd7b85a0d6c1d8da069b4986f4f963536d5230f40c1a6`，Runtime Catalog 為 `9223436f338df5c76609f7bf1b410bdce72cdce94b63ac0e76566e12c9da5690`，DMG 為 `a03dfa809798afbd8730ed64e78f781d069dc22768c3587b276408ac0da62f95`，Windows Setup 為 `77601f7453ffaada6182a09d60d5b36f208c3058a3fb8b3daf652964060567cc`。DMG `hdiutil verify`、內部 checksum、App／Helper codesign、版本 0.1.15 與 Desktop／Agent／Helper ARM64 均通過；Windows NSIS 可解出 x64 Desktop／Agent／Helper。Publish 後 Release、latest、Stable／Runtime Manifest 均由未帶 Token 的公開 URL 回傳 HTTP 200，Setup／DMG Range 回傳 HTTP 206，兩份公開 Manifest 與 Draft 驗證檔逐位元一致，latest 指向 `v0.1.15`；沒有待清理的 Draft。
- 0.1.15 修正 Windows Proxy 網域未同步至 Hosts、導致部分 Windows x64 VM 無法解析 Proxy `.test` 網域的問題；Agent 會經白名單 Helper 維護獨立 Proxy Hosts 區塊，設定更新、移除、狀態還原與解除安裝都會同步清理。Windows NSIS 另在複製檔案前檢查 Microsoft Visual C++ 2015-2022 x64 Runtime／`VCRUNTIME140.dll`，缺少時開啟官方下載並中止本次安裝，讓使用者安裝 prerequisite 後重試。Repository Owner 已在 Windows x64 VM 驗證 VC Runtime 缺失引導、重試安裝與 Proxy 連線通過；macOS 安裝程序未變，依既定規則沿用先前人工生命週期驗收。
- 2026-09-01：[`v0.1.14`](https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.14) 已發布為 latest Stable Release，Tag 固定在 Commit `279f030c6aaa252ba38406f0c069d549950069fa`。GitHub Actions [Run 33462756426](https://github.com/JimmyWon1028/fabdev/actions/runs/33462756426) 的 Windows x64 Installer／Connect／六個 Runtime、macOS ARM64 DMG／四個 Runtime、驗證與 Draft Jobs 全數通過。Release 共 30 個 Assets、685,358,353 bytes；全部重新下載後通過 `SHA256SUMS`、13 份個別 checksum、兩份逐位元一致 App Manifest、Runtime Catalog sequence 8／minimum App 0.1.14、10 個 Runtime gzip 與 DMG `hdiutil verify`。`SHA256SUMS` SHA-256 為 `858a9c7d2f5749e68b87b9f8aa96f0224bfc88412991c1f583d9657c9f95c6a9`，App／Stable Manifest 為 `3409e7e67a620571d3e7bfcca086582b52071ed541928ace76acd9b3949f3ebf`，Runtime Catalog 為 `8f2b525b9041ba6bec41b37957836fa6c6fbc7a593d707cb6f4ce67818c194fe`，DMG 為 `faf8708d840690f9ee68e1fe5d00d935046a0e71e1adb0c7ffd0edf99e7857b8`，Windows Setup 為 `a999a72190ecb5c660a0134367a560cc49dd0e1fda361ed16ad2a26f528aa49a`。Publish 後 Release、latest、App／Runtime Manifest 均由未帶 Token 的公開 URL 回傳 HTTP 200，Setup／DMG Range 回傳 HTTP 206，latest 指向 `v0.1.14`；沒有待清理的 Draft。
- Windows VM 已以既有 0.1.12 從公開 Stable Feed 偵測、下載並驗證 0.1.14 Setup。按「重新啟動並更新」後，0.1.12 主視窗與 Agent 退出，但 Setup 沒有啟動，磁碟版本維持 0.1.12，重現 Repository Owner 回報；原因是第一次交接仍由已安裝的 0.1.12 舊 launcher 執行，0.1.14 安裝包無法反向改寫舊 App 程式碼。同一個已驗證 Setup 以 `/UPDATE /P /R` 手動覆蓋後成功自動重啟，Desktop／Agent 畫面版本 0.1.14、Desktop 檔案版本 0.1.14、`demo.test` HTTP 200、原 SQLite 24,576 bytes 與 `demo.test.conf` 均保留，DNS／Nginx／PHP-FPM／Proxy 恢復運行。
- 0.1.14 已加入 Windows 獨立 PowerShell launcher、ready-file handshake、launcher log 與下載取消 Command／UI；完整 `pnpm test`、`pnpm lint`、`git diff --check`、Windows MSVC Actions 及 Windows updater／Desktop 回歸測試通過。0.1.12 本身不會顯示停止下載；0.1.14 發布後已是最新版本，無法在不偽造或替換公開 Stable Feed 的情況下實測「0.1.14 發起下一版更新」及下載中途取消，兩項保留至下一個公開 Patch 做 VM UI 驗收。macOS 本次只重新打包與驗證 DMG 映像，依 Repository Owner 指示不重跑安裝／啟動／移除或完整 Runtime／HTTPS 人工流程。
- 2026-09-01：[`v0.1.13`](https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.13) 已發布為 latest Stable Release，Tag 固定在 Commit `e35d894c9a5ccf287b9c581db1ce12ff09c0d942`。Release 共 30 個 Windows x64／macOS ARM64 Assets、683,496,859 bytes；Draft 全部重新下載後與上傳前集合逐位元一致，`SHA256SUMS` 與 13 份個別 checksum、兩份相同 App Manifest、10 項 Runtime Catalog sequence 7、全部 Runtime Archive gzip 與 DMG `hdiutil verify` 均通過。`SHA256SUMS` SHA-256 為 `8540668dcf5c8326e23bd35852a150c8a646d066ac4ee8d7419a28374e3a285f`，App／Stable Manifest 為 `b76157c0a76d235a8afee065dd3388bfae7890071cfeaa77612cdc1ede5f6f9d`，Runtime Catalog 為 `05c32631e430c335b00453e61c154a46401f0afdce221448dbc309f13511d2f7`。Publish 後 Release 頁、Stable／App／Runtime Feed、總表、DMG 與 Windows Setup 均以未帶 GitHub Token 的公開 URL 回傳 HTTP 200，latest 指向 `v0.1.13`。
- 第一次雙平台 workflow [Run 33446761377](https://github.com/JimmyWon1028/fabdev/actions/runs/33446761377) 的 Windows Node.js／MariaDB Runtime 與 Windows App／PHP／Connect／NSIS Jobs 全數通過；macOS Job 只因 PHP 7.4 打包階段健康檢查使用 PHP 8 才提供的 `str_contains()` 而停止。健康檢查已在本機 Commit `72573ab` 改為 PHP 7.4 相容的 `strpos(...) === false` 並以真實 PHP 7.4.33 FPM 通過；這不是 App 或 Runtime 執行期修改，因此依 Repository Owner 決定不重新打包，沿用已驗證的 `0.1.13` Windows CI 產物、macOS DMG 與 P4 Runtime 候選完成 Draft／Publish。
- 2026-09-01：Windows x64 與 macOS ARM64 的四個正式版本來源及 13 個 Cargo workspace lock entries 已同步為 `0.1.13`。完整 `pnpm test` 已在允許 localhost bind 的環境通過，`pnpm lint`、shell syntax、版本一致性與 `git diff --check` 亦通過。本機重新打包的 `fabDev-Community-0.1.13-macos-arm64.dmg` 為 99,708,248 bytes，SHA-256 `c6b91f5b735a8ab447bed9eb4a89808dfd633c1ce5d38eba51973146c3f0c9c7`；外層 checksum、`hdiutil verify`、App／Build `0.1.13`、深層 ad-hoc codesign 與 Desktop／Agent／CLI ARM64 架構均通過。依 [`AGENTS.md`](../AGENTS.md) 的既定規則，因安裝與更新程序未變，本版不重跑兩平台安裝／啟動／移除、完整 Runtime／HTTPS 人工流程或 Windows VM smoke test。
- 2026-09-01：macOS ARM64 P4 未標 Tag 候選已完成。以 `fabDev-Community-0.1.12-macos-arm64.dmg`（SHA-256 `ef08dc127989c244982ee5ca6a8e2d1fad7088a496b68eece4f90a21dc7b632b`）完成移除舊 App／資料後的乾淨安裝，App、Agent、Helper、唯一 `demo.test`、DNS、Nginx、PHP 8.2 與固定 80 入口正常；另從封裝版 `v0.1.3` 覆蓋至 `0.1.12`，原 Site ID、Demo 檔案 SHA-256、受管 `php.ini` SHA-256 與自訂標記均逐位元保留，更新後 HTTP 200／PHP 8.2.33。
- macOS Runtime Desktop UI 已從 sequence 7 本機 Catalog 實際下載並安裝 PHP 8.4.24、MariaDB 12.3.2、Node.js 20.20.2／24.20.0。PHP 下載在 56% 人工中斷後保留 28,704,768-byte `.part`，重試送出 `Range: bytes=28704768-`、收到 `206 Content-Range` 並從原進度完成；Updater 已補上 macOS `.part` 保留、既有前綴雜湊、Range 驗證、伺服器忽略 Range 時安全重啟，以及中斷續傳／忽略 Range 回歸測試。
- MariaDB 已完成 Socket／TCP 登入、停止／重啟、資料列持久性、Quit 暫停與 App 重開後依使用者偏好自動恢復。實機發現啟動時只重寫使用中的 PHP 8.2 設定、未同步已安裝但未啟用的 PHP 8.4；已改為每次 MariaDB 連線來源切換都重新產生所有已安裝 PHP minor 的 FPM 設定，再只重啟使用中的版本，8.2／8.4 `mysqli` 與 `PDO MySQL` 均自動指向 Managed Socket，停止後回復 System Socket。
- Node.js 20／24 的隔離 `node`／`npm`／`npx`／`corepack` 執行、20 → 24 全域切換、動態 `current` 與完整停用還原均通過。實機發現只寫 `.zprofile` 會被互動式 shell 後載入的 Homebrew／Herd `.zshrc` PATH 蓋過；macOS 終端整合已改為在 `.zprofile` 與 `.zshrc` 各維護單一可還原標記。login 與 interactive login shell 都解析到 fabDev 20.20.2／24.20.0，停用後兩個檔案 SHA-256 完整回復且四個 shim 全數移除。
- `demo.test` 已切換至 PHP 8.4.24，Agent 8080 與 Helper 固定 80 均 HTTP 200；啟用 HTTPS 後 HTTP 301、固定 443 HTTPS 200、SNI、`DNS:demo.test` SAN、CA chain、Login Keychain 信任及 CA／Site 私鑰 `0600` 均通過。Start → Stop 無殘留 → Start、服務與 MariaDB 運行中直接 `Command+Q`、backend Port／PID／Socket 清理、App 重開後 Web／MariaDB／PHP 8.4／HTTPS／資料自動恢復均通過；驗收後 Web 與 Managed MariaDB 已正常停止、3306 已釋放，隔離測試 CA 亦依精確 SHA-256 Fingerprint 從 Login Keychain 移除。
- 本輪完整 `pnpm test` 已在允許 localhost bind 的環境通過，包含 Desktop 69、Release 11、Platform 11、Services 49、Updater 19 與 macOS Helper 9 項測試；`pnpm lint`（Vue typecheck、rustfmt、Clippy、Swift lint）及 `git diff --check` 亦通過。未 Commit、Push、Tag、建立 Draft 或 Publish。
- 2026-09-01：ARM64 最後發布閘門已完成唯讀 Assets 基線。從公開 `v0.1.12` Stable Release 重新下載 20 個 Windows Assets，原 `SHA256SUMS` 的 Setup、Connect 與六個 Runtime 全數通過；再與已驗收的 10 個 macOS DMG／Runtime Assets 組成 30 檔雙平台集合。Windows 八個 binary 與公開檔逐位元一致，macOS 五個 binary 與 P4 候選逐位元一致；雙平台 App Manifest 保留原 Windows 發布時間並同時列出 Windows x64／macOS ARM64，Runtime Catalog sequence 7 含 Windows 6＋macOS 4 共 10 項，SHA-256 `6acfbc2586c061c68f3ed7ae2be98af629cf6225d8ec189e91e667645123c8a4`，DMG `hdiutil verify` 通過。最終集合共 30 檔、683,488,548 bytes；`SHA256SUMS` SHA-256 為 `d08f6aecce204804568d8a2cd88c2a8cc8bb7322c8fab6b373d317b7545e94ca`，App／Stable Manifest SHA-256 為 `73ae4f2ee0743a2d63c8bbcb71e14d779726348cc565e0c5beee83482c728939`。未上傳或替換任何遠端 Asset。
- 同次盤點確認 `v0.1.12` 已是公開、非 Draft Stable Release，現有 workflow 會明確拒絕同 Tag 再建 Release；公開 Tag 也固定在本輪未提交 macOS 修正之前。不得移動已公開 Tag，亦不得將無對應 Tag commit 的 dirty-worktree 產物直接補入 Stable Release。安全發布路徑需使用下一個同時包含 Windows 與 macOS 的共同 Patch 版本，待 Repository Owner 明確確認版本後才可 Commit、Push、Tag、重新打包、建立 Draft 或 Publish。
- 2026-08-31：已安全同步 `origin/main` 的 `v0.1.12`，並保留本機 macOS P1 修改及 `backup-before-v0.1.12-sync-2026-08-31` 備份 Stash。重新打包的 `fabDev-Community-0.1.12-macos-arm64.dmg` 為 99,705,414 bytes，SHA-256 `ef08dc127989c244982ee5ca6a8e2d1fad7088a496b68eece4f90a21dc7b632b`；外層 checksum、App 0.1.12、Desktop／Agent arm64、深層 ad-hoc codesign 與 DMG 內部 `SHA256SUMS` 全數通過，內建 Runtime 仍只含 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33／8.2.33。
- macOS ARM64 sequence 7 本機 Runtime 候選已重新由固定官方來源建立並完成實際健康檢查：PHP 8.4.24 為 51,047,476 bytes、490 個條目、SHA-256 `3aad33ef06f1770d6781b38778333eef44e0ee99050c02b47a5dcab3f1e6f900`；MariaDB 12.3.2 為 117,748,526 bytes、1,032 個條目、SHA-256 `762bcafdb63525ec7e6491defa4031248e752e8d6d75726b65080f813e28a23a`；Node.js 20.20.2 為 41,960,455 bytes、5,370 個條目、SHA-256 `d4a011ec50b2081e74497a62ede70fc7585de4ab705eeae9be9347887d52f8dd`；Node.js 24.20.0 為 53,393,422 bytes、5,888 個條目、SHA-256 `a2a05bda3cdfb2bc33305dd377810b19915bf6adf480d683e0aa5639d7688583`。四個 Archive 均為單一版本根目錄，PHP CLI／擴充／FPM FastCGI、MariaDB 初始化／Socket SQL／停止清理及整包 minimum OS 檢查通過。
- Draft Release workflow 已恢復 macOS ARM64 Job：同時建立 DMG 與四個線上 Runtime，與 Windows 六個 Runtime 一起產生 30 個 Release Assets 及單一跨平台 Catalog，仍只允許手動 Draft 且沒有 Publish 路徑。以 `v0.1.12` 六個公開 Windows Archive 完成 size／SHA-256 重驗後，真實 sequence 7 Catalog 共 10 項（Windows 6、macOS 4）、minimum App 0.1.12、Protocol 36，SHA-256 `6acfbc2586c061c68f3ed7ae2be98af629cf6225d8ec189e91e667645123c8a4`；macOS 本機 Release 候選 14 個檔案的總表與個別 checksum 全數通過。整合腳本已將 staging／output 正規化為絕對路徑，避免 Cargo Catalog 階段遺失相對輸入。完整 `pnpm test`、`pnpm lint`、workflow YAML、shell syntax 與 `git diff --check` 均通過；Desktop 69、Release 11、Runtime 22、Agent 23 與 macOS Helper 9 項測試通過。本輪未 Commit、Push、Tag、建立 Draft 或 Publish。
- 2026-08-31：`v0.1.12` 已發布為最新 Windows x64 Stable Release：<https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.12>。GitHub Actions Run `33389051643` 全數通過；20 個 Draft Assets 約 321 MiB，重新下載後通過總表、八份個別 SHA-256、App Manifest、Runtime Catalog sequence 6 與六個 Runtime Archive 完整性驗證。公開 Stable／Runtime Feed、Setup、PHP 7.4／8.2 端點均由匿名 URL 驗證成功。
- Windows VM 已由 `0.1.11` 使用 `/UPDATE /P /R` 原地覆蓋至 `0.1.12` 並自動重新啟動；唯一 `demo.test`、Site ID、PHP 8.2、全域 PHP 8.2.33 與設定檔雜湊均保留，Agent `0.1.12`／Protocol 36 與 HTTP 200 通過。Publish 後再實際移除並由公開 Catalog 重新下載安裝 PHP 7.4.33／8.2.33，兩版大小、SHA-256、CLI、`mysqli`、`pdo_mysql` 及移除標記清除均通過，最後恢復全域與 Site PHP 8.2。
- 2026-08-31：macOS ARM64 P1 Runtime 對齊已完成主要本機實作。PHP 8.4.24、MariaDB 12.3.2、Node.js 20.20.2／24.20.0 已具 Community Package 模式、固定官方 SHA-256／PGP 完整 Fingerprint、單一版本封裝根目錄、個別 checksum、整包 Mach-O minimum OS 強制檢查與完整 macOS Runtime Catalog 產生／驗證流程；整合腳本只建立本機輸出，不執行 Git、Tag、Draft 或 Publish。
- macOS Node.js 20／24 已支援並存安裝、更新、移除、全域版本切換與動態 `node`／`npm`／`npx`／`corepack` shim；`.zprofile` 與 `.zshrc` 各使用獨立可還原標記，確保互動式 shell 後載入的外部 PATH 不會蓋過 fabDev，shim 每次執行都解析 `runtimes/node/current`，停用後完整還原且不修改或接管 Homebrew、nvm、Herd 或系統 Node.js。
- MariaDB 線上更新改為執行中可安全更新：Agent 暫時停止服務但不改寫使用者啟動偏好，保留 Data／Config／Log，成功後使用新 active Runtime 重新啟動並立即重套 PHP MariaDB 連線；安裝或啟動失敗時恢復原 active Runtime，回滾測試確認既有資料、設定與 Log 不受影響。
- 四個真實 macOS ARM64 Runtime Archive 已由 Agent 忽略型整合測試完成解壓、並存安裝、active 狀態與 binary 健康檢查；Rust Runtime 21 項、Agent 一般 23 項與真實 Archive 1 項、Platform 11 項、Desktop 68 項、Release 規則 11 項、Vue production build、Clippy、shell syntax 與 Catalog sequence 901 驗證均通過。Node.js 24 官方 binary 的最低系統為 macOS 13.5，Catalog 已標記 `minimumOsVersion: 13.5`，Agent 會依目前 macOS 版本隱藏不相容套件。
- PHP 8.4.24 macOS ARM64 Community Runtime 已改由固定版本與 SHA-256 的官方原始碼建立隔離相容依賴，不再複製 Homebrew Runtime library 或 ImageMagick module。隔離候選的 31 個 Mach-O 全部為 macOS 13.0 或更早、外部依賴引用為 0；CLI 擴充、Imagick PNG、IMAP、Tidy、FPM 設定與實際 FastCGI PHP request 均通過。另以獨立 loopback 高位 Port 完成 bundled Nginx → PHP-FPM → PHP 8.4.24 Site HTTP 驗收，正常停止後 Nginx／FPM／Port／Socket／暫存目錄均已清理；使用已占用 Port 的預期失敗案例亦未留下暫存狀態。候選 Archive 為 51,039,206 bytes、493 個條目、單一 `8.4.24/` 根目錄，SHA-256 `c2231dac90b7cca407ed2ecc05e7a86511dbb40e6980e8d71b993e150bdf0afa`；這是隔離本機驗收候選，不是公開 Asset。
- PHP 真實候選另已通過隔離 Online Agent Protocol：本機 Catalog 驗證及 accepted state、正式 pending cache、Queued → Verified、`InstallDownloadedRuntime`、解壓與固定健康檢查均成功，安裝 8.4.24 時保留原 active 8.2.33。將 verified cache 內容篡改後，安裝前 SHA-256／大小重驗會拒絕候選、不建立 8.4.24 Runtime，active 版本保持不變；Runtime 安裝錯誤現在保留完整 anyhow error chain，Desktop 可顯示實際失敗原因。
- 同一 PHP 候選已由 updater 透過實際 loopback HTTP 串流完整 51,039,206 bytes，走正式 `.part` 寫入、大小／SHA-256 驗證與 atomic finalize，再接續 Online Agent Protocol 安裝測試；`validate-macos-php-online-flow.sh` 可重複執行此隔離流程。
- PHP 8.4.24 已再以獨立 `fabDev Runtime Test.app`、全新隔離 Application Data 與 Computer Use 完成 Desktop UI 驗收：Catalog sequence 903 由 `127.0.0.1` 高位 Port 實際 GET，UI 顯示 48.7 MiB／SHA-256 與兩次 Unsigned Community 確認，下載、100% 進度、安裝前重驗、解壓及固定健康檢查均成功。安裝後及 App 重啟後皆顯示 8.4.24 已安裝；`current`／全域 PHP 保持 8.2.33，`demo.test`、`demo2.test`、`tei.test` 仍使用 8.2。隔離 App／Agent／PHP-FPM／Feed 均已停止，正式 App／Agent PID 與 53535／8080／8443 listener 未變。此驗收不等同公開 GitHub Feed 的外部網路下載。
- 隔離 Desktop UI 安裝後，另由正式 App 正常退出釋放 53535／8080／8443，再讓同一 `fabDev Runtime Test.app` 與隔離 Agent 接手固定後端 Port。`demo.test` 經 UI 從 PHP 8.2 切換至 8.4；Helper 固定 53 Port 解析為 `127.0.0.1`，Agent 8080 與 Helper 固定 80 Port 皆回應 HTTP 200、`X-Powered-By: PHP/8.4.24`，Document Root 指向隔離 Site。驗收後 Site 已還原 PHP 8.2，隔離 App／Agent／dnsmasq／Nginx／PHP-FPM 均停止；正式 App 已重新啟動並恢復 Agent 已連線、服務運行及 `demo.test` HTTP 200／PHP 8.2.33。
- 已依明確重新打包授權建立本機 `fabDev-Community-0.1.11-macos-arm64.dmg`，大小 99,706,401 bytes、SHA-256 `95edd7c8e0f59a5f13be175af0d55c2245004b52c563006f551733173554e239`。DMG 為唯讀壓縮 UDZO；掛載後內部 `SHA256SUMS` 全數通過，App 深層 ad-hoc 簽章有效，Desktop／Agent／Helper／CLI 均為 arm64，App／CLI 版本皆為 0.1.11。基礎包只內建 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33／8.2.33；PHP 8.4、MariaDB 與 Node.js 維持線上選裝。本產物是未發布本機候選，未 Commit、Push、Tag、Draft 或 Publish。
- MariaDB 12.3.2 macOS ARM64 Community Runtime 已由官方 SHA-256／PGP 完整 Fingerprint 驗章後重新建置；封裝內 96 個 Mach-O 全部為 macOS 13.0 或更早、外部依賴引用為 0，資料目錄初始化、Unix Socket 啟動、`SELECT VERSION(), @@version_comment, 1 + 1`、正常停止及 Socket／PID 清理均通過。候選 Archive 為 117,744,853 bytes、1,032 個條目、單一 `12.3.2/` 根目錄，SHA-256 `85d1f44561b5becf6a802358926352ee8d1f87eefaac96e3dff64dfad932aec6`；這是隔離本機驗收候選，不是公開 Asset。Node 20 的最低系統 11.0 與 Node 24 的 13.5 亦已通過。正式 P1 尚需公開 Catalog／Assets，以及乾淨 macOS 的 Desktop 公開 Feed 外部網路下載、MariaDB 保留資料升級、Node 全域切換及失敗回復驗收。本輪未變更版本、Commit、Push、Tag、Draft 或 Publish。
- 2026-08-31：Windows x64 App 更新已改為安全退出後使用 Tauri NSIS `/UPDATE /P /R` 原地覆蓋並自動重新啟動，不走舊版移除流程；設定頁文字已同步說明 Sites、Runtime 與使用者資料會保留。
- Windows x64 App／Runtime GitHub Artifact 下載新增 8 MiB 分段、最多 4 路並行、4 次退避重試、跨 App 重啟續傳、完成後整包 SHA-256，以及設定頁速度／預估剩餘時間；macOS 下載與安裝流程保持不變。
- 本輪 Desktop 66 項、Release 規則 9 項、Updater 15 項、Vue production build、rustfmt、Updater Clippy 與 `git diff --check` 通過；Parallels Windows 11 的 x64 target Desktop 編譯及 Updater 15 項原生測試通過。公開 `v0.1.10` Windows Setup 的單 Byte Range 實測回傳 `206`、`Content-Range: bytes 0-0/49258503`。
- Windows x64 `0.1.11` 未簽章本機候選 NSIS 已完成，大小 49,295,735 bytes，SHA-256 `8c6bffb7099cfe1e8730eaa34012a973b402551e17f268d1421ab1311c5dc1c7`；PE／NSIS、File Version、Product Version、Desktop、Agent、Helper 與內建 PHP 7.4.33／8.2.33、Nginx 內容靜態檢查通過。
- Parallels Windows 11 ARM 的 x64 相容環境已由安裝版 `0.1.10` 使用 `/UPDATE /P /R` 原地更新至 `0.1.11`，Installer exit code 0 且 App 自動重新啟動。唯一 `demo.test` Site ID、Site Home、PHP 8.2、空白 Proxy、MariaDB 與 Connect 設定雜湊均保持不變；Agent 版本 `0.1.11`／Protocol 36、HTTP 200／PHP 8.2.33、Stop → Start 與 80／443 清理回歸通過。
- 2026-08-31：Windows x64 Node.js 20.20.2／24.20.0 與 MariaDB 12.3.2 已接入 Runtime Catalog v1、Agent Protocol 36 與 Desktop。Node 兩個 major 會各自顯示安裝／更新狀態，Node 20 顯示 EOL 警告；下載支援進度、取消、大小／SHA-256 與安裝前重新驗證。
- Node.js 安裝後驗證 `node.exe` 與 `npm.cmd`，但不自動切換全域或 PATH；按「設為全域」後才啟用 `node`／`npm`／`npx`／`corepack` shim。MariaDB 必須停止才能安裝或升級，成功後保留既有 data／config／log 並重新套用 PHP MariaDB 連線，失敗時恢復原 active Runtime。
- Windows Runtime 產包腳本已改為可在 macOS／Ubuntu 重跑並可只建置指定 Runtime；以本機已下載的固定上游檔案實際完成 MariaDB（約 99 MB）與 Node.js（約 36 MB）SHA-256、官方 PGP 完整 Fingerprint、單一版本根目錄及 descriptor 驗證。
- Parallels Windows 11 已用 x64 MSVC target 從目前 workspace 編譯 Agent 測試，並以真實 Node.js 20.20.2／24.20.0 Package 完成解壓、並存安裝、active 切換、動態 `node.cmd` shim 與 `node.exe`／`npm.cmd` 健康檢查；1 項原生整合測試通過。本輪 VM 編譯目錄與 Mac 驗收 Package 已清除。
- Draft Release workflow 已加入兩版 Node.js／MariaDB 的獨立 verified build job、Windows 原生真實 Archive 安裝／binary 健康檢查，以及四 Runtime Catalog 與 16 個 Release Assets 契約。`v0.1.11` Windows CI、封裝 App、Draft 與 Publish 後匿名 Feed 驗收均已完成。
- 本輪本機驗證：Desktop 64 項測試、Vue production build、Runtime 20 項與 Agent 18 項測試、Rust workspace、Release Script 8 項、workflow 與 shell syntax 均通過；另以 Windows VM 原生 x64 target 完成 Node.js 20／24 真實 Archive 驗收。
- `v0.1.11` 歷史 Stable Release：<https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.11>。16 個 Draft Assets 全數重新下載並通過總表、個別 SHA-256、Manifest 與 NSIS 完整性；Publish 後匿名 Release 頁、Stable Manifest、Runtime Catalog 與完整 Windows Setup 均為 HTTP 200，公開 Setup SHA-256 為 `3c12f1b24ffbd7675bc325b87c41f20459924a1ba14e6e3f58e9a41cbfb0c3ee`。
- Windows VM 使用 `v0.1.11` 實際 Updater 程式碼讀取公開 Stable Feed：由 `0.1.10` 判定 `0.1.11` 可更新、由 `0.1.11` 判定無新版，並以四路 8 MiB Range 下載 49,305,659 bytes Setup、完成整包 SHA-256 與 pending installer 驗證；公開 Range 實測回傳 `206`。

- `v0.1.3` 已發布為最新 Stable Release：<https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.3>。Release `379130930` 為非 Draft、非 Pre-release，共 9 個公開 Assets；遠端 annotated Tag、`main` 與 `origin/main` 均固定在 Commit `1d6625d42e16e65e2b188a5da2c4c4774f784f74`。
- 9 個 `v0.1.3` 公開 Assets 已由匿名 URL 重新下載；DMG、Windows Setup、Connect、`SHA256SUMS`、三份個別 checksum 與兩份逐位元一致的 Manifest 全數通過。公開 Stable Manifest 為 `0.1.3`、Agent Protocol 32、`requiresFullInstaller=true`，Unsigned Community 簽章欄位維持 `null`。
- Windows 11 ARM64 Parallels 的 x64 App 相容環境已從封裝版 `0.1.2` 於 App 內偵測、下載並更新至 `0.1.3`；大小與 SHA-256、Quit 後 Desktop／Agent／Nginx／PHP 清理、NSIS 覆蓋安裝及重新啟動均通過。原 `demo.test` Site ID、Site Home 與空白 Proxy 保留，更新後 HTTP 200／PHP 8.2.33。
- 完整測試：前端 55、Release Script 5、Rust 145、macOS Helper 9 項一般測試全數通過；另有 1 項需指定實際 MariaDB Runtime 與 2 項公開網路整合測試維持忽略，後兩項已在 P1 實作時分別手動執行通過。
- 隔離 HTTPS 流程已確認 CA chain、`tls-e2e.test` SAN、Nginx 1.30.4 `-t`、18444 高位 TLS 與實際 HTTPS 靜態檔回應；`demo.test` 已完成 Login Keychain 信任、HTTP 301 與正式 443 HTTPS 200 驗證。
- `pnpm lint`：TypeScript、rustfmt、Clippy 與 Swift lint 通過。
- Community Runtime Catalog 清單含 dnsmasq 2.93、Nginx 1.30.4、PHP 7.4.33、8.2.33、8.4.24 與 MariaDB 12.3.2；基礎 DMG 已依內建四個 Runtime 的規格重新打包。
- PHP 8.4.24 官方 SHA-256／PGP 驗證、CLI／FPM 設定、必要 Extension 與封裝通過；Mach-O 不依賴 `/opt/homebrew`。
- PHP 8.4.24 已透過 Agent 安裝至目前 Application Support；全域 PHP 維持 8.2.33。受管 `php.ini` 已驗證 64M 上傳限制與 Asia/Taipei，實際安裝 Binary 載入成功。
- DMG 唯讀掛載後，內外層 SHA-256、App 簽章與內建 Runtime Descriptor 雜湊全數通過；App 內只有 dnsmasq、Nginx、PHP 7.4／8.2，沒有 `-dev`、PHP 8.4 或 MariaDB Runtime。
- GitHub Actions Windows MSVC 完整檢查與 NSIS 建置成功；產物為 45 MiB 的單一自解壓安裝程式。
- MariaDB 12.3.2 Runtime 官方 SHA-256／PGP 驗證、Server／Client 版本與封裝通過；隔離流程 Start → TCP SQL → Stop → Start → 資料讀回通過。
- MariaDB 首次初始化已用實際 `Application Support` Runtime 與含空白的隔離資料路徑驗證；12.3.2 TCP SQL 查詢通過。
- Agent 重啟後會由 fabDev 專用 PID 與 Unix Socket 恢復 MariaDB 運行狀態，仍可從主控台單獨停止；失效 PID 不會被接管。
- App 啟動時會依 `state/mariadb.json` 恢復 MariaDB 最後一次成功啟動／停止的狀態，並與 Web stack 自動啟動設定分離。
- 左側 MariaDB 頁面可獨立啟停並持久設定非特權 TCP Port 與 Data Directory；運行中、非絕對路徑及非 MariaDB 的非空目錄會被拒絕。
- MariaDB 連線來源完全自動：Managed Service 實際運行時使用 fabDev Socket，未安裝或已安裝但停止時使用 System／Homebrew 連線；啟動或停止後立即重建使用中的 PHP-FPM 設定。Unix 依 Socket、Windows 依 fabDev PID 與 TCP readiness 判定 Managed 狀態。左側 MariaDB 頁面不顯示來源或 Socket 選項，未安裝時也不顯示設定卡片；已以 Adminer 驗證 `localhost` 登入 Homebrew MariaDB。
- 左側 MariaDB 頁面新增 macOS `my.cnf`／Windows `my.ini` 額外選項編輯器及 root 密碼設定；額外設定會由 MariaDB 驗證，受管連線與程序選項不能覆寫，密碼不持久化。
- 主控台安裝／移除隔離流程通過：執行中拒絕移除，停止後只移除 Runtime，重裝後可讀回保留資料。
- `fabdev-share` 雙向 TCP 轉送及停止後 Port 釋放測試通過；`fabdev-connect` 的 hosts 新增／移除、衝突拒絕、網域驗證、雙向轉送與 Port 釋放共 4 項測試通過。
- `fabdev-connect` 通過 `x86_64-pc-windows-msvc` 交叉編譯檢查；Windows GUI、UAC、實際 hosts 與瀏覽器流程待 Parallels Windows 11 驗收。
- Node.js 24.19.0 LTS 官方 macOS ARM64 Archive SHA-256 與發布者 PGP 簽章驗證通過；選裝套件已產生並確認 Node v24.19.0、npm 11.17.0、描述檔與單一 `24.19.0/` 封裝根目錄。
- Proxy 聚焦測試確認自訂新增／移除與驗證、設定持久化、HTTP Host 改寫、Credential CORS、實際 streaming response、單一 Port 衝突隔離及停止後 Port 釋放。

## 2026-08-30 工作日誌

- `0.1.2` 封裝版 App 更新驗收在 macOS 發現大小寫不敏感磁碟會把新舊 App 名稱誤判為兩份 App；已於 `0.1.3` 修正同 inode 路徑辨識，同名但確實為不同 App 時仍會安全拒絕更新。
- `v0.1.3` GitHub Actions 已完成 macOS ARM64、Windows x64 與 Draft Release Jobs。Draft 內 9 個 Assets 已重新下載並通過大小、SHA-256、Manifest、DMG 內部 checksum 與公開內容邊界驗證。
- macOS ARM64 已完成 `0.1.1 → 0.1.3` 覆蓋更新、資料保留、`demo.test` DNS／HTTP 200、PHP 8.2.33 與安全 Quit 清理；Windows 已完成 `0.1.1 → 0.1.3` 人工覆蓋、解除安裝資料保留與重新安裝生命週期驗收。
- Repository Owner 明確核准後，`v0.1.3` 已於 `2026-08-30T00:54:23Z` Publish。發布後 9 個 Assets、公開 Stable／App Manifest、匿名下載與 checksum 均再次驗證通過。
- Windows 封裝版 `0.1.2 → 0.1.3` App 內線上更新已完成：App 下載 48,728,655 bytes Setup、驗證 SHA-256 `fdb9fe3830791be471311f701d7ba1c4e8877e4ae3d7fa3a3e7b03b66aec4254`，安全 Quit 後開啟 NSIS；更新後 Desktop／Agent 皆為 `0.1.3`、Protocol 32，唯一 Site、Site Home 與空白 Proxy 均保留。
- 驗收期間觀察到 Windows Agent 會重複記錄 PHP 8.2 狀態端點 `404 Not Found`，主控台也持續等待 PHP-FPM 指標；實際 `demo.test` HTTP 200 與 PHP 8.2.33 正常，因此不阻擋 `v0.1.3`，但列入後續修正。
- P2 Runtime Catalog v1 規格已完成；第一個目標為 PHP 8.4.24 macOS ARM64／Windows x64 Side-by-side 線上安裝，採固定 GitHub Release URL、`.part`、大小／SHA-256、使用者確認、Agent 固定健康檢查與失敗清理。Unsigned Community 的 Catalog／Package signature 固定為 `null`，完整契約見 [`RUNTIME_ONLINE_UPDATE_SPEC.md`](RUNTIME_ONLINE_UPDATE_SPEC.md)。
- P2.1 已完成 Runtime Catalog v1 Typed Model、1 MiB Parser、產生器與嚴格 Validator；涵蓋固定 Product／Channel、RFC 3339 UTC 時間、SemVer／Protocol 相容、Sequence 與 Catalog SHA-256 防回退、兩平台 PHP 8.4.24 固定 URL／檔名、nullable signature、上游來源驗證、大小／SHA-256 及重複項目檢查，並維持既有本機 Runtime descriptor 相容。
- P2.2 已完成 Agent Protocol 33 Runtime 更新請求／回應、固定 Catalog URL、GitHub HTTPS Redirect Host 白名單、系統 Proxy／信任庫、Catalog 與 Sequence 快取、`.part` 串流大小／SHA-256、原子完成、驗證快取重用、背景進度輪詢、取消／失敗清理、啟動殘檔清理及 Shutdown 取消。公開 Runtime Feed 尚未發布，GitHub 匿名實際下載留待 P2.4。
- P2.3 已完成 PHP Runtime 線上安裝 UI、Unsigned Community 警告、版本／大小／SHA-256／進度顯示、下載與安裝兩次確認，以及 Protocol 33 `InstallDownloadedRuntime`。Agent 安裝前會重新驗證快取 Catalog 與 Package，解壓至 staging 後執行固定 CLI／版本檢查，安裝後再驗證必要 MySQL extensions 與 macOS FPM／Windows CGI；PHP 8.4.24 只並存安裝，不切換 `current`、全域 PHP 或 Site，失敗時清除本次新增內容。公開 Feed、真實兩平台 binary 與 Site HTTP 驗收留待 P2.4。
- P2.4 執行規劃已建立於 [`P2_4_RUNTIME_DRAFT_ACCEPTANCE_PLAN.md`](P2_4_RUNTIME_DRAFT_ACCEPTANCE_PLAN.md)：正式 Community Runtime 產生器、Windows 空白 php.ini／必要 extensions 回歸及 Draft workflow 的 14 個 Asset 契約已完成，正式候選版為 `0.1.6`。
- P2.4a 已實作正式 Rust Runtime Catalog v1 產生器、固定兩平台 PHP 8.4.24 Package metadata、Windows 專用可重現 Package 腳本、空白使用者 php.ini／內部必要 MySQL extensions 分離，以及包含 14 個 Assets 且永不自動 Publish 的 Draft workflow；本機 Release Script 7 項與靜態檢查通過，Windows MSVC Run `33293398434` 的格式、前端測試、Rust workspace、Connect、NSIS 及產物上傳也全數通過。
- `v0.1.4` 首次重新打包的 Windows Job 與三個 macOS Runtime 建置通過，但完整 Rust 測試發現 Windows Runtime `minimumOsVersion: "11"` 不符合兩段數字契約；Draft 未建立，遠端 Tag 保留且不移動。修正為 `"11.0"` 後，Root／Desktop package、Tauri config、Cargo workspace 與全部 fabDev workspace lock entries 已升為 `0.1.5`。
- `v0.1.5` Draft workflow Run `33295048040` 全數通過並建立 14 個 Assets；重新下載後的總表、個別 checksum、App／Runtime Manifest、DMG、NSIS、Connect 與兩平台 PHP 8.4.24 Archive 均通過靜態驗證。macOS `0.1.3 → 0.1.5` 覆蓋安裝保留唯一 `demo.test`，HTTP 200；Production Agent 隔離安裝 PHP 8.4.24 後 CLI、FPM、`mysqli`、`pdo_mysql` 正常，且未切換全域 PHP、Site 或 `current`。
- Windows `0.1.3 → 0.1.5` 覆蓋安裝已通過：Protocol 33、唯一 `demo.test`、全域 PHP 8.2.33、空白 Proxy 與 MariaDB 未安裝狀態均保留，HTTP 200。PHP 8.4.24 Package 的大小／SHA-256 驗證成功，但 Rust `tar` 完成 79 個檔案與 7 個子目錄解壓後，在套用 Windows 目錄 mtime 時回傳 `Access is denied (os error 5)`；失敗 staging、Catalog 快取與暫存檔已精確清除，既有服務與 PHP 8.2.33 仍正常。
- Windows Runtime 修正為解壓時不保留 Archive mtime，並在 Draft workflow 使用當次真實 `php-8.4.24-windows-x64-community.tar.gz` 執行 Rust 安裝回歸；本機 Rust workspace、Release tests、Clippy、格式與 diff 檢查通過。`v0.1.5` 不發布，需升版、Windows CI 真實 Package 安裝與兩平台重新驗收後才能 Publish。
- 未標記候選 Commit `1d16676` 已通過 Windows x64 CI Run `33300616719`：格式、前端測試、Desktop sidecars、內建 Windows Runtime、完整 Rust workspace、Connect、NSIS 與產物上傳全數成功；專案版本因此升為 `0.1.6`，真實 PHP 8.4.24 Package 安裝回歸留在 Draft workflow 執行。

## 2026-08-29 工作日誌

- 專案正式版本來源已由 `0.1.1` 更新為 `0.1.2` 本機候選版；本次已取得重新打包授權，但不包含 Commit、Push、Tag、Draft Release 或 Publish 授權。
- 本機 `fabDev-Community-0.1.2-macos-arm64.dmg` 已重新打包完成，大小為 98,639,468 bytes，SHA-256 為 `4b718f1f639347e93531ea192c5064883620f9fd09f509f0185fb0df2a754c2b`。Disk Image checksum、27 個內層 SHA-256、App／Build `0.1.2`、ad-hoc 簽章、ARM64 Desktop／Agent／CLI、固定四個內建 Runtime、安裝／移除程序來源一致性與公開內容邊界均通過。
- P1 App 更新已接上 Desktop 設定頁與 `crates/updater`：支援啟動後每日自動檢查、手動檢查、Stable 新版資訊、Release Notes、下載進度、完整 DMG／Setup.exe 下載、大小／SHA-256 驗證，以及走安全 Quit 後開啟已驗證安裝包。第一階段不做背景自動覆蓋安裝。
- 更新來源固定為 Public GitHub Releases Stable Manifest；Manifest 會嚴格驗證產品、Channel、版本、發布時間、官方 URL、平台、架構、完整安裝模式、檔名、大小與 SHA-256。Unsigned Community 要求 `signature: null`，網路使用平台原生 TLS、系統 Proxy 與系統信任庫。
- 實際公開 Stable Manifest 檢查通過；並以相同下載流程完整取得 99,295,774 bytes 的 macOS DMG，通過大小、SHA-256、待安裝 Manifest 快取與再次驗證後清除測試檔。前端 55 項、Updater 5 項一般測試與完整 `pnpm test`／`pnpm lint` 均通過；另有 2 項明確標記的公開網路整合測試維持忽略，已在本次分別手動執行通過。
- `v0.1.1` 已發布為最新 Stable Release：<https://github.com/JimmyWon1028/fabdev/releases/tag/v0.1.1>。Release `378823889` 狀態為非 Draft、非 Pre-release，共 9 個 Assets；未登入 Release 頁面回傳 HTTP 200。9 個 Assets 已從公開 URL 全部重新下載，大小、總表與個別 SHA-256、兩份 Manifest 及與發布前 Draft 的逐位元比對全數通過；遠端 annotated Tag 仍固定在 Commit `8d70808`。
- Repository Owner 已明確授權提交並推送發布前後驗收文件、更新 `v0.1.1` Release Notes、Publish 與公開下載驗證。
- `v0.1.1` Windows x64 Setup 已在 Parallels Windows 11 ARM 的 x64 模擬層完成 `0.1.0 → 0.1.1` 覆蓋更新、資料保留、Start／Stop／Start、PHP 7.4／8.2 切換、HTTP 200、解除安裝與乾淨資料基線首次安裝。首次啟動只有 `demo.test`，Proxy 為空；解除安裝清除 App、登錄、Hosts、程序與 Port，並依政策保留使用者資料。
- Draft Connect 已確認 Shared Folder 啟動後轉存相同 SHA-256 的本機 Runtime 並進入 UAC `--elevated`；同時驗證它會拒絕接管本機 fabDev 已存在的同名 Hosts 紀錄。多 Site 實際轉送與中斷清理維持 P2，不列為 P0 NSIS Publish 阻擋條件。
- quarantine DMG 副本保持原 SHA-256，Gatekeeper 對 ad-hoc、無 Team ID 的 App 如預期回報 rejected；管理員安裝已在完整生命週期驗收通過。53／80／443 檢查位於 Helper 寫入前，實際特權 Port 衝突因 sudo 授權已失效未再次重跑。
- macOS hosted release 的 `rust-objcopy` 警告來自 runner Rust 工具缺少 `libLLVM.dylib`；已讓 Tauri release build 與 Community CLI 明確使用 `CARGO_PROFILE_RELEASE_STRIP=none`。完整測試、lint 與無 stripping 警告的 release App build 通過，修正已推送至 main，但不移動固定的 `v0.1.1` Tag 或變更 Draft Assets。
- 專案正式版本來源已由 `0.1.0` 更新為 `0.1.1`；annotated `v0.1.1` Tag 固定在 Release Commit `8d70808`。GitHub Actions 已從該 Tag 重新建置 macOS ARM64／Windows x64 產物並建立 Draft，完成驗收後已另行 Publish。
- 本機重新打包的 `fabDev-Community-0.1.1-macos-arm64.dmg` 為 98,158,623 bytes，SHA-256 為 `fba390ef39b0fe6e0542a64448c4af954423bc2ea8a3e3ca47777397565a22fc`；DMG、27 個內層檔案、App／Build 版本、ad-hoc 簽章與 Desktop／Agent／CLI ARM64 均驗證通過，新版 Uninstaller 與來源一致。此 Hash 只記錄本機候選包，不取代未來 Draft Assets 的重新下載驗證。
- `v0.1.1` Draft 的 9 個 Assets 已全部重新下載；總表與三份個別 SHA-256、兩份逐位元一致的 Manifest、實際檔案大小及 DMG 內部 27 個校驗項目全數通過。DMG 為 99,295,774 bytes、SHA-256 `24849fd966de2f61c4641056f9ab1c6b0b0ed59308f2e9b3cb6388cdf60ddb28`；Windows Setup 為 48,332,278 bytes、SHA-256 `5bd0c91c8885e855c03865aba9909b02c26ee2e73503450c1af27fa3fd310319`；Connect 為 749,568 bytes、SHA-256 `2082d724e809a04111a78a74fe7f0aadd021218569a4589a8c6b7b9fd0a4710f`。
- 從 `v0.1.1` Draft 重新下載的 DMG 已在恢復至 fabDev 未安裝基線的 Mac 完成管理員首次安裝；App／Helper 正確安裝，首次初始化只有 `demo.test`、Proxy 清單為空，Site Home 已保存為 Demo 父目錄，重啟後沒有掃描其他本機專案。外部 Resolver 與 System／Homebrew MariaDB 全程保持不變。
- macOS App 選單的 `Quit fabDev` 已以實際 UI 操作驗證，Desktop、Agent、dnsmasq、Nginx、PHP-FPM、Proxy 與內部 Port 全部清理。`demo.test` 啟用 HTTPS 後，Login Keychain CA 信任、HTTP 301、HTTPS 200、leaf SAN 與私鑰 600 權限均通過。
- 同版覆蓋更新保留 Site ID、Site Home、HTTPS、CA／leaf certificate、Demo、空白 Proxy 與 Resolver；更新後手動開啟 App 可正常恢復服務。完整移除則清除 App、Helper、使用者資料、Demo、CA、程序與 listener，三個本次項目移至垃圾桶且可復原，外部 Resolver 與 MariaDB 仍未受影響。
- 從 GitHub Draft 重新下載的 `v0.1.0` DMG 已通過管理員安裝、Helper／Resolver 建立、唯一 `demo.test` 的 DNS、HTTP、HTTPS、憑證 SAN 與 Login Keychain 信任驗證；Proxy 首次安裝清單為空。
- 乾淨初始化發現 Site Home 未持久化，導致預設掃描其他本機專案；已改為建立 `demo.test` 後同步保存其父目錄，並加入不匯入同層無關資料夾的回歸測試。
- macOS App 選單的原生 Quit 項目會直接結束 Desktop，沒有停止 Agent 與 Web 服務；已換成具有 `Command+Q` 的 fabDev 自訂 Quit 項目，統一交由既有的安全退出流程處理。
- Community 移除程序原本只依目前資料目錄的 CA Fingerprint 撤銷信任，無法清除舊資料留下的 fabDev CA；已改為逐張核對精確 Subject、Issuer 及 Fingerprint，再移除所有符合的 fabDev 自簽 CA，且不依賴使用者資料仍存在。
- `v0.1.0` 原始移除程序已清除 App、Helper、資料與 Demo；殘留的舊 fabDev CA 已依精確 Fingerprint 人工移除，安裝前保留的外部 `/etc/resolver/test` 也已恢復。這項人工補救不算原始安裝包通過移除驗收。
- 以上三項為 `v0.1.0` Draft 的 P0 阻擋問題；原 Tag 與 Draft 保持不變且不得 Publish。修正需使用新的 Patch 版本重新打包、建立 Draft 並重跑完整驗收。

## 2026-08-26 工作日誌

- 完成 Agent Protocol 25 Proxy Manager；新增獨立 `fabdev-proxy` Rust Runtime、新增／編輯／移除、Credentials Origin、全部／單獨啟動停止、CLI、Desktop 頁面與 SQLite 設定及啟動狀態持久化。
- 隔離 Agent 流程確認自訂 Proxy 新增後可跨重啟保存；執行中的 Connection 移除時會停止並釋放 Port，第二次重啟後不會恢復已移除設定。
- Proxy Listener 固定 loopback，其他程序占用 Port 時只標記該 Connection Failed；上游請求或 15 秒 TCP Health Check 失敗標記 Degraded，不影響其他連線。
- 完成單一穩定 Node.js LTS Runtime 的狀態、安裝與移除；既有 SQLite `node_version` 欄位保留供舊資料相容，但不再由 Site 使用。
- 新增左側倒數第二項 Node.js 頁面；預設顯示未安裝，安裝後可由同一頁獨立移除。
- 建立 Node.js 官方 Archive／SHA-256／PGP 驗證與 fabDev Runtime 封裝腳本；Runtime 不修改 Homebrew、nvm、Herd、系統 Node.js 或使用者 PATH。
- 使用隔離 Agent 與實際選裝套件完成預設未安裝 → 安裝 → Node v24.19.0／npm 11.17.0 執行 → 移除 → 回到未安裝的完整流程。
- 完整前端、Rust workspace、macOS Helper 測試與 lint 通過；macOS 缺少 Windows MSVC C Header／Library 工具鏈，因此本機 Windows workspace 交叉檢查未完成，仍以 GitHub Actions Windows MSVC 為正式驗證環境。

## 2026-08-25 工作日誌

- 完成 Agent Protocol 20 的每 Site HTTPS 流程、本機 CA、`.test` SAN leaf certificate、Nginx 8443 TLS listener、HTTP 轉址及 System Helper 固定 `443→8443` 代理。
- 處理已安裝舊版 Helper 未包含 HTTPS 入口的狀況；更新並重新安裝 Helper 後，同意信任目前使用者 Login Keychain 內的 fabDev CA，再重新啟用 `demo.test` HTTPS。
- 最終以 `demo.test` 驗證 DNS、HTTP 301、正式 443 HTTPS 200、憑證 SAN 與 CA chain；先前瀏覽器的 `ERR_SSL_UNRECOGNIZED_NAME_ALERT` 已排除。
- 查閱 Laravel Herd 官方 AI／MCP 功能：Herd 讓外部 AI Client 透過 MCP 取得 Site／Runtime／Service 資訊，執行 Site 診斷、HTTPS／PHP 切換及服務管理；它不是內建生成式 AI 對話功能。
- fabDev 後續 MCP 方向定為既有 Agent Protocol 的薄型轉接層。第一階段優先提供唯讀的 `site_information`、`site_status`、`diagnose_site` 與 Log／服務狀態，再逐步開放需確認的 HTTPS、PHP 與服務啟停操作；不得提供任意 Shell 或擴大 Helper 權限。

## 驗證邊界

- 目前公開 Stable Manifest 為 `0.1.17`，同時提供 Windows x64 與 macOS ARM64。Windows 已完成封裝版 App 內 `0.1.2 → 0.1.3`、`0.1.11 → 0.1.12` 更新驗收、0.1.12 舊 launcher 失敗重現與 0.1.14 手動覆蓋驗收、0.1.15 VC Runtime prerequisite 與 Proxy 連線 VM 驗收，以及 0.1.17 Managed MariaDB 刪除／半成品復原實機 Gate；0.1.14 新 launcher 發起 `0.1.15` 更新與下載中途取消尚未執行完整 VM UI 補測。macOS 已完成 `0.1.1 → 0.1.3` 與 `0.1.3 → 0.1.12` 覆蓋更新，0.1.17 因安裝程序未變只做重新打包、映像與封裝內容驗證，不重跑人工生命週期測試。
- 更新失敗與重試由 Updater 聚焦測試覆蓋；公開 Release 的成功下載與覆蓋流程已實測，但不會為了製造故障而修改已發布 Asset 或 Stable Manifest。
- `fabdev-updater` 已通過 `x86_64-pc-windows-msvc` 交叉編譯；完整 Desktop 的 Windows 本機交叉檢查停在既有 bundled SQLite C 建置缺少 MSVC `stdlib.h`，需由 Windows MSVC GitHub Actions 或實機環境驗證，並非 Updater Rust 程式錯誤。
- `v0.1.0` 的首次 Site Home、App 選單 Quit 與舊 CA 清理三項阻擋問題，已由 `v0.1.1` Draft 在恢復至 fabDev 未安裝基線的 Mac 完成首次安裝、覆蓋更新與完整移除回歸。
- 覆蓋安裝程序結束後未觀察到 App 保持運行；手動開啟後所有服務與保留資料驗證通過。若後續要把「更新後自動重新開啟 App」列為發佈條件，仍需在另一個 macOS Session 重現確認。
- Gatekeeper quarantine 已驗證會拒絕 ad-hoc App；53／80／443 衝突腳本已確認先檢查再寫入，但本次未在 sudo 授權失效後重新建立實際特權 Port 衝突。
- release stripping 工具鏈警告已在 main 修正並以無警告 release App build 驗證；固定的 `v0.1.1` Tag 與既有 Draft Assets 不回寫此未來建置修正。
- Windows x64 Setup 已在 Parallels Windows 11 ARM 的 x64 模擬層完成生命週期驗收；乾淨實體 Windows x64、SmartScreen 簽章信譽與 IIS／Herd 共存尚未驗證。

## TODO

Laravel Herd 可借鏡但尚未完成的完整盤點與優先順序，見 [`HERD_REFERENCE_BACKLOG.md`](HERD_REFERENCE_BACKLOG.md)。

### P0：Community Beta

- [x] 完成 Public Repository、Release Asset 命名、Stable Channel、App Manifest v1、Draft／Publish 與回復契約；見 [`PUBLIC_RELEASE_SPEC.md`](PUBLIC_RELEASE_SPEC.md)。
- [x] 建立 Release Asset／Manifest／Checksum 產生器；驗證四個版本來源與 Agent Protocol，不覆蓋既有輸出，也不執行打包或發布。
- [x] 建立只接受手動雙重確認、既有 Tag 且只會建立 Draft 的 GitHub Actions Release workflow；只有最後 Job 具寫入權限，已用 `v0.1.0` 與 `v0.1.1` 完成兩平台建置與 Draft 建立。
- [x] 在恢復至 fabDev 未安裝基線的 Mac 驗證安裝 → 自動啟動 → `demo.test` → 更新 → 完整移除；`v0.1.1` 已通過原三項阻擋問題回歸與外部 Resolver／MariaDB 共存檢查。
- [x] 驗證 Gatekeeper、quarantine 與管理員授權；實際 53／80／443 特權 Port 衝突保留為後續補充驗收。
- [x] 修正 release stripping 工具鏈警告。
- [x] 建立第一個 `v0.1.0` Draft Release，重新下載 9 個 Assets，核對實際大小、Manifest 與 SHA-256；目前仍未 Publish。
- [x] 建立 `v0.1.1` Draft Release，重新下載 9 個 Assets 並驗證大小、Manifest、SHA-256、DMG 內容與公開內容邊界；完成 macOS／Windows 驗收後已 Publish。
- [x] Repository Owner 已在 Mac／Windows 驗收完成後人工核准 `v0.1.1` Publish。
- [x] 更新 `v0.1.1` Release Notes、Publish Stable Release，並完成未登入頁面、9 個公開 Assets、Checksum、Manifest、Draft 位元組與固定 Tag 驗證。
- [x] 建立並驗證 `v0.1.3` Draft Release；完成 macOS／Windows 覆蓋、Windows App 內線上更新、資料保留及 Publish 後公開下載驗收。
- [x] Repository Owner 已核准 `v0.1.3` Publish；Release Notes 已補上發布後驗收結果，Stable Tag 固定在 Commit `1d6625d`。

### P1：核心開發體驗

- [x] App 啟動後每日自動檢查與設定頁手動檢查 Stable Manifest；離線或更新失敗不阻止 App 啟動。
- [x] 顯示版本、發布資訊、Release Notes、安裝包資料與下載進度；完整安裝包使用 `.part`、大小／SHA-256 驗證、原子改名及開啟前再次驗證。
- [x] 使用者確認後先走既有安全 Quit，停止 Web、MariaDB、受管程序與 Agent，再開啟 DMG／Setup.exe；不做背景自動覆蓋安裝。
- [x] 使用高於 `0.1.1` 的封裝版完成 App 更新驗收：Windows 實測 `0.1.2 → 0.1.3` 的偵測、下載、完整性驗證、Quit、開啟 Setup 與覆蓋更新；macOS 完成 `0.1.1 → 0.1.3` 覆蓋及安裝器回歸，失敗與重試由 Updater 測試覆蓋。
- [ ] 修正 Windows PHP-CGI 狀態輪詢持續收到 404 並重複寫入日誌的問題；不得影響已正常運作的 Site HTTP／PHP 流程。
- [ ] 提供可由一般本機瀏覽器操作的 Web UI；新增只綁定 loopback、具身分驗證與權限限制的 HTTP／WebSocket API，並讓前端在非 Tauri 環境改走該 API。
- [ ] 建立 PHP 8.3 Community Runtime 與升級偵測通知。
- [x] 提供 macOS／Windows 全域終端機 PHP shim；切換全域版本時由固定 shim 動態跟隨 `current`／`current.version`。macOS 以可還原標記停用 Herd PHP PATH，Windows 使用目前使用者 PATH，不修改 Machine PATH。
- [ ] 擴充 Composer／Artisan 與 Site-aware shim，依目前目錄選擇 Site PHP。
- [ ] 加入可進版控的 `fabdev.yml` Site 設定。
- [ ] 提供 Redis、LDAP、ODBC 等選配 PHP Extension 管理。
- [ ] 建立 `fabdev-mcp` 薄型轉接層；先提供每 Site 範圍的資訊、狀態與 DNS → HTTP／HTTPS → Nginx → PHP → MariaDB 診斷，再加入具確認、白名單與敏感資訊遮罩的變更工具。

#### Windows 安裝體驗清單

- [x] Windows NSIS 安裝包支援繁體中文、簡體中文與英文；手動啟動安裝包時，語言選擇必須是第一個畫面，即使已有先前保存的安裝語言也不可略過，並依 Windows 預設 UI 語言預選對應語言。App 既有的 `/UPDATE /P /R` 被動更新流程不得因語言選擇視窗而停住。
- [x] Windows Desktop 必須維持單一實例；再次啟動只還原、顯示並聚焦既有主視窗，不得建立第二個 Desktop 程序、主視窗或系統匣圖示，也不得另啟 Agent。
- [x] Repository Owner 已以 Commit `a1e83db7b6c71d692c1eddc4ccbcaea4ca9897a9` 的 Windows x64 候選完成實機驗收：無保存語言與已有保存語言時，手動安裝的第一個畫面可選三種語言；`/UPDATE /P /R` 不顯示語言視窗；連續啟動四次只保留一個 `fabDev` 與一個 `fabdev-agent`，既有視窗會取得焦點。此候選功能通過但版本仍誤標為 `0.1.19`，不得取代已發布的 `v0.1.19`。
- [x] Commit `9f505906731402f610f8ff731e602f5a24b44b3d` 的四個正式版本來源與 13 個 fabDev Cargo.lock 套件皆為 `0.1.20`；Windows x64 Run `33612756679` 成功產出 `fabDev_0.1.20_x64-setup.exe`。Installer Artifact ID `9839992623`，ZIP 為 49,361,493 bytes，GitHub Artifact ZIP SHA-256 為 `aee5cb3c4ad8544f2e4235f131dc2b32f397e243812929f55fd43c2178bbcf19`；Windows 解壓後的 Setup.exe 本體 SHA-256 為 `0ed14fd93c748adc6f5638ef03527375afbdc77a807202b03250e283538fb6c9`。
- [ ] Repository Owner 只確認 Setup.exe 檔案內容或安裝後 App 顯示版本為 `0.1.20`；因功能程式碼未再變更，沿用上述實機功能 Gate，不重跑相同人工流程。

#### Windows 更新體驗清單

- [x] 以已安裝 `0.1.11` 實際執行 `0.1.11 -> 0.1.12` App 內更新，驗證 `/UPDATE /P /R` 不再顯示舊版移除／重新安裝流程。
- [x] Windows 更新按鈕改為「重新啟動並更新」，避免「退出並開啟安裝程式」讓使用者誤以為仍需操作 Installer。
- [ ] 顯示完整更新階段：下載、驗證、停止服務、安裝及重新啟動。
- [ ] 新版首次重新啟動後顯示更新成功版本，並說明 Sites、Runtime、PHP 設定與使用者資料已保留。
- [x] Windows App 更新下載加入停止／取消操作，並保留既有分段下載能力；0.1.14 回歸測試通過，下一個 Stable 補做 VM UI 中途取消驗收。
- [ ] 加入更新失敗回復入口：重新執行安裝、開啟錯誤紀錄、下載上一個 Stable 版本及還原更新前設定快照。
- [ ] 規劃統一更新中心，集中顯示 App、PHP、Node.js 與 MariaDB 的已安裝版本、可用版本及更新狀態。
- [ ] 提供一鍵診斷報告，涵蓋 DNS、Port、Nginx、PHP、MariaDB、Runtime 與 App 更新紀錄，並遮罩敏感資料。
- [ ] 在實體 Windows x64 與 IIS／Herd 共存環境補做安裝、更新、衝突與長時間運行驗收。
- [ ] 正式散布需求成熟後加入 Windows Code Signing；目前 Unsigned Community Build 維持 SHA-256 驗證與 SmartScreen 說明。

### P2：選裝與跨平台

- [ ] 依 [`RUNTIME_ONLINE_UPDATE_SPEC.md`](RUNTIME_ONLINE_UPDATE_SPEC.md) 完成 Runtime Catalog v1 與 PHP 8.4.24 兩平台 Side-by-side 線上安裝；Windows x64 已由 `v0.1.12` 完成 PHP 7.4／8.2／8.4 的 CI、封裝、Publish、匿名 Feed 與 PHP 7.4／8.2 真實線上重裝，macOS 發布流程依目前範圍保留後續處理。
- [x] 單一穩定版 Node.js LTS 獨立選裝、顯示狀態及移除。
- [x] 完成 Windows x64 Node.js／MariaDB 線上安裝與升級的發布驗收；`v0.1.11` Catalog、Agent、Desktop、可重現產包、Windows CI、封裝版 App、Stable Publish 與匿名 Feed 均已完成。
- [ ] Node.js 多版本、全域版本、`.nvmrc`／`fabdev.yml` 與選用的專案感知 CLI shim。
- [x] macOS ARM64 MariaDB 選裝服務。
- [ ] Windows MariaDB 安裝版與 Portable 版的 Runtime、資料及升級策略。
- [x] Windows Platform Adapter 與 Unsigned Community NSIS 安裝包。
- [x] 在 Parallels Windows 11 ARM 的 x64 模擬層以乾淨資料基線驗證安裝 → UAC Helper／Hosts → `demo.test` → PHP 切換 → 完整移除；實體 Windows x64 仍待補測。
- [ ] 在 Parallels Windows 11 驗證 `fabdev-connect.exe` → UAC → 多 Site hosts → `http://site-one.test`／`http://site-two.test` → 並行載入 → 中斷清理。
- [ ] Developer ID、notarization 與 `SMAppService` Signed Distribution。

#### macOS 功能對齊執行進度（盤點、P0 與 P1 本機實作已完成）

- [x] 以兩平台最後共同發布的 `v0.1.3` 為基準，完成 `v0.1.4` 至 `v0.1.11` Windows-only App 更新、Runtime Catalog、Node.js、MariaDB 與 Release 流程差異盤點。
- [x] P0：完成跨平台 Runtime 與更新能力契約；新增 macOS ARM64 PHP 8.4.24、MariaDB 12.3.2、Node.js 20／24 Catalog 產生器與 Manifest 資產入口，Agent 依目前平台／架構篩選並接受尚無本平台套件的 Catalog，PHP／MariaDB／Node.js UI 不再以 Windows-only 條件隱藏線上 Runtime。
  - 驗證：`pnpm test`、`pnpm lint`、Desktop production build 與 `git diff --check` 通過；Desktop 68 tests、Release Manifest 10 tests、Runtime 21 tests、Agent 20 tests 及 macOS Helper 9 tests 均通過。
  - 當時邊界：P0 只完成共用契約、Catalog／Manifest 產生能力、Desktop 顯示與回歸測試；後續已完成 macOS 本機 Runtime／DMG 候選及跨平台 Draft workflow，但公開 Feed、乾淨機下載與發布驗收仍依下列 P1／P4 執行。
- [x] P1：完成 macOS ARM64 PHP 8.4.24 線上安裝／更新，加入公開 Runtime Catalog、Package、下載驗證、安裝健康檢查與失敗清理。
  - 隔離相容依賴、本機 Community Package、真實 Archive 健康檢查、Online Agent Protocol、下載中斷續傳、篡改 cache 拒絕、Desktop UI 並存安裝、重啟持久性、固定 53／80 Site 與 PHP 8.4 回應均通過；`v0.1.13` 公開 Catalog／Package、大小、SHA-256 與匿名 HTTP 200 已完成檔案級驗收。因安裝程序未變，依既定規則不重跑公開 Feed 的 Desktop 安裝流程。
- [x] P1：完成 macOS ARM64 MariaDB 12.3.2 線上安裝／升級；更新時自動暫停服務，並保留既有 Data、Config、Log、啟動偏好與 PHP MariaDB 自動連線切換。
  - 執行中安全暫停、偏好保留、新版重啟、連線重套、失敗回滾、96 個 Mach-O、初始化、SQL 查詢與停止清理均通過；`v0.1.13` 公開 Catalog／Package、大小、SHA-256 與匿名 HTTP 200 已完成檔案級驗收。因安裝程序未變，沿用 P4 的資料保留與 Socket 自動切換驗收。
- [x] P1：完成 macOS ARM64 Node.js 20／24 並存安裝、更新、移除、全域版本切換及動態 `node`／`npm`／`npx`／`corepack` terminal shim；不得修改或接管 Homebrew、nvm、Herd 或系統 Node.js。
  - 兩版官方 Archive 驗證、Community Package、Desktop 並存安裝、20 → 24 全域切換及可還原 `.zprofile`／`.zshrc` shim 均通過；`v0.1.13` 公開 Catalog／Package、大小、SHA-256 與匿名 HTTP 200 已完成檔案級驗收。因安裝程序未變，沿用 P4 的實際安裝與全域切換結果。
- [ ] P2：將 App Installer 的分段、並行、退避重試、跨 App 重啟續傳、大小／SHA-256 與進度統計抽成兩平台共用下載流程。
- [ ] P2：完成 macOS「重新啟動並更新」流程；安全停止服務與 Agent，重新驗證 DMG，覆蓋 App，失敗時回復，成功後自動重新啟動，並保留 Sites、Runtime、憑證、MariaDB 資料與使用者設定。
- [x] P3：恢復兩平台 Draft Release workflow；建置 macOS ARM64 DMG 與 PHP／MariaDB／Node.js Runtime Assets，產生同時包含 macOS 與 Windows 的 App Manifest、Runtime Catalog 及 SHA-256 契約，且永不自動 Publish。
  - `v0.1.13` workflow 已觸發；Windows x64 App 與六個 Runtime Jobs 通過，macOS Job 因 PHP 7.4 打包健康檢查相容問題停止。依不重新打包決定，使用該 Run 已建好的 Windows 產物及 P4 已驗證的 macOS 產物組成 30 檔 Draft，完成重新下載驗證後人工 Publish；workflow 本身仍沒有自動 Publish 路徑。
- [x] P4：使用未標 Tag 的 macOS ARM64 候選版完成乾淨安裝、`v0.1.3` 覆蓋更新、下載中斷續傳、PHP／MariaDB／Node.js、HTTPS、Start／Stop、Safe Quit、自動重新啟動及資料保留驗收。
  - `v0.1.12` DMG 乾淨安裝與 `v0.1.3 → 0.1.12` 覆蓋均通過；PHP 8.4.24、MariaDB 12.3.2、Node.js 20.20.2／24.20.0 由隔離 Desktop UI 實際安裝，PHP 中斷續傳從 28,704,768-byte `.part`／56% 恢復。
  - `demo.test` 的 Site ID、檔案、`php.ini`、PHP 8.4、HTTPS、CA／leaf、MariaDB 資料列及 Runtime 安裝狀態均跨 Quit／重開保留；HTTP 301、HTTPS 200、Start／Stop、Safe Quit 與無殘留 Port／PID／Socket 通過。
  - P4 實機發現並修正 macOS Runtime `.part` 無法續傳、Node `.zprofile` 被 `.zshrc` PATH 蓋過、MariaDB 只更新 active PHP FPM Socket 三項缺口；均已加入聚焦回歸測試並通過完整 test／lint。
- [x] P4：共同版本 `0.1.13`、Commit／Push／固定 Tag、重新打包、30 檔 Draft、全部 Asset 重新下載驗證、Publish、latest Stable 與公開 Feed 檔案級驗證均已完成；依安裝／更新程序未變的規則，不重跑兩平台生命週期與完整 Runtime／HTTPS 人工流程，macOS 功能對齊至此結案。
- P5：依使用者決定不執行 macOS Intel x86_64；本次以 macOS ARM64 發布完成為終點，不宣稱 macOS 雙架構支援。

### P3：正式服務產品線

- [ ] 未來另立 `fabDev Server` 產品；不得直接沿用 Desktop 本機開發模式，其 Control Plane、Data Plane、網路安全、備份、更新、監控及第一版驗收架構記錄於 `docs/FABDEV_ARCHITECTURE.md` 第 15 節，不納入目前單機、單人 fabDev Desktop 的實作範圍。

### fabDev Desktop 產品化驗收目標

- [ ] 可管理至少 100 個 Site，並同時啟用 20 個 Site，不得出現 UI、Registry 或服務狀態錯亂。
- [ ] 使用固定 ERP 測試 Fixture 驗證同時處理至少 50 個本機 HTTP 請求，過程不得出現請求錯誤或受管程序異常退出。
- [ ] Web Stack 與 MariaDB 連續運行 72 小時，不得出現程序遺失、持續性記憶體增長、Port、PID 或 Socket 殘留。
- [ ] Start All → Stop All → Start All、Quit → Relaunch 及 Agent Upgrade 等生命週期流程累計執行至少 500 次，不得殘留受管程序或破壞服務狀態。
- [ ] 強制終止 Nginx、PHP-FPM／PHP-CGI、MariaDB 或 Agent 後，必須能明確診斷並安全恢復，不得接管非 fabDev 程序。
- [ ] App、Agent、Helper 或 Runtime 更新失敗時，不得破壞既有 Site、Runtime、`php.ini`、MariaDB 設定或資料，並可回復至更新前的可用狀態。
- [ ] 在乾淨 macOS 與實體 Windows x64，以及安裝 Herd、Valet 或 IIS 的共存環境，完成安裝、啟動、PHP 切換、更新、衝突處理及完整移除驗證。
