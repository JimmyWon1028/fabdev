# fabDev Connect（Windows x64）

`fabdev-connect.exe` 讓 Windows 11 電腦或 Parallels VM 不必手動修改 hosts，就能用瀏覽器開啟另一台 fabDev 主機暫時分享的 `.test` Site。

## 使用方式

1. 在 fabDev 主機啟動 Web 服務。
2. 到 Sites 畫面對 `site-one.test`、`site-two.test` 等需要的 Site 分別按「局網分享」，記下共用的 `IP:Port`。
3. 在 Windows 執行 `fabdev-connect.exe`，接受 UAC 管理員授權。
4. 輸入主機的 `IP:Port`，並以空白或逗號分隔輸入 `site-one.test, site-two.test`，按「連線」。
5. 「開啟網站」會開啟清單中的第一個 Site；其他 Site 可在任意瀏覽器輸入例如 `http://site-two.test`。
6. 使用完畢按「中斷」再關閉程式；主機端可逐一停止 Site，Stop All 則停止全部分享。

程式會把最後輸入的主機與 Sites 儲存在 `%LOCALAPPDATA%\FabDev\fabdev-connect.json`，下次啟動自動還原；關閉程式仍會照常清除受管 hosts 與釋放 Port 80。

若 exe 位於 Parallels Shared Folders，例如 `E:\programs` 對應的 `\\Mac\diskD`，啟動器會先把同一版本複製到 `%LOCALAPPDATA%\FabDev\fabdev-connect-runtime.exe`，再從 Windows 本機路徑要求 UAC，避免提升權限後無法重新開啟共享磁碟上的程式。

程式只會管理以下標記內的 hosts 內容，並在每次更新前建立 `hosts.fabdev-connect.backup`：

```text
# BEGIN FABDEV CONNECT
127.0.0.1 site-one.test
127.0.0.1 site-two.test
# END FABDEV CONNECT
```

若 Windows hosts 已有任何同名且未受管理的 Site，或 IIS／其他 Web Server 已占用 `127.0.0.1:80`，連線會明確失敗，不會覆寫或停止其他程式。

此功能沒有 TLS、登入或 Client 授權，只適合可信任局網內 1–2 台 Client 的短時間開發測試，不是 `fabDev Server`，也不得公開到網際網路。
