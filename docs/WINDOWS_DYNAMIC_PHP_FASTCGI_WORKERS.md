# Windows x64 動態 PHP FastCGI Worker 設計紀錄

> 文件狀態：設計提案，尚未實作
> 記錄日期：2026-09-10
> 適用平台：Windows x64
> 目前發布基線：fabDev `0.1.25`／Agent Protocol `40`

## 1. 背景

Windows 官方 PHP Runtime 提供 `php-cgi.exe`，但不提供 Unix 平台的 PHP-FPM Process Manager。PHP-FPM 的 `pm = dynamic`、`pm = ondemand`、`pm.max_children`、閒置 Worker 回收等設定，不能直接套用到 Windows。

fabDev `0.1.25` 在 Windows 啟動一個 `php-cgi.exe` 主進程，並透過環境變數建立固定 FastCGI pool：

```text
PHP_FCGI_CHILDREN=4
PHP_FCGI_MAX_REQUESTS=500
```

使用者可以針對每個 PHP 系列選擇 `2`、`4` 或 `8` 個 Worker，預設為 `4`。套用新數量時，fabDev 會重新啟動該 PHP 系列的完整 FastCGI pool；運行期間不會依請求量自動增加或減少。

目前實作位置：

- `crates/services/src/lib.rs`：FastCGI 設定、保存、PHP 啟停及環境變數。
- `crates/core/src/protocol.rs`：Agent request／response 契約。
- `packages/contracts/src/index.ts`：TypeScript 契約。
- `apps/desktop/src/views/RuntimesView.vue`：PHP 設定 UI。
- `apps/desktop/src/utils/locales.ts`：介面文字。

官方技術邊界：

- [PHP Windows 的 `PHP_FCGI_CHILDREN`](https://www.php.net/manual/en/migration71.windows-support.php)：第一個 `php-cgi.exe` 一次建立指定數量的 Children，並共用同一個 TCP Socket。
- [PHP-FPM 設定](https://www.php.net/manual/en/install.fpm.configuration.php)：`dynamic`／`ondemand` 屬於 FPM Process Manager 能力。
- [PHP Windows FPM 問題紀錄](https://bugs.php.net/bug.php?id=62447)：FPM 核心設計依賴 Windows 無法等價提供的 `fork()` 模型。

## 2. 目標

在不引入 Apache、IIS、Herd 或其他外部 Web Stack 的前提下，讓 fabDev Windows FastCGI pool 具備接近 Wamp／`mod_fcgid` 的行為：

```text
fabDev 啟動       1 個 Worker
請求增加          1 → 2 → 3 → 4
持續高負載        最多增加到使用者設定上限
閒置一段時間      4 → 3 → 2 → 1
Worker 異常退出   自動補回
Worker 滿 500 次  自動替換
```

第一版應保持以下產品契約：

- 每個 PHP 系列獨立管理自己的 pool。
- 最大 Worker 仍只允許 `2`、`4` 或 `8`，預設 `4`。
- 最少保留 `1` 個 Worker。
- 所有 FastCGI listener 只綁定 `127.0.0.1`。
- `PHP_FCGI_MAX_REQUESTS=500` 維持內建值，不增加一般使用者設定。
- macOS 繼續使用既有 PHP-FPM `pm = dynamic`，不受此設計影響。
- 不修改 Site、Proxy、PHP Runtime 或 MariaDB 的既有資料契約，除非動態 pool 實作確實需要。

## 3. 不採用的方式

### 3.1 依負載修改 `PHP_FCGI_CHILDREN`

不建議在運行期間將 `PHP_FCGI_CHILDREN` 從 `1` 改成 `2`、`4` 或 `8`，再重啟原本的 `php-cgi.exe`。

原因是環境變數只在進程建立時讀取。每次擴充或縮減都必須停止整個 pool，會中斷正在處理的請求，並在重啟期間產生短暫 502。

### 3.2 引入 Apache／IIS

Apache `mod_fcgid` 或 IIS FastCGI Module 可以管理 Windows PHP Process，但會增加第二套 Web Server、設定來源、Port、服務生命週期與錯誤模型，違反 fabDev 目前由 Nginx 統一本機 Web 入口的架構。

### 3.3 第一版自行實作完整 FastCGI Router

讓 Nginx 永遠連到一個 Rust FastCGI Router，再由 Router 分配動態 Worker，是較完整的長期架構，但第一版必須自行正確處理 FastCGI record、request body streaming、STDERR、取消、排隊、backpressure 與異常連線，風險和測試範圍過大。

## 4. 建議架構

第一版採用「獨立 Port Worker＋Nginx upstream＋Rust Supervisor」。

```text
Desktop
  │
  └─ Agent Protocol
       │
       └─ WindowsFastCgiPoolManager
            ├─ PHP 7.4 Pool
            │    ├─ php-cgi.exe → 127.0.0.1:port-1
            │    └─ php-cgi.exe → 127.0.0.1:port-2
            └─ PHP 8.2 Pool
                 ├─ php-cgi.exe → 127.0.0.1:port-1
                 ├─ php-cgi.exe → 127.0.0.1:port-2
                 └─ php-cgi.exe → 127.0.0.1:port-3

Nginx
  └─ 每個 PHP 系列一組 upstream
       └─ 只包含已 Ready 的 Worker endpoint
```

每個 `php-cgi.exe` 都是不設定 `PHP_FCGI_CHILDREN` 的獨立 Worker：

```text
php-cgi.exe -b 127.0.0.1:<worker-port> -c <php.ini>
```

每個 Worker 保留：

```text
PHP_FCGI_MAX_REQUESTS=500
```

Agent 負責建立、監控、替換及停止每一個 Worker。

## 5. Pool 與 Worker 狀態

每個 PHP 系列由一個 `WindowsFastCgiPool` 管理：

```text
WindowsFastCgiPool
  phpVersion
  minWorkers
  maxWorkers
  idleTimeoutSeconds
  scaleUpCooldownSeconds
  workers[]
```

每個 Worker 至少記錄：

```text
WindowsFastCgiWorker
  id
  port
  pid
  child
  state
  startedAt
  lastBusyAt
  lastIdleAt
  restartCount
```

Worker 狀態：

```text
Starting → Ready → Busy → Idle → Draining → Stopped
                    └──────────────→ Failed
```

狀態轉換必須由單一 serialized reconcile loop 處理，避免同時執行設定保存、Runtime 移除、App Quit、Worker 擴充與 Worker 回收。

## 6. 動態擴充規則

建議第一版內建值：

| 設定 | 預設值 |
| --- | ---: |
| 最少 Worker | 1 |
| 最大 Worker | 4 |
| 允許的最大值選項 | 2／4／8 |
| 負載取樣間隔 | 1 秒 |
| 擴充確認次數 | 連續 3 次 |
| 每次增加數量 | 1 |
| 擴充冷卻時間 | 3 秒 |
| 閒置回收時間 | 60 秒 |
| 每次減少數量 | 1 |
| 每個 Worker 最大請求數 | 500 |

擴充條件建議：

1. 目前 Worker 數小於 `maxWorkers`。
2. 所有 Worker 都正在處理連線，或最近三次取樣的使用率都達 75%。
3. 距離上次擴充已超過冷卻時間。
4. 啟動一個新 Worker。
5. 等待新 Worker 通過 FastCGI readiness。
6. 將新 endpoint 加入 Nginx upstream。
7. `nginx -t` 通過後才 reload。

新 Worker 未 Ready 或 Nginx 設定驗證失敗時，不得影響既有 pool；應停止新 Worker、保留原 upstream，並記錄錯誤。

## 7. 動態回收規則

回收條件建議：

1. 目前 Worker 數大於 `minWorkers`。
2. 指定 Worker 沒有活躍連線。
3. Pool 使用率持續偏低。
4. Worker 閒置至少 60 秒。
5. 距離上次擴充或回收已超過冷卻時間。

安全回收順序：

```text
Worker 標記 Draining
  → 從新 Nginx upstream 移除
  → nginx -t
  → nginx -s reload
  → 等待既有連線結束
  → 停止 php-cgi.exe
  → 釋放 Port 與 Process Handle
```

不得先終止 Worker 再更新 upstream，否則 Nginx 可能將新請求送到已失效的 endpoint。

## 8. Nginx 整合

現在每個 Site 直接使用單一 endpoint：

```nginx
fastcgi_pass 127.0.0.1:19082;
```

動態 pool 應改成每個 PHP 系列一組 upstream：

```nginx
upstream fabdev_php_82 {
  least_conn;
  server 127.0.0.1:<worker-port-1>;
  server 127.0.0.1:<worker-port-2>;
}

location ~ \.php$ {
  fastcgi_pass fabdev_php_82;
  fastcgi_next_upstream error timeout invalid_header;
  fastcgi_next_upstream_tries 2;
}
```

Nginx 支援 FastCGI upstream 與 `least_conn`：[Nginx Load Balancing](https://nginx.org/en/docs/http/load_balancing.html)。Windows Nginx 支援 `nginx -s reload`，會讀取新設定並讓舊 Worker graceful shutdown：[Nginx for Windows](https://nginx.org/en/docs/windows.html)。

每次 upstream 變更必須使用以下順序：

```text
產生暫存設定
  → nginx -t
  → 原子替換正式設定
  → nginx -s reload
  → 驗證 Nginx 仍在運行
```

若 reload 失敗，必須恢復前一版 upstream，不得留下部分套用狀態。

## 9. Port 配置

目前每個 PHP 系列只有一個固定 Port，例如 PHP 8.2 使用 `19082`。動態 pool 需要每個 Worker 一個獨立 Port。

實作時應遵守：

- 保留現有 Port 作為該 PHP 系列的第一個 Worker，降低遷移風險。
- 其他 Worker 使用由 Agent 管理的固定 loopback Port 區段。
- 不依賴隨機 Port，也不把任意 Port 交給 Windows Helper。
- 啟動前確認 Port 可用；衝突時不得覆蓋其他應用程式。
- 不同 PHP 系列、不同 Worker slot 不得產生碰撞。
- 實際配置需保存或由穩定且有碰撞檢查的規則重建。

精確 Port 分配公式在實作前仍需確認，不在本文件先固定未驗證的演算法。

## 10. 負載判斷

第一版不實作 FastCGI Router，可使用以下資料判斷 Worker 狀態：

- Windows `GetExtendedTcpTable`：查看每個 Worker Port 的 TCP 連線。
- Nginx access log：加入 `$upstream_addr`、`$upstream_response_time` 與狀態。
- Child process state：判斷 `php-cgi.exe` 是否退出。
- Worker 的最後忙碌與最後閒置時間。

第一版擴縮決策應以活躍連線為主，access log 用於診斷與後續調校。若實際測試發現 TCP 連線因 keepalive 無法代表請求狀態，再評估增加輕量 FastCGI Router 或其他 request-level telemetry。

## 11. Worker 替換與異常恢復

Worker 可能因以下情況退出：

- 已處理 500 次請求。
- PHP fatal crash。
- Runtime 被移除或更新。
- 使用者修改 PHP 設定。
- App／Agent 停止。

Supervisor 必須區分預期退出與異常退出：

- 預期退出且該 slot 仍在 upstream：立即在同一 Port 補回 Worker。
- 異常退出：記錄 exit code，依退避策略重試。
- 短時間連續失敗：停止無限重啟，標示 PHP pool degraded，保留其他健康 Worker。
- 全部 Worker 失敗：PHP 服務狀態改為 error，不得把 Nginx 或整個 App 誤報為正常。
- Runtime 更新／移除：先停止 reconcile loop，再 drain 並停止完整 pool。

建議重啟退避：

```text
1 秒 → 2 秒 → 5 秒 → 10 秒，最多 30 秒
```

## 12. Windows Process 管理

所有 `php-cgi.exe` 應加入 Windows Job Object，並設定 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`。Agent 正常或異常結束時，關閉 Job Object 即可清理整組 Worker，避免留下孤兒進程。[Microsoft Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)

停止順序維持：

```text
停止接收新請求
  → 停止或 reload Nginx
  → drain PHP Worker
  → 關閉 PHP Job Object
  → 清除 PID／狀態檔
```

## 13. 設定契約

目前每個 PHP 系列保存：

```json
{
  "workers": 4
}
```

建議新格式：

```json
{
  "schemaVersion": 2,
  "mode": "dynamic",
  "minWorkers": 1,
  "maxWorkers": 4,
  "idleTimeoutSeconds": 60
}
```

相容策略：

- 舊的 `workers` 讀取為新的 `maxWorkers`。
- `minWorkers` 預設 `1`。
- `idleTimeoutSeconds` 預設 `60`。
- 缺少設定檔時使用動態模式、最少 `1`、最多 `4`。
- 寫入新格式前保留錯誤回復能力，不覆蓋無法解析的使用者設定。

若 Agent Protocol 對外回傳 `mode`、`minWorkers`、`maxWorkers` 或即時狀態，必須同步修改：

- `crates/core/src/protocol.rs`
- `packages/contracts/src/index.ts`
- Agent Protocol 版本
- Desktop store 與測試

## 14. UI 建議

PHP 設定頁第一版只顯示必要資訊：

```text
FastCGI Worker 模式：動態
最大 Worker：4
目前 Worker：1 / 4
每個 Worker 最大請求數：500（內建）
閒置回收：60 秒（內建）
```

使用者可修改的項目先維持：

- 最大 Worker：`2`／`4`／`8`。

下列參數先維持內建，不開放設定：

- 最少 Worker。
- 負載取樣間隔。
- 擴充門檻。
- 冷卻時間。
- 閒置回收時間。
- 最大請求數。

這可以保持 PHP 相關設定集中在 PHP 設定頁，又避免一般使用者必須理解 Process Manager 調校細節。

## 15. 記錄與診斷

每次狀態改變應寫入結構化 log：

```text
php=8.2 event=worker_spawn slot=2 port=<port>
php=8.2 event=worker_ready slot=2 pid=<pid>
php=8.2 event=scale_up active=1 target=2 reason=all_workers_busy
php=8.2 event=scale_down active=3 target=2 reason=idle_timeout
php=8.2 event=worker_exit slot=1 exit_code=<code> expected=true
php=8.2 event=pool_degraded healthy=1 max=4
```

一般 UI 只顯示摘要；詳細 Port、PID 與擴縮原因保留在診斷 log，不增加一般設定畫面的複雜度。

## 16. 預計修改範圍

若後續取得實作授權，預計至少影響：

- `crates/services/src/lib.rs`
  - 將單一 `Child` 改為 Windows pool manager。
  - 加入 Worker 狀態、reconcile loop、擴縮與 drain。
  - 產生 PHP upstream 並安全 reload Nginx。
  - 加入設定遷移與回復。
- `crates/core/src/protocol.rs`
  - 視 UI 契約增加模式、最大值與即時 pool 狀態。
- `packages/contracts/src/index.ts`
  - 同步 TypeScript request／response。
- `crates/agent/src/main.rs`
  - 同步設定命令與狀態回傳。
- `apps/desktop/src/views/RuntimesView.vue`
  - 將固定 Worker 數量改成最大 Worker，顯示目前數量。
- `apps/desktop/src/utils/locales.ts`
  - 同步英文、繁體中文與簡體中文文字。
- Nginx 設定產生器與相關測試。

不得修改 macOS `resources/php/www.conf` 的既有 PHP-FPM 動態模型。

## 17. 測試 Gate

### 17.1 Rust 單元測試

- 缺少設定時為 `min=1`、`max=4`。
- 舊 `workers` 正確遷移成 `maxWorkers`。
- 只接受最大值 `2`、`4`、`8`。
- 不低於最小值、不超過最大值。
- 連續高負載才擴充，不因單次尖峰擴充。
- 閒置滿時間才回收。
- 擴充與回收有 cooldown。
- Worker crash、500 次退出與 restart backoff。
- Nginx 驗證失敗時回復舊 upstream。
- 多 PHP 系列互不影響。
- macOS 行為不變。

### 17.2 Windows CI 整合測試

1. 啟動時只有 `1` 個 PHP Worker。
2. 同時送出多個慢 PHP 請求，Worker 自動增加。
3. 最大值設為 `4` 時不得出現第 `5` 個 Worker。
4. 請求完成並閒置後逐步縮回 `1`。
5. 擴縮期間請求不得出現非預期 502。
6. 強制終止 Worker 後能自動補回。
7. Worker 滿 500 次請求後能替換且服務持續可用。
8. 停止服務後不得殘留 PID、Port 或 `php-cgi.exe`。
9. PHP 7.4 與 8.2 同時使用時各自擴縮。
10. Site timeout、Proxy timeout、PHP 設定及 Runtime 安裝／移除回歸通過。

### 17.3 Repository Owner 實機 Gate

- 啟動時工作管理員可觀察到最小 Worker 數。
- 併發請求時 Worker 增加且回應速度正常。
- 閒置 60 秒後 Worker 逐步釋放。
- 切換 `2`／`4`／`8` 上限後行為正確。
- App Quit、更新與移除後無殘留 PHP Process。
- 與 Wamp／IIS／其他已佔用 Port 的環境共存時，不接管或終止外部 Process。

## 18. 分階段實作建議

### 第一階段：Supervisor 基礎

- 每個 PHP 系列使用多個獨立 Port。
- 固定啟動 `minWorkers`。
- Worker crash／500 次退出自動補回。
- Job Object 清理。
- 暫不啟用自動擴縮。

### 第二階段：Nginx upstream 與安全 drain

- Site 改用 PHP 系列 upstream。
- 加入 `least_conn`。
- 完成新增、移除、`nginx -t`、reload 與 rollback。

### 第三階段：動態擴縮

- 啟用連線取樣、scale-up、idle scale-down 與 cooldown。
- 加入即時 pool 狀態及診斷 log。
- Windows CI 壓力與生命週期測試。

### 第四階段：UI

- 將目前的 Worker 數量選項改成最大 Worker。
- 顯示目前／最大 Worker。
- 保持其他調校參數為內建值。

## 19. 實作前仍需確認

- 預設是否採 `minWorkers=1`、`maxWorkers=4`。
- 閒置回收是否固定為 60 秒。
- 使用者是否只調整最大 Worker，不顯示進階參數。
- 舊 `workers` 是否直接遷移成動態模式的 `maxWorkers`。
- 各 PHP 系列的 Worker Port 區段與碰撞處理規則。
- 第一版負載來源採 TCP connection、Nginx access log，或兩者並用。

本文件只記錄設計方向，不構成實作、進版、CI、重新打包或發布授權。
