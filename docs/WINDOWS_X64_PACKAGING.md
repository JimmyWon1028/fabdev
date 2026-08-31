# Windows x64 打包與驗收

這份文件整理 fabDev Windows x64 Community 安裝包的建置、安裝、驗收與除錯經驗。目標不是只產生 `.exe`，而是確認安裝後能在 Windows 11 正常啟動 Nginx、PHP 與 `demo.test`，停止後也不殘留程序或 Port。

## 產物範圍

Windows Community 版使用 Tauri NSIS 建立 Current User 單檔安裝程式，目標架構為 `x86_64-pc-windows-msvc`。安裝包包含：

- fabDev Desktop、Agent 與 Windows Helper。
- Nginx 1.30.4。
- PHP 7.4.33 NTS x64 與 PHP 8.2.33 NTS x64。
- 全新資料目錄首次啟動時唯一的 `demo.test` 範例站台。
- 不建立預設 Proxy，也不封裝建置電腦的 Site、SQLite 或其他使用者資料。

標準輸出位於：

```text
target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe
```

對外整理後的安裝包可放在 `artifacts/windows-x64/`，但 `target/`、`distribution/windows/runtime/`、`artifacts/` 與 Runtime binary 都不應提交進 Git。

## 建置環境

建議使用原生 Windows x64 或 GitHub Actions `windows-latest`，並準備：

- Node.js 24。
- pnpm 11.22.0。
- Rust stable 與 `x86_64-pc-windows-msvc` target。
- Visual Studio Build Tools、MSVC x64 toolchain 與 Windows SDK。
- 至少約 10 GiB 可用空間，供依賴、Rust target、Runtime 與 NSIS 暫存檔使用。

Parallels 的 Windows 11 ARM VM 可以執行與驗收 x64 App，但在 ARM VM 內交叉建置 x64 會受到 MSVC toolchain 與模擬層影響。正式可重現流程仍以 `.github/workflows/windows-x64.yml` 的 `windows-latest` x64 runner 為準。

## 標準建置流程

在 PowerShell 的專案根目錄執行：

```powershell
pnpm install --frozen-lockfile
cargo fmt --all -- --check
pnpm -r --if-present test
node scripts/build-desktop-bundle-assets.mjs
./scripts/prepare-windows-runtimes.ps1
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
cargo build -p fabdev-connect --release --target x86_64-pc-windows-msvc
pnpm --filter @fabdev/desktop exec tauri build --target x86_64-pc-windows-msvc --bundles nsis --config src-tauri/tauri.windows.conf.json
```

`scripts/prepare-windows-runtimes.ps1` 會下載並核對固定 SHA-256，然後產生：

```text
distribution/windows/runtime/
  manifest.json
  nginx/current/
  php/7.4.33/
  php/8.2.33/
```

下載或雜湊失敗時必須停止打包，不可改成略過驗證。

## 選裝 Runtime Package

PHP 8.4、MariaDB 與 Node.js 不強制安裝在基礎 NSIS 內，使用以下指令建立三組 Windows x64 選裝套件：

```bash
./scripts/build-windows-runtime-packages.sh
```

輸出位於 `artifacts/windows-x64/runtimes/`，每個 Runtime 都包含配對的 Release JSON 與 `.tar.gz`：

```text
php-8.4.24-windows-x64.{json,tar.gz}
mariadb-12.3.2-windows-x64.{json,tar.gz}
node-20.20.2-windows-x64.{json,tar.gz}
node-24.20.0-windows-x64.{json,tar.gz}
```

建置流程固定並驗證官方 Windows Archive SHA-256；MariaDB 與 Node.js 另外驗證上游 PGP 簽章及允許的完整 Key Fingerprint。安裝時 Agent 會再次核對 Release 的平台、架構、大小與 Runtime Package SHA-256。

在 Windows UI 分別由 PHP 設定、MariaDB 與 Node.js 頁面選擇同一 Runtime 的 JSON 與 `.tar.gz`。PHP 8.4、MariaDB、Node.js 仍維持選裝，不會在更新基礎 NSIS 時覆蓋或自動移除。

## Sidecar 必須與本次程式碼一致

`scripts/build-desktop-bundle-assets.mjs` 除了建立前端資產，也會建立並複製本次版本的 Agent 與 Helper 到 Tauri sidecar 位置：

```text
apps/desktop/src-tauri/binaries/fabdev-agent-x86_64-pc-windows-msvc.exe
apps/desktop/src-tauri/binaries/fabdev-windows-helper-x86_64-pc-windows-msvc.exe
```

曾發生 Desktop 使用新程式碼、安裝包卻仍封裝舊 Agent 的情況。打包前後應核對來源、sidecar 與實際安裝檔案的時間及 SHA-256：

```powershell
Get-FileHash -Algorithm SHA256 .\target\x86_64-pc-windows-msvc\release\fabdev-agent.exe
Get-FileHash -Algorithm SHA256 .\apps\desktop\src-tauri\binaries\fabdev-agent-x86_64-pc-windows-msvc.exe
Get-FileHash -Algorithm SHA256 "$env:LOCALAPPDATA\FabDev\fabdev-agent.exe"
```

若安裝位置因 Tauri 版本而不同，先由工作管理員或安裝目錄確認實際路徑，再比較，不要只看建置目錄。

## 從 macOS 同步至 Parallels Windows

建議在 Windows 使用獨立的建置目錄，不要直接從共享資料夾編譯。若使用 `robocopy /MIR` 同步來源，至少排除：

```text
.git
node_modules
target
.build
artifacts
dist
.pnpm-store
apps/desktop/src-tauri/binaries
distribution
```

`/MIR` 會刪除目的端不存在於來源端的檔案。若先產生 `distribution/windows/runtime/`，再用 `/MIR` 同步且未排除 `distribution`，已下載的 Runtime 會被刪除。同步後應重新執行 Runtime 準備與 sidecar 建置。

## 安裝與更新測試

更新測試不需要先移除舊版。先從 fabDev 停止所有服務並關閉 Desktop／Agent，再覆蓋安裝，才能同時驗證資料保留與更新流程。

App 內更新會先驗證完整 Setup、停止服務並退出 Desktop，再由隱藏的 PowerShell 等待舊程序完全結束，最後以以下參數啟動 Current User NSIS：

```powershell
Start-Process -FilePath .\fabDev_x64-setup.exe -ArgumentList "/UPDATE", "/P", "/R"
```

`/UPDATE` 讓 Tauri NSIS 直接覆蓋既有安裝而不執行舊版移除流程，`/P` 使用被動模式，`/R` 完成後重新啟動 App。安裝前後需比對 Site Registry、Runtime 目錄、MariaDB data／config／log 與 Proxy 設定，確認都保持不變。

Windows x64 App 與 Runtime 從 GitHub Releases 下載時，使用 8 MiB 分段、最多 4 路並行、退避重試與 `.resume` 續傳。驗收需包含中斷後重試、設定頁速度／剩餘時間、最終整包大小／SHA-256，以及完成或明確取消後不殘留分段檔。

遠端或自動化測試請用 PowerShell 等待 NSIS 完成：

```powershell
Start-Process -FilePath .\fabDev_0.1.0_x64-setup.exe -ArgumentList "/S" -Wait
```

直接執行 `setup.exe /S` 在部分遠端環境可能先回傳，但背景安裝程序尚未完成。不要立刻啟動 fabDev；應先確認安裝目錄存在，並核對已安裝 Agent 的雜湊或修改時間。

## 不顯示多餘終端視窗

Release Desktop 應使用 Windows GUI subsystem：

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
```

Agent 啟動 Nginx、PHP 或其他背景程序時，Windows release 應使用 `CREATE_NO_WINDOW`。這兩層要同時處理：Desktop subsystem 只避免 fabDev 自己的 Console，背景程序的建立旗標則避免啟動 Windows Terminal 或黑色命令視窗。

可用 Visual Studio `dumpbin` 檢查：

```powershell
dumpbin /headers .\fabDev.exe | Select-String subsystem
```

預期為 `Windows GUI`，啟動 fabDev 時也不應出現 `WindowsTerminal.exe`。

## Windows 路徑顯示與設定輸出

Rust 在 Windows 呼叫 `canonicalize()` 後可能得到 extended-length path，例如：

```text
\\?\C:\Users\dev\Sites
```

不同用途需分開處理：

- UI 顯示：移除本機磁碟的 `\\?\` 前綴，顯示 `C:\Users\dev\Sites`。
- UNC 路徑：保留正常 Windows UNC 語意，不可把網路路徑誤改成本機磁碟。
- Nginx 設定：輸出 `C:/Users/dev/...`，避免反斜線被 Nginx 當成跳脫字元。
- 內部資料：保留可正確存取檔案的 Path，不要為了畫面顯示而直接覆寫資料庫原值。

至少要檢查 Site Home 設定、Site 卡片、編輯／移除確認、MariaDB 路徑與「在檔案總管顯示」等所有會呈現路徑的畫面。

## NSIS 不會保留空目錄

壓縮及安裝流程通常不會保留空的 Nginx `logs`、`temp` 與其子目錄。若 Agent 假設這些目錄已在壓縮包內，首次啟動會看到類似錯誤：

```text
could not open error log file
CreateDirectory() ".../nginx/current/temp/client_body_temp" failed
```

因此 Agent 在執行 `nginx -t` 之前必須主動建立 Nginx 需要的目錄，包括：

```text
logs/
temp/client_body_temp/
temp/proxy_temp/
temp/fastcgi_temp/
temp/uwsgi_temp/
temp/scgi_temp/
```

不可依賴 Git、ZIP 或 NSIS 保存空資料夾。

## MariaDB Windows 初始化與本機 TLS

Windows 官方 ZIP 內的 `mariadb-install-db.exe` 與 Unix 的 `mariadb-install-db` 參數不同。Windows 初始化只傳入：

```text
--datadir=<fabDev managed data directory>
--silent
```

不可傳入 `--basedir`、`--no-defaults`、`--auth-root-authentication-method`、`--skip-name-resolve` 或 `--skip-test-db`；這些 Unix 參數會使 Windows 初始化立即失敗。

MariaDB 12.3 Windows ZIP 初始化後不會自動產生 `private_key.pem`。fabDev Managed MariaDB 只綁定 `127.0.0.1`，因此 Windows 管理設定明確使用 `skip-ssl`，避免資料庫因找不到私鑰而中止。這只影響本機 PHP 到資料庫的 loopback 連線，不影響 Nginx 對 Site 提供的 HTTPS。

## 清理舊版殘留程序

舊 Agent 異常結束後可能留下 Nginx master／worker。新 Agent 啟動時若只依賴 PID 檔，會因舊程序占用 Port 80／443 而失敗。

Windows 清理必須以程序可執行檔的完整路徑為條件，只處理位於 fabDev Runtime 目錄下的 Nginx／PHP。不可使用全域 `taskkill /IM nginx.exe`，以免終止使用者的其他 Nginx、Herd、Docker 或開發環境。

## 安裝包靜態檢查

建置成功後先保存檔案大小與 SHA-256，再用 7-Zip 測試及列出內容：

```powershell
Get-Item .\fabDev_0.1.0_x64-setup.exe | Select-Object Name,Length,LastWriteTime
Get-FileHash -Algorithm SHA256 .\fabDev_0.1.0_x64-setup.exe
7z t .\fabDev_0.1.0_x64-setup.exe
7z l .\fabDev_0.1.0_x64-setup.exe
```

單檔安裝包應實際包含 Desktop、Agent、Helper、兩個 PHP Runtime、Nginx、Runtime manifest、demo 與 uninstaller。不要只因檔名是 `Setup.exe` 就認定內容完整；體積異常小的安裝程式可能只是需要額外套件的 bootstrapper。

## Windows 11 實機驗收清單

建議在已安裝舊版的 VM 做覆蓋更新，再用另一個全新資料目錄或乾淨 VM 驗證首次安裝。既有資料測試不能證明預設 Registry 正確。

1. 安裝程式完成，fabDev 保持安裝，不在測試後移除。
2. 啟動時不出現黑色 Console 或 Windows Terminal。
3. Desktop 與 Agent Protocol 相容，首頁不顯示「開發服務尚未就緒」。
4. 全新資料只有一個 `demo.test`，沒有預設 Proxy。
5. PHP 7.4.33 與 8.2.33 均已安裝且可選用。
6. 啟動服務前後，Nginx `logs` 與 `temp` 所需目錄都存在。
7. UI 使用 Windows 習慣顯示路徑，沒有 `\\?\C:\...`。
8. `curl http://demo.test` 回傳 HTTP 200，頁面顯示預期 PHP 版本。
9. 停止全部服務後，沒有 fabDev 管理的 Nginx／PHP 殘留，Port 80／443 已釋放。
10. 再次啟動仍能回傳 HTTP 200，確認 Start → Stop → Start 可重複。

## 2026-08-28 Parallels 驗收紀錄

本次 Windows 11 VM 驗收結果：

- NSIS x64 安裝包大小為 48,324,392 bytes。
- SHA-256：`dfdc0b14146336ee1ae0feef5eb17c329655dedb93dfbc06a7c6358cb893d623`。
- 7-Zip 測試通過，安裝包可列出 214 個檔案。
- Desktop PE subsystem 為 `Windows GUI`；fabDev 啟動時沒有產生額外的 Windows Terminal 視窗。
- 前端測試共 10 個檔案、50 個測試通過。
- Windows x64 `fabdev-services` 35 個測試與 `fabdev-agent` 12 個測試全數通過。
- 打包 sidecar 與覆蓋安裝後 Agent SHA-256 一致：`c24e7d1cb48a6f93ee57288b25511b78fca9fa6cf7d2a5ebb5b8319eaf48dca9`。
- `demo.test` 回傳 HTTP 200，PHP 版本為 8.2.33。
- PHP 8.4.24、Node.js 20.20.2／24.20.0 與 MariaDB 12.3.2 選裝 Runtime 均成功安裝，覆蓋安裝基礎 NSIS 後仍完整保留。
- MariaDB 狀態為 running，只監聽 `127.0.0.1:3306`，並以 root TCP 實際查詢到 `12.3.2-MariaDB`。
- PHP 8.4.24 執行檔回報 NTS Visual C++ 2022 x64；Node.js 20 與 24 必須各自回報 Catalog 固定版本，並驗證切換全域後 `node`／`npm` shim 立即跟隨。
- 選裝 Runtime Package SHA-256：PHP 8.4 `d54f692f1126c05ea3710b84b78ac9a002ca030c7a62bba675e73da2f9772b14`、MariaDB `482b38fdaf9434393f051ad7669063193829625de76dd43878d52fb9001863ce`、Node.js `afb919956c008e8de3ac39454ef9fa06cdc041106ea209ee4794a4ea7a206451`。
- Windows Site 列的 PHP Runtime 下拉框固定為 104px，使用跨平台一致的雙向箭頭，版面與 macOS 對齊。
- Stop 後 Nginx／PHP 與 Port 80／443 均完成清理。
- 再次 Start 後 `demo.test` 仍正常。
- 測試完成後保留 fabDev 安裝與服務，未執行移除。

當次整理後的檔案位置為：

```text
artifacts/windows-x64/FabDev_0.1.0_x64-setup.exe
```

此紀錄證明該產物在本次 Parallels Windows 11 VM 可安裝與使用；正式發佈仍應補做乾淨的實體 Windows x64、IIS／Herd 共存及簽章／下載鏈驗證。

## 2026-08-29 `v0.1.1` Draft 驗收紀錄

本次直接使用從 GitHub Draft 重新下載並核對過的兩個 Windows x64 產物：

- Setup 大小 48,332,278 bytes，SHA-256 `5bd0c91c8885e855c03865aba9909b02c26ee2e73503450c1af27fa3fd310319`。
- Connect 大小 749,568 bytes，SHA-256 `2082d724e809a04111a78a74fe7f0aadd021218569a4589a8c6b7b9fd0a4710f`。

Windows 11 ARM VM 以 x64 模擬執行 Setup。先從既有 `0.1.0` 覆蓋更新，Installer exit code 為 0、登錄版本為 `0.1.1`，SQLite 雜湊保持不變，原 Site ID、Site Home、PHP 8.2 與空白 Proxy 均保留。Protocol 32、Nginx 1.30.4、PHP 7.4.33／8.2.33、Start → Stop → Start、兩個 PHP 版本的 `demo.test` HTTP 200，以及 Stop 後程序與 80／443 清理全部通過。

解除安裝 exit code 為 0，App、登錄、受管 Hosts、Nginx／PHP 與 listener 均移除；`data` 與 Connect 設定依保留政策留在安裝目錄。為驗證首次安裝，保留目錄整包移至 VM 內具名可復原備份，確認安裝前沒有 fabDev 資料後再安裝同一 Setup。首次啟動只建立 `demo.test`，Proxy 清單為空，PHP 7.4.33／8.2.33 齊全，HTTP 200 通過。

Connect 從 Parallels Shared Folder 啟動後，成功把自己轉存為本機 `fabdev-connect-runtime.exe`，本機 Runtime SHA-256 與 Draft Asset 相同，並以 `runas --elevated` 執行。當本機 fabDev Helper 已管理同名 `demo.test` 時，Connect 依設計拒絕覆寫該 Hosts 紀錄。多 Site 實際轉送與中斷清理仍列在 P2，不是本次 P0 NSIS 發布阻擋條件。

此結果不能取代乾淨實體 Windows x64、SmartScreen 簽章信譽、IIS／Herd 共存與企業安全軟體環境驗證。

## 2026-08-31 `0.1.11` 本機原地更新驗收

Windows x64 未簽章本機候選 Setup 為 49,295,735 bytes，SHA-256 `8c6bffb7099cfe1e8730eaa34012a973b402551e17f268d1421ab1311c5dc1c7`。7-Zip 完整性、NSIS 3 Unicode、File／Product Version `0.1.11`、Desktop、Agent、Helper、PHP 7.4.33／8.2.33 與 Nginx 封裝內容均通過靜態檢查。

Parallels Windows 11 ARM 的 x64 相容環境由既有 `0.1.10` 使用 `/UPDATE /P /R` 原地覆蓋；Installer exit code 為 0，沒有先執行舊版移除程序，完成後 App 自動重新啟動。登錄、Agent 與 Protocol 分別更新為 `0.1.11`、`0.1.11` 與 36，安裝後 Agent／Helper SHA-256 與候選 sidecar 一致。

更新前後唯一 `demo.test` 的 Site ID、路徑、Site Home、PHP 8.2 與空白 Proxy 均相同；MariaDB `my.ini` 與 Connect 設定 SHA-256 保持不變。`demo.test` 回傳 HTTP 200／PHP 8.2.33，Stop 後 Nginx／PHP 與 80／443 全部清理，重新 Start 後再次回傳 HTTP 200。

Tag `v0.1.11` 的 GitHub Actions Windows x64 與 Draft Release Jobs 全數通過。16 個 Draft Assets 共 263 MiB，重新下載後通過 `SHA256SUMS`、六份個別 checksum、兩份逐位元一致的 App Manifest、Runtime Catalog sequence 5 與四個 Runtime Archive gzip 完整性；CI Setup 為 49,305,659 bytes，SHA-256 `3c12f1b24ffbd7675bc325b87c41f20459924a1ba14e6e3f58e9a41cbfb0c3ee`，NSIS 3 Unicode 內含 214 個檔案。

Publish 後以未帶 GitHub Token 的公開 URL 重新取得 Release 頁、Stable Manifest、Runtime Catalog 與完整 Setup，均為 HTTP 200，內容與 Draft 驗收版本一致。兩個實際 8 MiB Range 均回傳 206；Windows VM 再以 `v0.1.11` 的 `fabdev-updater` 讀取公開 Feed，正確判定 `0.1.10 -> 0.1.11` 可更新與 `0.1.11` 無新版，並完成四路分段下載、大小、整包 SHA-256 及 pending installer 驗證。

## 常見問題快速對照

| 症狀 | 優先檢查 |
| --- | --- |
| Nginx validation 找不到 `logs/error.log` | Agent 是否在 `nginx -t` 前建立 `logs` 與 `temp` |
| Port 80／443 已占用 | 舊版 fabDev Nginx 是否殘留；只按完整 exe 路徑清理 |
| UI 顯示 `\\?\C:\...` | 顯示層是否正規化 Windows extended-length path |
| 啟動時出現黑色視窗 | Desktop GUI subsystem 與子程序 `CREATE_NO_WINDOW` 是否同時生效 |
| MariaDB 初始化回報 unknown option | Windows 是否誤用了 Unix `mariadb-install-db` 參數 |
| MariaDB 啟動回報 Unable to get private key | Windows Managed 設定是否包含 loopback 專用的 `skip-ssl` |
| 修正已打包但行為仍舊 | 安裝包是否封裝舊 Agent sidecar；比對 SHA-256 |
| 靜默安裝後立刻啟動失敗 | 是否用 `Start-Process -Wait` 等待 NSIS 完成 |
| App 更新仍出現移除／安裝選擇頁 | 啟動參數是否同時包含 `/UPDATE /P /R`，以及舊 Desktop PID 是否已完全退出 |
| GitHub 下載中斷後從 0 開始 | Pending 目錄的 `.resume` 分段是否仍存在，伺服器是否正確回傳 `206 Content-Range` |
| Runtime 突然消失 | `robocopy /MIR` 是否刪除 `distribution/windows/runtime` |

## 發佈邊界

目前公開版本是 Unsigned Community Build，下載來源與 checksum 固定使用 GitHub Releases；未購買 Windows code signing 憑證，因此仍會有 SmartScreen 警告。VM 驗收不能完全取代乾淨的實體 Windows x64 測試，也不能省略與 IIS、Herd、其他 Nginx／PHP 共存的測試。

任何安裝包都不得包含 Token、私鑰、真實 Site、使用者 SQLite、絕對使用者路徑或建置環境資料。
