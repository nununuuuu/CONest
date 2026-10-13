//! Provider-specific parsing and execution stay outside the application shell.
//! The existing Bilibili pipeline is retained behind the frontend adapter; yt-dlp
//! uses its own executable and task storage while sharing CONest's semaphore.
pub mod ytdlp;
