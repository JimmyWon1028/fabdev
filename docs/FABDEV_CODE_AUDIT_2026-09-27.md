# fabDev 程式碼檢查報告

日期：2026-09-27（Asia/Taipei）
檢查基準：`main`，HEAD `c76be5ef5c7269de480bc75efc07417c33624bd7`，App `0.1.28`，Agent Protocol `40`。
交付性質：初次問題報告與後續核准修正紀錄。初次檢查未修改產品程式碼；後續修正狀態見下節。

目前狀態：F01～F09 的原始碼修正均已完成。Windows 原生執行與 Name 的 macOS 輸入效果仍待驗證，尚未重新打包或發布。

## 後續核准修正：F01～F04

2026-09-27 Repository Owner 核准先修前四項，並要求保留現有功能。四項原始碼修正已完成；F05～F09 未納入第一批，第二批狀態見下節。下方原始問題與證據保留為修正前紀錄。

| 項目 | 修正內容 | 驗證 |
| --- | --- | --- |
| F01 | PHP 關閉時，Nginx 拒絕 `.php`、大小寫變體與其 PATH_INFO 路徑 | HTTP／HTTPS 實際回傳 403；HTML、JS、404 維持正常 |
| F02 | HTTP／HTTPS 的隱藏路徑規則移至 PHP handler 之前 | 隱藏 PHP 檔及隱藏目錄中的 PHP 均為 403；一般 PHP 與前端路由為 200 |
| F03 | Site Home 同步成功後，依 Site ID 更新分享清單與網域；最後一個分享移除時關閉 listener | 移除、同網域重建、切換 Home 保留 linked 分享、舊網域拒絕與失敗回復測試通過 |
| F04 | 使用 Windows 原生 PEM 解碼、X.509 context 與 `CERT_SHA1_HASH_PROP_ID`，取得 DER 憑證指紋 | Windows MSVC 目標含測試程式的 check／Clippy 通過；Windows 實機執行待驗 |

F04 沿用現有固定 CA 路徑驗證、`certutil -user -store/-addstore/-delstore` 與 UAC 操作流程；未修改 Installer hook。新增測試核對固定公用測試憑證的 thumbprint，並涵蓋 PEM 換行差異、空檔、無效憑證與缺少檔案。憑證 context 僅在記憶體建立，不會因此加入憑證存放區。[Microsoft context 文件](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-certcreatecertificatecontext)、[憑證屬性文件](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-certgetcertificatecontextproperty)

本次只修改四個產品／測試檔案：`crates/sites/src/lib.rs`、`crates/agent/src/main.rs`、`helpers/windows/Cargo.toml`、`helpers/windows/src/windows.rs`。沒有改 Protocol、資料格式、UI、版本、Runtime Catalog 或發布設定；原有未提交修改保留。

修正後驗證：

- `pnpm test`：Desktop 140、Release 規則 20、Rust 306、macOS Helper 10，共 476 項通過；7 項既有 ignored。
- `pnpm lint`、`git diff --check` 通過。
- Windows Helper `check --all-targets --target x86_64-pc-windows-msvc` 與同目標 Clippy `-D warnings` 通過。這只包含 Windows 測試編譯，沒有執行 Windows 專屬測試。
- 真實 Nginx／PHP-FPM 隔離 Start → HTTP／PHP／HTTPS → Stop：34 個請求案例通過，包含靜態資源、一般 PHP、前端路由、隱藏路徑拒絕、PHP 原始碼拒絕及 HTTP → HTTPS 301；兩個主程序 exit code 均為 0，三個測試 Port 可重新綁定，PID 檔已移除且沒有測試 Socket 殘留。測試使用獨立自簽憑證，未變更系統信任；不將此宣稱為真實 CA trust 驗收。
- Windows 實際信任、重複信任、移除與其他 CA 保留，仍需後續 Windows 驗證；本次未執行 Windows CI、實機流程或修改 Root store。
- 修改僅在原始碼；未重新打包、部署、替換 Helper、提交或推送。既有安裝版本與已產生的 Site 設定未被本次測試更新。

修正後紀錄：[完整測試](/private/tmp/fabdev-priority4-test.log)、[lint](/private/tmp/fabdev-priority4-lint.log)、[Windows check](/private/tmp/fabdev-priority4-windows-check.log)、[Windows Clippy](/private/tmp/fabdev-priority4-windows-clippy.log)、[34 項服務實測](/private/tmp/fabdev-priority4-nginx-results.json)。

## 第二批核准修正：F05、F07、F09 與 Name 輸入

Repository Owner 核准繼續修正後，另要求優先處理 Add Site 的 Name 首字自動大寫。第二批完成下列原始碼修改；當時未納入的 F06、F08，已於第三批處理。

| 項目 | 修正內容 | 驗證範圍 |
| --- | --- | --- |
| Name 輸入 | Name 保留 `autocapitalize="none"`，補上 `autocorrect="off"`、`spellcheck="false"`，停用文字更正；未加入強制大小寫轉換 | 前端測試與型別檢查通過；macOS 原生輸入行為仍待確認 |
| F05 | Store 按 operation ID 保留各 Runtime 工作並持續輪詢；刷新 Catalog 不清除進度。PHP、Node.js、MariaDB 共用追蹤；取消依所選 package 操作，延遲回應不能覆蓋已取消狀態 | 7 項新增回歸涵蓋無頁面持續下載、多 Runtime、輪詢合併、取消競態、斷線後刷新重試、下載失敗重試、package 身分核對與保留已驗證下載後安裝 |
| F07 | Windows 以 fabDev PID 檔、存活程序的 Runtime 執行檔路徑與已保存 Port 的 TCP readiness 恢復 MariaDB；狀態查詢、重複啟動、設定修改與改密碼共用恢復判定 | 可攜測試驗證有效 PID、自訂 Port、外部程序、無效／已失效 PID 與未就緒 Port。另新增 Windows 原生隔離程序測試，涵蓋重新接回、保持啟動偏好與停止清理；原生測試待 Windows CI 執行 |
| F09 | Windows 候選及 Draft workflow 在 Installer 建置前執行 Rust workspace tests，補 `apps/connect/**`、`helpers/windows/**` 觸發路徑 | Release 規則 21 項通過；既有 Unix Socket 測試限制於 Unix，跨平台 Nginx fixture 改用絕對暫存路徑，PHP 8.5 測試分別核對兩平台 OPcache 模板 |

F05 的操作保留範圍是同一次 Desktop 執行期間的切頁與刷新，不包含 App／Agent 重啟後恢復下載。下載完成時，已卸載頁面不會繼續彈出安裝確認；返回頁面後仍由使用者明確啟動安裝。Agent Protocol、資料格式、既有 Runtime 安裝與服務啟停契約保持不變。

本批驗證：

- `pnpm test` 通過：Desktop 146、Release 規則 21、Rust 307、macOS Helper 10，共 484 項，既有 7 項 ignored。其後增加的第 7 項 Runtime 安裝回歸，已連同該組全部 7 項及前端型別檢查通過；目前 Desktop 共 147 項。
- `pnpm lint`、`git diff --check` 通過。最後調整的 PHP 8.5 平台模板測試另行執行通過。
- Services、Sites、Windows Helper 的 `check --all-targets --target x86_64-pc-windows-gnu --offline` 通過，包含新增 Windows 測試的編譯檢查；Services 仍有 5 項既有 Windows 編譯警告。
- Windows MSVC Services 檢查受本機缺少 Windows C 標頭限制，`libsqlite3-sys` 編譯回報 `stdlib.h file not found`；不宣稱已通過 MSVC 或 Windows 原生執行。尚未觸發 Windows CI、建立候選或執行 Windows 實機流程。
- 原有主題、README 與前一批修正保留；未進版、提交、推送、重新打包、替換 Helper 或修改已安裝的服務。

本批紀錄：[完整測試](/private/tmp/fabdev-followup-test.log)、[lint](/private/tmp/fabdev-followup-lint.log)、[Release 規則](/private/tmp/fabdev-followup-release-test.log)、[Windows GNU check](/private/tmp/fabdev-followup-windows-check.log)、[MSVC 環境限制](/private/tmp/fabdev-followup-windows-msvc-check.log)。

## 第三批核准修正：F08、F06

Repository Owner 以「OK go」核准處理剩餘兩項。這批只新增修改 Proxy、Services 與本報告，保留前兩批及原有未提交內容。

### F08：Proxy 探測等待不再阻塞接收與停止

[Proxy 主迴圈](/Users/jimmywon/ai/fabdev/crates/proxy/src/lib.rs:846) 將健康檢查保存為單一持續的非同步 future，與接收連線、停止通知及已完成連線清理一起輪詢。探測依序執行，不會重疊；停止時先取消探測，再沿用既有的連線 drain 流程。15 秒檢查間隔、5 秒探測逾時、健康狀態門檻、HTTP client 恢復，以及各 Connection 的 upstream response timeout 都保持不變。這使用 Tokio 支援的持續 future 輪詢方式。[Tokio select 文件](https://docs.rs/tokio/latest/tokio/macro.select.html)

新增兩項可重現的隔離測試，刻意讓探測永久等待：修正前 HTTP 請求超過一秒未完成，停止需等到約六秒的 Manager 強制清理；修正後 HTTP 200 與停止均在一秒內完成，探測取消、沒有重疊工作，Proxy 與測試 upstream 的 Port 可重新綁定。其餘 CORS、憑證來源、Host 改寫、POST 不重試及 streaming response 測試也通過。

### F06：Windows 正確辨識已退出程序

[Windows 程序判斷](/Users/jimmywon/ai/fabdev/crates/services/src/lib.rs:3853) 對非零 PID 的 `OpenProcess` 失敗立即讀取錯誤碼，只有 `ERROR_INVALID_PARAMETER` 判為不存在；權限不足及其他不明錯誤仍保守保留舊 PHP。取得 handle 後，只有 `WAIT_OBJECT_0` 判定已退出，`WAIT_FAILED` 不再誤判為可安全回收。handle 仍在每次查詢後關閉；三秒 drain 期限與 PHP 切換流程保持不變。[OpenProcess 文件](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openprocess)、[WaitForSingleObject 文件](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject)

新增 Windows 錯誤分類測試及隔離子程序測試，涵蓋運行中、已退出、關閉 handle 後的 PID、權限不足、wait 失敗，以及舊 worker 退出後 PHP child／PID／Socket 清理；既有等待 worker 測試也納入 Windows。這些 Windows 分支已通過含測試程式的交叉編譯，尚未在 Windows 原生執行。

本批驗證：

- `pnpm test`：Desktop 147、Release 規則 21、Rust 309、macOS Helper 10，共 **487 項通過**，7 項既有 ignored。
- `pnpm lint`、`git diff --check` 通過。Proxy 19 項直接回歸測試通過，包含上述真實 loopback HTTP 請求與停止清理。
- Services Windows GNU 目標的 `check --all-targets` 與 Clippy 成功，包含新增 Windows 測試的編譯。仍有 5 項既有編譯警告，Clippy 另指出兩個既有 `needless_return`；這些位置沒有混入本批修改。
- Windows MSVC／原生測試仍待 Windows CI；本機環境限制沿用第二批記錄，未觸發 CI 或執行 Windows 實機流程。
- 未修改 UI、Protocol、資料格式、版本或 Runtime；未重新打包、安裝、替換 Helper、提交、推送或發布。

本批紀錄：[修正前重現](/private/tmp/fabdev-final2-proxy-red.log)、[Proxy 回歸](/private/tmp/fabdev-final2-proxy-test.log)、[完整測試](/private/tmp/fabdev-final2-test.log)、[lint](/private/tmp/fabdev-final2-lint.log)、[Windows GNU check](/private/tmp/fabdev-final2-windows-check.log)、[Windows GNU Clippy](/private/tmp/fabdev-final2-windows-clippy.log)。

## 初次檢查結論

找到 **9 項建議處理的問題：1 項 P1、8 項 P2**。最優先的是未啟用 PHP 的 Site 會直接回傳 PHP 原始碼；另有隱藏 PHP 檔案封鎖失效、Site Home 分享狀態未同步、Windows CA 指紋錯誤、Runtime 下載狀態遺失及 Windows 程序回收缺口。

現有自動測試全部通過，但未涵蓋這些情境。這次結果表示測試之外仍有具體缺陷，不能將「測試全綠」視為兩平台完整驗收。

優先級定義：P1 建議優先修正，避免敏感資料暴露；P2 建議排入後續修正批次。這是本次報告的排序，不變更專案規範或發布 Gate。

| 編號 | 優先級 | 問題 | 證據程度 |
| --- | --- | --- | --- |
| F01 | P1 | 未啟用 PHP 的 Site 直接提供 `.php` 原始碼 | 實際 Nginx 隔離重現 |
| F02 | P2 | PHP location 優先於隱藏檔封鎖，隱藏 PHP 檔仍送往 FastCGI | 實際 Nginx 隔離重現 |
| F03 | P2 | Site Home 同步移除 Site，卻保留 LAN 分享清單與 listener | 隔離 Agent 邏輯與分享 listener 重現 |
| F04 | P2 | Windows CA 查詢／移除使用 PEM 檔案雜湊，指紋種類錯誤 | 真實 CA 產生函式、雜湊比對與 API 契約確認；Windows 實機待驗 |
| F05 | P2 | 切換 Runtime 頁面會遺失下載操作 ID | 實際 Store 搭配模擬 Agent 回覆重現 |
| F06 | P2 | Windows 將不存在的 PID 判定為存活，舊 PHP 程序可能無法回收 | 程式碼與 Windows API 契約確認；Windows 實機待驗 |
| F07 | P2 | Windows Agent 重啟後不恢復既有 MariaDB 狀態 | 平台分支與呼叫流程確認；Windows 實機待驗 |
| F08 | P2 | Proxy 健康探測等待期間暫停接受新連線及處理停止通知 | 非同步控制流程確認；網路延遲實測待驗 |
| F09 | P2 | Windows CI 只編譯 Rust 測試，未執行測試；部分 Windows 路徑未觸發 CI | Workflow 靜態確認 |

## 檢查範圍與基準

先閱讀專案工作記憶、架構與進度文件，再以目前程式碼核對歷史診斷，沒有把舊記錄直接當成尚未修正的問題。

檢查涵蓋 Desktop Vue／Store、Tauri Command 與啟動／退出、Core／SQLite／Protocol、Agent、Sites／Nginx、DNS／PHP／MariaDB 服務、Proxy、Runtime 安裝與更新、Terminal shim、LAN Share／Connect、兩平台 Helper，以及建置／安裝／Release workflow。

相關目錄盤點包含 171 個原始碼、測試、樣式、腳本與設定檔，約 56,660 行；排除依賴目錄、產物及 Runtime binary。方法是檔案盤點、主要執行流程與錯誤分支人工追蹤、完整既有測試及重點隔離實驗，沒有宣稱每個平台分支都經實機執行。

開始時工作目錄已有 README、主題／Dark mode、偏好設定、Store 與相關測試修改，並有三個未追蹤的主題相關檔案。本報告將這些現況一併納入；沒有還原、覆寫或提交它們。下列缺陷位置均位於既有功能流程，未發現它們由本次主題修改造成。

## 問題明細

### F01 — P1：未啟用 PHP 時，PHP 原始碼可被直接下載

位置：[crates/sites/src/lib.rs:102](/Users/jimmywon/ai/fabdev/crates/sites/src/lib.rs:102)、[crates/sites/src/lib.rs:111](/Users/jimmywon/ai/fabdev/crates/sites/src/lib.rs:111)。

`fastcgi_endpoint = None` 時，產生器只省略 PHP location；一般 `try_files $uri $uri/ =404` 仍會交付實際存在的 `.php` 檔案。將含 PHP 的既有專案切成不使用 PHP，或以靜態 Site 加入這類目錄，就會發生。

**實測：**以正式 `render_nginx_site()` 產生設定，啟動隔離的實際 Nginx，放入只含假資料的 `secret.php`。`GET /secret.php` 回傳 `200`，內容包含原始 `<?php` 與假設定值。

**影響：**PHP 檔中的連線資訊或其他設定可能以原始碼形式公開。預設 Site 只綁 loopback，不能直接推論為網際網路暴露；若使用者另行啟用 LAN Share，影響範圍會擴大至可存取該分享的裝置。本次未讀取或測試真實專案機密。

**建議修正：**PHP 關閉時明確拒絕 PHP script 路徑，並確認大小寫及支援副檔名的策略；HTML／CSS／JS 等靜態資源維持正常。補上真實 HTTP 回歸，驗證 PHP → 靜態切換後不能取得 PHP 原始碼，HTTP 與 HTTPS 設定都要涵蓋。

### F02 — P2：隱藏 PHP 檔案繞過隱藏檔封鎖

位置：[crates/sites/src/lib.rs:78](/Users/jimmywon/ai/fabdev/crates/sites/src/lib.rs:78)、[crates/sites/src/lib.rs:114](/Users/jimmywon/ai/fabdev/crates/sites/src/lib.rs:114)、[crates/sites/src/lib.rs:153](/Users/jimmywon/ai/fabdev/crates/sites/src/lib.rs:153)。

PHP 正規表示式 location 被插在 `location ~ /\.` 封鎖規則之前。對同時符合兩者的 `/.hidden.php` 或 `/.private/file.php`，Nginx 會先採用 PHP location。Nginx 官方規則是依設定順序採用第一個符合的正規表示式 location。[官方 location 文件](https://nginx.org/en/docs/http/ngx_http_core_module.html#location)

**實測：**同一隔離 PHP Site 的 `/.hidden.txt` 回傳 `403`，但 `/.hidden.php` 回傳 `502`，因為它被送往刻意未啟動的 FastCGI fixture。這證明封鎖被繞過；本次沒有宣稱已在真實 PHP-FPM 上執行隱藏程式。

**影響：**應受隱藏路徑保護的 PHP 檔仍能進入 PHP handler。一般 `.env` 的拒絕規則並未因此全面失效，問題限定於同時匹配 PHP 的路徑。

**建議修正：**讓隱藏路徑拒絕規則優先於 PHP handler，或在 handler 前加等效保護。測試直接隱藏檔、隱藏資料夾中的 PHP、普通 PHP、普通資產，以及 HTTP／HTTPS。

### F03 — P2：Site Home 移除專案後，LAN 分享仍保留

位置：[crates/agent/src/main.rs:3058](/Users/jimmywon/ai/fabdev/crates/agent/src/main.rs:3058)、[crates/agent/src/main.rs:3092](/Users/jimmywon/ai/fabdev/crates/agent/src/main.rs:3092)。對照已有分享更新／移除邏輯：[crates/agent/src/main.rs:479](/Users/jimmywon/ai/fabdev/crates/agent/src/main.rs:479)、[crates/agent/src/main.rs:981](/Users/jimmywon/ai/fabdev/crates/agent/src/main.rs:981)。

一般 Site 編輯與移除會同步 `lan_share`，但 `sync_home_sites()` 只更新資料庫、網域及服務設定。若已分享的 Site Home 子目錄消失，或切換 Site Home，分享狀態沒有跟著收斂。

**實測：**使用暫存 Site Home、記憶體資料庫、目前 Agent 同步函式及只綁 loopback 的分享 listener。移除空白假專案後執行同步，`sites_after = 0`，但 `share_after.sites` 仍含該 Site ID 與網域。測試最後明確停止 listener，並確認 Port 可重新綁定。

**影響：**分享清單與實際 Site 清單不一致；最後一個 Site 消失時分享入口仍保留。由於分享授權依網域判斷，若該網域之後被另一個 Site 使用，舊授權可能套用到新內容；這項新內容暴露是依路由邏輯推導，本次未做 LAN 裝置端到端測試。

**建議修正：**Site Home 批次同步時，同步撤銷被移除 Site 的分享、更新保留 Site 的網域，最後一個分享移除時停止 listener。補上目錄移除、切換 Home、同網域重建及失敗回復案例。

### F04 — P2：Windows CA 指紋計算使用錯誤的資料

位置：[helpers/windows/src/windows.rs:180](/Users/jimmywon/ai/fabdev/helpers/windows/src/windows.rs:180)、[helpers/windows/src/windows.rs:200](/Users/jimmywon/ai/fabdev/helpers/windows/src/windows.rs:200)。CA 檔案產生位置：[crates/services/src/tls.rs:54](/Users/jimmywon/ai/fabdev/crates/services/src/tls.rs:54)。

CA 存成 PEM，但 `certificate_sha1()` 使用 `certutil -hashfile ca.crt SHA1`。這是 PEM 檔案本身的雜湊，後續卻被當作憑證 thumbprint 傳給 `-store Root` 與 `-delstore Root`。檔案雜湊與憑證存放區的憑證識別不是同一種值。[Microsoft certutil 文件](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/certutil)

**實測：**呼叫正式 `ensure_local_ca()` 產生獨立測試 CA，再比較 PEM bytes SHA-1 與解碼 DER 的 SHA-1，結果不同：

```text
PEM file SHA-1: 26eaab0d68f1df1231680cbc137936af722d81af
DER thumbprint: 6509663693e1bc4d6c40d3e176261b7a111819b1
hashes_match: false
```

**影響：**已信任的 CA 仍可能被判定為未信任，重複要求 UAC；取消信任／解除安裝也不能以這個值正確定位 CA。解除安裝呼叫位於 [installer-hooks.nsh:57](/Users/jimmywon/ai/fabdev/apps/desktop/src-tauri/windows/installer-hooks.nsh:57)，目前沒有檢查該命令的 exit code。Windows 憑證存放區的實際結果尚待實機驗證。

**建議修正：**解析 X.509 憑證後取得 DER thumbprint，或使用 Windows 憑證 API 的原生 thumbprint；保留精準選取單一 fabDev CA 的限制。Windows 回歸要驗證首次信任、重複信任、查詢、移除，以及其他 CA 不受影響。若連帶修改解除安裝程序，須依既有規範恢復對應人工驗收。

### F05 — P2：切換 Runtime 頁面後，下載進度與取消入口遺失

位置：[apps/desktop/src/stores/fabdev.ts:461](/Users/jimmywon/ai/fabdev/apps/desktop/src/stores/fabdev.ts:461)、[RuntimesView.vue:46](/Users/jimmywon/ai/fabdev/apps/desktop/src/views/RuntimesView.vue:46)、[RuntimesView.vue:159](/Users/jimmywon/ai/fabdev/apps/desktop/src/views/RuntimesView.vue:159)。Node.js 與 MariaDB 頁面也會在進入時呼叫同一個檢查更新動作。

`checkRuntimeUpdates()` 成功後無條件將 `runtimeUpdateOperation` 設為 `null`。頁面卸載會停止原本的輪詢，但 Agent 的下載工作持續執行。頁面重新進入後清空 operation ID，也沒有重新列出／接回既有操作的流程。

**實測：**以目前真正的 Pinia Store、模擬 Tauri Agent 回覆，先設置 `status = downloading` 的操作，再呼叫 `checkRuntimeUpdates()`，結果操作變成 `null`；發出的請求只有 `checkRuntimeUpdates`，沒有取消下載。

**影響：**下載進度與取消入口遺失；再次下載同版本時，Agent 會回覆 `the requested Runtime is already downloading`，見 [crates/agent/src/main.rs:274](/Users/jimmywon/ai/fabdev/crates/agent/src/main.rs:274)。已驗證而未安裝的操作也可能因刷新遺失前端入口。

**建議修正：**查 Catalog 不清除仍需追蹤的操作；將下載輪詢生命週期移到 Store／App 層，或提供可恢復的操作查詢。PHP、Node.js、MariaDB 應採一致行為。回歸涵蓋下載途中切頁再返回、刷新 Catalog、取消、驗證完成後切頁再安裝。

### F06 — P2：Windows 已退出的 Nginx worker 可能被永久判定為存活

位置：[crates/services/src/lib.rs:3841](/Users/jimmywon/ai/fabdev/crates/services/src/lib.rs:3841)、[crates/services/src/lib.rs:1665](/Users/jimmywon/ai/fabdev/crates/services/src/lib.rs:1665)。

Windows `process_running()` 對所有 `OpenProcess()` 失敗都回傳 `true`。舊 worker 結束且程序物件已釋放後，無法再以 PID 開啟 handle，卻被當作仍在執行；`wait_for_processes_to_exit()` 因而等到三秒逾時，保留本來可以停止的舊 PHP 程序。

Windows API 要求以 `GetLastError()` 區分失敗原因，`NULL` 本身並不代表程序仍在執行。[OpenProcess 文件](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-openprocess)；程序物件的存續取決於未關閉的 handle。[程序終止文件](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process)

**影響：**切換 Site PHP／全域 PHP 後，Windows 可能重複等待三秒且保留無使用者的 PHP 程序，直到停止 Web services。這是平台程式碼缺陷；本輪未在 Windows 測量實際殘留數量。現有對應回收測試被限制為 `#[cfg(unix)]`，見 [lib.rs:4741](/Users/jimmywon/ai/fabdev/crates/services/src/lib.rs:4741)。

**建議修正：**在取得 worker 清單時持有可等待的程序 handle，或明確區分不存在的 PID 與權限不足，避免以單一 `true` 吞掉所有失敗。Windows 測試涵蓋運行中、剛退出、不存在、存取被拒，以及正常 reload 後舊 PHP 回收。

### F07 — P2：Windows Agent 意外重啟後，遺失仍在執行的 MariaDB 狀態

位置：[crates/services/src/lib.rs:4224](/Users/jimmywon/ai/fabdev/crates/services/src/lib.rs:4224)、[crates/services/src/lib.rs:4239](/Users/jimmywon/ai/fabdev/crates/services/src/lib.rs:4239)、[crates/services/src/lib.rs:4183](/Users/jimmywon/ai/fabdev/crates/services/src/lib.rs:4183)。

Unix 有 MariaDB PID／Socket 恢復邏輯；Windows 所走的 `not(unix)` 分支固定回傳 `None`／`false`。服務是以 `kill_on_drop(false)` 啟動，Windows 終止父程序也不會自動終止其子程序，因此 Agent 異常退出後 MariaDB 可以繼續存活。[Microsoft 程序終止文件](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process)

**影響：**新 Agent 沒有 child handle，將 MariaDB 顯示為 Installed；恢復上次啟動狀態或再次啟動時，又在 [lib.rs:563](/Users/jimmywon/ai/fabdev/crates/services/src/lib.rs:563) 因埠被原程序使用而失敗。PHP Socket／連線選擇另有 Windows 判斷，不能補足 Supervisor 狀態缺口。這裡指的是意外退出／重啟，不是已正常停止服務的升級或 Quit 流程。

**建議修正：**Windows 恢復時結合 fabDev PID 檔、程序執行路徑與 TCP readiness；只接回確認屬於 fabDev 的 MariaDB。Windows 回歸要保留 MariaDB、單獨重啟 Agent，再核對 UI 狀態、獨立停止／啟動、原偏好與資料保留。

### F08 — P2：Proxy 健康檢查與接受連線共用等待分支

位置：[crates/proxy/src/lib.rs:843](/Users/jimmywon/ai/fabdev/crates/proxy/src/lib.rs:843)、[crates/proxy/src/lib.rs:908](/Users/jimmywon/ai/fabdev/crates/proxy/src/lib.rs:908)。

`tokio::select!` 選到健康檢查 tick 後，直接在分支內 `await check_upstream()`。該 TCP／DNS 探測最多等待五秒；等待期間迴圈不再執行 `listener.accept()`，也不會處理 stop 通知。

**影響：**upstream 連線或 DNS 遲滯時，新 client 即使已建立 TCP 連線，也要等待探測結束才進入 Proxy handler；停止操作同樣受延遲。已經分派出去的 client task 仍能執行，不能將此描述為全部連線都被同步阻塞。本輪依控制流程確認，未在使用者真實 upstream 製造延遲。

**建議修正：**讓探測以最多一個獨立、可取消的 task 執行，將結果送回主迴圈；接受連線與停止訊號應保持可處理。補上探測掛起時仍能 accept、及 stop 能及時完成的測試，不改既有 upstream timeout 設定。

### F09 — P2：Windows CI 沒有執行 Rust 單元測試，且觸發路徑不完整

位置：[.github/workflows/windows-x64.yml:82](/Users/jimmywon/ai/fabdev/.github/workflows/windows-x64.yml:82)、[.github/workflows/release-draft.yml:302](/Users/jimmywon/ai/fabdev/.github/workflows/release-draft.yml:302)、[windows-x64.yml:8](/Users/jimmywon/ai/fabdev/.github/workflows/windows-x64.yml:8)。

兩個 Windows job 使用 `cargo check --workspace --all-targets`，沒有 `cargo test`。這會編譯測試，但不執行測試；macOS 本機全綠也不會執行 `#[cfg(windows)]` 分支。另外，自動 push 的 path filter 未列 `helpers/windows/**` 與 `apps/connect/**`，僅修改這些元件不會觸發該 workflow。

**影響：**Windows 版本可以建置成功，但 Windows 專屬測試沒有執行證據；本報告 F04、F06、F07 所涉及的平台缺口更容易漏過。現有 Windows CI 仍有前端、Release 規則及候選 workflow 的 PHP FastCGI PowerShell 測試，並非完全沒有測試。

**建議修正：**候選 Gate 加上 Windows Rust 測試執行，補齊必要 path filter；先處理真正需外部環境的例外，避免一律略過。維持「同一批修正完成後才集中跑一次候選 CI」的既有規則，不為每個小修正重跑完整 Windows 流程。

## 本輪驗證結果

| 驗證 | 結果 |
| --- | --- |
| Desktop Vitest | 16 個測試檔、140 項通過 |
| Release／distribution JavaScript 規則測試 | 20 項通過 |
| Rust workspace 測試 | 302 項通過、7 項既有 ignored |
| macOS Swift Helper | 10 項通過，包含連續 UDP、timeout 後恢復與 idle 清理 |
| `pnpm lint` | 通過：Vue typecheck、Rust format／Clippy、Helper 檢查 |
| `git diff --check` | 通過 |
| 隔離 Nginx | F01、F02 重現；測試程序正常退出 |
| 隔離 Store | F05 重現；只模擬 Agent 回覆，未下載 Runtime |
| 隔離 Site Home／Share | F03 重現；測試 Port 已釋放 |
| CA bytes／thumbprint | F04 的雜湊種類不符已確認 |

第一次完整測試受 sandbox 不允許綁定 loopback 影響；取得工具執行核准後，以同一條 `pnpm test` 重跑並成功。這不是產品測試失敗。額外稽核程式僅在暫存路徑建立，沒有修改產品 source。

7 項 ignored 涵蓋實際 macOS Runtime Archive／PHP 線上安裝、MariaDB TCP／Socket 密碼整合、真實 PHP Share，以及公開 Stable Manifest／DMG 網路下載。沒有將它們計入通過數。

本輪原始紀錄：

- [完整測試紀錄](/private/tmp/fabdev-audit-20260927-test-unrestricted.log)
- [lint 紀錄](/private/tmp/fabdev-audit-20260927-lint.log)
- [Nginx 與 CA 比對結果](/private/tmp/fabdev-audit-20260927-probe.json)
- [Runtime Store 結果](/private/tmp/fabdev-audit-runtime-store.json)
- [Site Home／Share 結果](/private/tmp/fabdev-audit-sitehome.log)

這些暫存證據可能被系統清除；關鍵輸入、結果與位置已摘要保存在本報告，不需要將測試 CA 或私鑰加入 repository。

## 已核對的既有修復、待辦與限制

- 先前 macOS Helper UDP 只處理單筆 datagram 的問題，現在已有持續接收與清理邏輯，本輪 10 項 Helper 測試通過；不再列為未修正問題。
- Rust／TypeScript 的 Protocol 版本同為 40。Runtime 安裝的雜湊／健康檢查／回復流程、Site 修改回復與 MariaDB 連線切換等既有測試，本輪均通過；這不等於所有實際 Runtime 都完成整合驗收。
- Windows App 下載進度以完整 8 MiB 分段更新、macOS 共用下載／取消／自動替換 App 等項目，已有 [FABDEV_PROGRESS.md:312](/Users/jimmywon/ai/fabdev/docs/FABDEV_PROGRESS.md:312) 與 [FABDEV_PROGRESS.md:385](/Users/jimmywon/ai/fabdev/docs/FABDEV_PROGRESS.md:385) 記錄，本報告沒有把它們重列為新發現，也未啟動該 P2 工作。
- 額外測了 PHP 啟用 Site 的 `.PHP`／`.PhP` 路徑；本機 Nginx 仍送往 FastCGI，未重現大小寫造成的原始碼洩漏，因此未列為缺陷。
- 現有主題／Dark mode 修改通過相關單元測試與 typecheck；未做兩平台 WebView 的完整視覺檢查，不能據此宣稱所有配色、尺寸與互動都已人工驗收。
- 本輪沒有 Windows 實機、Windows CI、完整服務 Start → 真實 PHP／MariaDB → Stop、安裝／更新／移除人工驗收，也沒有重新驗證公開 Release Assets。沒有執行第三方依賴 CVE 掃描或模糊測試。
- 沒有重新打包、重建線上 Runtime Package、修改 Catalog、安裝／替換 Helper、操作真實憑證信任或更動使用者現有服務。

## 建議修正順序

1. 先處理 F01／F02 的 Nginx 存取邊界，並補真實 HTTP 回歸。
2. 接著處理 F03 分享同步與 F04 Windows CA 定位，驗證只撤銷對應授權／憑證。
3. 修正 F05 下載狀態生命週期，以及 F06／F07 的 Windows 程序辨識。
4. 處理 F08 探測等待，並以 F09 補足同一批 Windows 候選的測試證據。

修正時應維持最小範圍；不需要因此改版號、重構整個服務層、修改既有 Sites／Proxy 排版或啟動發布。本報告交付後，修正與發布仍是後續獨立工作。
