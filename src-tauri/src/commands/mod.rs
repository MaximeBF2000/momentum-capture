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
    println!("[Tauri Command] start_recording() called");
    
    // CRITICAL FIX: Clone the recorder (cheap - uses Arc internally) and release lock IMMEDIATELY
    // Then call start() on the clone WITHOUT holding the outer lock
    println!("[Tauri Command] Acquiring recorder lock to clone...");
    let recorder_clone = {
        let recorder_guard = recorder.lock().unwrap();
        println!("[Tauri Command] ✓ Recorder lock acquired");
        let clone = recorder_guard.clone();
        println!("[Tauri Command] ✓ Recorder cloned");
        clone
        // Lock released IMMEDIATELY when guard goes out of scope
    };
    println!("[Tauri Command] ✓ Recorder lock released (calling start() WITHOUT holding lock)");
    
    // Call start() on the clone - this takes a long time but doesn't hold the outer lock
    println!("[Tauri Command] Calling recorder.start() (no lock held)...");
    let start_result = recorder_clone.start(options.clone());
    println!("[Tauri Command] recorder.start() returned");
    
    match start_result {
        Ok(_) => {
            println!("[Tauri Command] ✓ recorder.start() succeeded");
        }
        Err(e) => {
            eprintln!("[Tauri Command] ✗ recorder.start() failed: {}", e);
            return Err(e);
        }
    }
    
    // Emit event immediately after starting recording to avoid UI delay
    app.emit("recording-started", ())?;
    
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
    
    println!("[Tauri Command] start_recording() completed");
    Ok(())
}

#[tauri::command]
pub async fn pause_recording(
    recorder: State<'_, Mutex<Recorder>>,
    app: AppHandle,
) -> AppResult<()> {
    println!("[Tauri Command] pause_recording() called");
    {
        let recorder_guard = recorder.lock().unwrap();
        recorder_guard.pause()?;
    } // Release lock immediately
    
    app.emit("recording-paused", ())?;
    Ok(())
}

#[tauri::command]
pub async fn resume_recording(
    recorder: State<'_, Mutex<Recorder>>,
    app: AppHandle,
) -> AppResult<()> {
    println!("[Tauri Command] resume_recording() called");
    {
        let recorder_guard = recorder.lock().unwrap();
        recorder_guard.resume()?;
    } // Release lock immediately
    
    app.emit("recording-resumed", ())?;
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    recorder: State<'_, Mutex<Recorder>>,
    app: AppHandle,
    camera_preview: State<'_, Mutex<CameraPreview>>,
) -> AppResult<()> {
    println!("[Tauri Command] stop_recording() called");
    println!("[Tauri Command] Attempting to acquire recorder lock...");
    
    // Try to acquire lock with timeout detection
    let lock_start = std::time::Instant::now();
    
    // First try non-blocking lock to see if it's available
    let recorder_guard = match recorder.try_lock() {
        Ok(guard) => {
            let lock_duration = lock_start.elapsed();
            println!("[Tauri Command] ✓ Recorder lock acquired immediately (took {:?})", lock_duration);
            guard
        }
        Err(std::sync::TryLockError::WouldBlock) => {
            eprintln!("[Tauri Command] ⚠ Recorder lock is held by another thread");
            eprintln!("[Tauri Command]   This indicates a potential deadlock or long-running operation");
            eprintln!("[Tauri Command]   Attempting blocking wait (this may hang if there's a deadlock)...");
            eprintln!("[Tauri Command]   If this hangs, check for:");
            eprintln!("[Tauri Command]     1. Another thread holding the lock indefinitely");
            eprintln!("[Tauri Command]     2. A deadlock where lock holder waits for this thread");
            eprintln!("[Tauri Command]     3. A panic that poisoned the lock");
            
            // Lock is held, try to wait with timeout awareness
            // Use blocking wait but log it - this is where it's hanging
            println!("[Tauri Command] Calling recorder.lock() - THIS MAY BLOCK INDEFINITELY");
            let guard = recorder.lock().unwrap();
            let lock_duration = lock_start.elapsed();
            println!("[Tauri Command] ✓ Recorder lock acquired after wait (took {:?})", lock_duration);
            guard
        }
        Err(std::sync::TryLockError::Poisoned(poisoned)) => {
            eprintln!("[Tauri Command] ✗ Recorder lock is POISONED (panic occurred while holding lock)");
            eprintln!("[Tauri Command]   Attempting to recover...");
            let guard = poisoned.into_inner();
            let lock_duration = lock_start.elapsed();
            println!("[Tauri Command] ✓ Recovered from poisoned lock (took {:?})", lock_duration);
            guard
        }
    };
    
    let recorder = recorder_guard;
    
    println!("[Tauri Command] Calling recorder.stop()...");
    let stop_result = recorder.stop();
    println!("[Tauri Command] recorder.stop() returned");
    
    let (screen_file, audio_file) = match stop_result {
        Ok(files) => {
            println!("[Tauri Command] ✓ recorder.stop() succeeded");
            files
        }
        Err(e) => {
            eprintln!("[Tauri Command] ✗ recorder.stop() failed: {}", e);
            return Err(e);
        }
    };
    
    println!("[Tauri Command] Stopped recording. Screen file: {:?}, Audio file: {:?}", screen_file, audio_file);
    
    // Check settings to determine if camera overlay should remain visible
    let settings = settings::load_settings()?;
    
    // Only stop camera preview and hide overlay if camera is disabled in settings
    if !settings.camera_enabled {
        {
            let preview = camera_preview.lock().unwrap();
            let _ = preview.stop();
        }
        if let Some(window) = app.get_webview_window("camera-overlay") {
            let _ = window.hide();
        }
    }
    
    // Emit stopped event immediately to unblock UI
    app.emit("recording-stopped", ())?;
    
    // Move file operations to background task to avoid blocking UI
    let app_clone = app.clone();
    let screen_file_clone = screen_file.clone();
    tokio::spawn(async move {
        // With muxed recording, screen_file and audio_file are the same file
        // Just copy it to the downloads directory (no merge needed)
        let downloads_dir = match get_downloads_dir() {
            Ok(dir) => dir,
            Err(e) => {
                let error = format!("Failed to get downloads directory: {}", e);
                println!("ERROR: {}", error);
                let _ = app_clone.emit("recording-error", serde_json::json!({
                    "message": error
                }));
                return;
            }
        };
        
        println!("Downloads directory: {:?}", downloads_dir);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let final_path = downloads_dir.join(format!("momentum-recording-{}.mp4", timestamp));
        println!("Final output path: {:?}", final_path);
        
        // Check if the muxed file exists
        if !screen_file_clone.exists() {
            let error = format!("Recording file not found: {:?}", screen_file_clone);
            println!("ERROR: {}", error);
            let _ = app_clone.emit("recording-error", serde_json::json!({
                "message": error
            }));
            return;
        }
        
        println!("Muxed recording file exists. Copying to downloads...");
        
        // Copy the muxed file to downloads (no merge needed)
        match std::fs::copy(&screen_file_clone, &final_path) {
            Ok(_) => {
                // Verify the output file was created
                if !final_path.exists() {
                    let error = format!("Output file was not created: {:?}", final_path);
                    println!("ERROR: {}", error);
                    let _ = app_clone.emit("recording-error", serde_json::json!({
                        "message": error
                    }));
                    return;
                }
                
                println!("Successfully copied output file: {:?}", final_path);
                
                // Clean up temp file (only one file now since it's muxed)
                let _ = std::fs::remove_file(&screen_file_clone);
                
                let _ = app_clone.emit("recording-saved", serde_json::json!({
                    "path": final_path.to_string_lossy()
                }));
            }
            Err(e) => {
                let error = format!("Failed to copy recording file: {}", e);
                println!("ERROR: {}", error);
                let _ = app_clone.emit("recording-error", serde_json::json!({
                    "message": error
                }));
            }
        }
    });
    
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
    println!("[Tauri Command] toggle_microphone() called");
    {
        let recorder_guard = recorder.lock().unwrap();
        recorder_guard.toggle_microphone(enabled)
    } // Release lock immediately
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
