use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::{AppError, AppResult};

use super::ffmpeg::find_ffmpeg;

pub struct AudioRecorder {
    audio_process: Arc<Mutex<Option<std::process::Child>>>,
    audio_file: Option<PathBuf>,
    is_recording: Arc<Mutex<bool>>,
    mic_enabled: Arc<Mutex<bool>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            audio_process: Arc::new(Mutex::new(None)),
            audio_file: None,
            is_recording: Arc::new(Mutex::new(false)),
            mic_enabled: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start(&mut self, output_path: &PathBuf, mic_enabled: bool) -> AppResult<()> {
        *self.is_recording.lock().unwrap() = true;
        *self.mic_enabled.lock().unwrap() = mic_enabled;
        
        // Use AVFoundation via ffmpeg for audio capture
        // TODO: Replace with native AVFoundation APIs
        let ffmpeg_path = find_ffmpeg();
        let mut cmd = Command::new(&ffmpeg_path);
        
        if mic_enabled {
            cmd.args(&[
                "-f", "avfoundation",
                "-i", ":0", // Microphone only
                "-c:a", "aac",
                "-b:a", "128k",
                "-y",
            ]);
        } else {
            // Record silence for timestamp alignment
            cmd.args(&[
                "-f", "lavfi",
                "-i", "anullsrc=channel_layout=stereo:sample_rate=44100",
                "-c:a", "aac",
                "-b:a", "128k",
                "-t", "3600", // Max 1 hour of silence
                "-y",
            ]);
        }
        cmd.arg(output_path);
        cmd.stderr(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stdin(std::process::Stdio::piped()); // Enable stdin for graceful shutdown

        let mut process = cmd.spawn()
            .map_err(|e| AppError::Recording(format!("Failed to start audio capture: {}", e)))?;

        // Wait a bit to check if process started successfully (same as screen recording)
        std::thread::sleep(Duration::from_millis(500));
        if let Ok(Some(_)) = process.try_wait() {
            return Err(AppError::Recording("Audio capture process exited immediately. Check microphone permissions in System Settings > Privacy & Security.".to_string()));
        }

        *self.audio_process.lock().unwrap() = Some(process);
        self.audio_file = Some(output_path.clone());

        Ok(())
    }

    pub fn stop(&mut self) -> AppResult<PathBuf> {
        *self.is_recording.lock().unwrap() = false;
        
        let audio_path = self.audio_file.clone()
            .ok_or_else(|| AppError::Recording("No audio file path".to_string()))?;
        
        if let Some(mut process) = self.audio_process.lock().unwrap().take() {
            println!("Stopping audio recording process (PID: {})...", process.id());
            
            // Use SIGINT for graceful shutdown (same as screen recorder)
            let pid = process.id();
            
            #[cfg(target_os = "macos")]
            {
                let kill_result = Command::new("kill")
                    .args(&["-INT", &pid.to_string()])
                    .output();
                
                match kill_result {
                    Ok(output) => {
                        if output.status.success() {
                            println!("Sent SIGINT to audio recording process (PID: {})", pid);
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
            
            // Wait for FFmpeg to finalize the file
            let start = std::time::Instant::now();
            let mut process_finished = false;
            let timeout_seconds = 5;
            
            loop {
                match process.try_wait() {
                    Ok(Some(status)) => {
                        println!("Audio recording process finished with status: {:?}", status);
                        process_finished = true;
                        break; // Process finished
                    }
                    Ok(None) => {
                        if start.elapsed().as_secs() >= timeout_seconds {
                            // Timeout - try SIGTERM, then SIGKILL
                            println!("Audio recording timeout after {}s, trying SIGTERM...", timeout_seconds);
                            #[cfg(target_os = "macos")]
                            {
                                let _ = Command::new("kill")
                                    .args(&["-TERM", &pid.to_string()])
                                    .output();
                                std::thread::sleep(Duration::from_millis(500));
                            }
                            
                            // If still running, force kill
                            if process.try_wait().ok().flatten().is_none() {
                                println!("Force killing audio recording process...");
                                if let Err(e) = process.kill() {
                                    println!("Failed to kill process: {}", e);
                                }
                            }
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        println!("Audio recording process error: {:?}, killing", e);
                        let _ = process.kill();
                        break;
                    }
                }
            }
            
            // Wait for process to finish if not already finished
            if !process_finished {
                println!("Waiting for audio recording process to exit...");
                let _ = process.wait();
            }
            
            // Give FFmpeg extra time to finalize the file
            std::thread::sleep(Duration::from_millis(1000));
            
            // Verify file exists
            if !audio_path.exists() {
                return Err(AppError::Recording(format!("Audio recording file was not created: {:?}", audio_path)));
            }
            
            // Check file size to ensure it's not empty
            if let Ok(metadata) = std::fs::metadata(&audio_path) {
                if metadata.len() == 0 {
                    return Err(AppError::Recording(format!("Audio recording file is empty: {:?}", audio_path)));
                }
                println!("Audio recording file created successfully: {:?} ({} bytes)", audio_path, metadata.len());
            }
        }

        self.audio_file.take()
            .ok_or_else(|| AppError::Recording("No audio file path".to_string()))
    }

    pub fn pause(&mut self) -> AppResult<()> {
        // Stop current audio recording
        if let Some(mut process) = self.audio_process.lock().unwrap().take() {
            // Try to gracefully stop FFmpeg by sending 'q' to stdin
            if let Some(mut stdin) = process.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(b"q");
                let _ = stdin.flush();
            }
            
            // Wait for process to finish
            let start = std::time::Instant::now();
            loop {
                match process.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if start.elapsed().as_secs() > 5 {
                            let _ = process.kill();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(_) => {
                        let _ = process.kill();
                        break;
                    }
                }
            }
            let _ = process.wait();
        }
        *self.is_recording.lock().unwrap() = false;
        Ok(())
    }

    pub fn resume(&mut self, output_path: &PathBuf, mic_enabled: bool) -> AppResult<()> {
        // Start new audio recording segment
        self.start(output_path, mic_enabled)
    }

    pub fn toggle_microphone(&mut self, output_path: &PathBuf, enabled: bool) -> AppResult<()> {
        // Stop current recording
        self.pause()?;
        
        // Start new recording with new mic state
        self.resume(output_path, enabled)
    }
}
