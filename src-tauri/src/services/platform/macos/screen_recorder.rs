use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::{AppError, AppResult};

use super::ffmpeg::find_ffmpeg;

pub struct ScreenRecorder {
    screen_process: Arc<Mutex<Option<std::process::Child>>>,
    screen_file: Option<PathBuf>,
    is_paused: Arc<Mutex<bool>>,
}

impl ScreenRecorder {
    pub fn new() -> Self {
        Self {
            screen_process: Arc::new(Mutex::new(None)),
            screen_file: None,
            is_paused: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start(&mut self, output_path: &PathBuf) -> AppResult<()> {
        // Use AVFoundation via ffmpeg for screen capture
        // TODO: Replace with native AVFoundation/ScreenCaptureKit APIs
        let ffmpeg_path = find_ffmpeg();
        let mut cmd = Command::new(&ffmpeg_path);
        
        cmd.args(&[
            "-f", "avfoundation",
            "-framerate", "30",
            "-i", "1:none", // Screen capture device (device 1 is "Capture screen 0")
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-crf", "23",
            "-pix_fmt", "yuv420p",
            "-movflags", "+faststart", // Enable fast start for better playback
            "-avoid_negative_ts", "make_zero", // Ensure timestamps start at 0
            "-y",
        ]);
        cmd.arg(output_path);
        cmd.stderr(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stdin(std::process::Stdio::piped()); // Enable stdin for graceful shutdown

        let mut process = cmd.spawn()
            .map_err(|e| AppError::Recording(format!("Failed to start screen capture: {}", e)))?;

        // Capture stderr in a background thread for debugging
        let stderr = process.stderr.take();
        if let Some(mut stderr) = stderr {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buffer = [0u8; 2048];
                loop {
                    match stderr.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            let msg = String::from_utf8_lossy(&buffer[..n]);
                            if msg.contains("error") || msg.contains("Error") || msg.contains("ERROR") {
                                eprintln!("Screen FFmpeg stderr: {}", msg);
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Wait a bit to check if process started successfully
        std::thread::sleep(Duration::from_millis(500));
        if let Ok(Some(status)) = process.try_wait() {
            return Err(AppError::Recording(format!("Screen capture process exited immediately with status: {:?}. Check screen recording permissions in System Settings > Privacy & Security.", status)));
        }

        println!("Screen recording process started successfully (PID: {})", process.id());
        *self.screen_process.lock().unwrap() = Some(process);
        self.screen_file = Some(output_path.clone());

        Ok(())
    }

    pub fn pause(&mut self) -> AppResult<()> {
        if let Some(mut process) = self.screen_process.lock().unwrap().take() {
            let _ = process.kill();
            std::thread::sleep(Duration::from_millis(500));
            let _ = process.wait();
        }
        *self.is_paused.lock().unwrap() = true;
        Ok(())
    }

    pub fn resume(&mut self) -> AppResult<()> {
        let output_path = self.screen_file.as_ref()
            .ok_or_else(|| AppError::Recording("No screen file path".to_string()))?;
        
        // Create a new temp file for the resumed segment
        // Note: This is a limitation - proper pause/resume would require concatenating segments
        // For now, we'll just continue recording to the same file (overwrites paused segment)
        let ffmpeg_path = find_ffmpeg();
        let mut cmd = Command::new(&ffmpeg_path);
        cmd.args(&[
            "-f", "avfoundation",
            "-framerate", "30",
            "-i", "1:none", // Screen capture device (device 1 is "Capture screen 0")
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-crf", "23",
            "-pix_fmt", "yuv420p",
            "-movflags", "+faststart",
            "-y",
        ]);
        cmd.arg(output_path);
        cmd.stderr(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());

        let mut process = cmd.spawn()
            .map_err(|e| AppError::Recording(format!("Failed to resume screen capture: {}", e)))?;

        std::thread::sleep(Duration::from_millis(500));
        if let Ok(Some(_)) = process.try_wait() {
            return Err(AppError::Recording("Screen capture process exited immediately".to_string()));
        }

        *self.screen_process.lock().unwrap() = Some(process);
        *self.is_paused.lock().unwrap() = false;
        Ok(())
    }

    pub fn stop(&mut self) -> AppResult<PathBuf> {
        let screen_path = self.screen_file.clone()
            .ok_or_else(|| AppError::Recording("No screen file path".to_string()))?;
            
        if let Some(mut process) = self.screen_process.lock().unwrap().take() {
            println!("Stopping screen recording process (PID: {})...", process.id());
            
            // FFmpeg with avfoundation on macOS doesn't reliably respond to 'q' via stdin
            // Use SIGINT (Ctrl+C) instead, which FFmpeg handles gracefully
            let pid = process.id();
            
            // Try sending SIGINT first (graceful shutdown)
            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                let kill_result = Command::new("kill")
                    .args(&["-INT", &pid.to_string()])
                    .output();
                
                match kill_result {
                    Ok(output) => {
                        if output.status.success() {
                            println!("Sent SIGINT to screen recording process (PID: {})", pid);
                        } else {
                            let stderr = String::from_utf8_lossy(&output.stderr);
                            println!("Failed to send SIGINT: {}", stderr);
                        }
                    }
                    Err(e) => {
                        println!("Failed to execute kill command: {}", e);
                    }
                }
            }
            
            // Wait for FFmpeg to finalize the file (SIGINT allows graceful shutdown)
            let start = std::time::Instant::now();
            let mut process_finished = false;
            let timeout_seconds = 5; // Give FFmpeg time to finalize
            
            loop {
                match process.try_wait() {
                    Ok(Some(status)) => {
                        println!("Screen recording process finished with status: {:?}", status);
                        process_finished = true;
                        break; // Process finished
                    }
                    Ok(None) => {
                        if start.elapsed().as_secs() >= timeout_seconds {
                            // Timeout - try SIGTERM, then SIGKILL
                            println!("Screen recording timeout after {}s, trying SIGTERM...", timeout_seconds);
                            #[cfg(target_os = "macos")]
                            {
                                let _ = Command::new("kill")
                                    .args(&["-TERM", &pid.to_string()])
                                    .output();
                                std::thread::sleep(Duration::from_millis(500));
                            }
                            
                            // If still running, force kill
                            if process.try_wait().ok().flatten().is_none() {
                                println!("Force killing screen recording process...");
                                if let Err(e) = process.kill() {
                                    println!("Failed to kill process: {}", e);
                                }
                            }
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        println!("Screen recording process error: {:?}, killing", e);
                        let _ = process.kill();
                        break;
                    }
                }
            }
            
            // Wait for process to finish if not already finished
            if !process_finished {
                println!("Waiting for screen recording process to exit...");
                let _ = process.wait();
            }
            
            // Give FFmpeg extra time to finalize the file
            println!("Waiting for file finalization...");
            std::thread::sleep(Duration::from_millis(2000)); // Increased to 2 seconds
            
            // Verify file exists
            if !screen_path.exists() {
                return Err(AppError::Recording(format!("Screen recording file was not created: {:?}", screen_path)));
            }
            
            // Check file size to ensure it's not empty
            if let Ok(metadata) = std::fs::metadata(&screen_path) {
                if metadata.len() == 0 {
                    return Err(AppError::Recording(format!("Screen recording file is empty: {:?}", screen_path)));
                }
                println!("Screen recording file created successfully: {:?} ({} bytes)", screen_path, metadata.len());
            }
        } else {
            println!("No screen recording process found");
        }

        self.screen_file.take()
            .ok_or_else(|| AppError::Recording("No screen file path".to_string()))
    }
}
