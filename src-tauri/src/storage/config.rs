use anyhow::{anyhow, Result};
use sea_query::{
    ColumnDef, Iden, OnConflict, Query, SqliteQueryBuilder, Table, TableCreateStatement,
};
use sea_query_binder::SqlxBinder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use sqlx::Row;
use std::{env, path::{Path, PathBuf}, sync::Arc};
use tauri::Manager;

use super::db::{get_db, TableSpec};
use crate::{
    queue::runtime::RUNTIME,
    shared::{get_app_handle, Sidecar, Theme, WindowEffect, CONFIG},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CacheKey {
    Log,
    Temp,
    Webview,
    Database,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Settings {
    pub add_metadata: bool,
    pub auto_check_update: bool,
    pub auto_download: bool,
    pub auto_start: bool,
    pub block_pcdn: bool,
    pub check_update: bool,
    pub clipboard: bool,
    pub convert: SettingsConvert,
    pub default: SettingsDefault,
    pub down_dir: PathBuf,
    pub drag_search: bool,
    pub format: SettingsFormat,
    pub language: String,
    pub max_conc: usize,
    pub notify: bool,
    pub startup_mode: String,
    pub close_behavior: String,
    pub temp_dir: PathBuf,
    pub theme: Theme,
    pub window_effect: WindowEffect,
    pub organize: SettingsOrganize,
    pub proxy: SettingsProxy,
    pub sidecar: SettingsSidecar,
    pub speed_limit: serde_json::Number,
}

impl Settings {
    pub fn get_cache(&self, key: &CacheKey) -> Result<PathBuf> {
        let app = get_app_handle();
        let path = app.path();
        let result = match key {
            CacheKey::Log => path.app_log_dir()?,
            CacheKey::Temp => self.temp_dir(),
            CacheKey::Webview => match std::env::consts::OS {
                "macos" => path
                    .app_cache_dir()?
                    .join("../WebKit/CONest/WebsiteData"),
                "linux" => path.app_cache_dir()?.join("conest"),
                _ => path.app_local_data_dir()?.join("EBWebView"), // windows
            },
            CacheKey::Database => path.app_data_dir()?.join("Storage"),
        };
        Ok(result)
    }
    pub fn sidecar(&self, key: Sidecar) -> PathBuf {
        let raw_path = match key {
            Sidecar::Aria2c => &self.sidecar.aria2c,
            Sidecar::FFmpeg => &self.sidecar.ffmpeg,
            Sidecar::DanmakuFactory => &self.sidecar.danmakufactory,
        };
        if raw_path.exists() {
            raw_path.clone()
        } else {
            SettingsSidecar::get_default(key.as_str())
        }
    }
    pub fn temp_dir(&self) -> PathBuf {
        self.temp_dir.join("com.nununuuuu.conest")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SettingsProxy {
    pub address: String,
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SettingsDefault {
    pub res: usize,
    pub abr: usize,
    pub enc: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SettingsFormat {
    pub series: String,
    pub item: String,
    pub file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SettingsConvert {
    pub danmaku: bool,
    pub mp4: bool,
    pub mp3: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SettingsOrganize {
    pub auto_rename: bool,
    pub top_folder: bool,
    pub sub_folder: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SettingsSidecar {
    pub aria2c: PathBuf,
    pub ffmpeg: PathBuf,
    pub danmakufactory: PathBuf,
}

impl Default for SettingsSidecar {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsSidecar {
    pub fn new() -> Self {
        Self {
            aria2c: SettingsSidecar::get_default("aria2c"),
            ffmpeg: SettingsSidecar::get_default("ffmpeg"),
            danmakufactory: SettingsSidecar::get_default("DanmakuFactory"),
        }
    }
    pub fn get_default(name: &str) -> PathBuf {
        let parent = std::env::current_exe()
            .ok()
            .and_then(|v| v.parent().map(|v| v.to_path_buf()))
            .expect("Failed to get parent dir");
        match env::consts::OS {
            "windows" => parent.join(name).with_extension("exe"),
            "linux" => parent.join(format!("conest-{name}")),
            _ => parent.join(name),
        }
    }
}

#[derive(Iden)]
pub enum Config {
    Table,
    Name,
    Value,
}

pub struct ConfigTable;

impl TableSpec for ConfigTable {
    const NAME: &'static str = "config";
    const LATEST: i32 = 2;
    fn create_stmt() -> TableCreateStatement {
        Table::create()
            .table(Config::Table)
            .col(ColumnDef::new(Config::Name).text().not_null().primary_key())
            .col(ColumnDef::new(Config::Value).text().not_null())
            .to_owned()
    }
}

#[cfg(target_os = "windows")]
fn sync_autostart(settings: &Settings) -> Result<()> {
    use std::process::Command;

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "CONest";

    if !settings.auto_start {
        let _ = Command::new("reg.exe")
            .args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])
            .output();
        return Ok(());
    }

    let exe = std::env::current_exe()?;
    let mode = if settings.startup_mode == "background" {
        " --background"
    } else {
        ""
    };
    let value = format!("\"{}\"{}", exe.display(), mode);

    let output = Command::new("reg.exe")
        .args([
            "add",
            RUN_KEY,
            "/v",
            VALUE_NAME,
            "/t",
            "REG_SZ",
            "/d",
            &value,
            "/f",
        ])
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "Failed to update Windows autostart entry: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn sync_autostart(_settings: &Settings) -> Result<()> {
    Ok(())
}

pub fn read() -> Arc<Settings> {
    CONFIG.load_full()
}

pub async fn load() -> Result<()> {
    let (sql, values) = Query::select()
        .columns([Config::Name, Config::Value])
        .from(Config::Table)
        .build_sqlx(SqliteQueryBuilder);

    let pool = get_db().await?;
    let rows = sqlx::query_with(&sql, values).fetch_all(&pool).await?;
    let mut local = serde_json::Map::new();
    for r in rows {
        let n: String = r.try_get("name")?;
        let v: Value = serde_json::from_str(&r.try_get::<String, _>("value")?)?;
        local.insert(n, v);
    }

    let map = serde_json::to_value(read())?
        .as_object()
        .cloned()
        .ok_or(anyhow!("Failed to read config"))?;
    for (k, v) in map {
        if !local.contains_key(&k) {
            insert(&k, &v).await?;
            local.insert(k, v);
        } else if let (Value::Object(default), Value::Object(local)) = (
            v,
            local
                .get_mut(&k)
                .ok_or(anyhow!("Failed to get local config"))?,
        ) {
            for (k, v) in default {
                if !local.contains_key(&k) {
                    local.insert(k, v);
                }
            }
            insert(&k, &Value::Object(local.to_owned())).await?;
        }
    }
    // One-time migration from the old CONest default "exit" close behavior
    // to the new default "minimize". After this marker is written, a user can
    // still manually choose "exit" and that choice will be respected.
    const CLOSE_BEHAVIOR_DEFAULT_MIGRATION: &str = "_close_behavior_default_minimize_v1";
    if !local.contains_key(CLOSE_BEHAVIOR_DEFAULT_MIGRATION) {
        if local
            .get("close_behavior")
            .and_then(Value::as_str)
            .map(|value| value == "exit")
            .unwrap_or(false)
        {
            let value = Value::String("minimize".into());
            insert("close_behavior", &value).await?;
            local.insert("close_behavior".into(), value);
        }

        let marker = Value::Bool(true);
        insert(CLOSE_BEHAVIOR_DEFAULT_MIGRATION, &marker).await?;
        local.insert(CLOSE_BEHAVIOR_DEFAULT_MIGRATION.into(), marker);
    }

    let mut settings: Settings = serde_json::from_value(Value::Object(local))?;

    // Migrate stale absolute Sidecar paths left by an older portable/build folder.
    // If the saved executable no longer exists, fall back to the executable next
    // to the currently running CONest binary and persist the repaired paths.
    let mut sidecar_changed = false;

    if !settings.sidecar.aria2c.is_file() {
        settings.sidecar.aria2c = SettingsSidecar::get_default("aria2c");
        sidecar_changed = true;
    }
    if !settings.sidecar.ffmpeg.is_file() {
        settings.sidecar.ffmpeg = SettingsSidecar::get_default("ffmpeg");
        sidecar_changed = true;
    }
    if !settings.sidecar.danmakufactory.is_file() {
        settings.sidecar.danmakufactory = SettingsSidecar::get_default("DanmakuFactory");
        sidecar_changed = true;
    }

    if sidecar_changed {
        insert("sidecar", &serde_json::to_value(&settings.sidecar)?).await?;
    }

    sync_autostart(&settings)?;
    CONFIG.store(Arc::new(settings));
    Ok(())
}

pub async fn insert(name: &str, value: &Value) -> Result<()> {
    if let Some(value) = value.as_u64().filter(|_| name == "max_conc") {
        RUNTIME.new_conc(value as usize).await;
    }

    let (sql, values) = Query::insert()
        .into_table(Config::Table)
        .columns([Config::Name, Config::Value])
        .values([name.into(), serde_json::to_string(&value)?.into()])?
        .on_conflict(
            OnConflict::column(Config::Name)
                .update_columns([Config::Value])
                .to_owned(),
        )
        .build_sqlx(SqliteQueryBuilder);

    let pool = get_db().await?;
    sqlx::query_with(&sql, values).execute(&pool).await?;
    Ok(())
}


async fn validate_writable_directory(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path).await?;
    let meta = tokio::fs::metadata(path).await?;
    if !meta.is_dir() {
        return Err(anyhow!("Not a directory: {}", path.display()));
    }

    let probe = path.join(format!(".conest-write-test-{}", std::process::id()));
    tokio::fs::write(&probe, b"ok").await?;
    tokio::fs::remove_file(&probe).await?;
    Ok(())
}

fn validate_sidecars(value: &Value) -> Result<()> {
    let Some(map) = value.as_object() else {
        return Err(anyhow!("Invalid sidecar setting"));
    };
    for key in ["aria2c", "ffmpeg", "danmakufactory"] {
        if let Some(path) = map.get(key).and_then(Value::as_str) {
            if !Path::new(path).is_file() {
                return Err(anyhow!("Sidecar executable not found: {path}"));
            }
        }
    }
    Ok(())
}

pub async fn write(settings: serde_json::Map<String, Value>) -> Result<()> {
    let mut map = serde_json::to_value(read())?;
    let keys = map
        .as_object()
        .map(|v| v.keys().cloned().collect::<Vec<String>>())
        .ok_or(anyhow!("Failed to read config"))?;

    let ftr = settings.into_iter().filter(|(k, _)| keys.contains(k));
    let obj = map
        .as_object_mut()
        .ok_or(anyhow!("Failed to get mutable config"))?;

    for (k, v) in ftr {
        match k.as_str() {
            "down_dir" | "temp_dir" => {
                let path = v
                    .as_str()
                    .ok_or(anyhow!("Invalid path setting: {k}"))?;
                validate_writable_directory(Path::new(path)).await?;
            }
            "sidecar" => validate_sidecars(&v)?,
            _ => {}
        }

        insert(&k, &v).await?;
        obj.insert(k, v);
    }
    let next_settings: Settings = serde_json::from_value(map)?;
    sync_autostart(&next_settings)?;
    CONFIG.store(Arc::new(next_settings));

    #[cfg(debug_assertions)]
    log::info!("CONFIG: \n{}", serde_json::to_string_pretty(&read())?);
    Ok(())
}
