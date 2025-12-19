use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::{AppError, AppResult};

use super::MuxedRecorder;

/// Coordinates muxed recording operations to keep legacy APIs working.
pub struct SynchronizedRecorder {
    pub muxed_recorder: MuxedRecorder,  // Made public to allow direct access for deadlock fix
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
        println!("[SynchronizedRecorder] start() called");
        println!("Starting muxed recording:");
        println!("  Output file: {:?}", screen_path);
        println!("  Mic enabled: {}", mic_enabled);

        // CRITICAL: muxed_recorder.start() calls screencapturekit_recorder::start_recording()
        // which does heavy setup work but uses its own internal STATE mutex
        // The synchronized_recorder lock will be released immediately after this call returns
        println!("[SynchronizedRecorder] Calling muxed_recorder.start()...");
        // Use single muxed FFmpeg process for screen + audio
        // This calls screencapturekit_recorder::start_recording() which does heavy work
        let start_result = self.muxed_recorder.start(screen_path, mic_enabled);
        println!("[SynchronizedRecorder] muxed_recorder.start() returned");
        
        match start_result {
            Ok(_) => {
                println!("[SynchronizedRecorder] ✓ muxed_recorder.start() succeeded");
            }
            Err(e) => {
                eprintln!("[SynchronizedRecorder] ✗ muxed_recorder.start() failed: {}", e);
                return Err(e);
            }
        }
        
        // NOTE: We return here WITHOUT doing the sleep/verification
        // The caller will release the lock, then do the sleep/verification without holding locks
        println!("[SynchronizedRecorder] start() completed (lock will be released by caller, then sleep/verification happens)");
        
        Ok(())
    }
    
    // Separate method to verify process after start (called without holding lock)
    pub fn verify_started(&self) -> AppResult<()> {
        println!("[SynchronizedRecorder] verify_started() called");
        println!("[SynchronizedRecorder] Waiting 1s for process initialization...");
        // Wait for process to initialize - NO LOCKS HELD HERE
        std::thread::sleep(Duration::from_millis(1000));
        println!("[SynchronizedRecorder] ✓ Wait complete");
        
        println!("[SynchronizedRecorder] Verifying process is running...");
        // Verify process is running
        let process_exists = {
            let process_guard = self.muxed_recorder.recording_process.lock().unwrap();
            process_guard.is_some()
        };
        
        if !process_exists {
            eprintln!("[SynchronizedRecorder] ✗ Recording process not found");
            return Err(AppError::Recording("Recording process not found".to_string()));
        }
        
        println!("[SynchronizedRecorder] ✓ Process verified running");
        println!("Muxed recording process initialized successfully");
        println!("[SynchronizedRecorder] verify_started() completed");
        
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
