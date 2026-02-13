use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, RecordingOptions};
use crate::services::camera::CameraPreview;
use crate::services::immersive::ImmersiveMode;
use crate::services::recording::{
    Recorder, RecordingPausedInfo, RecordingResumedInfo, RecordingStoppedInfo,
};
use crate::services::settings::SettingsStore;

#[tauri::command]
pub async fn start_recording(
    options: RecordingOptions,
    app: AppHandle,
) -> AppResult<()> {
    let app_handle = app.clone();
    let options_clone = options.clone();

    tauri::async_runtime::spawn(async move {
        let recorder = app_handle.state::<Recorder>().clone();
        let result = recorder.start(options_clone);
        match result {
            Ok(info) => {
                recorder.start_elapsed_task(app_handle.clone());
                let _ = app_handle.emit("recording-started", info);

                if options.include_camera {
                    let immersive_state = app_handle.state::<Arc<Mutex<ImmersiveMode>>>();
                    let camera_preview = app_handle.state::<Mutex<CameraPreview>>();
                    let immersive = is_immersive_enabled(&immersive_state);
                    if let Err(err) =
                        apply_camera_overlay_visibility(&app_handle, &camera_preview, true, immersive)
                    {
                        let _ = app_handle.emit(
                            "recording-error",
                            json!({ "message": err.to_string() }),
                        );
                    }
                }
            }
            Err(err) => {
                let _ = app_handle.emit("recording-error", json!({
                    "message": err.to_string()
                }));
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn pause_recording(
    recorder: State<'_, Recorder>,
    app: AppHandle,
) -> AppResult<()> {
    let info: RecordingPausedInfo = recorder.pause()?;
    app.emit("recording-paused", info)?;
    Ok(())
}

#[tauri::command]
pub async fn resume_recording(
    recorder: State<'_, Recorder>,
    app: AppHandle,
) -> AppResult<()> {
    let info: RecordingResumedInfo = recorder.resume()?;
    app.emit("recording-resumed", info)?;
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
) -> AppResult<()> {
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        let recorder = app_handle.state::<Recorder>().clone();
        let result = recorder.stop();
        match result {
            Ok(stop_result) => {
                let _ = app_handle.emit(
                    "recording-stopped",
                    RecordingStoppedInfo {
                        elapsed_ms: stop_result.elapsed_ms,
                    },
                );

                let camera_preview = app_handle.state::<Mutex<CameraPreview>>();
                let immersive_state = app_handle.state::<Arc<Mutex<ImmersiveMode>>>();
                let settings_store = app_handle.state::<SettingsStore>();
                if let Ok(settings) = settings_store.load() {
                    let immersive = is_immersive_enabled(&immersive_state);
                    if let Err(err) = apply_camera_overlay_visibility(
                        &app_handle,
                        &camera_preview,
                        settings.camera_enabled,
                        immersive,
                    ) {
                        let _ = app_handle.emit(
                            "recording-error",
                            json!({ "message": err.to_string() }),
                        );
                        return;
                    }
                }

                if let Err(err) = save_recording_file(&app_handle, stop_result.output_path) {
                    let _ = app_handle.emit("recording-error", json!({
                        "message": err.to_string()
                    }));
                    return;
                }
            }
            Err(err) => {
                let _ = app_handle.emit("recording-error", json!({
                    "message": err.to_string()
                }));
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn get_settings(settings_store: State<'_, SettingsStore>) -> AppResult<AppSettings> {
    settings_store.load()
}

#[tauri::command]
pub async fn update_settings(
    settings: AppSettings,
    settings_store: State<'_, SettingsStore>,
    app: AppHandle,
) -> AppResult<()> {
    settings_store.save(&settings)?;
    app.emit("settings-updated", settings.clone())?;
    Ok(())
}

#[tauri::command]
pub async fn set_camera_overlay_visible(
    visible: bool,
    app: AppHandle,
    camera_preview: State<'_, Mutex<CameraPreview>>,
    immersive_mode: State<'_, Arc<Mutex<ImmersiveMode>>>,
) -> AppResult<()> {
    let immersive = is_immersive_enabled(&immersive_mode);
    apply_camera_overlay_visibility(&app, &camera_preview, visible, immersive)
}

#[tauri::command]
pub async fn toggle_microphone_during_recording(enabled: bool) -> AppResult<()> {
    Err(AppError::Recording(format!(
        "Cannot toggle microphone source mid-recording (requested: {})",
        enabled
    )))
}

#[tauri::command]
pub async fn set_mic_muted(
    muted: bool,
    recorder: State<'_, Recorder>,
) -> AppResult<()> {
    recorder.set_mic_muted(muted);
    Ok(())
}

#[tauri::command]
pub async fn set_system_audio_muted(
    muted: bool,
    recorder: State<'_, Recorder>,
) -> AppResult<()> {
    recorder.set_system_audio_muted(muted);
    Ok(())
}

#[tauri::command]
pub async fn set_immersive_mode(
    enabled: bool,
    app: AppHandle,
    camera_preview: State<'_, Mutex<CameraPreview>>,
    immersive_mode: State<'_, Arc<Mutex<ImmersiveMode>>>,
) -> AppResult<()> {
    apply_immersive_state(&app, &immersive_mode, enabled, &camera_preview)
}

#[tauri::command]
pub async fn toggle_immersive_mode(
    app: AppHandle,
    camera_preview: State<'_, Mutex<CameraPreview>>,
    immersive_mode: State<'_, Arc<Mutex<ImmersiveMode>>>,
) -> AppResult<()> {
    let next = !is_immersive_enabled(&immersive_mode);
    apply_immersive_state(&app, &immersive_mode, next, &camera_preview)
}

pub(crate) fn toggle_immersive_mode_from_menu(app: &AppHandle) -> AppResult<()> {
    let immersive_state = app.state::<Arc<Mutex<ImmersiveMode>>>();
    let camera_preview = app.state::<Mutex<CameraPreview>>();
    let next = !is_immersive_enabled(&immersive_state);
    apply_immersive_state(app, &immersive_state, next, &camera_preview)
}

#[tauri::command]
pub async fn update_immersive_shortcut(
    shortcut: String,
    app: AppHandle,
    settings_store: State<'_, SettingsStore>,
) -> AppResult<()> {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        return Err(AppError::Settings("Shortcut cannot be empty".into()));
    }

    crate::update_toggle_menu_shortcut(&app, trimmed)?;
    let mut current = settings_store.load()?;
    current.immersive_shortcut = trimmed.to_string();
    settings_store.save(&current)?;
    crate::register_immersive_shortcut_handler(&app, trimmed)?;
    app.emit(
        "immersive-shortcut-updated",
        json!({ "shortcut": trimmed }),
    )?;
    app.emit("settings-updated", current)?;
    Ok(())
}

fn is_immersive_enabled(state: &State<'_, Arc<Mutex<ImmersiveMode>>>) -> bool {
    match state.lock() {
        Ok(guard) => guard.is_enabled(),
        Err(poisoned) => poisoned.into_inner().is_enabled(),
    }
}

fn apply_camera_overlay_visibility(
    app: &AppHandle,
    camera_preview: &State<'_, Mutex<CameraPreview>>,
    requested_visible: bool,
    immersive_enabled: bool,
) -> AppResult<()> {
    let window = app
        .get_webview_window("camera-overlay")
        .ok_or_else(|| AppError::Camera("Camera overlay window not found".to_string()))?;

    if requested_visible {
        {
            let mut preview = camera_preview.lock().unwrap();
            preview.set_app_handle(app.clone());
            if !preview.is_running() {
                preview.start()?;
            }
        }

        if immersive_enabled {
            window.hide()?;
        } else {
            window.show()?;
        }
    } else {
        window.hide()?;
        {
            let preview = camera_preview.lock().unwrap();
            if preview.is_running() {
                let _ = preview.stop();
            }
        }
    }
    Ok(())
}

fn apply_immersive_state(
    app: &AppHandle,
    immersive_mode: &State<'_, Arc<Mutex<ImmersiveMode>>>,
    enabled: bool,
    camera_preview: &State<'_, Mutex<CameraPreview>>,
) -> AppResult<()> {
    {
        let mut state = immersive_mode.lock().unwrap();
        state.set_enabled(enabled);
    }

    if let Some(window) = app.get_webview_window("overlay") {
        if enabled {
            window.hide()?;
        } else {
            window.show()?;
            window.set_focus().ok();
        }
    }

    let settings_store = app.state::<SettingsStore>();
    let camera_enabled = settings_store.load()?.camera_enabled;
    apply_camera_overlay_visibility(app, camera_preview, camera_enabled, enabled)?;

    app.emit(
        "immersive-mode-changed",
        json!({ "enabled": enabled }),
    )?;
    Ok(())
}

fn save_recording_file(app: &AppHandle, temp_path: PathBuf) -> AppResult<()> {
    let settings_store = app.state::<SettingsStore>();
    let settings = settings_store.load().unwrap_or_default();
    let target_dir = resolve_output_dir(&settings)?;
    std::fs::create_dir_all(&target_dir)?;
    let timestamp = current_time_seconds();
    let final_path = target_dir.join(format!("momentum-recording-{}.mp4", timestamp));

    if !temp_path.exists() {
        return Err(AppError::Recording(format!(
            "Recording file not found: {:?}",
            temp_path
        )));
    }

    std::fs::copy(&temp_path, &final_path)?;
    if !final_path.exists() {
        return Err(AppError::Recording(format!(
            "Output file was not created: {:?}",
            final_path
        )));
    }

    let _ = std::fs::remove_file(&temp_path);

    app.emit(
        "recording-saved",
        json!({ "path": final_path.to_string_lossy() }),
    )?;

    Ok(())
}

fn resolve_output_dir(settings: &AppSettings) -> AppResult<PathBuf> {
    if let Some(path) = &settings.save_location {
        return Ok(PathBuf::from(path));
    }

    dirs::download_dir().ok_or_else(|| {
        AppError::Recording("Failed to resolve downloads directory".to_string())
    })
}

fn current_time_seconds() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}
