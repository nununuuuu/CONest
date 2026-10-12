use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    thread,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use tauri::Emitter;

use crate::{
    shared::get_app_handle,
    services::queue::{
        atomics::TaskState,
        manager::{clear_completed, MANAGER},
        runtime::{CtrlEvent, RUNTIME},
        task::{SubTask, SubTaskStatus, TaskPrepare, TaskType},
        types::{MediaNfoThumb, PopupSelect},
    },
    storage::tasks,
};

pub const LOCAL_API_HOST: &str = "127.0.0.1";
pub const LOCAL_API_PORT_START: u16 = 17345;
pub const LOCAL_API_PORT_END: u16 = 17349;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ExtensionRequest {
    pub urls: Vec<String>,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default)]
    pub browser_cookie: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub tab_id: Option<i64>,
}

fn default_scope() -> String { "current".into() }

static EXTENSION_FRONTEND_READY: AtomicBool = AtomicBool::new(false);

fn extension_task_options() -> &'static Mutex<HashMap<String, Value>> {
    static OPTIONS: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();
    OPTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[tauri::command]
#[specta::specta]
pub fn set_extension_frontend_ready(ready: bool) -> crate::errors::TauriResult<()> {
    EXTENSION_FRONTEND_READY.store(ready, Ordering::Release);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_extension_task_options(
    id: String,
    options: Value,
) -> crate::errors::TauriResult<()> {
    extension_task_options()
        .lock()
        .map_err(|_| anyhow::anyhow!("extension task options are poisoned"))?
        .insert(id, options);
    Ok(())
}

fn pending_requests() -> &'static Mutex<VecDeque<ExtensionRequest>> {
    static PENDING: OnceLock<Mutex<VecDeque<ExtensionRequest>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn recent_request_ids() -> &'static Mutex<VecDeque<String>> {
    static RECENT: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
    RECENT.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn remember_request_id(id: &str) -> Result<bool> {
    if id.is_empty() {
        return Ok(true);
    }

    let mut recent = recent_request_ids()
        .lock()
        .map_err(|_| anyhow::anyhow!("extension request id queue is poisoned"))?;
    if recent.iter().any(|known| known == id) {
        return Ok(false);
    }

    recent.push_back(id.to_string());
    while recent.len() > 512 {
        recent.pop_front();
    }
    Ok(true)
}

#[tauri::command]
#[specta::specta]
pub fn take_extension_requests() -> crate::errors::TauriResult<Vec<ExtensionRequest>> {
    let mut queue = pending_requests()
        .lock()
        .map_err(|_| anyhow::anyhow!("extension request queue is poisoned"))?;
    Ok(queue.drain(..).collect())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExtensionSettingsRequest {
    id: String,
    select: PopupSelect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExtensionConfigureRequest {
    id: String,
    #[serde(default)]
    browser_cookie: Option<String>,
}

#[derive(Clone, Debug, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionSettingsResult {
    select: PopupSelect,
    subtasks: Vec<SubTask>,
    status: HashMap<String, SubTaskStatus>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionTask {
    id: String,
    title: String,
    cover: String,
    media_type: String,
    state: crate::services::queue::atomics::TaskState,
    progress: f64,
    ts: u64,
    select: PopupSelect,
    thumb_options: Vec<MediaNfoThumb>,
    settings_options: Option<Value>,
}

async fn task_snapshot() -> Vec<ExtensionTask> {
    // Clone the Arc<Task> handles first, then release MANAGER.tasks before
    // awaiting individual task locks. This avoids nested block_on calls and
    // prevents /api/tasks from failing while the app is running.
    let task_list: Vec<_> = {
        let tasks = MANAGER.tasks.read().await;
        tasks.values().cloned().collect()
    };

    let task_ids: std::collections::HashSet<String> =
        task_list.iter().map(|task| task.id.clone()).collect();
    let options_snapshot = extension_task_options()
        .lock()
        .map(|mut options| {
            options.retain(|id, _| task_ids.contains(id));
            options.clone()
        })
        .unwrap_or_default();

    let mut items = Vec::with_capacity(task_list.len());
    for task in task_list {
        let state = task.state.get();
        let progress = {
            let subtasks = task.subtasks.read().await.clone();
            let status = task.status.read().await;
            if subtasks.is_empty() {
                0.0
            } else {
                let sum: f64 = subtasks
                    .iter()
                    .map(|subtask| {
                        let Some(v) = status.get(&subtask.id) else {
                            return 0.0;
                        };
                        if v.content == 0 {
                            0.0
                        } else {
                            (v.chunk as f64 / v.content as f64).clamp(0.0, 1.0)
                        }
                    })
                    .sum();
                (sum / subtasks.len() as f64 * 100.0).clamp(0.0, 100.0)
            }
        };

        let select = task.select.read().await.clone();
        let thumb_options = task.nfo.read().await.thumbs.clone();

        items.push(ExtensionTask {
            id: task.id.clone(),
            title: task.item.title.clone(),
            cover: task.item.cover.clone(),
            media_type: task.media_type.clone(),
            state,
            progress,
            ts: task.ts,
            select,
            thumb_options,
            settings_options: options_snapshot.get(&task.id).cloned(),
        });
    }

    items.sort_by(|a, b| b.ts.cmp(&a.ts));
    items.truncate(50);
    items
}

fn extension_subtasks(id: &str, select: &PopupSelect) -> Vec<SubTask> {
    // Mirror src/services/queue.ts::selectToSubTasks() so editing from the
    // extension preserves the same resources and ordering as CONest itself.
    let mut types = Vec::new();

    if select.misc.opus_content {
        types.push(TaskType::OpusContent);
    }
    if select.misc.opus_images {
        types.push(TaskType::OpusImages);
    }
    if select.misc.ai_summary {
        types.push(TaskType::AiSummary);
    }
    if select.misc.subtitles.as_str().is_some() {
        types.push(TaskType::Subtitles);
    }
    if select.nfo.album {
        types.push(TaskType::AlbumNfo);
    }
    if select.nfo.single {
        types.push(TaskType::SingleNfo);
    }
    if select.danmaku.live {
        types.push(TaskType::LiveDanmaku);
    }
    if select.danmaku.history.as_str().is_some() {
        types.push(TaskType::HistoryDanmaku);
    }
    if !select.thumb.is_empty() {
        types.push(TaskType::Thumb);
    }
    if select.media.video || select.media.audio_video {
        types.push(TaskType::Video);
    }
    if select.media.audio || select.media.audio_video {
        types.push(TaskType::Audio);
    }
    if select.media.audio_video {
        types.push(TaskType::AudioVideo);
    }

    types
        .into_iter()
        .enumerate()
        .map(|(idx, task_type)| {
            SubTask::new(format!("{id}ext{idx:02}"), task_type)
        })
        .collect()
}

async fn update_task_settings(req: ExtensionSettingsRequest) -> Result<ExtensionSettingsResult> {
    let task = MANAGER.get_task(&req.id).await?;
    if task.state.get() != TaskState::Backlog {
        anyhow::bail!("only backlog tasks can be edited");
    }

    let subtasks = extension_subtasks(&req.id, &req.select);
    if subtasks.is_empty() {
        anyhow::bail!("at least one download item must be selected");
    }

    let mut status_map = HashMap::new();
    for subtask in &subtasks {
        status_map.insert(
            subtask.id.clone(),
            SubTaskStatus {
                content: 0,
                chunk: 0,
            },
        );
    }

    *task.select.write().await = req.select.clone();
    *task.subtasks.write().await = subtasks.clone();
    *task.status.write().await = status_map.clone();

    let prepare = TaskPrepare {
        select: req.select,
        subtasks,
        nfo: task.nfo.read().await.clone(),
        folder: task.folder.read().await.clone(),
    };
    tasks::update_prepare(&req.id, &prepare).await?;
    tasks::update_status(&req.id, &status_map).await?;

    Ok(ExtensionSettingsResult {
        select: prepare.select,
        subtasks: prepare.subtasks,
        status: status_map,
    })
}

#[tauri::command(async)]
#[specta::specta]
pub async fn update_extension_task_settings(
    id: String,
    select: PopupSelect,
) -> crate::errors::TauriResult<ExtensionSettingsResult> {
    Ok(update_task_settings(ExtensionSettingsRequest { id, select }).await?)
}


fn response(stream: &mut TcpStream, status: &str, body: &str) -> Result<()> {
    let payload = format!(
        "HTTP/1.1 {status}\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Headers: Content-Type, X-CONest-Extension\r\n\
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Private-Network: true\r\n\
Cache-Control: no-store\r\n\
Content-Type: application/json; charset=utf-8\r\n\
Content-Length: {}\r\n\
Connection: close\r\n\r\n{}",
        body.as_bytes().len(), body
    );
    stream.write_all(payload.as_bytes())?;
    Ok(())
}

fn handle(mut stream: TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;

    // Read until the complete HTTP body has arrived. A single TCP read is not
    // guaranteed to contain headers + JSON, even for small extension requests.
    let mut buffer = Vec::with_capacity(8192);
    let mut header_end: Option<usize> = None;
    let mut expected_total: Option<usize> = None;
    loop {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk)?;
        if n == 0 { break; }
        buffer.extend_from_slice(&chunk[..n]);
        if buffer.len() > 128 * 1024 {
            return response(&mut stream, "413 Payload Too Large", r#"{"ok":false,"error":"request too large"}"#);
        }
        if header_end.is_none() {
            if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                let end = pos + 4;
                header_end = Some(end);
                let head = String::from_utf8_lossy(&buffer[..pos]);
                let content_len = head.lines()
                    .find_map(|line| line.split_once(':'))
                    .filter(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                expected_total = Some(end + content_len);
            }
        }
        if expected_total.is_some_and(|total| buffer.len() >= total) { break; }
    }

    let header_end = header_end.unwrap_or(buffer.len());
    let head = String::from_utf8_lossy(&buffer[..header_end.min(buffer.len())]);
    let body = if header_end <= buffer.len() { &buffer[header_end..] } else { &[] };
    let first = head.lines().next().unwrap_or("");

    if first.starts_with("OPTIONS ") {
        return response(&mut stream, "204 No Content", "");
    }

    let bridge_header = head.lines().find_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.trim().eq_ignore_ascii_case("x-conest-extension"))
        .map(|(_, value)| value.trim());
    let bridge_client = first.contains("client=conest-extension");
    let bridge_allowed = bridge_header == Some("1") || bridge_client;

    if first.starts_with("GET /health") {
        return response(
            &mut stream,
            "200 OK",
            &json!({
                "ok": true,
                "app": "CONest",
                "ready": EXTENSION_FRONTEND_READY.load(Ordering::Acquire)
            }).to_string(),
        );
    }

    if first.starts_with("POST /api/native-register") {
        if !bridge_allowed {
            return response(&mut stream, "403 Forbidden", r#"{"ok":false,"error":"extension client marker required"}"#);
        }

        return match crate::native_messaging::register() {
            Ok(()) => response(
                &mut stream,
                "200 OK",
                r#"{"ok":true,"registered":true}"#,
            ),
            Err(e) => response(
                &mut stream,
                "500 Internal Server Error",
                &json!({
                    "ok": false,
                    "error": format!("Native Messaging registration failed: {e:#}")
                })
                .to_string(),
            ),
        };
    }

    if first.starts_with("GET /api/tasks") {
        if !bridge_allowed {
            return response(&mut stream, "403 Forbidden", r#"{"ok":false,"error":"extension client marker required"}"#);
        }
        let tasks = tauri::async_runtime::block_on(task_snapshot());
        return response(&mut stream, "200 OK", &json!({ "ok": true, "tasks": tasks }).to_string());
    }

    if first.starts_with("POST /api/clear-completed") {
        if !bridge_allowed {
            return response(&mut stream, "403 Forbidden", r#"{"ok":false,"error":"extension client marker required"}"#);
        }

        return match tauri::async_runtime::block_on(clear_completed()) {
            Ok(()) => response(&mut stream, "200 OK", r#"{"ok":true,"cleared":true}"#),
            Err(e) => response(
                &mut stream,
                "500 Internal Server Error",
                &json!({ "ok": false, "error": e.to_string() }).to_string(),
            ),
        };
    }

    if first.starts_with("POST /api/remove") {
        if !bridge_allowed {
            return response(&mut stream, "403 Forbidden", r#"{"ok":false,"error":"extension client marker required"}"#);
        }

        let id = first
            .split_whitespace()
            .nth(1)
            .and_then(|path| path.split('?').nth(1))
            .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("id=")))
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            return response(&mut stream, "400 Bad Request", r#"{"ok":false,"error":"missing task id"}"#);
        }

        let result = tauri::async_runtime::block_on(async {
            let task = MANAGER.get_task(&id).await?;
            match task.state.get() {
                TaskState::Backlog => {
                    // Same path as CONest's trash button for a backlog task.
                    task.cancel_backlog().await
                }
                TaskState::Completed | TaskState::Failed => {
                    // Match DownPage/Task.vue -> ctrlEvent(Cancel, sid, id).
                    // Find the scheduler that owns this task, then use the same
                    // runtime cancellation/removal path as CONest itself.
                    let schedulers: Vec<_> = {
                        let schedulers = MANAGER.schedulers.read().await;
                        schedulers.values().cloned().collect()
                    };

                    let mut owner_sid = None;
                    for scheduler in schedulers {
                        if scheduler
                            .list
                            .read()
                            .await
                            .iter()
                            .any(|task_id| task_id == &id)
                        {
                            owner_sid = Some(scheduler.sid.clone());
                            break;
                        }
                    }

                    let sid = owner_sid
                        .ok_or_else(|| anyhow::anyhow!("task scheduler not found"))?;
                    RUNTIME
                        .ctrl
                        .send_task(
                            Some(&sid),
                            &id,
                            &CtrlEvent::Cancel,
                            TaskState::Cancelled,
                        )
                        .await
                }
                _ => anyhow::bail!(
                    "only backlog, completed or failed tasks can be removed from the extension"
                ),
            }
        });

        return match result {
            Ok(()) => {
                if let Ok(mut options) = extension_task_options().lock() {
                    options.remove(&id);
                }
                response(&mut stream, "200 OK", r#"{"ok":true,"removed":true}"#)
            }
            Err(e) => response(
                &mut stream,
                "400 Bad Request",
                &json!({ "ok": false, "error": e.to_string() }).to_string(),
            ),
        };
    }

    if first.starts_with("POST /api/configure") {
        if !bridge_allowed {
            return response(&mut stream, "403 Forbidden", r#"{"ok":false,"error":"extension client marker required"}"#);
        }

        let req: ExtensionConfigureRequest = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(e) => {
                return response(
                    &mut stream,
                    "400 Bad Request",
                    &json!({
                        "ok": false,
                        "error": format!("invalid configure json: {e}")
                    })
                    .to_string(),
                );
            }
        };

        if req.id.is_empty() {
            return response(&mut stream, "400 Bad Request", r#"{"ok":false,"error":"missing task id"}"#);
        }

        let editable = tauri::async_runtime::block_on(async {
            MANAGER
                .get_task(&req.id)
                .await
                .map(|task| task.state.get() == TaskState::Backlog)
        });
        match editable {
            Ok(true) => {}
            Ok(false) => {
                return response(&mut stream, "400 Bad Request", r#"{"ok":false,"error":"only backlog tasks can be edited"}"#);
            }
            Err(e) => {
                return response(
                    &mut stream,
                    "404 Not Found",
                    &json!({ "ok": false, "error": e.to_string() }).to_string(),
                );
            }
        }

        let app = get_app_handle();
        app.emit("extension-configure-task", req)
            .context("Failed to emit extension configure request")?;
        return response(&mut stream, "202 Accepted", r#"{"ok":true}"#);
    }

    if first.starts_with("POST /api/start") {
        if !bridge_allowed {
            return response(&mut stream, "403 Forbidden", r#"{"ok":false,"error":"extension client marker required"}"#);
        }
        let id = first
            .split_whitespace()
            .nth(1)
            .and_then(|path| path.split('?').nth(1))
            .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("id=")))
            .unwrap_or_default()
            .to_string();
        if id.is_empty() {
            return response(&mut stream, "400 Bad Request", r#"{"ok":false,"error":"missing task id"}"#);
        }
        get_app_handle()
            .emit("extension-start-task", id)
            .context("Failed to emit extension start request")?;
        return response(&mut stream, "202 Accepted", r#"{"ok":true}"#);
    }

    if first.starts_with("POST /api/task-settings") {
        if !bridge_allowed {
            return response(&mut stream, "403 Forbidden", r#"{"ok":false,"error":"extension client marker required"}"#);
        }

        let req: ExtensionSettingsRequest = match serde_json::from_slice(body) {
            Ok(v) => v,
            Err(e) => {
                return response(&mut stream, "400 Bad Request", &json!({
                    "ok": false,
                    "error": format!("invalid settings json: {e}")
                }).to_string());
            }
        };

        match tauri::async_runtime::block_on(update_task_settings(req)) {
            Ok(_) => return response(&mut stream, "200 OK", r#"{"ok":true}"#),
            Err(e) => {
                return response(&mut stream, "400 Bad Request", &json!({
                    "ok": false,
                    "error": e.to_string()
                }).to_string());
            }
        }
    }

    if !first.starts_with("POST /api/download") {
        return response(&mut stream, "404 Not Found", r#"{"ok":false,"error":"not found"}"#);
    }

    if !bridge_allowed {
        return response(&mut stream, "403 Forbidden", r#"{"ok":false,"error":"extension client marker required"}"#);
    }

    let req: ExtensionRequest = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            return response(&mut stream, "400 Bad Request", &serde_json::json!({
                "ok": false, "error": format!("invalid json: {e}")
            }).to_string());
        }
    };
    if req.urls.is_empty() || req.urls.len() > 200 {
        return response(&mut stream, "400 Bad Request", r#"{"ok":false,"error":"urls must contain 1-200 items"}"#);
    }
    if req.urls.iter().any(|u| !u.starts_with("https://www.bilibili.com/") && !u.starts_with("https://b23.tv/")) {
        return response(&mut stream, "400 Bad Request", r#"{"ok":false,"error":"unsupported url"}"#);
    }

    if let Some(request_id) = req.request_id.as_deref() {
        if !remember_request_id(request_id)? {
            return response(
                &mut stream,
                "200 OK",
                r#"{"ok":true,"queued":true,"duplicate":true}"#,
            );
        }
    }

    {
        let mut queue = pending_requests()
            .lock()
            .map_err(|_| anyhow::anyhow!("extension request queue is poisoned"))?;
        queue.push_back(req);
    }

    // This event is only a wake-up signal. The request itself stays in the
    // Rust queue until App.vue explicitly drains it, so startup timing cannot
    // lose the user's first click.
    let _ = get_app_handle().emit("extension-request-pending", ());
    response(&mut stream, "202 Accepted", r#"{"ok":true,"queued":true}"#)
}

pub fn init() -> Result<()> {
    let mut bound = None;
    let mut last_error = None;

    for port in LOCAL_API_PORT_START..=LOCAL_API_PORT_END {
        let addr = format!("{LOCAL_API_HOST}:{port}");
        match TcpListener::bind(&addr) {
            Ok(listener) => {
                bound = Some((listener, addr));
                break;
            }
            Err(error) => {
                log::warn!("Extension API bind failed at {addr}: {error}");
                last_error = Some(error);
            }
        }
    }

    let (listener, addr) = bound.ok_or_else(|| {
        anyhow::anyhow!(
            "Failed to bind local extension API on ports {}-{}: {}",
            LOCAL_API_PORT_START,
            LOCAL_API_PORT_END,
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown error".into())
        )
    })?;

    thread::Builder::new()
        .name("conest-extension-api".into())
        .spawn(move || {
            for incoming in listener.incoming() {
                match incoming {
                    Ok(stream) => {
                        if let Err(e) = handle(stream) {
                            log::warn!("Extension API request failed: {e:#}");
                        }
                    }
                    Err(e) => log::warn!("Extension API accept failed: {e}"),
                }
            }
        })?;

    log::info!("Extension API listening on http://{addr}");
    Ok(())
}
