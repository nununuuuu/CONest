# CONest Chrome / Edge Extension

## 安裝

1. 啟動修改版 CONest（本機 API 會監聽 `127.0.0.1:17345`）。
2. Chrome 開啟 `chrome://extensions`；Edge 開啟 `edge://extensions`。
3. 開啟「開發人員模式」。
4. 選擇「載入未封裝項目」，指向本資料夾 `extension/`。
5. 打開 Bilibili 影片頁，右下角會出現「CONest 下載」。

## 功能

- 下載目前影片 / P
- 下載全部多 P
- 合集批次：收集目前頁面已經載入 DOM 的 Bilibili 影片連結（最多 200 個），逐項送入 Queue

> 合集目前採「已載入項目」策略。如果 Bilibili 使用虛擬列表或分頁，尚未載入到 DOM 的項目不會被收集；可先捲動列表讓項目載入。
