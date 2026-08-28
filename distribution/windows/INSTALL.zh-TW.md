# fabDev Community for Windows 安裝說明

fabDev Windows x64 Community Build 是未簽署版本，內含 Nginx 1.30.4、PHP 7.4.33 與 PHP 8.2.33。安裝程式不會使用或覆蓋電腦上既有的 PHP、Nginx、Herd 或 MariaDB。

## 安裝

1. 執行 `fabDev_*_x64-setup.exe`。Windows SmartScreen 若顯示警告，請確認檔名及發佈來源後選擇「其他資訊」→「仍要執行」。
2. 啟動 fabDev。首次安裝 Runtime 可能需要一些時間；空白環境會自動建立唯一的 `demo.test`。
3. 新增或移除 `.test` Site 時，接受 Windows UAC 提示，讓 fabDev 只更新系統 `hosts` 檔中的受管區塊。
4. 確認主控台的 Nginx、PHP 與網域服務皆為 Running，再開啟例如 `http://demo.test`。

Sites 畫面可對每個 Site 啟用 HTTPS。第一次啟用時接受 UAC 提示，fabDev Windows Helper 只會驗證並信任 `%LOCALAPPDATA%\FabDev\config\tls\ca.crt`，Site 私鑰不會離開 fabDev 資料目錄；啟用後可直接開啟例如 `https://demo.test`。

PHP 7.4 與 PHP 8.2 需要 Microsoft Visual C++ 2015–2022 Redistributable (x64)。若服務記錄顯示缺少 DLL，請先從 Microsoft 官方網站安裝後重啟 fabDev。

## 移除

從 Windows「已安裝的應用程式」移除 fabDev。解除安裝時接受 UAC 提示，以移除 Windows `hosts` 中的 fabDev 受管區塊與 fabDev 本機 CA 信任。解除安裝程式會保留 `%LOCALAPPDATA%\FabDev` 內的 Site Registry、PHP 設定、TLS 檔案與 Runtime，避免更新或重裝時遺失。若要連同資料移除，請先在 fabDev 停止所有服務，再手動備份及刪除該目錄。
