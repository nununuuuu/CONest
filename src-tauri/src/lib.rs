pub mod commands;
pub mod errors;
pub mod native_messaging;

use crate::queue::manager::clear_completed;
use crate::services::local_api::{
    set_extension_frontend_ready, set_extension_task_options,
    take_extension_requests, update_extension_task_settings,
};
pub mod services;
pub mod shared;
pub mod storage;

use commands::*;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_specta::{collect_commands, collect_events, Builder};

#[cfg(debug_assertions)]
use tauri_plugin_log::fern::colors::{Color, ColoredLevelConfig};

pub(crate) fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_visible(false);
    }
}

fn show_tray(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_visible(true);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    if std::env::args().any(|arg| arg == "--register-native-messaging-only") {
        native_messaging::register()?;
        return Ok(());
    }

    std::panic::set_hook(Box::new(|e| {
        let bt = std::backtrace::Backtrace::force_capture();
        log::error!("Panicked: {e}\n{bt:?}");
    }));
    let builder = Builder::<tauri::Wry>::new()
        // Then register them (separated by a comma)
        .commands(collect_commands![
            // Essentials
            meta,
            init,
            set_window,
            config_write,
            open_cache,
            get_size,
            clean_cache,
            db_import,
            db_export,
            export_data,
            install_release_version,
            // Login
            stop_login,
            exit,
            sms_login,
            pwd_login,
            switch_cookie,
            scan_login,
            refresh_cookie,
            // Queue
            ctrl_event,
            open_folder,
            submit_task,
            plan_scheduler,
            plan_scheduler_task,
            process_scheduler,
            clear_completed,
            take_extension_requests,
            update_extension_task_settings,
            set_extension_frontend_ready,
            set_extension_task_options,
            // update_max_conc,
        ])
        .events(collect_events![
            shared::HeadersData,
            shared::ProcessError,
            queue::frontend::QueueEvent
        ]);

    #[cfg(debug_assertions)] // <- Only export on non-release builds
    builder
        .export(
            specta_typescript::Typescript::default()
                .bigint(specta_typescript::BigIntExportBehavior::Number)
                .header("// @ts-nocheck"),
            "../src/services/backend.ts",
        )
        .expect("Failed to export typescript bindings");

    let log_builder = tauri_plugin_log::Builder::new()
        .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
        .level(log::LevelFilter::Info)
        .level_for("sqlx::query", log::LevelFilter::Warn);

    #[cfg(debug_assertions)]
    let log_builder = log_builder.with_colors(
        ColoredLevelConfig::new()
            .error(Color::Red)
            .warn(Color::Yellow)
            .info(Color::Green)
            .debug(Color::Blue)
            .trace(Color::Magenta),
    );

    tauri::Builder::default()
        .plugin(log_builder.build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            let background = args.iter().any(|arg| arg == "--background");
            if !background {
                show_main_window(app);
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            let version = &app.package_info().version;
            log::info!("CONest v{}", version);
            builder.mount_events(app);
            shared::APP_HANDLE.set(app.app_handle().clone())?;

            // Start the extension bridge immediately. It no longer depends on
            // database/aria2/ffmpeg bootstrap completing first.
            if let Err(e) = services::local_api::init() {
                log::warn!("Extension API unavailable: {e:#}");
            }

            #[cfg(target_os = "windows")]
            if let Err(e) = native_messaging::register() {
                log::warn!("Native Messaging registration failed: {e:#}");
            }


            let show_item =
                MenuItem::with_id(app, "show", "顯示 CONest", true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", "退出 CONest", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            TrayIconBuilder::with_id("main-tray")
                .icon(
                    app.default_window_icon()
                        .expect("CONest default window icon missing")
                        .clone(),
                )
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            if let Some(tray) = app.tray_by_id("main-tray") {
                let _ = tray.set_visible(false);
            }

            if std::env::args().any(|arg| arg == "--background") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                    show_tray(app.app_handle());
                }
            }

            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }
            tauri::async_runtime::spawn(async move {
                let result: Result<(), crate::TauriError> = async {
                    storage::init().await?;
                    services::init().await?;
                    Ok(())
                }
                .await;

                let signal = result
                    .as_ref()
                    .map(|_| ())
                    .map_err(|e| format!("{e}"));
                shared::set_bootstrap_result(signal).await;

                if let Err(e) = result {
                    shared::process_err(e, "bootstrap");
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let minimize_to_tray = std::panic::catch_unwind(|| {
                    storage::config::read().close_behavior == "minimize"
                })
                .unwrap_or(false);

                if minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                    show_tray(window.app_handle());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running CONest");
    Ok(())
}
