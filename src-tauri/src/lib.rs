mod error;
mod models;
mod services;
mod commands;

use crate::error::{AppError, AppResult};
use crate::models::AppSettings;
use services::{Recorder, CameraPreview, immersive::ImmersiveMode};
use services::platform::macos::ffmpeg::FfmpegLocator;
use services::settings::SettingsStore;
use std::sync::{mpsc, Arc, Mutex};
use tauri::{
    menu::{Menu, MenuId, MenuItemBuilder, MenuItemKind, Submenu},
    AppHandle, Manager, PhysicalPosition,
};

const TOGGLE_IMMERSIVE_MENU_ID: &str = "toggle-immersive-mode";
const OPEN_SETTINGS_MENU_ID: &str = "open-settings-window";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle();

            let ffmpeg_locator = Arc::new(FfmpegLocator::new());
            let camera_preview = CameraPreview::new(ffmpeg_locator.clone());
            let camera_sync = camera_preview.sync_handle();

            app.manage(Recorder::new(ffmpeg_locator, camera_sync));
            app.manage(Mutex::new(camera_preview));
            app.manage(Arc::new(Mutex::new(ImmersiveMode::new())));
            app.manage(SettingsStore::new(None)?);

            position_overlay_windows(&app_handle);

            let settings = app.state::<SettingsStore>().load().unwrap_or_default();
            initialize_camera_overlay(&app_handle, &settings)?;
            build_app_menu(&app_handle, &settings)?;
            register_menu_handlers(&app_handle)?;
            register_immersive_shortcut_handler(&app_handle, &settings.immersive_shortcut)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_recording,
            commands::pause_recording,
            commands::resume_recording,
            commands::stop_recording,
            commands::get_settings,
            commands::update_settings,
            commands::set_camera_overlay_visible,
            commands::toggle_microphone_during_recording,
            commands::set_mic_muted,
            commands::set_system_audio_muted,
            commands::toggle_immersive_mode,
            commands::set_immersive_mode,
            commands::update_immersive_shortcut,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn position_overlay_windows(app: &tauri::AppHandle) {
    if let Some(overlay_window) = app.get_webview_window("overlay") {
        if let Ok(monitor) = overlay_window.primary_monitor() {
            if let Some(monitor) = monitor {
                if let Ok(window_size) = overlay_window.outer_size() {
                    let monitor_size = monitor.size();
                    let x = monitor_size.width as i32 - window_size.width as i32 - 20;
                    overlay_window
                        .set_position(PhysicalPosition::new(x, 20))
                        .ok();
                }
            }
        }
    }

    if let Some(camera_window) = app.get_webview_window("camera-overlay") {
        if let Ok(monitor) = camera_window.primary_monitor() {
            if let Some(monitor) = monitor {
                if let Ok(window_size) = camera_window.outer_size() {
                    let monitor_size = monitor.size();
                    let x = monitor_size.width as i32 - window_size.width as i32 - 20;
                    let y = monitor_size.height as i32 - window_size.height as i32 - 20;
                    camera_window
                        .set_position(PhysicalPosition::new(x, y))
                        .ok();
                }
            }
        }
    }
}

fn initialize_camera_overlay(app: &tauri::AppHandle, settings: &AppSettings) -> AppResult<()> {
    let camera_state = app.state::<Mutex<CameraPreview>>();
    if settings.camera_enabled {
        if let Some(window) = app.get_webview_window("camera-overlay") {
            window.show()?;
        }
        let mut preview = camera_state.lock().unwrap();
        preview.set_app_handle(app.clone());
        if !preview.is_running() {
            preview.start()?;
        }
    } else if let Some(window) = app.get_webview_window("camera-overlay") {
        window.hide()?;
    }
    Ok(())
}

fn build_app_menu(app: &AppHandle, settings: &AppSettings) -> AppResult<()> {
    let menu = Menu::default(app)?;
    let pkg_name = app.package_info().name.clone();

    if let Some(app_submenu) = find_app_submenu(&menu, &pkg_name)? {
        append_menu_items(app_submenu, app, settings)?;
    } else {
        let submenu = Submenu::new(app, pkg_name, true)?;
        append_menu_items(submenu.clone(), app, settings)?;
        menu.append(&submenu)?;
    }

    app.set_menu(menu)?;
    Ok(())
}

fn register_menu_handlers(app: &AppHandle) -> AppResult<()> {
    app.on_menu_event(|app, event| {
        if event.id == MenuId::new(TOGGLE_IMMERSIVE_MENU_ID) {
            if let Err(err) = commands::toggle_immersive_mode_from_menu(app) {
                eprintln!("[Menu] Failed to toggle immersive mode: {}", err);
            }
        } else if event.id == MenuId::new(OPEN_SETTINGS_MENU_ID) {
            if let Err(err) = show_settings_window(app) {
                eprintln!("[Menu] Failed to open settings window: {}", err);
            }
        }
    });

    Ok(())
}

fn find_app_submenu<'a>(
    menu: &'a Menu<tauri::Wry>,
    name: &str,
) -> AppResult<Option<Submenu<tauri::Wry>>> {
    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item {
            if submenu.text()? == name {
                return Ok(Some(submenu.clone()));
            }
        }
    }
    Ok(None)
}

fn append_menu_items(
    submenu: Submenu<tauri::Wry>,
    app: &AppHandle,
    settings: &AppSettings,
) -> AppResult<()> {
    let toggle_item = MenuItemBuilder::new("Toggle Immersive Mode")
        .id(MenuId::new(TOGGLE_IMMERSIVE_MENU_ID))
        .accelerator(&settings.immersive_shortcut)
        .build(app)?;
    submenu.append(&toggle_item)?;

    let settings_item = MenuItemBuilder::new("Settings…")
        .id(MenuId::new(OPEN_SETTINGS_MENU_ID))
        .accelerator("Cmd+,")
        .build(app)?;
    submenu.append(&settings_item)?;
    Ok(())
}

pub(crate) fn update_toggle_menu_shortcut(
    app: &AppHandle,
    shortcut: &str,
) -> AppResult<()> {
    if let Some(menu) = app.menu() {
        if let Some(item_kind) = menu.get(&MenuId::new(TOGGLE_IMMERSIVE_MENU_ID)) {
            if let Some(item) = item_kind.as_menuitem() {
                item.set_accelerator(Some(shortcut))?;
            }
        }
    }
    Ok(())
}

pub(crate) fn register_immersive_shortcut_handler(
    app: &AppHandle,
    shortcut: &str,
) -> AppResult<()> {
    let trimmed = shortcut.trim().to_string();
    let (tx, rx) = mpsc::channel();
    let callback_app = app.clone();

    app.run_on_main_thread(move || {
        let result = if trimmed.is_empty() {
            services::hotkey::unregister_hotkey()
        } else {
            let app_for_callback = callback_app.clone();
            let callback = Arc::new(move || {
                let handle = app_for_callback.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = commands::toggle_immersive_mode_from_menu(&handle) {
                        eprintln!("[Shortcut] Failed to toggle immersive mode: {}", err);
                    }
                });
            });
            services::hotkey::register_hotkey(&trimmed, callback)
        };

        let _ = tx.send(result);
    })
    .map_err(|err| AppError::Settings(format!("Failed to schedule hotkey registration: {}", err)))?;

    rx.recv()
        .unwrap_or_else(|_| {
            Err(AppError::Settings(
                "Failed to finalize hotkey registration".into(),
            ))
        })
}

pub(crate) fn show_settings_window(app: &AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show()?;
        window.set_focus()?;
        Ok(())
    } else {
        Err(AppError::Settings(
            "Settings window is not registered in tauri.conf.json".into(),
        ))
    }
}
