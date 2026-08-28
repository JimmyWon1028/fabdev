# fabDev Community 安裝指南

fabDev Community 是未使用 Apple Developer ID 簽署的 macOS Apple Silicon 開發版本。安裝過程透明、需要一次管理員授權；App 內建 DNS、Nginx、PHP 7.4 與 PHP 8.2，Helper 來自同一個 DMG。

## 系統需求

- Apple Silicon Mac（arm64）
- macOS 13 或更新版本
- Port 53、80、443 未被 Herd、Valet、Apache 或其他服務占用

## 安裝

1. 完全 Quit Herd 或其他本機 Web 開發工具。
2. 打開 fabDev Community DMG。
3. 雙擊 `Install-fabDev.command`。
4. 確認安裝清單，輸入 macOS 管理員密碼。
5. 安裝完成後會開啟 `/Applications/fabDev.app`。
6. 如果 macOS 阻擋 App，請在 Finder 對 fabDev 按右鍵，選擇「打開」並確認。
7. fabDev 開啟後會自動啟動開發服務；主控台顯示服務均為「運行中」後，開啟 [http://demo.test](http://demo.test)。

App 首次啟動會自動補齊 PHP 7.4.33、PHP 8.2.33、Nginx 1.30.4 與 dnsmasq 2.93，並以 PHP 8.2 建立唯一的初始 Site `demo.test`。PHP 8.4 與 MariaDB 12.3.2 是選裝 Runtime，需另外選擇對應的 Community Package 安裝；MariaDB 不會跟隨 Web 服務自動啟動，需在主控台使用獨立按鈕啟動或停止。若 `127.0.0.1:3306` 已被其他 MariaDB 使用，fabDev 會保留外部服務並拒絕啟動自己的 MariaDB。

DMG 內建 Runtime 使用 `*-macos-arm64-community` 來源，封裝進 App 前會驗證大小與 SHA-256。PHP 8.4 與 MariaDB 的獨立 Community Package 仍附描述檔、大小與 SHA-256；`community-ad-hoc` 表示其中的 macOS binary 採 ad-hoc code signing。

## 安全與完整性

此版本沒有 Apple Developer ID 與 notarization。下載後應將 DMG 的 SHA-256 與發佈頁公布值比對：

```bash
shasum -a 256 fabDev-Community-*.dmg
```

DMG 內的安裝程式也會在變更系統前驗證 `SHA256SUMS`。System Helper 只允許固定的 `53→53535`、`80→8080`、`443→8443`，不執行 PHP、Runtime binary、憑證操作、自訂 Port、路徑或任意命令。首次替 Site 啟用 HTTPS 時會把 fabDev CA 加入目前使用者的 Login Keychain；憑證與私鑰保存在 fabDev Application Support，Community 移除程序會撤銷對應的使用者信任。

## 更新

從 menu bar 選擇 `Quit fabDev`；fabDev 會先停止 Web 服務、MariaDB 與 Agent，確認沒有受管程序後才退出。接著掛載新版 DMG，再執行 `Install-fabDev.command`。安裝程式會保留 Sites、`php.ini` 與其他使用者資料；已安裝的 Runtime 不會重複覆蓋。

## 移除

在 DMG 內雙擊 `Uninstall-fabDev.command`。程序會停止服務、移除 System Helper，並把 App 移到垃圾桶。最後可選擇保留資料，或將以下內容一併移到垃圾桶：

- 既有的 Application Support 資料目錄
- 既有的 Community Demo 目錄

## 問題排除

- Port 衝突：先 Quit Herd、Valet、Apache 或其他 DNS／Web Server，再重新安裝。
- Helper 記錄：`/var/log/fabdev-system-helper.log`
- App 無法啟動：到「系統設定 → 隱私權與安全性」確認允許 fabDev，或在 Finder 右鍵選擇「打開」。
- Chrome 顯示 `ERR_BLOCKED_BY_CLIENT`：以無痕視窗測試，或停用會封鎖本機 `.test` 網域的擴充功能。
