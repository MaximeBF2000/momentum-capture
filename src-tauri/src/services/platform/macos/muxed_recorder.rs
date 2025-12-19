use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::services::platform::{device_resolver, screencapturekit_recorder};

use super::ffmpeg::find_ffmpeg;

pub struct MuxedRecorder {
    pub recording_process: Arc<Mutex<Option<std::process::Child>>>,  // Made public for deadlock fix
    pub output_file: Option<PathBuf>,  // Made public for deadlock fix
    is_paused: Arc<Mutex<bool>>,
    pub mic_enabled: Arc<Mutex<bool>>,  // Made public for deadlock fix
}

impl MuxedRecorder {
    pub fn new() -> Self {
        Self {
            recording_process: Arc::new(Mutex::new(None)),
            output_file: None,
            is_paused: Arc::new(Mutex::new(false)),
            mic_enabled: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start(&mut self, output_path: &PathBuf, mic_enabled: bool) -> AppResult<()> {
        println!("[MuxedRecorder] start() called");
        *self.mic_enabled.lock().unwrap() = mic_enabled;
        self.output_file = Some(output_path.clone());
        
        println!("[MuxedRecorder] Starting recording:");
        println!("[MuxedRecorder]   Output: {:?}", output_path);
        println!("[MuxedRecorder]   Mic enabled: {}", mic_enabled);
        
        // Check if ScreenCaptureKit is available (macOS 12.3+)
        let use_screencapturekit = screencapturekit_recorder::is_available();
        
        if use_screencapturekit {
            println!("[MuxedRecorder] ScreenCaptureKit available (macOS 12.3+)");
            println!("[MuxedRecorder] Using ScreenCaptureKit for native system audio capture");
            println!("[MuxedRecorder] Calling screencapturekit_recorder::start_recording()...");
            println!("[MuxedRecorder]   NOTE: This does heavy setup work but uses its own STATE mutex");
            println!("[MuxedRecorder]   The synchronized_recorder lock will be released immediately after this returns");
            // Try ScreenCaptureKit first, fall back to FFmpeg if not fully implemented
            // This call does heavy setup work but uses its own internal STATE mutex
            // The caller should release synchronized_recorder lock immediately after this returns
            let start_result = screencapturekit_recorder::start_recording(output_path, mic_enabled);
            println!("[MuxedRecorder] screencapturekit_recorder::start_recording() returned");
            
            match start_result {
                Ok(()) => {
                    println!("[MuxedRecorder] ✓ ScreenCaptureKit start succeeded");
                    return Ok(());
                }
                Err(e) => {
                    println!("[MuxedRecorder] ScreenCaptureKit implementation incomplete, falling back to FFmpeg");
                    println!("[MuxedRecorder] Error: {}", e);
                    // Continue to FFmpeg fallback
                }
            }
        } else {
            println!("[MuxedRecorder] ScreenCaptureKit not available (requires macOS 12.3+), using FFmpeg");
            println!("[MuxedRecorder] Note: System audio capture requires macOS 12.3+ with ScreenCaptureKit");
        }
        
        // Fallback to FFmpeg with AVFoundation (no system audio on older macOS)
        println!("[MuxedRecorder] Using FFmpeg with AVFoundation");
        
        // Resolve device indices using Swift resolver
        let devices = device_resolver::resolve_avf_indices()?;
        let screen_index = devices.get_screen_index()?;
        let mic_index = devices.get_mic_index()?;
        
        println!("[MuxedRecorder] Resolved device indices:");
        println!("[MuxedRecorder]   Screen: {}", screen_index);
        println!("[MuxedRecorder]   Built-in mic: {}", mic_index);
        println!("[MuxedRecorder]   System audio: Not available (requires macOS 12.3+ for ScreenCaptureKit)");
        
        // Single FFmpeg process that muxes screen and audio together
        let ffmpeg_path = find_ffmpeg();
        let mut cmd = Command::new(&ffmpeg_path);
        
        println!("[MuxedRecorder] Audio configuration:");
        println!("[MuxedRecorder]   Mic enabled: {}", mic_enabled);
        println!("[MuxedRecorder]   System audio: Not available (requires macOS 12.3+)");
        
        // Single input: Screen + Microphone (or screen only if mic disabled)
        let audio_index = mic_index; // Use mic index (will be muted if mic_enabled is false)
        println!("[MuxedRecorder] Configuring single input: Screen device {}, Audio device {} (built-in mic)", screen_index, audio_index);
        
        cmd.args(&[
            "-f", "avfoundation",
            "-framerate", "30",
            "-capture_cursor", "1",
            "-capture_mouse_clicks", "0",
            "-i", &format!("{}:{}", screen_index, audio_index), // Screen + Audio
        ]);
        
        // Video filter: Convert pixel format from uyvy422 to yuv420p
        // Use scale filter to ensure proper color space conversion
        // Don't use fps filter here - framerate is already set on input
        println!("[MuxedRecorder] Configuring video filter: format conversion");
        cmd.args(&[
            "-vf", "scale=iw:ih:flags=fast_bilinear,format=yuv420p", // Scale ensures proper conversion, then format
        ]);
        
        // Video codec options
        println!("[MuxedRecorder] Configuring video codec: H.264");
        cmd.args(&[
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-crf", "23",
            "-g", "30", // GOP size (keyframe every 30 frames)
            "-r", "30", // Explicit output frame rate
            "-pix_fmt", "yuv420p", // Explicitly set output pixel format
        ]);
        
        // Audio processing
        println!("[MuxedRecorder] Configuring audio codec: AAC");
        if mic_enabled {
            // Single audio stream: microphone only
            println!("[MuxedRecorder]   Audio: Mic only");
            cmd.args(&[
                "-c:a", "aac",
                "-b:a", "128k",
                "-ar", "48000", // Sample rate
            ]);
        } else {
            // Mic disabled: mute audio
            println!("[MuxedRecorder]   Audio: Muted (volume=0)");
            cmd.args(&[
                "-af", "volume=0", // Muted - set volume to 0
                "-c:a", "aac",
                "-b:a", "128k",
                "-ar", "48000", // Sample rate
            ]);
        }
        
        // Output container options
        println!("[MuxedRecorder] Configuring output container: MP4");
        cmd.args(&[
            "-movflags", "+faststart",
            "-avoid_negative_ts", "make_zero",
            "-y", // Overwrite output file
        ]);
        
        cmd.arg(output_path);
        
        // Log key configuration for debugging
        println!("[MuxedRecorder] Configuration summary:");
        println!("[MuxedRecorder]   Input: avfoundation device {}:{} (screen + mic)", screen_index, mic_index);
        println!("[MuxedRecorder]   Audio: Mic only, {}", if mic_enabled { "enabled" } else { "muted" });
        println!("[MuxedRecorder]   Video: Auto-detect resolution@30fps, H.264");
        println!("[MuxedRecorder]   Audio codec: AAC 48kHz");
        println!("[MuxedRecorder]   Output: {:?}", output_path);
        
        // Capture stderr for debugging (we'll log it)
        cmd.stderr(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::null());
        cmd.stdin(std::process::Stdio::piped());

        println!("[MuxedRecorder] Spawning FFmpeg process...");
        let mut process = cmd.spawn()
            .map_err(|e| {
                let error_msg = format!("Failed to start muxed recording: {}. Check screen and microphone permissions in System Settings > Privacy & Security.", e);
                println!("[MuxedRecorder] ERROR: {}", error_msg);
                AppError::Recording(error_msg)
            })?;
        
        println!("[MuxedRecorder] FFmpeg process spawned successfully (PID: {})", process.id());

        // Capture stderr in a background thread for debugging - log everything to see what's happening
        let stderr = process.stderr.take();
        if let Some(mut stderr) = stderr {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buffer = [0u8; 4096];
                let mut partial_line = String::new();
                loop {
                    match stderr.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            let msg = String::from_utf8_lossy(&buffer[..n]);
                            partial_line.push_str(&msg);
                            
                            // Process complete lines
                            while let Some(newline_pos) = partial_line.find('\n') {
                                let line = partial_line[..newline_pos].trim();
                                if !line.is_empty() {
                                    // Log ALL FFmpeg output for debugging (especially important for black screen issue)
                                    let lower = line.to_lowercase();
                                    if lower.contains("error") || 
                                       lower.contains("failed") ||
                                       lower.contains("warning") ||
                                       lower.contains("cannot") ||
                                       lower.contains("invalid") ||
                                       lower.contains("not found") {
                                        eprintln!("[FFmpeg ERROR/WARNING] {}", line);
                                    } else {
                                        // Log info messages too for debugging
                                        eprintln!("[FFmpeg INFO] {}", line);
                                    }
                                }
                                partial_line = partial_line[newline_pos + 1..].to_string();
                            }
                        }
                        Err(_) => break,
                    }
                }
                // Print remaining partial line
                if !partial_line.trim().is_empty() {
                    eprintln!("[FFmpeg] {}", partial_line.trim());
                }
            });
        }

        // Wait a bit to check if process started successfully
        println!("[MuxedRecorder] Waiting for process initialization...");
        std::thread::sleep(Duration::from_millis(1000)); // Increased wait time
        
        if let Ok(Some(status)) = process.try_wait() {
            let error_msg = format!("Muxed recording process exited immediately with status: {:?}. This usually indicates:\n1. Missing screen recording permissions (System Settings > Privacy & Security > Screen Recording)\n2. Missing microphone permissions (System Settings > Privacy & Security > Microphone)\n3. Invalid device indices (check device list above)", status);
            println!("[MuxedRecorder] ERROR: {}", error_msg);
            return Err(AppError::Recording(error_msg));
        }

        println!("[MuxedRecorder] Process initialized successfully - recording started");
        *self.recording_process.lock().unwrap() = Some(process);
        self.output_file = Some(output_path.clone());

        Ok(())
    }

    pub fn pause(&mut self) -> AppResult<()> {
        if let Some(mut process) = self.recording_process.lock().unwrap().take() {
            let pid = process.id();
            
            // Send SIGINT for graceful shutdown
            #[cfg(target_os = "macos")]
            {
                let kill_result = Command::new("kill")
                    .args(&["-INT", &pid.to_string()])
                    .output();
                
                if let Err(e) = kill_result {
                    println!("Failed to send SIGINT: {}", e);
                }
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
        *self.is_paused.lock().unwrap() = true;
        Ok(())
    }

    pub fn resume(&mut self, output_path: &PathBuf, mic_enabled: bool) -> AppResult<()> {
        // Start new recording segment (pause/resume creates gaps - acceptable for MVP)
        self.start(output_path, mic_enabled)?;
        *self.is_paused.lock().unwrap() = false;
        Ok(())
    }

    pub fn stop(&mut self) -> AppResult<PathBuf> {
        println!("[MuxedRecorder] stop() called");

        let output_path = match self.output_file.clone() {
            Some(path) => {
                println!("[MuxedRecorder] ✓ Output path: {:?}", path);
                path
            }
            None => {
                eprintln!("[MuxedRecorder] ✗ No output file path set");
                return Err(AppError::Recording("No output file path".to_string()));
            }
        };

        // ALWAYS try ScreenCaptureKit stop first - don't rely on is_recording_active()
        // because the FFmpeg process might have exited which clears the STATE
        println!("[MuxedRecorder] Calling screencapturekit_recorder::stop_recording() (always try this first)...");
        let stop_result = screencapturekit_recorder::stop_recording();
        println!("[MuxedRecorder] stop_recording() returned");

        match stop_result {
            Ok(_) => {
                println!("[MuxedRecorder] ✓ stop_recording() succeeded");
                println!("[MuxedRecorder] Returning output path: {:?}", output_path);
                return Ok(output_path);
            }
            Err(e) => {
                // If ScreenCaptureKit stop fails (e.g., wasn't active), try fallback
                println!("[MuxedRecorder] ScreenCaptureKit stop failed: {}, trying fallback FFmpeg stop", e);
            }
        }
            
        if let Some(mut process) = self.recording_process.lock().unwrap().take() {
            println!("Stopping muxed recording process (PID: {})...", process.id());
            
            let pid = process.id();
            
            // Send SIGINT for graceful shutdown
            #[cfg(target_os = "macos")]
            {
                let kill_result = Command::new("kill")
                    .args(&["-INT", &pid.to_string()])
                    .output();
                
                match kill_result {
                    Ok(output) => {
                        if output.status.success() {
                            println!("Sent SIGINT to muxed recording process (PID: {})", pid);
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
            // FFmpeg needs time to flush buffers and finalize the MP4 container
            let start = std::time::Instant::now();
            let mut process_finished = false;
            let timeout_seconds = 10; // Increased timeout for proper finalization
            
            loop {
                match process.try_wait() {
                    Ok(Some(status)) => {
                        println!("Muxed recording process finished with status: {:?}", status);
                        process_finished = true;
                        break;
                    }
                    Ok(None) => {
                        if start.elapsed().as_secs() >= timeout_seconds {
                            println!("Muxed recording timeout after {}s, trying SIGTERM...", timeout_seconds);
                            #[cfg(target_os = "macos")]
                            {
                                let _ = Command::new("kill")
                                    .args(&["-TERM", &pid.to_string()])
                                    .output();
                                std::thread::sleep(Duration::from_millis(1000));
                            }
                            
                            if process.try_wait().ok().flatten().is_none() {
                                println!("Force killing muxed recording process...");
                                if let Err(e) = process.kill() {
                                    println!("Failed to kill process: {}", e);
                                }
                            }
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(200));
                    }
                    Err(e) => {
                        println!("Muxed recording process error: {:?}, killing", e);
                        let _ = process.kill();
                        break;
                    }
                }
            }
            
            if !process_finished {
                println!("Waiting for muxed recording process to exit...");
                let _ = process.wait();
            }
            
            // Give FFmpeg extra time to finalize the file and write metadata
            println!("Waiting for file finalization...");
            std::thread::sleep(Duration::from_millis(3000)); // Increased wait time
            
            // Verify file exists
            if !output_path.exists() {
                return Err(AppError::Recording(format!("Muxed recording file was not created: {:?}", output_path)));
            }
            
            // Check file size
            if let Ok(metadata) = std::fs::metadata(&output_path) {
                if metadata.len() == 0 {
                    return Err(AppError::Recording(format!("Muxed recording file is empty: {:?}", output_path)));
                }
                println!("Muxed recording file created successfully: {:?} ({} bytes)", output_path, metadata.len());
            }
        } else {
            println!("No muxed recording process found");
        }

        self.output_file.take()
            .ok_or_else(|| AppError::Recording("No output file path".to_string()))
    }

    pub fn set_mic_enabled(&mut self, enabled: bool) -> AppResult<()> {
        *self.mic_enabled.lock().unwrap() = enabled;
        
        // To change mic state during recording, we need to restart the process
        // This is a limitation - ideally we'd use FFmpeg filter_complex to toggle,
        // but for simplicity we'll restart with new settings
        let output_path = self.output_file.clone();
        if let Some(output_path) = output_path {
            let is_paused = *self.is_paused.lock().unwrap();
            if !is_paused {
                // Pause, then resume with new mic state
                self.pause()?;
                self.resume(&output_path, enabled)?;
            }
        }
        
        Ok(())
    }
}
