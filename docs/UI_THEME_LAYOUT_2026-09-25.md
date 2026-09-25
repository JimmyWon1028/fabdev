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
