# Desktop 主題與版面調整

日期：2026-09-25（Asia/Taipei）。Repository Owner 已確認畫面並授權紀錄、commit 與 push。此批修改位於 `0.1.27` 之後的開發程式碼，未進版、打包或發布 App／Runtime。

## 已確認的效果

- Glassmorphism 採青藍、天空藍與淡紫漸層，左側導覽與右側內容共用連續背景。白色半透明毛玻璃卡片搭配深藍文字；水滴使用不同輪廓、大小、高光與排列，並套用到 Sites／Proxy 項目。
- 原 Cyberpunk 更名為 Tron，保留 `cyberpunk` 偏好識別碼。全視窗使用已預覽並獲核准的 Tron 城市背景，導覽加入青色光線、電路與圓環元素。
- Tron 後新增獨立 Cyberpunk，使用 `cyberpunk-city` 偏好識別碼。背景以 SVG 呈現藍紫、桃紅霓虹城市；左右共用同一背景。
- Tron／Cyberpunk 的卡片與清單使用半透明底色與模糊，讓背景隱約透出並保持文字清晰。對話框另保留較高遮蔽程度。
- PHP、MariaDB、Node.js Runtime 清單與設定卡片的最大寬度改為與 Sites 相同的 `1180px`。PHP 全域版本、線上安裝、終端機及 php.ini 面板與清單對齊。
- Site Home 第一張卡片與 Sites 清單同寬；清單操作欄可縮小並換行，按鈕不壓縮，避免「移除」文字或按鈕超出右緣。

## 素材與偏好相容性

- `apps/desktop/src/themes/tron-city.jpg`：已核准的 1920 × 1080 背景；來源為 https://getwallpapers.com/wallpaper/full/7/9/c/903009-tron-legacy-backgrounds-1920x1080-desktop.jpg 。
- `cyberpunk-city.svg`、`cyberpunk-circuit.svg`、`glass-droplets.svg` 為本次介面使用的 SVG 素材；背景均由本機資產載入。
- 英文、繁體中文、簡體中文均顯示獨立的 Tron／Cyberpunk 名稱；偏好測試覆蓋舊識別碼保留與兩種主題切換持久化。

## 驗證

- 提交前完整 `pnpm test` 通過：Desktop 108、Release 規則 20、Rust 302、macOS Helper 10 項；另有 7 項需要外部環境的 Rust 測試維持 ignored。首次 sandbox 執行因禁止綁定 loopback 而失敗，使用系統環境重跑後全數通過。
- `pnpm lint` 與 `git diff --check` 通過。
- 前端 TypeScript 檢查與 Vite 建置通過。
- 隔離瀏覽器使用測試資料驗證 6 種主題、3 種視窗寬度：Runtime 與設定卡片對齊 Sites，PHP 輔助面板與 php.ini 編輯區右緣一致。
- Sites 以 6 種主題、3 種語言及 11 種視窗寬度驗證共 198 組情境：Site Home／清單右緣一致，按鈕與文字未裁切。
- 主題背景載入、透明卡片、選取狀態、對話框與鍵盤焦點已做瀏覽器檢查；Repository Owner 已確認最終畫面。此項不代表重新完成 Windows／macOS 安裝與服務生命週期驗收。

## 同日追加：新主題與即時鍵盤切換

Repository Owner 已確認本批效果，並再次授權紀錄、commit 與 push。新增及排序如下：

1. Default
2. Notion
3. Solarized
4. Neo-Brutalism
5. Glassmorphism
6. Tron
7. Cyberpunk
8. Graphite
9. Retro Terminal
10. Blueprint

- Graphite 使用石墨灰背景、霧面卡片與冰藍強調色；Retro Terminal 使用黑底、琥珀色、等寬字體與淡掃描線；Blueprint 使用工程藍底、細網格與線框。
- 原先預覽並實作的 Porcelain 已依指示改為 Solarized，使用米黃底、灰青文字、柔和藍色按鈕與橘色警示。為保留已選主題，內部識別碼仍為 `porcelain`，選單與三種語言均顯示 Solarized。
- 四款主題由 `studio-themes.css` 使用限定主題範圍的配色變數套用，保留 Sites／Proxy 排版、項目寬度、狀態語意與文字可讀性。
- 原生主題 select 改為 `theme-select.vue` 彈出清單；按上／下鍵移動後立即套用並保存，Home／End 可移到首／末項。Enter、Esc、Tab 或點擊外部收起；Esc 保留目前已套用的主題。清單以 Teleport 顯示並依視窗空間決定向上／向下展開。
- 開啟時明確聚焦觸發按鈕，修復 macOS WebKit 滑鼠點擊未自動聚焦導致方向鍵無反應的問題；選單提供 combobox／listbox 語意、目前項目識別與選取狀態。

追加驗證：偏好持久化、主題排序與三語名稱測試；隔離瀏覽器驗證全部 10 款主題的選單與鍵盤切換，並檢查新主題在 900／1280／1440／1800px 視窗的 Sites、PHP、MariaDB、Node.js、設定版面，以及 Proxy 選取、編輯彈窗和鍵盤焦點。Solarized 替換後另確認名稱、順序、米黃配色及重新載入保存結果。原生 WKWebView 已驗證點擊取得焦點、ArrowDown／ArrowUp 即時切換；前端 TypeScript 與 Vite 建置通過。

本批提交前完整 `pnpm test` 通過：Desktop 113、Release 規則 20、Rust 302、macOS Helper 10 項；7 項需要外部環境的 Rust 測試維持 ignored。`pnpm lint`、前端建置及 `git diff --check` 通過。未變更版本、Protocol、App／Runtime 發布資產。
