mod auth;
mod claude_providers;
mod codex;
mod commands;
mod droid_models;
mod management;
mod server;
mod thinking_proxy;
mod usage;
mod watcher;

use commands::{start_proxy_stack, stop_proxy_stack, AppState};
use std::{fs, path::PathBuf};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;

fn runtime_config_file_name() -> &'static str {
    if cfg!(debug_assertions) {
        "config.dev.yaml"
    } else {
        "config.yaml"
    }
}

fn prepare_runtime_config(
    app: &tauri::App,
    bundled_config_path: &PathBuf,
) -> Result<PathBuf, String> {
    if !bundled_config_path.exists() {
        return Err(format!(
            "Bundled config template not found: {}",
            bundled_config_path.display()
        ));
    }

    let app_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))?;

    fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;

    let runtime_config_path = app_data_dir.join(runtime_config_file_name());
    if !runtime_config_path.exists() {
        fs::copy(bundled_config_path, &runtime_config_path)
            .map_err(|e| format!("Failed to seed runtime config: {}", e))?;
        log::info!(
            "[Setup] Seeded runtime config from {:?} to {:?}",
            bundled_config_path,
            runtime_config_path
        );
    } else {
        log::info!(
            "[Setup] Using existing runtime config at {:?}",
            runtime_config_path
        );
    }

    Ok(runtime_config_path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::start_server,
            commands::stop_server,
            commands::connect_service,
            commands::disconnect_service,
            commands::remove_account,
            commands::fetch_usage,
            commands::fetch_all_usage,
            commands::import_from_kiro_ide,
            commands::open_auth_folder,
            commands::open_external_url,
            commands::get_server_logs,
            commands::clear_server_logs,
            commands::start_file_watcher,
            commands::stop_file_watcher,
            commands::copy_server_url,
            commands::get_autostart_enabled,
            commands::set_autostart_enabled,
            commands::start_thinking_proxy,
            commands::stop_thinking_proxy,
            commands::is_thinking_proxy_running,
            commands::get_codex_keys,
            commands::get_codex_accounts,
            commands::add_codex_key,
            commands::delete_codex_key,
            commands::delete_codex_account,
            commands::import_codex_token,
            commands::get_service_routing_overview,
            commands::apply_service_account_mode,
            commands::get_droid_custom_models,
            commands::save_droid_custom_model,
            commands::delete_droid_custom_model,
            commands::duplicate_droid_custom_model,
            commands::set_droid_default_model,
            commands::get_claude_providers,
            commands::save_claude_provider,
            commands::apply_claude_provider,
            commands::delete_claude_provider,
            commands::duplicate_claude_provider,
            commands::set_claude_provider_enabled,
            commands::test_claude_provider_connectivity,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let state = app.state::<AppState>();

            let binary_name = if cfg!(windows) {
                "cli-proxy-api-plus.exe"
            } else {
                "cli-proxy-api-plus"
            };

            let (binary_path, bundled_config_path) = if cfg!(debug_assertions) {
                let dev_resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
                log::info!(
                    "[Setup] Dev mode: CARGO_MANIFEST_DIR={}",
                    env!("CARGO_MANIFEST_DIR")
                );
                (
                    dev_resources.join(binary_name),
                    dev_resources.join("config.yaml"),
                )
            } else {
                let mut found_binary = std::path::PathBuf::new();
                let mut found_config = std::path::PathBuf::new();

                if let Ok(bin) = app
                    .path()
                    .resolve(binary_name, tauri::path::BaseDirectory::Resource)
                {
                    if bin.exists() {
                        found_binary = bin;
                    }
                }

                if let Ok(cfg) = app
                    .path()
                    .resolve("config.yaml", tauri::path::BaseDirectory::Resource)
                {
                    if cfg.exists() {
                        found_config = cfg;
                    }
                }

                if !found_binary.exists() {
                    if let Ok(resource_path) = app.path().resource_dir() {
                        let bin = resource_path.join(binary_name);
                        let cfg = resource_path.join("config.yaml");
                        if bin.exists() {
                            found_binary = bin;
                            found_config = cfg;
                        }
                    }
                }

                if !found_binary.exists() {
                    if let Ok(exe_path) = std::env::current_exe() {
                        if let Some(exe_dir) = exe_path.parent() {
                            let resources_dir = exe_dir.join("resources");
                            let bin = resources_dir.join(binary_name);
                            if bin.exists() {
                                found_binary = bin;
                                found_config = resources_dir.join("config.yaml");
                            } else {
                                let bin = exe_dir.join(binary_name);
                                if bin.exists() {
                                    found_binary = bin;
                                    found_config = exe_dir.join("config.yaml");
                                }
                            }
                        }
                    }
                }

                (found_binary, found_config)
            };

            let config_path = match prepare_runtime_config(app, &bundled_config_path) {
                Ok(path) => path,
                Err(err) => {
                    log::warn!(
                        "[Setup] Failed to prepare runtime config, falling back to bundled config: {}",
                        err
                    );
                    bundled_config_path.clone()
                }
            };

            log::info!("Binary path: {:?}", binary_path);
            log::info!("Bundled config path: {:?}", bundled_config_path);
            log::info!("Config path: {:?}", config_path);

            if binary_path.exists() {
                state.server.set_binary_path(binary_path);
                state.server.set_config_path(config_path);
                log::info!("cli-proxy-api-plus binary found");
            } else {
                log::warn!("cli-proxy-api-plus binary not found, using simulation mode");
            }

            let app_handle = app.handle().clone();
            if let Ok(mut watcher) = state.file_watcher.lock() {
                if let Err(e) = watcher.start(app_handle) {
                    log::warn!("Failed to start file watcher: {}", e);
                }
            }

            let server_started = {
                let should_start = state.server.has_binary();
                if should_start {
                    log::info!("Auto-starting proxy server...");
                    match start_proxy_stack(state.inner()) {
                        Ok(_) => {
                            *state.server_running.lock().unwrap() = true;
                            log::info!(
                                "Proxy stack auto-started ({} -> {})",
                                state.server.proxy_port(),
                                state.server.backend_port()
                            );
                            true
                        }
                        Err(e) => {
                            log::warn!("Failed to auto-start server: {}", e);
                            false
                        }
                    }
                } else {
                    false
                }
            };

            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
            let toggle_server = MenuItem::with_id(
                app,
                "toggle_server",
                if server_started {
                    "Stop Server"
                } else {
                    "Start Server"
                },
                true,
                None::<&str>,
            )?;
            let copy_url =
                MenuItem::with_id(app, "copy_url", "Copy Server URL", true, None::<&str>)?;

            let menu = Menu::with_items(app, &[&show, &toggle_server, &copy_url, &quit])?;

            let tooltip = if server_started {
                "TOAPIPROXY - Running (port 8317)"
            } else {
                "TOAPIPROXY - Stopped"
            };

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip(tooltip)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "quit" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            if let Ok(mut watcher) = state.file_watcher.lock() {
                                watcher.stop();
                            }
                            let _ = stop_proxy_stack(state.inner());
                        }
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "toggle_server" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            let mut running = state.server_running.lock().unwrap();
                            if *running {
                                let _ = stop_proxy_stack(state.inner());
                                *running = false;
                            } else if start_proxy_stack(state.inner()).is_ok() {
                                *running = true;
                            }
                        }
                    }
                    "copy_url" => {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text("http://127.0.0.1:8317");
                            log::info!("Server URL copied to clipboard");
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if cfg!(debug_assertions) {
                    if let Some(state) = window.app_handle().try_state::<AppState>() {
                        if let Ok(mut watcher) = state.file_watcher.lock() {
                            watcher.stop();
                        }
                        let _ = stop_proxy_stack(state.inner());
                    }
                    api.prevent_close();
                    window.app_handle().exit(0);
                } else {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
