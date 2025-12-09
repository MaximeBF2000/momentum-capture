use crate::models::{RecordingOptions, AppSettings};
use crate::error::AppResult;
use crate::services::{recording::Recorder, camera::CameraPreview, settings};
use tauri::{AppHandle, Emitter, Manager, State};
use std::sync::Mutex;

#[tauri::command]
pub async fn start_recording(
    options: RecordingOptions,
    recorder: State<'_, Mutex<Recorder>>,
    app: AppHandle,
    camera_preview: State<'_, Mutex<CameraPreview>>,
) -> AppResult<()> {
    let recorder = recorder.lock().unwrap();
    recorder.start(options.clone())?;
    
    // Show camera overlay if camera is enabled
    if options.include_camera {
        if let Some(window) = app.get_webview_window("camera-overlay") {
            window.show()?;
            {
                let mut preview = camera_preview.lock().unwrap();
                preview.set_app_handle(app.clone());
                // Only start if not already running
                if !preview.is_running() {
                    preview.start()?;
                }
            }
        }
    }
    
    app.emit("recording-started", ())?;
    Ok(())
}

#[tauri::command]
pub async fn pause_recording(
    recorder: State<'_, Mutex<Recorder>>,
    app: AppHandle,
) -> AppResult<()> {
    let recorder = recorder.lock().unwrap();
    recorder.pause()?;
    
    app.emit("recording-paused", ())?;
    Ok(())
}

#[tauri::command]
pub async fn resume_recording(
    recorder: State<'_, Mutex<Recorder>>,
    app: AppHandle,
) -> AppResult<()> {
    let recorder = recorder.lock().unwrap();
    recorder.resume()?;
    
    app.emit("recording-resumed", ())?;
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    recorder: State<'_, Mutex<Recorder>>,
    app: AppHandle,
    camera_preview: State<'_, Mutex<CameraPreview>>,
) -> AppResult<()> {
    let recorder = recorder.lock().unwrap();
    let (screen_file, audio_file) = recorder.stop()?;
    
    println!("Stopped recording. Screen file: {:?}, Audio file: {:?}", screen_file, audio_file);
    
    // Stop camera preview if running
    {
        let preview = camera_preview.lock().unwrap();
        let _ = preview.stop();
    }
    if let Some(window) = app.get_webview_window("camera-overlay") {
        let _ = window.hide();
    }
    
    app.emit("recording-stopped", ())?;
    
    // With muxed recording, screen_file and audio_file are the same file
    // Just copy it to the downloads directory (no merge needed)
    let downloads_dir = get_downloads_dir()?;
    println!("Downloads directory: {:?}", downloads_dir);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let final_path = downloads_dir.join(format!("momentum-recording-{}.mp4", timestamp));
    println!("Final output path: {:?}", final_path);
    
    // Check if the muxed file exists
    if !screen_file.exists() {
        let error = format!("Recording file not found: {:?}", screen_file);
        println!("ERROR: {}", error);
        return Err(crate::error::AppError::Recording(error));
    }
    
    println!("Muxed recording file exists. Copying to downloads...");
    
    // Copy the muxed file to downloads (no merge needed)
    std::fs::copy(&screen_file, &final_path)
        .map_err(|e| {
            let error = format!("Failed to copy recording file: {}", e);
            println!("ERROR: {}", error);
            let error = crate::error::AppError::Recording(error);
            let _ = app.emit("recording-error", serde_json::json!({
                "message": format!("{}", error)
            }));
            error
        })?;
    
    // Verify the output file was created
    if !final_path.exists() {
        let error = format!("Output file was not created: {:?}", final_path);
        println!("ERROR: {}", error);
        let error = crate::error::AppError::Recording(error);
        let _ = app.emit("recording-error", serde_json::json!({
            "message": format!("{}", error)
        }));
        return Err(error);
    }
    
    println!("Successfully copied output file: {:?}", final_path);
    
    // Clean up temp file (only one file now since it's muxed)
    let _ = std::fs::remove_file(&screen_file);
    
    app.emit("recording-saved", serde_json::json!({
        "path": final_path.to_string_lossy()
    }))?;
    
    Ok(())
}

#[tauri::command]
pub async fn get_settings() -> AppResult<AppSettings> {
    settings::load_settings()
}

#[tauri::command]
pub async fn update_settings(
    settings: AppSettings,
) -> AppResult<()> {
    settings::save_settings(&settings)
}

#[tauri::command]
pub async fn toggle_microphone_during_recording(
    enabled: bool,
    recorder: State<'_, Mutex<Recorder>>,
) -> AppResult<()> {
    let recorder = recorder.lock().unwrap();
    recorder.toggle_microphone(enabled)
}

#[tauri::command]
pub async fn set_camera_overlay_visible(
    visible: bool,
    app: AppHandle,
    camera_preview: State<'_, Mutex<CameraPreview>>,
) -> AppResult<()> {
    let window = app.get_webview_window("camera-overlay")
        .ok_or_else(|| crate::error::AppError::Camera("Camera overlay window not found".to_string()))?;
    
    let mut preview = camera_preview.lock().unwrap();
    
    if visible {
        // Show window and start camera stream
        window.show()?;
        preview.set_app_handle(app.clone());
        preview.start()?;
        println!("Camera overlay shown and camera stream started");
    } else {
        // Stop camera stream and hide window
        preview.stop()?;
        window.hide()?;
        println!("Camera stream stopped and overlay hidden");
    }
    
    Ok(())
}

fn get_downloads_dir() -> AppResult<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("sh")
            .arg("-c")
            .arg("echo ~/Downloads")
            .output()?;
        let path = String::from_utf8(output.stdout)?;
        Ok(std::path::PathBuf::from(path.trim()))
    }
    
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("powershell")
            .arg("-Command")
            .arg("[Environment]::GetFolderPath('MyDocuments') + '\\Downloads'")
            .output()?;
        let path = String::from_utf8(output.stdout)?;
        Ok(std::path::PathBuf::from(path.trim()))
    }
    
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let output = Command::new("sh")
            .arg("-c")
            .arg("xdg-user-dir DOWNLOAD")
            .output()?;
        let path = String::from_utf8(output.stdout)?;
        Ok(std::path::PathBuf::from(path.trim()))
    }
}
