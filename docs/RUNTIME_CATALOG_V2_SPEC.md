# Runtime Catalog v2 規格

## 目的

fabDev App、Runtime 安裝列表與 Runtime Package 分開管理：

- `JimmyWon1028/fabdev`：App 原始碼、Windows／macOS App 安裝程式與 App Release。
- `JimmyWon1028/fabdev-runtimes`：Runtime 完整索引、Runtime Catalog 與 Runtime Package Release。
- App 版本不再決定 PHP、MariaDB 或 Node.js Package 的內容；Runtime Package 只有在上游版本新增或同版本需要修正重包時才更新。

## Release 與檔案結構

Runtime 倉庫使用遞增的 `catalog-vN` Release，例如：

```text
fabdev-runtimes/
  runtime-index-v1.json
  releases/
    catalog-v1/
      fabdev-runtime-v2.json
      php-8.2.33-windows-x64-community.tar.gz
      ...初始移轉的既有 Package
    catalog-v2/
      fabdev-runtime-v2.json
      php-8.2.33-windows-x64-community.tar.gz
      ...只有本次新增或重包的 Package
```

`runtime-index-v1.json` 是版本控制中的完整 Runtime 索引；每次產生的 `fabdev-runtime-v2.json` 也是完整安裝列表。Catalog Release 不必重新上傳未變動的 Package，未變動項目繼續使用先前 `catalog-vN` 的完整 URL。

Runtime Package 檔名只包含上游版本、平台與架構：

```text
php-8.2.33-windows-x64-community.tar.gz
```

同版本修正重包可以沿用相同檔名，但必須發布在新的 `catalog-vN` Release。因 Release tag 不同，完整 URL 不同；索引中的 `size` 與 `sha256` 也必須更新。

## 不可變規則

已發布 Release 內的 Package 不可原地替換。需要修正時：

1. 建立下一個 `catalog-vN`。
2. 以相同上游版本與檔名重新產生 Package。
3. 更新完整索引中的 URL、大小與 SHA-256。
4. 新 Catalog 保留所有其他未變動項目的舊 URL。
5. 發布後保留舊 Release，供已接受舊 Catalog 的用戶端繼續下載與驗證。

這裡的「不可變」是指一個完整 URL 對應的 bytes 與 SHA-256 永遠不變，不是禁止同一個上游版本重新打包。

## Catalog v2 契約

Catalog 固定入口：

```text
https://github.com/JimmyWon1028/fabdev-runtimes/releases/latest/download/fabdev-runtime-v2.json
```

主要規則：

- `schemaVersion` 固定為 `2`。
- `catalogSequence` 從 `1` 開始單調遞增，不可倒退或以相同序號發布不同內容。
- Package URL 必須是 `JimmyWon1028/fabdev-runtimes` 的 `catalog-vN` Release URL。
- Package 所在的 `catalog-vN` 不得大於目前 Catalog 的 `catalogSequence`。
- 每個 `name + version + platform + architecture` 在一份 Catalog 中只能出現一次。
- `fileName` 不含 Catalog 或 App 版本；Package 身分由完整 URL、大小與小寫 SHA-256 共同決定。
- UI 只顯示上游版本；內部使用 Package SHA-256 判斷同版本是否需要更新。

正式產生指令：

```bash
./scripts/run-cargo.sh run --locked -p fabdev-runtime --bin fabdev-runtime-catalog -- \
  generate-v2 \
  <catalog-sequence> \
  <generated-at> \
  <expires-at> \
  <minimum-app-version> \
  <runtime-index-v1.json> \
  <fabdev-runtime-v2.json>
```

產生器只讀取完整索引，不要求所有歷史 Package 存在於本機，也不會重新打包 Package。

## 用戶端行為

1. App 首次啟動或使用者重新整理時，先下載 Catalog v2。
2. 尚未接受過 Catalog v2 且 v2 不可用時，可以回退既有 Catalog v1。
3. 一旦接受 Catalog v2，更新失敗時只能使用已驗證的 v2 快取，不可回退 v1，避免降級。
4. Catalog v1 與 v2 使用不同快取檔；Package 快取以 SHA-256 命名，不以顯示檔名命名。
5. 使用者按下載時才下載 Package，並驗證大小與 SHA-256。
6. 安裝後在 Runtime 版本目錄記錄 Package SHA-256 與 Catalog sequence。
7. 上游版本相同但 Catalog SHA-256 不同時，UI 顯示可更新；安裝採 staging、舊目錄備份、健康檢查及失敗回復。

## 初始移轉 Gate

`catalog-v1` 是唯一允許自動補寫舊安裝 receipt 的移轉基準，因此必須符合：

- Package bytes 必須與既有正式 0.1.20 Runtime Package 完全相同，只能複製，不可重包。
- 每個複製後的 Package 必須重新核對來源 Release SHA-256、目的 Release SHA-256 與大小，三者完全一致。
- `runtime-index-v1.json` 與 `fabdev-runtime-v2.json` 必須包含 Windows x64 與 macOS ARM64 的完整安裝列表。
- `catalog-v1` 不得混入任何同版本修正版；修正版從 `catalog-v2` 開始。

如果用戶端第一次看到的 v2 已經大於 sequence 1，而既有 Runtime 沒有 receipt，該同版本 Package 必須視為可更新，不能假設內容相同。

## 發布 Gate

1. 驗證新建或重包 Package 的上游來源、簽署資料、大小與 SHA-256。
2. 更新完整索引，只改本次變動項目的 URL、大小與 SHA-256。
3. 產生並驗證完整 Catalog v2。
4. 確認未變動項目的 URL 與 SHA-256 沒有改變。
5. 建立 `catalog-vN` Draft，先上傳 Catalog 與本次變動 Package並完成靜態驗證。
6. 發布 Runtime Release，讓固定 `releases/latest` 入口可被正式候選 App 讀取；發布後不可原地修改 Asset。
7. Repository Owner 以候選 App 執行下載／安裝 Gate；若失敗，修正內容必須建立下一個 `catalog-vN`，不得覆蓋舊 Release。
8. Runtime Gate 通過後再進入獨立的 App Release 流程。

建立倉庫、重新打包、上傳、推送或 Publish 都是個別外部動作，仍需 Repository Owner 明確授權。
