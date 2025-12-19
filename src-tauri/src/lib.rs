mod error;
mod models;
mod services;
mod commands;

use services::{Recorder, CameraPreview};
use std::sync::Mutex;
use tauri::{Manager, PhysicalPosition};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Initialize services
            app.manage(Mutex::new(Recorder::new()));
            app.manage(Mutex::new(CameraPreview::new()));

            // Position overlay window in top-right
            if let Some(overlay_window) = app.get_webview_window("overlay") {
                if let Ok(monitor) = overlay_window.primary_monitor() {
                    if let Some(monitor) = monitor {
                        let monitor_size = monitor.size();
                        let window_size = overlay_window.outer_size().unwrap();
                        let x = monitor_size.width as i32 - window_size.width as i32 - 20;
                        let y = 20;
                        overlay_window.set_position(PhysicalPosition::new(x, y)).ok();
                    }
                }
            }

            // Position camera overlay window in bottom-right
            if let Some(camera_window) = app.get_webview_window("camera-overlay") {
                if let Ok(monitor) = camera_window.primary_monitor() {
                    if let Some(monitor) = monitor {
                        let monitor_size = monitor.size();
                        let window_size = camera_window.outer_size().unwrap();
                        let x = monitor_size.width as i32 - window_size.width as i32 - 20;
                        let y = monitor_size.height as i32 - window_size.height as i32 - 20;
                        camera_window.set_position(PhysicalPosition::new(x, y)).ok();
                    }
                }
            }

            // Load settings and show camera overlay if enabled
            println!("[App] Loading settings...");
            match services::settings::load_settings() {
                Ok(settings) => {
                    println!("[App] Settings loaded: camera_enabled={}, mic_enabled={}", 
                        settings.camera_enabled, settings.mic_enabled);
                    if settings.camera_enabled {
                        println!("[App] Camera is enabled, showing camera overlay...");
                        if let Some(camera_window) = app.get_webview_window("camera-overlay") {
                            match camera_window.show() {
                                Ok(_) => {
                                    println!("[App] Camera overlay window shown successfully");
                                    if let Ok(mut preview) = app.state::<Mutex<CameraPreview>>().try_lock() {
                                        preview.set_app_handle(app.handle().clone());
                                        match preview.start() {
                                            Ok(_) => println!("[App] Camera preview started successfully"),
                                            Err(e) => eprintln!("[App] ERROR: Failed to start camera preview: {}", e),
                                        }
                                    } else {
                                        eprintln!("[App] ERROR: Could not lock CameraPreview state");
                                    }
                                }
                                Err(e) => eprintln!("[App] ERROR: Failed to show camera overlay window: {}", e),
                            }
                        } else {
                            eprintln!("[App] ERROR: Camera overlay window not found");
                        }
                    } else {
                        println!("[App] Camera is disabled in settings");
                    }
                }
                Err(e) => {
                    eprintln!("[App] ERROR: Failed to load settings: {}", e);
                    println!("[App] Using default settings (camera disabled)");
                }
            }

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
