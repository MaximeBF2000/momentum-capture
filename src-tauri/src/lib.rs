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
            if let Ok(settings) = services::settings::load_settings() {
                if settings.camera_enabled {
                    if let Some(camera_window) = app.get_webview_window("camera-overlay") {
                        camera_window.show().ok();
                        if let Ok(mut preview) = app.state::<Mutex<CameraPreview>>().try_lock() {
                            preview.set_app_handle(app.handle().clone());
                            preview.start().ok();
                        }
                    }
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
