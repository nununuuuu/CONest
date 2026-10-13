use crate::{
    shared::{get_app_handle, random_string, Sidecar, WORKING_PATH},
    storage::config,
    TauriResult,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::{collections::BTreeMap, path::PathBuf, process::Stdio, sync::LazyLock};
use tauri::{Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{Mutex, OnceCell},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct EngineSettings {
    pub executable: String,
    pub deno: String,
    pub ffmpeg: String,
    pub cookie_file: String,
    pub cookie_browser: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MediaEntry {
    pub id: String,
    pub url: String,
    pub title: String,
    pub thumbnail: String,
    pub duration: Option<f64>,
    pub index: usize,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MediaCollection {
    pub title: String,
    pub platform: String,
    pub entries: Vec<MediaEntry>,
    pub is_playlist: bool,
    pub warnings: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct DownloadOptions {
    pub mode: String,
    pub height: u32,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: String,
    pub bitrate: u32,
    pub accelerator: String,
    pub output_dir: String,
    pub subtitles: String,
    pub auto_subtitles: bool,
    pub embed_subtitles: bool,
    pub thumbnail: bool,
    pub metadata: bool,
    pub sponsorblock: bool,
    pub section_start: Option<f64>,
    pub section_end: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ProviderTask {
    pub id: String,
    pub entry: MediaEntry,
    pub options: DownloadOptions,
    pub engine: EngineSettings,
    pub state: String,
    pub phase: String,
    pub progress: f64,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub error: String,
    pub log: Vec<String>,
    pub output: String,
}

#[derive(Default)]
struct Tasks {
    tasks: BTreeMap<String, ProviderTask>,
    running: BTreeMap<String, CancellationToken>,
}
static TASKS: LazyLock<Mutex<Tasks>> = LazyLock::new(|| Mutex::new(Tasks::default()));
static LOADED: OnceCell<()> = OnceCell::const_new();
static SAVE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static ENGINE_LOCK: LazyLock<tokio::sync::RwLock<()>> =
    LazyLock::new(|| tokio::sync::RwLock::new(()));

fn tasks_path() -> PathBuf {
    WORKING_PATH.join("Providers/yt-dlp-tasks.json")
}
fn settings_path() -> PathBuf {
    WORKING_PATH.join("Providers/yt-dlp-settings.json")
}

async fn load() -> Result<()> {
    LOADED
        .get_or_try_init(|| async {
            let path = tasks_path();
            if path.exists() {
                let data = tokio::fs::read(&path).await?;
                let mut tasks: BTreeMap<String, ProviderTask> = serde_json::from_slice(&data)
                    .context(
                        "yt-dlp task storage could not be read; original file was preserved",
                    )?;
                for task in tasks.values_mut() {
                    if matches!(task.state.as_str(), "queued" | "running" | "processing") {
                        task.state = "stopped".into();
                        task.phase = "上次執行已中斷，可重試並續傳".into();
                    }
                }
                TASKS.lock().await.tasks = tasks;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await?;
    Ok(())
}

async fn save() -> Result<()> {
    let _guard = SAVE_LOCK.lock().await;
    let data = serde_json::to_vec_pretty(&TASKS.lock().await.tasks)?;
    let path = tasks_path();
    tokio::fs::create_dir_all(path.parent().context("Missing storage directory")?).await?;
    let temp = path.with_extension("json.tmp");
    tokio::fs::write(&temp, &data).await?;
    // On Windows rename cannot replace an existing file. A backup remains
    // available if the replacement fails; never silently discard task history.
    let backup = path.with_extension("json.bak");
    if path.exists() {
        tokio::fs::copy(&path, &backup).await?;
    }
    tokio::fs::copy(&temp, &path).await?;
    let _ = tokio::fs::remove_file(temp).await;
    Ok(())
}

async fn update(id: &str, change: impl FnOnce(&mut ProviderTask)) {
    update_state(id, change, false).await;
}

async fn update_state(id: &str, change: impl FnOnce(&mut ProviderTask), finished: bool) {
    let snapshot = {
        let mut state = TASKS.lock().await;
        if finished {
            state.running.remove(id);
        }
        state.tasks.get_mut(id).map(|task| {
            change(task);
            task.clone()
        })
    };
    if let Some(task) = snapshot {
        let _ = get_app_handle().emit("provider-task", task);
    }
}

fn executable(settings: &EngineSettings) -> PathBuf {
    if !settings.executable.is_empty() {
        return PathBuf::from(&settings.executable);
    }
    let cached = WORKING_PATH.join("Providers/bin").join(if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    });
    if cached.is_file() {
        return cached;
    }
    let parent = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    let name = if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    };
    let bundled = parent.join(name);
    if bundled.is_file() {
        return bundled;
    }
    if let Ok(dir) = get_app_handle().path().resource_dir() {
        let resource = dir.join("providers").join(name);
        if resource.is_file() {
            return resource;
        }
    }
    PathBuf::from(name)
}

fn provider_ffmpeg(settings: &EngineSettings) -> PathBuf {
    if !settings.ffmpeg.is_empty() {
        return PathBuf::from(&settings.ffmpeg);
    }
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
    {
        let bundled = parent.join("providers").join(name);
        if bundled.is_file() {
            return bundled;
        }
    }
    if let Ok(dir) = get_app_handle().path().resource_dir() {
        let bundled = dir.join("providers").join(name);
        if bundled.is_file() {
            return bundled;
        }
    }
    config::read().sidecar(Sidecar::FFmpeg)
}

fn command(path: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(path);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.as_std_mut().process_group(0);
    }
    cmd.kill_on_drop(true);
    cmd
}

fn valid_url(raw: &str) -> Result<()> {
    if let Some(query) = raw.strip_prefix("ytsearch20:") {
        if query.trim().is_empty() {
            bail!("搜尋文字不可為空");
        }
        return Ok(());
    }
    let url = tauri::Url::parse(raw).context("請輸入完整網址")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        bail!("只接受 http/https 網址");
    }
    Ok(())
}

fn common_args(settings: &EngineSettings) -> Result<Vec<String>> {
    let mut args = vec![
        "--ignore-config".into(),
        "--no-color".into(),
        "--encoding".into(),
        "utf-8".into(),
        "--socket-timeout".into(),
        "30".into(),
    ];
    if !settings.cookie_file.is_empty() {
        if !PathBuf::from(&settings.cookie_file).is_file() {
            bail!("Cookie 檔案不存在");
        }
        args.extend(["--cookies".into(), settings.cookie_file.clone()]);
    } else if !settings.cookie_browser.is_empty() {
        if ![
            "chrome", "edge", "firefox", "brave", "vivaldi", "opera", "chromium", "safari",
        ]
        .contains(&settings.cookie_browser.as_str())
        {
            bail!("不支援的 Cookie 瀏覽器");
        }
        args.extend([
            "--cookies-from-browser".into(),
            settings.cookie_browser.clone(),
        ]);
    }
    let parent = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    let mut deno = if settings.deno.is_empty() {
        parent.join(if cfg!(windows) { "deno.exe" } else { "deno" })
    } else {
        PathBuf::from(&settings.deno)
    };
    if !deno.is_file() && settings.deno.is_empty() {
        if let Ok(dir) = get_app_handle().path().resource_dir() {
            deno = dir
                .join("providers")
                .join(if cfg!(windows) { "deno.exe" } else { "deno" });
        }
    }
    if deno.is_file() {
        args.extend([
            "--js-runtimes".into(),
            format!("deno:{}", deno.to_string_lossy()),
        ]);
    }
    let cfg = config::read();
    if !cfg.proxy.address.is_empty() {
        let mut proxy = tauri::Url::parse(&cfg.proxy.address)?;
        let _ = proxy.set_username(&cfg.proxy.username);
        let _ = proxy.set_password(Some(&cfg.proxy.password));
        args.extend(["--proxy".into(), proxy.to_string()]);
    }
    args.extend([
        "--ffmpeg-location".into(),
        provider_ffmpeg(settings).to_string_lossy().to_string(),
    ]);
    Ok(args)
}

fn entry(value: &Value, index: usize, fallback: &str) -> MediaEntry {
    let id = value["id"].as_str().unwrap_or_default().to_string();
    let mut url = value["webpage_url"]
        .as_str()
        .or_else(|| value["url"].as_str())
        .unwrap_or(fallback)
        .to_string();
    if !url.starts_with("http")
        && !id.is_empty()
        && value["ie_key"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Youtube")
    {
        url = format!("https://www.youtube.com/watch?v={id}");
    }
    let available = !value.is_null()
        && url.starts_with("http")
        && !matches!(
            value["availability"].as_str(),
            Some("private" | "needs_auth" | "premium_only")
        );
    MediaEntry {
        id,
        url,
        title: value["title"]
            .as_str()
            .unwrap_or("無法取得的項目")
            .to_string(),
        thumbnail: value["thumbnail"]
            .as_str()
            .or_else(|| {
                value["thumbnails"]
                    .as_array()
                    .and_then(|v| v.last())
                    .and_then(|v| v["url"].as_str())
            })
            .unwrap_or_default()
            .to_string(),
        duration: value["duration"].as_f64(),
        index,
        available,
    }
}

pub fn parse_collection(value: &Value, url: &str, warnings: String) -> MediaCollection {
    let is_playlist = value["entries"].is_array();
    let entries = match value["entries"].as_array() {
        Some(items) => items
            .iter()
            .enumerate()
            .map(|(i, v)| entry(v, i + 1, ""))
            .collect(),
        None => vec![entry(value, 1, url)],
    };
    MediaCollection {
        title: value["title"].as_str().unwrap_or("媒體內容").into(),
        platform: value["extractor_key"]
            .as_str()
            .or_else(|| value["extractor"].as_str())
            .unwrap_or("yt-dlp")
            .into(),
        entries,
        is_playlist,
        warnings,
    }
}

#[tauri::command(async)]
#[specta::specta]
pub async fn ytdlp_analyze(
    url: String,
    playlist: bool,
    settings: EngineSettings,
) -> TauriResult<MediaCollection> {
    let _engine_guard = ENGINE_LOCK.read().await;
    valid_url(&url)?;
    let mut args = common_args(&settings)?;
    args.extend([
        "--dump-single-json".into(),
        "--skip-download".into(),
        "--flat-playlist".into(),
        "--ignore-errors".into(),
    ]);
    args.push(
        if playlist {
            "--yes-playlist"
        } else {
            "--no-playlist"
        }
        .into(),
    );
    args.extend(["--".into(), url.clone()]);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        command(executable(&settings)).args(&args).output(),
    )
    .await
    .context("解析逾時，請重試或縮小清單範圍")??;
    let warnings = String::from_utf8_lossy(&output.stderr).to_string();
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| anyhow!("yt-dlp 解析失敗：{}", warnings.trim()))?;
    Ok(parse_collection(&value, &url, warnings))
}

pub fn validate_options(o: &DownloadOptions) -> Result<()> {
    if !["video", "audio"].contains(&o.mode.as_str()) {
        bail!("下載模式無效");
    }
    if !["mp4", "mkv", "webm"].contains(&o.container.as_str()) {
        bail!("影片容器無效");
    }
    if !["original", "h264", "h265", "vp9", "av1"].contains(&o.video_codec.as_str()) {
        bail!("影片編碼無效");
    }
    if !["original", "aac", "opus", "mp3", "flac", "wav"].contains(&o.audio_codec.as_str()) {
        bail!("音訊編碼無效");
    }
    if !["cpu", "nvidia", "intel", "amd", "apple"].contains(&o.accelerator.as_str()) {
        bail!("轉碼裝置無效");
    }
    if o.mode == "video"
        && o.container == "webm"
        && (!matches!(o.video_codec.as_str(), "original" | "vp9" | "av1")
            || !matches!(o.audio_codec.as_str(), "original" | "opus"))
    {
        bail!("WebM 請搭配 VP9/AV1 與 Opus，或改用 MKV");
    }
    if o.mode == "video" && o.container == "mp4" && matches!(o.audio_codec.as_str(), "wav" | "flac")
    {
        bail!("FLAC/PCM 音訊請選擇 MKV 容器");
    }
    if o.bitrate > 512 || (o.bitrate > 0 && o.bitrate < 32) {
        bail!("位元率應為 32–512 kbps 或來源預設");
    }
    if o.output_dir.trim().is_empty() {
        bail!("請選擇儲存位置");
    }
    if let (Some(start), Some(end)) = (o.section_start, o.section_end) {
        if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
            bail!("裁剪結束時間必須大於開始時間");
        }
    } else if o.section_start.is_some() || o.section_end.is_some() {
        bail!("裁剪必須同時設定開始與結束時間");
    }
    Ok(())
}

pub fn download_args(task: &ProviderTask) -> Result<Vec<String>> {
    let o = &task.options;
    validate_options(o)?;
    let mut args = vec![
        "--no-playlist".into(),
        "--continue".into(),
        "--no-overwrites".into(),
        "--newline".into(),
        "--progress".into(),
        "--progress-delta".into(),
        "1".into(),
        "--no-simulate".into(),
        "--windows-filenames".into(),
    ];
    args.extend([
        "--progress-template".into(),
        "download:CONEST_PROGRESS:%(progress)j".into(),
        "--progress-template".into(),
        "postprocess:CONEST_PROCESS:%(progress)j".into(),
        "--print".into(),
        "after_move:CONEST_FILE:%(filepath)j".into(),
    ]);
    let filter = if o.height > 0 {
        format!("[height<={}]", o.height)
    } else {
        String::new()
    };
    let format = if o.mode == "audio" {
        "bestaudio/best".into()
    } else {
        format!("bestvideo{filter}+bestaudio/best{filter}")
    };
    args.extend([
        "--format".into(),
        format,
        "--paths".into(),
        o.output_dir.clone(),
        "--output".into(),
        format!("%(title).180B-%(id)s-{}.%(ext)s", task.id),
    ]);
    if o.mode == "audio" {
        args.push("--extract-audio".into());
        args.extend([
            "--audio-format".into(),
            if o.audio_codec == "original" {
                "best".into()
            } else {
                o.audio_codec.clone()
            },
        ]);
        if o.bitrate > 0 && !matches!(o.audio_codec.as_str(), "flac" | "wav" | "original") {
            args.extend(["--audio-quality".into(), format!("{}K", o.bitrate)]);
        }
    } else {
        // Download to a permissive intermediate before explicit conversion.
        // A source H.264 stream cannot be remuxed into WebM before VP9 encoding.
        let intermediate = if o.video_codec != "original" || o.audio_codec != "original" {
            "mkv"
        } else {
            &o.container
        };
        args.extend([
            "--merge-output-format".into(),
            intermediate.into(),
            "--remux-video".into(),
            intermediate.into(),
        ]);
    }
    if !o.subtitles.is_empty() {
        args.extend([
            "--write-subs".into(),
            "--sub-langs".into(),
            o.subtitles.clone(),
        ]);
        if o.auto_subtitles {
            args.push("--write-auto-subs".into());
        }
        if o.embed_subtitles {
            args.push("--embed-subs".into());
        }
    }
    if o.thumbnail {
        args.push("--write-thumbnail".into());
    }
    if o.metadata {
        args.push("--embed-metadata".into());
    }
    if o.sponsorblock {
        args.extend(["--sponsorblock-remove".into(), "sponsor".into()]);
    }
    if let (Some(start), Some(end)) = (o.section_start, o.section_end) {
        args.extend([
            "--download-sections".into(),
            format!("*{start}-{end}"),
            "--force-keyframes-at-cuts".into(),
        ]);
    }
    Ok(args)
}

async fn stop_process_tree(child: &mut tokio::process::Child) {
    if let Some(pid) = child.id() {
        #[cfg(windows)]
        {
            let _ = command("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .output()
                .await;
        }
        #[cfg(unix)]
        {
            let _ = command("kill")
                .args(["-TERM", "--", &format!("-{pid}")])
                .output()
                .await;
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

async fn run_process(
    id: &str,
    mut cmd: Command,
    cancel: &CancellationToken,
    find_output: bool,
) -> Result<String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("無法啟動執行檔，請檢查引擎與路徑")?;
    let stdout = child.stdout.take().context("Missing stdout")?;
    let stderr = child.stderr.take().context("Missing stderr")?;
    let mut out = BufReader::new(stdout).lines();
    let mut err = BufReader::new(stderr).lines();
    let (mut out_end, mut err_end) = (false, false);
    let mut file = String::new();
    loop {
        if out_end && err_end {
            break;
        }
        tokio::select! {
            _ = cancel.cancelled() => { stop_process_tree(&mut child).await; bail!("已停止"); }
            line = out.next_line(), if !out_end => {
                match line? {
                    None => out_end = true,
                    Some(line) => {
                        process_line(id, &line, find_output, &mut file).await;
                    }
                }
            }
            line = err.next_line(), if !err_end => { match line? { None => err_end = true, Some(line) => process_line(id, &line, find_output, &mut file).await } }
        }
    }
    let status = tokio::select! { _ = cancel.cancelled() => { stop_process_tree(&mut child).await; bail!("已停止"); }, result = child.wait() => result? };
    if !status.success() {
        let text = TASKS
            .lock()
            .await
            .tasks
            .get(id)
            .map(|t| {
                t.log
                    .iter()
                    .rev()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        bail!("程序失敗（{}）：{}", status, text);
    }
    Ok(file)
}

async fn process_line(id: &str, line: &str, find_output: bool, file: &mut String) {
    if let Some(raw) = line.strip_prefix("CONEST_FILE:") {
        if find_output {
            *file = serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_string());
        }
    } else if let Some(raw) = line.strip_prefix("CONEST_PROGRESS:") {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            update(id, |task| {
                let bytes = v["downloaded_bytes"].as_f64().unwrap_or(0.0);
                let total = v["total_bytes"]
                    .as_f64()
                    .or_else(|| v["total_bytes_estimate"].as_f64())
                    .unwrap_or(0.0);
                task.progress = if total > 0.0 {
                    (bytes / total * 100.0).clamp(0.0, 100.0)
                } else {
                    task.progress
                };
                task.speed = v["speed"].as_f64();
                task.eta = v["eta"].as_f64();
                task.phase = if v["status"] == "finished" {
                    "正在合併或後處理"
                } else {
                    "下載中"
                }
                .into();
            })
            .await;
        }
    } else if line.starts_with("CONEST_PROCESS:") {
        update(id, |task| {
            task.state = "processing".into();
            task.phase = "合併或後處理中".into();
        })
        .await;
    } else {
        add_log(id, &line).await;
    }
}

async fn add_log(id: &str, line: &str) {
    // Never persist cookie content or proxy credentials. Command arguments are
    // not logged, and URL tokens are omitted from diagnostic output.
    let cleaned = regex::Regex::new(r"https?://\S+")
        .map(|r| r.replace_all(line, "[URL]").to_string())
        .unwrap_or_default();
    update(id, |task| {
        task.log.push(cleaned);
        if task.log.len() > 120 {
            task.log.remove(0);
        }
    })
    .await;
}

pub fn video_encoder(codec: &str, accelerator: &str) -> Result<&'static str> {
    let result = match (codec, accelerator) {
        ("h264", "cpu") => "libx264",
        ("h265", "cpu") => "libx265",
        ("vp9", "cpu") => "libvpx-vp9",
        ("av1", "cpu") => "libaom-av1",
        ("h264", "nvidia") => "h264_nvenc",
        ("h265", "nvidia") => "hevc_nvenc",
        ("av1", "nvidia") => "av1_nvenc",
        ("h264", "intel") => "h264_qsv",
        ("h265", "intel") => "hevc_qsv",
        ("vp9", "intel") => "vp9_qsv",
        ("av1", "intel") => "av1_qsv",
        ("h264", "amd") => "h264_amf",
        ("h265", "amd") => "hevc_amf",
        ("av1", "amd") => "av1_amf",
        ("h264", "apple") => "h264_videotoolbox",
        ("h265", "apple") => "hevc_videotoolbox",
        _ => bail!("此裝置/編碼組合不支援，請選擇 CPU 或其他編碼"),
    };
    Ok(result)
}

async fn transcode(task: &ProviderTask, input: &str, cancel: &CancellationToken) -> Result<String> {
    let o = &task.options;
    if o.mode != "video" || (o.video_codec == "original" && o.audio_codec == "original") {
        return Ok(input.into());
    }
    let ffmpeg = provider_ffmpeg(&task.engine);
    let video = if o.video_codec == "original" {
        "copy"
    } else {
        video_encoder(&o.video_codec, &o.accelerator)?
    };
    if video != "copy" {
        let mut probe = command(&ffmpeg);
        probe.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=64x64:rate=1",
            "-frames:v",
            "1",
            "-c:v",
            video,
            "-f",
            "null",
            "-",
        ]);
        run_process(&task.id, probe, cancel, false)
            .await
            .context("編碼器無法執行；原始下載檔案仍保留，可改選 CPU 後重試")?;
    }
    let audio = match o.audio_codec.as_str() {
        "aac" => "aac",
        "mp3" => "libmp3lame",
        "opus" => "libopus",
        "flac" => "flac",
        "wav" => "pcm_s16le",
        _ => "copy",
    };
    let target = PathBuf::from(input).with_extension(format!("converted.{}", o.container));
    let mut cmd = command(&ffmpeg);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "warning",
        "-nostats",
        "-i",
        input,
        "-map",
        "0:v?",
        "-map",
        "0:a?",
        "-map",
        "0:s?",
        "-c:s",
        if o.container == "mp4" {
            "mov_text"
        } else if o.container == "webm" {
            "webvtt"
        } else {
            "copy"
        },
        "-c:v",
        video,
        "-c:a",
        audio,
    ]);
    if o.metadata {
        cmd.args(["-map_metadata", "0"]);
    }
    if matches!(video, "libx264" | "libx265") {
        cmd.args(["-preset", "medium", "-crf", "22"]);
    }
    if video == "libvpx-vp9" {
        cmd.args(["-crf", "32", "-b:v", "0"]);
    }
    if video == "libaom-av1" {
        cmd.args(["-crf", "30", "-b:v", "0", "-cpu-used", "4"]);
    }
    if o.bitrate > 0 && !matches!(audio, "copy" | "flac" | "pcm_s16le") {
        cmd.args(["-b:a", &format!("{}k", o.bitrate)]);
    }
    if o.container == "mp4" {
        cmd.args(["-movflags", "+faststart"]);
    }
    cmd.arg(&target).arg("-y");
    update(&task.id, |t| {
        t.state = "processing".into();
        t.phase = format!("影音轉碼中 · {}", o.accelerator);
        t.progress = 0.0;
        t.speed = None;
        t.eta = None;
    })
    .await;
    run_process(&task.id, cmd, cancel, false).await?;
    Ok(target.to_string_lossy().into())
}

fn download_rate_limit(value: &serde_json::Number) -> Option<String> {
    // The existing shared setting uses KiB/s, matching aria2's K suffix.
    value
        .as_f64()
        .filter(|rate| *rate > 0.0)
        .map(|_| format!("{value}K"))
}

async fn execute(id: String, cancel: CancellationToken) {
    let result: Result<String> = async {
        let sem = crate::services::queue::runtime::RUNTIME.semaphore.read().await.clone();
        let _permit = tokio::select! { _ = cancel.cancelled() => bail!("已停止"), permit = sem.acquire_owned() => permit? };
        let task = TASKS.lock().await.tasks.get(&id).cloned().context("Task missing")?;
        if cancel.is_cancelled() { bail!("已停止"); }
        tokio::fs::create_dir_all(&task.options.output_dir).await?;
        update(&id, |t| { t.state = "running".into(); t.phase = "解析下載格式".into(); t.error.clear(); }).await;
        save().await?;
        let engine_guard = ENGINE_LOCK.read().await;
        let mut args = common_args(&task.engine)?;
        args.extend(download_args(&task)?);
        if let Some(limit) = download_rate_limit(&config::read().speed_limit) { args.extend(["--limit-rate".into(), limit]); }
        args.extend(["--".into(), task.entry.url.clone()]);
        let mut cmd = command(executable(&task.engine));
        cmd.args(args);
        let output = run_process(&id, cmd, &cancel, true).await?;
        drop(engine_guard);
        if output.is_empty() || !PathBuf::from(&output).is_file() { bail!("下載程序未回報有效輸出檔案；請查看任務日誌"); }
        transcode(&task, &output, &cancel).await
    }.await;
    match result {
        Ok(output) => {
            update_state(
                &id,
                |t| {
                    t.state = "completed".into();
                    t.phase = "已完成".into();
                    t.progress = 100.0;
                    t.output = output;
                    t.speed = None;
                    t.eta = None;
                },
                true,
            )
            .await
        }
        Err(error) => {
            update_state(
                &id,
                |t| {
                    t.state = if cancel.is_cancelled() {
                        "stopped"
                    } else {
                        "failed"
                    }
                    .into();
                    t.phase = if cancel.is_cancelled() {
                        "已停止，可重試續傳"
                    } else {
                        "失敗"
                    }
                    .into();
                    t.error = error.to_string();
                    t.speed = None;
                    t.eta = None;
                },
                true,
            )
            .await
        }
    }
    TASKS.lock().await.running.remove(&id);
    if let Err(error) = save().await {
        log::error!("yt-dlp task persistence failed: {error:#}");
    }
}

#[tauri::command(async)]
#[specta::specta]
pub async fn ytdlp_submit(
    entries: Vec<MediaEntry>,
    options: DownloadOptions,
    settings: EngineSettings,
) -> TauriResult<Vec<ProviderTask>> {
    load().await?;
    validate_options(&options)?;
    if entries.is_empty() {
        return Err(anyhow!("請選擇下載項目").into());
    }
    for e in &entries {
        valid_url(&e.url)?;
        if !e.available {
            return Err(anyhow!("項目無法下載：{}", e.title).into());
        }
    }
    let tasks: Vec<_> = entries
        .into_iter()
        .map(|entry| ProviderTask {
            id: format!("yt-{}", random_string(12)),
            entry,
            options: options.clone(),
            engine: settings.clone(),
            state: "backlog".into(),
            phase: "待開始".into(),
            progress: 0.0,
            speed: None,
            eta: None,
            error: String::new(),
            log: vec![],
            output: String::new(),
        })
        .collect();
    {
        let mut state = TASKS.lock().await;
        for task in &tasks {
            state.tasks.insert(task.id.clone(), task.clone());
        }
    }
    save().await?;
    if config::read().auto_download {
        for task in &tasks {
            start(&task.id).await?;
        }
    }
    Ok(tasks)
}

async fn start(id: &str) -> Result<()> {
    let cancel = CancellationToken::new();
    {
        let mut state = TASKS.lock().await;
        if state.running.contains_key(id) {
            bail!("任務已在執行或等待中");
        }
        let task = state.tasks.get_mut(id).context("找不到任務")?;
        if task.state == "completed" {
            bail!("已完成任務不可重複開始");
        }
        task.state = "queued".into();
        task.phase = "等待共用下載位置".into();
        task.error.clear();
        state.running.insert(id.into(), cancel.clone());
    }
    update(id, |_| {}).await;
    if let Err(error) = save().await {
        TASKS.lock().await.running.remove(id);
        update(id, |task| {
            task.state = "failed".into();
            task.error = error.to_string();
        })
        .await;
        return Err(error);
    }
    let owned = id.to_string();
    tauri::async_runtime::spawn(async move {
        execute(owned, cancel).await;
    });
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn ytdlp_control(id: String, action: String) -> TauriResult<()> {
    load().await?;
    match action.as_str() {
        "start" | "retry" => start(&id).await?,
        "stop" => {
            let state = TASKS.lock().await;
            state.running.get(&id).context("任務沒有在執行")?.cancel();
        }
        "remove" => {
            let mut state = TASKS.lock().await;
            if state.running.contains_key(&id) {
                return Err(anyhow!("請先停止任務").into());
            }
            state.tasks.remove(&id);
        }
        "open" => {
            let state = TASKS.lock().await;
            let task = state.tasks.get(&id).context("找不到任務")?;
            tauri_plugin_opener::open_path(&task.options.output_dir, None::<&str>)?;
        }
        _ => return Err(anyhow!("未知任務動作").into()),
    }
    save().await?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn ytdlp_edit(
    id: String,
    options: DownloadOptions,
    settings: EngineSettings,
) -> TauriResult<ProviderTask> {
    load().await?;
    validate_options(&options)?;
    let task = {
        let mut state = TASKS.lock().await;
        if state.running.contains_key(&id) {
            return Err(anyhow!("請先停止任務再修改設定").into());
        }
        let task = state.tasks.get_mut(&id).context("找不到任務")?;
        if task.state == "completed" {
            return Err(anyhow!("已完成任務的設定不可修改").into());
        }
        task.options = options;
        task.engine = settings;
        task.clone()
    };
    save().await?;
    update(&id, |_| {}).await;
    Ok(task)
}

#[tauri::command(async)]
#[specta::specta]
pub async fn ytdlp_tasks() -> TauriResult<Vec<ProviderTask>> {
    load().await?;
    Ok(TASKS.lock().await.tasks.values().cloned().collect())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn ytdlp_settings(settings: Option<EngineSettings>) -> TauriResult<EngineSettings> {
    let path = settings_path();
    if let Some(value) = settings {
        tokio::fs::create_dir_all(path.parent().context("Missing settings directory")?).await?;
        tokio::fs::write(path, serde_json::to_vec_pretty(&value)?).await?;
        return Ok(value);
    }
    if path.exists() {
        Ok(serde_json::from_slice(&tokio::fs::read(path).await?)?)
    } else {
        Ok(EngineSettings::default())
    }
}

#[tauri::command(async)]
#[specta::specta]
pub async fn ytdlp_engine_info(settings: EngineSettings) -> TauriResult<String> {
    let output = command(executable(&settings))
        .arg("--version")
        .output()
        .await
        .context("找不到 yt-dlp，請在下載引擎設定選擇執行檔")?;
    if !output.status.success() {
        return Err(anyhow!("yt-dlp 無法執行").into());
    }
    Ok(format!(
        "yt-dlp {} · {}",
        String::from_utf8_lossy(&output.stdout).trim(),
        executable(&settings).display()
    ))
}

#[tauri::command(async)]
#[specta::specta]
pub async fn ytdlp_update(settings: EngineSettings) -> TauriResult<String> {
    let _guard = ENGINE_LOCK
        .try_write()
        .map_err(|_| anyhow!("引擎正在解析或下載，請完成或停止後再更新"))?;
    let mut path = executable(&settings);
    if settings.executable.is_empty() && path.is_file() {
        let cached = WORKING_PATH.join("Providers/bin").join(if cfg!(windows) {
            "yt-dlp.exe"
        } else {
            "yt-dlp"
        });
        if path != cached {
            tokio::fs::create_dir_all(cached.parent().context("Missing engine directory")?).await?;
            tokio::fs::copy(&path, &cached).await?;
            path = cached;
        }
    }
    let output = command(path)
        .args(["--update-to", "stable"])
        .output()
        .await?;
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(anyhow!("引擎更新失敗：{report}").into());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn opts() -> DownloadOptions {
        DownloadOptions {
            mode: "video".into(),
            height: 1080,
            container: "mkv".into(),
            video_codec: "original".into(),
            audio_codec: "original".into(),
            bitrate: 0,
            accelerator: "cpu".into(),
            output_dir: "downloads".into(),
            subtitles: String::new(),
            auto_subtitles: false,
            embed_subtitles: false,
            thumbnail: false,
            metadata: false,
            sponsorblock: false,
            section_start: None,
            section_end: None,
        }
    }
    #[test]
    fn shared_speed_limit_retains_kibibytes_per_second() {
        assert_eq!(download_rate_limit(&0.into()), None);
        assert_eq!(download_rate_limit(&128.into()), Some("128K".into()));
        assert_eq!(
            download_rate_limit(&serde_json::Number::from_f64(0.5).unwrap()),
            Some("0.5K".into())
        );
    }

    #[tokio::test]
    async fn cancellation_terminates_a_running_subprocess() {
        let Ok(ffmpeg) = std::env::var("CONEST_TEST_FFMPEG") else {
            return;
        };
        let mut cmd = command(ffmpeg);
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "color=size=64x64:rate=1",
            "-t",
            "30",
            "-f",
            "null",
            "-",
        ]);
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            stop.cancel();
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_process("cancellation-test", cmd, &cancel, false),
        )
        .await
        .expect("Cancelled child must terminate promptly");
        assert!(result.unwrap_err().to_string().contains("已停止"));
    }

    #[test]
    fn unavailable_playlist_entries_keep_positions() {
        let v = serde_json::json!({"title":"list","entries":[{"id":"a","url":"https://example.com/a","title":"A"},null,{"id":"b","url":"https://example.com/b","title":"B"}]});
        let c = parse_collection(&v, "https://example.com/list", String::new());
        assert_eq!(c.entries.len(), 3);
        assert!(!c.entries[1].available);
        assert_eq!(c.entries[2].index, 3);
    }
    #[test]
    fn reject_incompatible_container_and_clip() {
        let mut o = opts();
        o.container = "webm".into();
        o.video_codec = "h264".into();
        assert!(validate_options(&o).is_err());
        o = opts();
        o.section_start = Some(10.0);
        o.section_end = Some(5.0);
        assert!(validate_options(&o).is_err());
    }
    #[test]
    fn cpu_and_gpu_encode_are_explicit() {
        assert_eq!(video_encoder("h265", "cpu").unwrap(), "libx265");
        assert_eq!(video_encoder("h265", "nvidia").unwrap(), "hevc_nvenc");
        assert!(video_encoder("vp9", "nvidia").is_err());
    }
    #[test]
    fn max_resolution_is_not_silently_removed() {
        let task = ProviderTask {
            id: "id".into(),
            entry: entry(
                &serde_json::json!({"url":"https://example.com/video"}),
                1,
                "",
            ),
            options: opts(),
            engine: EngineSettings::default(),
            state: String::new(),
            phase: String::new(),
            progress: 0.0,
            speed: None,
            eta: None,
            error: String::new(),
            log: vec![],
            output: String::new(),
        };
        let a = download_args(&task).unwrap();
        let f = &a[a.iter().position(|s| s == "--format").unwrap() + 1];
        assert_eq!(f, "bestvideo[height<=1080]+bestaudio/best[height<=1080]");
    }
}
