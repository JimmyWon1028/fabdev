# 我的 Codex 命令

這份文件統一保存可直接交給 Codex 執行的 fabDev 工作命令。

使用方式：複製命令區塊內的完整句子，貼到 fabDev 的 Codex Task。

## macOS Helper UDP DNS

### 開始修復並驗證

```text
開始修復並驗證 macOS Helper UDP DNS
```

執行內容：

- 修復 macOS System Helper 的 UDP 53 → 53535 DNS 轉送生命週期。
- 加入同一 UDP socket 連續查詢、並行查詢、timeout 後恢復與資源清理測試。
- 執行 Helper test、lint 與 build。
- 取得管理員權限授權後，由 Codex 替換本機 Helper。
- 由 Codex 實際驗證 DNS、53／80／443、`megatower.test`、Port、PID 與 Socket。

預估時間：50～75 分鐘；若遇到額外的 `Network.framework` lifecycle 問題，最多約 90 分鐘。

使用者只需在 macOS 顯示管理員授權時輸入密碼，不需自行執行測試。

限制：不重新打包 Community DMG、不進版、不發布，也不修改 Nginx、PHP、Site、Agent Protocol 或遠端 Proxy 設定。

目前狀態：2026-09-24 已完成 macOS Helper UDP DNS 修復與本機驗證；結果見 [fabDev 進度紀錄](FABDEV_PROGRESS.md#2026-09-24-macos-helper-udp-dns-修復與驗證)。此命令保留供日後同類問題重查，重跑前應先確認當時版本與現場狀態。
