use crate::models::RecordingOptions;
use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

#[cfg(target_os = "macos")]
use crate::services::platform::screencapturekit_recorder;

pub struct RecordingState {
    pub is_recording: bool,
    pub is_paused: bool,
    pub start_time: Option<std::time::Instant>,
    pub paused_duration: Duration,
    pub output_file: Option<PathBuf>,
    pub include_microphone: bool,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self {
            is_recording: false,
            is_paused: false,
            start_time: None,
            paused_duration: Duration::ZERO,
            output_file: None,
            include_microphone: false,
        }
    }
}

#[derive(Clone)]
pub struct Recorder {
    state: Arc<Mutex<RecordingState>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::default())),
        }
    }

    pub fn start(&self, options: RecordingOptions) -> AppResult<()> {
        println!("[Recorder] start() called");
        
        // Generate output file path
        let temp_dir = std::env::temp_dir();
        let recording_id = Uuid::new_v4();
        let output_file = temp_dir.join(format!("momentum_screen_{}.mp4", recording_id));
        
        // Check and update state
        {
            let mut state = self.state.lock().unwrap();
            if state.is_recording {
                return Err(AppError::Recording("Recording already in progress".to_string()));
            }
            state.is_recording = true;
            state.is_paused = false;
            state.start_time = Some(std::time::Instant::now());
            state.paused_duration = Duration::ZERO;
            state.output_file = Some(output_file.clone());
            state.include_microphone = options.include_microphone;
        }
        
        // Start ScreenCaptureKit recording
        #[cfg(target_os = "macos")]
        {
            match screencapturekit_recorder::start_recording(&output_file, options.include_microphone) {
                Ok(_) => {
                    println!("[Recorder] ✓ Recording started");
                    Ok(())
                }
                Err(e) => {
                    // Rollback state
                    let mut state = self.state.lock().unwrap();
                    state.is_recording = false;
                    Err(e)
                }
            }
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            Err(AppError::Recording("Platform not supported".to_string()))
        }
    }

    pub fn pause(&self) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        if !state.is_recording {
            return Err(AppError::Recording("No recording in progress".to_string()));
        }
        if state.is_paused {
            return Err(AppError::Recording("Recording already paused".to_string()));
        }
        state.is_paused = true;
        #[cfg(target_os = "macos")]
        {
            screencapturekit_recorder::set_recording_paused(true);
        }
        Ok(())
    }

    pub fn resume(&self) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        if !state.is_recording {
            return Err(AppError::Recording("No recording in progress".to_string()));
        }
        if !state.is_paused {
            return Err(AppError::Recording("Recording is not paused".to_string()));
        }
        state.is_paused = false;
        #[cfg(target_os = "macos")]
        {
            screencapturekit_recorder::set_recording_paused(false);
        }
        Ok(())
    }

    pub fn stop(&self) -> AppResult<(PathBuf, PathBuf)> {
        println!("[Recorder] stop() called");
        
        {
            let state = self.state.lock().unwrap();
            if !state.is_recording {
                return Err(AppError::Recording("No recording in progress".to_string()));
            }
        }
        
        // Stop ScreenCaptureKit recording
        #[cfg(target_os = "macos")]
        {
            let result = screencapturekit_recorder::stop_recording();
            
            // Update state regardless of result
            {
                let mut state = self.state.lock().unwrap();
                state.is_recording = false;
                state.is_paused = false;
                state.start_time = None;
                state.paused_duration = Duration::ZERO;
            }
            
            match result {
                Ok(path) => {
                    println!("[Recorder] ✓ Recording stopped: {:?}", path);
                    screencapturekit_recorder::set_recording_paused(false);
                    // Return same path for both (audio is embedded in MP4)
                    Ok((path.clone(), path))
                }
                Err(e) => Err(e)
            }
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            Err(AppError::Recording("Platform not supported".to_string()))
        }
    }

    pub fn toggle_microphone(&self, enabled: bool) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        if !state.is_recording {
            return Err(AppError::Recording("No recording in progress".to_string()));
        }
        state.include_microphone = enabled;
        // Note: Can't change mic mid-recording with current implementation
        Ok(())
    }
}
