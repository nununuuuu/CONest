// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

use serde::Serialize;
use specta::Type;
use std::{collections::HashMap, env, path::PathBuf, sync::Arc};
use tauri::{async_runtime, Emitter};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_updater::UpdaterExt;
use tokio::fs;

// Re-export for lib.rs to register commands
pub use crate::{
    errors::{TauriError, TauriResult},
    services::{
        self, aria2c, ffmpeg,
        login::{
            self, exit, pwd_login, refresh_cookie, scan_login, sms_login, stop_login, switch_cookie,
        },
        queue::{
            self,
            atomics::QueueType,
            ctrl_event,
            open_folder,
            plan_scheduler,
            plan_scheduler_task,
            process_scheduler,
            scheduler::SchedulerView,
            // update_max_conc,
            submit_task,
            task::TaskView,
        },
    },
    shared::{self, get_app_handle, set_window, HEADERS, READY},
    storage::{
        self,
        config::{self, CacheKey},
        cookies, db, queue as queues, schedulers, tasks,
    },
};

#[derive(Serialize, Type)]
pub struct InitData {
    version: String,
    hash: String,
    config: Arc<config::Settings>,
    tasks: HashMap<String, TaskView>,
    schedulers: HashMap<String, SchedulerView>,
    queue: HashMap<QueueType, Vec<String>>,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command(async)]
#[specta::specta]
pub async fn install_release_version(
    app: tauri::AppHandle,
    version: String,
) -> TauriResult<()> {
    let version = version.trim().trim_start_matches('v').to_string();
    if version.is_empty() {
        return Err(anyhow::anyhow!("Version is empty").into());
    }

    let tag = format!("v{version}");
    let endpoint = tauri::Url::parse(&format!(
        "https://github.com/nununuuuu/CONest/releases/download/{tag}/latest.json"
    ))?;

    let wanted = version.clone();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])?
        .version_comparator(move |_current, release| release.version.to_string() == wanted)
        .build()?;

    let update = updater
        .check()
        .await?
        .ok_or_else(|| anyhow::anyhow!("Release {tag} has no compatible signed updater package"))?;

    if update.version.trim_start_matches('v') != version {
        return Err(anyhow::anyhow!(
            "Updater manifest version mismatch: expected {}, got {}",
            version,
            update.version
        )
        .into());
    }

    let progress_app = app.clone();
    let progress_version = version.clone();
    let mut downloaded = 0usize;

    update
        .download_and_install(
            move |chunk, total| {
                downloaded = downloaded.saturating_add(chunk);
                let percent = total
                    .filter(|total| *total > 0)
                    .map(|total| (downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
                    .unwrap_or(0.0);
                let _ = progress_app.emit(
                    "version-install-progress",
                    serde_json::json!({
                        "version": progress_version,
                        "downloaded": downloaded,
                        "total": total,
                        "percent": percent
                    }),
                );
            },
            || {},
        )
        .await?;

    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn get_size(key: CacheKey, event: tauri::ipc::Channel<u64>) -> TauriResult<()> {
    let path = config::read().get_cache(&key)?;
    let mut bytes = 0u64;
    let mut count = 0;
    for entry in walkdir::WalkDir::new(&path)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() {
            match fs::metadata(path).await {
                Ok(meta) => {
                    bytes += meta.len();
                    count += 1;
                    if count > 200 {
                        event.send(bytes)?;
                        count = 0;
                    }
                }
                Err(_) => continue,
            }
        }
    }
    event.send(bytes)?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn clean_cache(key: CacheKey) -> TauriResult<()> {
    let path = config::read().get_cache(&key)?;
    if key == CacheKey::Database {
        db::close_db().await?;
        fs::remove_file(&path).await?;
        let app = get_app_handle();
        app.restart();
    }
    let mut entries = fs::read_dir(&path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        async_runtime::spawn(async move {
            let _ = if path.is_dir() {
                fs::remove_dir_all(&path).await
            } else {
                fs::remove_file(&path).await
            };
        });
    }
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn open_cache(key: CacheKey) -> TauriResult<()> {
    let path = config::read().get_cache(&key)?;
    tauri_plugin_opener::open_path(path, None::<&str>)?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn config_write(settings: serde_json::Map<String, serde_json::Value>) -> TauriResult<()> {
    config::write(settings).await?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn db_export(output: PathBuf) -> TauriResult<()> {
    db::export(output).await?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn db_import(app: tauri::AppHandle, input: PathBuf) -> TauriResult<()> {
    db::import(input).await?;
    app.restart();
}

#[tauri::command(async)]
#[specta::specta]
pub async fn export_data(output: PathBuf, data: serde_json::Value) -> TauriResult<()> {
    let json = serde_json::to_string_pretty(&data)?;
    fs::write(output, json.as_bytes()).await?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub async fn meta(app: tauri::AppHandle) -> TauriResult<InitData> {
    // Do not read queue/config tables before migrations and all services are ready.
    shared::wait_bootstrap().await?;
    let version = app.package_info().version.to_string();
    let hash = env!("GIT_HASH").to_string();
    let config = config::read();

    let tasks = tasks::load().await?;
    let schedulers = schedulers::load().await?;
    let queue = queues::load().await?;

    Ok(InitData {
        version,
        hash,
        config,
        tasks,
        schedulers,
        queue,
    })
}

#[tauri::command(async)]
#[specta::specta]
pub async fn init() -> TauriResult<()> {
    if READY.set(()).is_err() {
        #[cfg(not(debug_assertions))]
        return Err(anyhow::anyhow!("403 Forbidden").into());
    }
    login::stop_login();
    login::get_buvid().await?;
    login::get_bili_ticket().await?;
    login::get_uuid().await?;
    HEADERS.refresh().await?;
    Ok(())
}
