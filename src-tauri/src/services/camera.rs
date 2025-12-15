use crate::error::{AppError, AppResult};
#[cfg(target_os = "macos")]
use crate::services::platform::device_resolver;
use std::sync::{Arc, Mutex};
use std::process::{Command, Stdio};
use std::io::Read;
use tauri::{AppHandle, Emitter};
use std::thread;
use base64::{Engine as _, engine::general_purpose};
use serde_json;

pub struct CameraPreview {
    is_running: Arc<Mutex<bool>>,
    app_handle: Arc<Mutex<Option<AppHandle>>>,
}

impl CameraPreview {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(Mutex::new(false)),
            app_handle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_app_handle(&mut self, app: AppHandle) {
        *self.app_handle.lock().unwrap() = Some(app);
    }

    pub fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }

    fn find_ffmpeg() -> Option<String> {
        // Try common macOS FFmpeg locations
        let possible_paths = vec![
            "ffmpeg", // System PATH
            "/opt/homebrew/bin/ffmpeg", // Homebrew on Apple Silicon
            "/usr/local/bin/ffmpeg", // Homebrew on Intel
            "/usr/bin/ffmpeg", // System location
        ];
        
        for path in possible_paths {
            if Command::new(path).arg("-version").output().is_ok() {
                println!("[CameraPreview] Found FFmpeg at: {}", path);
                return Some(path.to_string());
            }
        }
        
        None
    }

    pub fn start(&self) -> AppResult<()> {
        let mut is_running = self.is_running.lock().unwrap();
        
        if *is_running {
            // Already running, just return success
            return Ok(());
        }

        // Find FFmpeg executable
        let ffmpeg_path = Self::find_ffmpeg()
            .ok_or_else(|| AppError::Camera(
                "FFmpeg is not installed or not found in PATH. Please install FFmpeg via Homebrew: brew install ffmpeg".to_string()
            ))?;
        
        println!("[CameraPreview] Starting camera preview with FFmpeg: {}", ffmpeg_path);

        // Resolve camera device index
        #[cfg(target_os = "macos")]
        let camera_index = {
            match device_resolver::resolve_avf_indices() {
                Ok(devices) => {
                    match devices.get_camera_index() {
                        Ok(idx) => {
                            println!("[CameraPreview] Resolved built-in camera index: {}", idx);
                            idx
                        }
                        Err(e) => {
                            eprintln!("[CameraPreview] Failed to resolve camera index: {}, falling back to 0", e);
                            0
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[CameraPreview] Failed to resolve device indices: {}, falling back to 0", e);
                    0
                }
            }
        };

        *is_running = true;

        let is_running_clone = self.is_running.clone();
        let app_handle_clone = self.app_handle.clone();

        // Start FFmpeg in a separate thread
        let ffmpeg_path_clone = ffmpeg_path.clone();
        #[cfg(target_os = "macos")]
        let camera_index_clone = camera_index;
        thread::spawn(move || {
            let mut cmd = Command::new(&ffmpeg_path_clone);
            
            #[cfg(target_os = "macos")]
            {
                // Use resolved built-in camera index
                // Use 30 fps for smooth preview, lower quality for speed
                cmd.args(&[
                    "-f", "avfoundation",
                    "-framerate", "30",
                    "-video_size", "640x480",
                    "-i", &format!("{}:", camera_index_clone), // Built-in camera, no audio
                    "-vf", "fps=30", // Keep at 30 fps for smooth preview
                    "-f", "image2pipe",
                    "-vcodec", "mjpeg",
                    "-q:v", "3", // Lower quality number = higher quality but faster encoding
                    "-"
                ]);
            }
            
            #[cfg(target_os = "windows")]
            {
                cmd.args(&[
                    "-f", "dshow",
                    "-i", "video=Integrated Camera",
                    "-vf", "fps=10",
                    "-f", "image2pipe",
                    "-vcodec", "mjpeg",
                    "-q:v", "5",
                    "-"
                ]);
            }
            
            #[cfg(target_os = "linux")]
            {
                cmd.args(&[
                    "-f", "v4l2",
                    "-i", "/dev/video0",
                    "-vf", "fps=10",
                    "-f", "image2pipe",
                    "-vcodec", "mjpeg",
                    "-q:v", "5",
                    "-"
                ]);
            }
            
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped()); // Capture stderr for debugging
            
            let mut process = match cmd.spawn() {
                Ok(p) => {
                    println!("[CameraPreview] FFmpeg process spawned successfully (PID: {})", p.id());
                    p
                }
                Err(e) => {
                    let error_msg = format!("Failed to spawn camera FFmpeg process: {}. FFmpeg path used: {}", e, ffmpeg_path_clone);
                    eprintln!("[CameraPreview] ERROR: {}", error_msg);
                    *is_running_clone.lock().unwrap() = false;
                    
                    // Try to emit error to frontend
                    if let Some(app) = app_handle_clone.lock().unwrap().as_ref() {
                        let _ = app.emit("camera-error", serde_json::json!({
                            "message": error_msg
                        }));
                    }
                    return;
                }
            };
            
            // Read stderr in a separate thread to capture errors (but don't log everything)
            let stderr = process.stderr.take();
            if let Some(mut stderr) = stderr {
                let is_running_err = is_running_clone.clone();
                std::thread::spawn(move || {
                    let mut buffer = [0u8; 1024];
                    while *is_running_err.lock().unwrap() {
                        if let Ok(n) = stderr.read(&mut buffer) {
                            if n > 0 {
                                let error_msg = String::from_utf8_lossy(&buffer[..n]);
                                // Only log actual errors, not warnings or info
                                if error_msg.contains("Error") || error_msg.contains("error") {
                                    eprintln!("Camera FFmpeg error: {}", error_msg);
                                }
                            }
                        }
                    }
                });
            }

            let mut stdout = process.stdout.take().unwrap();
            let mut frame_id = 0u64;
            let mut jpeg_data = Vec::with_capacity(50000); // Pre-allocate for typical JPEG size
            let mut buffer = [0u8; 65536]; // Larger buffer for better performance
            let mut found_start = false;
            let mut last_frame_time = std::time::Instant::now();

            while *is_running_clone.lock().unwrap() {
                match stdout.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        for i in 0..n {
                            let byte = buffer[i];
                            
                            // Look for JPEG start marker (FF D8)
                            if !found_start && i < n - 1 && buffer[i] == 0xFF && buffer[i + 1] == 0xD8 {
                                found_start = true;
                                jpeg_data.clear();
                                jpeg_data.push(byte);
                            } else if found_start {
                                jpeg_data.push(byte);
                                
                                // Check for JPEG end marker (FF D9)
                                if jpeg_data.len() >= 2 && 
                                   jpeg_data[jpeg_data.len() - 2] == 0xFF && 
                                   jpeg_data[jpeg_data.len() - 1] == 0xD9 {
                                    // Complete JPEG frame found
                                    // Only emit if enough time has passed (throttle to ~30 FPS max)
                                    let now = std::time::Instant::now();
                                    if now.duration_since(last_frame_time).as_millis() >= 33 {
                                        let base64_frame = general_purpose::STANDARD.encode(&jpeg_data);
                                        
                                        if let Some(app) = app_handle_clone.lock().unwrap().as_ref() {
                                            // Emit to all windows (camera-overlay will receive it)
                                            match app.emit("camera-frame", serde_json::json!({
                                                "id": frame_id,
                                                "width": 640,
                                                "height": 480,
                                                "format": "jpeg",
                                                "data_base64": base64_frame
                                            })) {
                                                Ok(_) => {
                                                    // Only log first frame to avoid spam
                                                    if frame_id == 0 {
                                                        println!("[CameraPreview] First camera frame emitted successfully");
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!("[CameraPreview] ERROR: Failed to emit camera frame: {}", e);
                                                }
                                            }
                                        } else {
                                            if frame_id == 0 {
                                                eprintln!("[CameraPreview] WARNING: App handle not available, cannot emit frames");
                                            }
                                        }
                                        
                                        frame_id += 1;
                                        last_frame_time = now;
                                    }
                                    
                                    found_start = false;
                                    jpeg_data.clear();
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(())
    }

    pub fn stop(&self) -> AppResult<()> {
        let mut is_running = self.is_running.lock().unwrap();
        
        if !*is_running {
            // Already stopped, return success
            return Ok(());
        }

        *is_running = false;
        
        // Note: The FFmpeg process will detect is_running=false and exit naturally
        // We don't need to kill it explicitly as the thread checks the flag
        
        Ok(())
    }
}
