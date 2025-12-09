use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Helper function to find FFmpeg executable on macOS
fn find_ffmpeg() -> String {
    // Try common macOS FFmpeg locations
    let possible_paths = vec![
        "ffmpeg", // System PATH
        "/opt/homebrew/bin/ffmpeg", // Homebrew on Apple Silicon
        "/usr/local/bin/ffmpeg", // Homebrew on Intel
        "/usr/bin/ffmpeg", // System location
    ];
    
    for path in possible_paths {
        if Command::new(path).arg("-version").output().is_ok() {
            println!("[FFmpeg] Found FFmpeg at: {}", path);
            return path.to_string();
        }
    }
    
    // Default to "ffmpeg" and let it fail with a clear error if not found
    eprintln!("[FFmpeg] WARNING: FFmpeg not found in common locations, trying system PATH");
    "ffmpeg".to_string()
}

// Single muxed recorder that captures screen and audio together in one FFmpeg process
pub struct MuxedRecorder {
    recording_process: Arc<Mutex<Option<std::process::Child>>>,
    output_file: Option<PathBuf>,
    is_paused: Arc<Mutex<bool>>,
    mic_enabled: Arc<Mutex<bool>>,
}

// Legacy structs kept for compatibility during transition
pub struct ScreenRecorder {
    screen_process: Arc<Mutex<Option<std::process::Child>>>,
    screen_file: Option<PathBuf>,
    is_paused: Arc<Mutex<bool>>,
}

pub struct AudioRecorder {
    audio_process: Arc<Mutex<Option<std::process::Child>>>,
    audio_file: Option<PathBuf>,
    is_recording: Arc<Mutex<bool>>,
    mic_enabled: Arc<Mutex<bool>>,
}

// Synchronized recorder - now uses single muxed process
pub struct SynchronizedRecorder {
    muxed_recorder: MuxedRecorder,
    is_paused: Arc<Mutex<bool>>,
}

impl SynchronizedRecorder {
    pub fn new() -> Self {
        Self {
            muxed_recorder: MuxedRecorder::new(),
            is_paused: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start(&mut self, screen_path: &PathBuf, _audio_path: &PathBuf, mic_enabled: bool) -> AppResult<()> {
        println!("Starting muxed recording:");
        println!("  Output file: {:?}", screen_path);
        println!("  Mic enabled: {}", mic_enabled);

        // Use single muxed FFmpeg process for screen + audio
        self.muxed_recorder.start(screen_path, mic_enabled)?;
        
        // Wait for process to initialize
        std::thread::sleep(Duration::from_millis(1000));
        
        // Verify process is running
        let process_exists = {
            let process_guard = self.muxed_recorder.recording_process.lock().unwrap();
            process_guard.is_some()
        };
        
        if !process_exists {
            return Err(AppError::Recording("Recording process not found".to_string()));
        }
        
        println!("Muxed recording process initialized successfully");
        
        Ok(())
    }

    pub fn pause(&mut self) -> AppResult<()> {
        println!("Pausing muxed recording...");
        self.muxed_recorder.pause()?;
        *self.is_paused.lock().unwrap() = true;
        Ok(())
    }

    pub fn resume(&mut self, screen_path: &PathBuf, _audio_path: &PathBuf, mic_enabled: bool) -> AppResult<()> {
        println!("Resuming muxed recording...");
        // Resume muxed recorder
        self.muxed_recorder.resume(screen_path, mic_enabled)?;
        *self.is_paused.lock().unwrap() = false;
        Ok(())
    }

    pub fn stop(&mut self) -> AppResult<(PathBuf, PathBuf)> {
        println!("Stopping muxed recording...");
        
        // Stop muxed recorder - returns single output file
        let output_file = self.muxed_recorder.stop()?;
        
        println!("Muxed recording process stopped successfully");
        
        // Return same file for both screen and audio (they're muxed together)
        // This maintains compatibility with existing code that expects (screen_file, audio_file)
        Ok((output_file.clone(), output_file))
    }

    pub fn toggle_microphone(&mut self, enabled: bool) -> AppResult<()> {
        println!("Toggling microphone: {}", enabled);
        self.muxed_recorder.set_mic_enabled(enabled)
    }
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
        *self.mic_enabled.lock().unwrap() = mic_enabled;
        
        println!("[MuxedRecorder] Starting recording:");
        println!("[MuxedRecorder]   Output: {:?}", output_path);
        println!("[MuxedRecorder]   Mic enabled: {}", mic_enabled);
        
        // First, list available devices for debugging
        println!("[MuxedRecorder] Listing available AVFoundation devices...");
        let ffmpeg_path = find_ffmpeg();
        let list_cmd = Command::new(&ffmpeg_path)
            .args(&["-f", "avfoundation", "-list_devices", "true", "-i", ""])
            .output();
        
        if let Ok(output) = list_cmd {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("[MuxedRecorder] Available devices:\n{}", stderr);
        }
        
        // Single FFmpeg process that muxes screen and audio together
        // Use single avfoundation input "video_device:audio_device" for proper synchronization
        let ffmpeg_path = find_ffmpeg();
        let mut cmd = Command::new(&ffmpeg_path);
        
        // Single input: Screen + Microphone
        // Format: "video_index:audio_index" - this ensures proper sync
        // From device list: Screen is at index 3, MacBook Pro mic is at index 1
        // Note: Device indices may vary, but typically screen is the last video device
        println!("[MuxedRecorder] Configuring input: Screen device 3, Audio device 1 (MacBook Pro mic)");
        println!("[MuxedRecorder]   Note: If this fails, check device list above for correct indices");
        cmd.args(&[
            "-f", "avfoundation",
            "-framerate", "30", // Exact 30fps (device requires exact 30, not 29.97)
            "-video_size", "1920x1440", // Explicit size to match screen resolution
            "-i", "3:1", // Screen device 3, Audio device 1 (MacBook Pro microphone) - SINGLE INPUT for sync
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
        
        // Audio codec and filter options
        // Always apply volume filter - set to 0 when muted, 1.0 when enabled
        println!("[MuxedRecorder] Configuring audio codec: AAC");
        if mic_enabled {
            println!("[MuxedRecorder]   Audio: Enabled (normal volume)");
            cmd.args(&[
                "-c:a", "aac",
                "-b:a", "128k",
                "-ar", "48000", // Sample rate
            ]);
        } else {
            println!("[MuxedRecorder]   Audio: Muted (volume=0)");
            // Apply volume filter to mute audio (gain = 0)
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
        println!("[MuxedRecorder]   Input: avfoundation device 3:1 (screen device 3 + MacBook Pro mic device 1)");
        println!("[MuxedRecorder]   Video: 1920x1440@30fps, H.264");
        println!("[MuxedRecorder]   Audio: AAC 48kHz, Mic: {}", if mic_enabled { "enabled" } else { "muted" });
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
                                    // Log all FFmpeg output, but highlight errors/warnings
                                    if line.to_lowercase().contains("error") || 
                                       line.to_lowercase().contains("failed") ||
                                       line.to_lowercase().contains("warning") {
                                        eprintln!("[FFmpeg ERROR/WARNING] {}", line);
                                    } else {
                                        eprintln!("[FFmpeg] {}", line);
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
        let output_path = self.output_file.clone()
            .ok_or_else(|| AppError::Recording("No output file path".to_string()))?;
            
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
