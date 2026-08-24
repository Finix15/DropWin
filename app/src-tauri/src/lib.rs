use std::sync::Arc;
use std::sync::Mutex;
use tauri::Manager;
use tauri::{DragDropEvent, WindowEvent};
use tracing::{error, info, warn};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{POINT, WAIT_OBJECT_0};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
mod commands;
mod config;
mod drop_registry;
mod drop_window;
mod file;
mod file_drop;
mod internal_drag;
mod logging;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod mouse_monitor;
mod thumbnail;
#[cfg(desktop)]
mod tray;

#[cfg(target_os = "windows")]
mod custom_drop;

use commands::{config_ops::*, drag_ops::*, file_ops::*, window_ops::*};
use config::{AppConfig, RuntimeDropSize};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use mouse_monitor::start_mouse_monitor;

fn build_app() -> tauri::Builder<tauri::Wry> {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(vec!["--autostart"])
                .build(),
        );

    #[cfg(target_os = "windows")]
    {
        builder = builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    info!("Global hotkey event received: {:?}", shortcut);
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let mut cursor_pos = POINT { x: 0, y: 0 };
                        let _ = unsafe { GetCursorPos(&mut cursor_pos) };
                        let _ = crate::drop_window::create_drop_window(
                            app.clone(),
                            (cursor_pos.x as f64, cursor_pos.y as f64),
                            true,
                            "hotkey",
                        );
                    }
                })
                .build(),
        );
    }

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_plugin_key_intercept::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![
            // start_drag,
            start_multi_drag,
            start_text_drag,
            open_popup_window,
            hide_popup_for_drop_move,
            show_popup_after_drop_move,
            close_popup_window,
            add_files,
            mark_drop_received,
            save_pasted_text,
            save_pasted_data_base64,
            download_image_to_shelf,
            remove_files,
            get_files,
            rename_file,
            get_file_icon_base64,
            clear_files,
            refresh_file_list,
            get_config,
            get_runtime_drop_size,
            save_config,
            preview_drop_opacity,
            open_settings_window,
            close_settings_window,
            restart_app,
            set_autostart,
            register_hotkey,
            resolve_blacklist_executables,
            check_input_monitoring_permission,
            open_input_monitoring_settings,
        ])
        .setup(|app| {
            // Ensure config directory exists first
            let config_dir = app.handle().path().app_config_dir().unwrap_or_else(|_| {
                error!("Failed to get app config directory");
                std::process::exit(1);
            });
            if !config_dir.exists() {
                info!("Creating app config directory at {:?}", config_dir);
                if let Err(e) = std::fs::create_dir_all(&config_dir) {
                    error!("Failed to create app config directory: {}", e);
                    return Err(format!("Failed to create app config directory: {}", e).into());
                }
            }

            // Load configuration once
            let config = AppConfig::load(app.handle());
            app.manage(Arc::new(Mutex::new(config.clone())));
            app.manage(RuntimeDropSize(config.drop_size));
            app.manage(drop_registry::DropRegistry::default());
            app.manage(internal_drag::InternalDragState::default());

            #[cfg(target_os = "windows")]
            {
                app.manage(drop_window::MouseDropPool::default());
                drop_window::prepare_mouse_drop_window(app.handle().clone())?;
            }

            // Register hotkey if configured
            if !config.hotkey.is_empty() {
                info!("Registering startup hotkey");
                // Wait a bit before registering the hotkey
                std::thread::sleep(std::time::Duration::from_millis(100));
                if let Err(e) = register_hotkey(app.handle().clone(), config.hotkey.clone()) {
                    warn!("Failed to register startup hotkey: {}", e);
                } else {
                    info!("Startup hotkey registered successfully");
                }
            }

            #[cfg(desktop)]
            {
                let handle = app.handle();
                tray::create_tray(handle)?;
            }

            // Start the mouse monitor with configuration (Windows and macOS)
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            {
                let app_handle = app.handle().clone();
                let config_state = app.state::<Arc<Mutex<AppConfig>>>();
                let config_guard = config_state
                    .lock()
                    .map_err(|e| format!("Failed to lock config: {}", e))?;
                start_mouse_monitor(config_guard.mouse_monitor.clone(), app_handle.clone());
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(target_os = "windows")]
            if matches!(
                event,
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
            ) && window.label().starts_with(drop_registry::DROP_LABEL_PREFIX)
            {
                if let Some(webview_window) = window.app_handle().get_webview_window(window.label())
                {
                    if let Err(error) = drop_window::apply_rounded_region(&webview_window) {
                        warn!("Failed to reshape {}: {error}", window.label());
                    }
                }
            }

            if let WindowEvent::DragDrop(drop_event) = event {
                if let Ok(drop_id) = drop_registry::drop_id_from_label(window.label()) {
                    match drop_event {
                        DragDropEvent::Enter { paths, .. } => {
                            info!(
                                "Native drag entered Drop {drop_id} with {} path(s)",
                                paths.len()
                            );
                        }
                        DragDropEvent::Leave => {
                            info!("Native drag left Drop {drop_id}");
                        }
                        DragDropEvent::Drop { paths, .. } => {
                            info!("Received {} dropped file(s) in Drop {drop_id}", paths.len());
                            let app_handle = window.app_handle();
                            let registry = app_handle.state::<drop_registry::DropRegistry>();
                            file_drop::handle_file_drop_from_paths(
                                paths.clone(),
                                drop_id.to_string(),
                                registry.inner().clone(),
                                app_handle.clone(),
                            );
                        }
                        DragDropEvent::Over { .. } => {}
                        _ => {}
                    }
                }
            } else if matches!(event, WindowEvent::Destroyed)
                && window.label().starts_with(drop_registry::DROP_LABEL_PREFIX)
            {
                if let Ok(drop_id) = drop_registry::drop_id_from_label(window.label()) {
                    drop_window::close_drop(window.app_handle(), drop_id);
                }
            }
        })
}

fn run_app() {
    let app = build_app()
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(|_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::setup_logging();
    tracing::info!("Starting DropWin application");

    #[cfg(target_os = "windows")]
    {
        // Check for existing instance
        unsafe {
            let mutex_name = windows::core::w!("Global\\DropWinAppMutex");
            let mutex = CreateMutexW(None, true, mutex_name);

            if let Ok(mutex) = mutex {
                if WaitForSingleObject(mutex, 0) == WAIT_OBJECT_0 {
                    run_app();

                    // Clean up the mutex
                    let _ = ReleaseMutex(mutex);
                } else {
                    // Another instance is already running
                    info!("Another instance of the application is already running");
                    std::process::exit(0);
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        run_app();
    }
}
