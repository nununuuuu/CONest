# CONest 統一工作台與下載插件

## 工作台

介面採頂部導覽、左側媒體清單、右側輸出設定及共用任務中心。Bilibili 與其他平台使用同一個匯入入口；切換內容時只更新該來源可用的設定。

| 原有功能 | 保留入口 |
| --- | --- |
| Bilibili 影片、番劇、合集、收藏夾、音訊、課程與互動影片解析 | 內容工作台的自動判斷、原有內容類型、分頁與分類 |
| 畫質、音軌、編碼、彈幕、字幕、封面、附加內容及匯出 | Bilibili 輸出設定的完整設定、匯出與原有選取視窗 |
| 暫停、續傳、取消、重試、修改任務、排程與歷史記錄 | 任務中心的統一列表詳情、原生排程與完整控制 |
| 收藏、歷史、帳號、登入、主題、關於 | 頂部匯入來源選單與設定 |
| 全域下載策略、命名、儲存、代理、速度及並行數 | 原有設定分頁；下載引擎另有專屬設定 |
| 瀏覽器擴充、Native Messaging、剪貼簿、拖放、托盤與視窗控制 | 保留原有實作及事件 |

## 內建插件分工

- `src/services/providers.ts` 只負責來源分流，原有 Bilibili 解析、登入、aria2/FFmpeg、佇列、資料庫與排程實作不變。
- `src/services/ytdlp.ts` 與 `src-tauri/src/providers/ytdlp.rs` 是獨立的多平台插件。引擎設定與任務歷史分開保存，不覆寫 Bilibili 的帳號或任務資料。
- 兩者共用既有下載並行數信號量，統一任務列表只轉換顯示模型，不替換原生排程。yt-dlp 任務目前是立即／待開始下載；原生排程功能保留於原生任務。
- 這是程式內建插件邊界，尚未提供第三方插件的安裝或熱載入。

## 合併的多平台能力

使用 yt-dlp 原生解析器支援單一影片、YouTube 合集／播放清單、全選或逐項選取及關鍵字搜尋。不可取得的清單項目保留原始位置並標示不可選取，不會默默重新編號。

新增僅音訊、畫質上限、MP4/MKV/WebM 容器、H.264/H.265/VP9/AV1 視訊編碼、AAC/Opus/MP3/FLAC/WAV 音訊編碼、音訊位元率、CPU 或裝置支援的 GPU 轉碼、字幕、自動字幕、封面、中繼資料、時間裁剪與 SponsorBlock。CPU/GPU 用於 FFmpeg 後處理，網路下載由 yt-dlp 執行；保留來源編碼時不做多餘重編碼。

新任務提供儲存、停止、重試續傳、日誌及開啟資料夾。影片轉碼失敗保留已下載來源檔案。停止會終止子程序樹；重啟後中斷任務顯示可重試。GPU 硬體及 FFmpeg 支援不足會明確報錯。

## 打包與版本

`build.bat` 在編譯前下載官方 yt-dlp 與 Deno，驗證 SHA-256 並測試執行檔版本。它們同時包含於安裝版資源與可攜資料夾，二進位檔案不提交進原始碼。

版本保持 `v0.0.0`。`Validate integrated workspace` 在 main 原始碼更新時建置 Windows 安裝版、可攜版與擴充套件，提供 Actions artifacts，不覆寫已發布的 Release。正式版本仍由既有發版流程根據 package.json 的 X.Y.Z 產生 vX.Y.Z。

## 檢查

- `pnpm web:build`：Vue/TypeScript 檢查及前端建置。
- `node --experimental-strip-types --test scripts/tests/provider-routing.mjs`：原生 ID、網址、分享文字及多平台分流。
- `python scripts/tests/provider-headless.py`：使用生產插件的實際程式碼，在不依賴桌面/WebKit 主機下測試清單解析、格式驗證、編碼器選取、真實本機媒體下載、CPU 轉碼及任務持久保存。可用 CONEST_TEST_YTDLP、CONEST_TEST_FFMPEG、CONEST_TEST_CARGO 指定工具。
- Windows Actions 使用 build.bat 編譯實際 Tauri 主機，再用打包引擎執行下載測試及驗證所有輸出檔案。

本機測試使用生成的媒體，無需網站帳號。實際網站可用性仍取決於 yt-dlp 解析器、Cookie、平台限制與硬體；未以這些測試聲稱每個網站或硬體皆已實測。
