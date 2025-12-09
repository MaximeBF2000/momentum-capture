use crate::models::RecordingOptions;
use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

#[cfg(target_os = "macos")]
use crate::services::platform::macos::SynchronizedRecorder;

pub struct RecordingState {
    pub is_recording: bool,
    pub is_paused: bool,
    pub start_time: Option<std::time::Instant>,
    pub paused_duration: Duration,
    pub screen_file: Option<PathBuf>,
    pub audio_file: Option<PathBuf>,
    pub include_microphone: bool,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self {
            is_recording: false,
            is_paused: false,
            start_time: None,
            paused_duration: Duration::ZERO,
            screen_file: None,
            audio_file: None,
            include_microphone: false,
        }
    }
}

pub struct Recorder {
    state: Arc<Mutex<RecordingState>>,
    #[cfg(target_os = "macos")]
    synchronized_recorder: Arc<Mutex<SynchronizedRecorder>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::default())),
            #[cfg(target_os = "macos")]
            synchronized_recorder: Arc::new(Mutex::new(SynchronizedRecorder::new())),
        }
    }

    pub fn start(&self, options: RecordingOptions) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        
        if state.is_recording {
            return Err(AppError::Recording("Recording already in progress".to_string()));
        }

        // Create temp directory for recording files
        let temp_dir = std::env::temp_dir();
        let recording_id = Uuid::new_v4();
        
        // Screen recording file
        let screen_file = temp_dir.join(format!("momentum_screen_{}.mp4", recording_id));
        
        // Audio recording file (will be created even if mic is off for timestamp alignment)
        let audio_file = temp_dir.join(format!("momentum_audio_{}.aac", recording_id));

        #[cfg(target_os = "macos")]
        {
            // Use synchronized separate recorders that start together
            // Both processes spawn, wait for initialization, then start recording together
            let mut sync_rec = self.synchronized_recorder.lock().unwrap();
            sync_rec.start(&screen_file, &audio_file, options.include_microphone)?;
        }

        state.is_recording = true;
        state.is_paused = false;
        state.start_time = Some(std::time::Instant::now());
        state.paused_duration = Duration::ZERO;
        state.screen_file = Some(screen_file);
        state.audio_file = Some(audio_file);
        state.include_microphone = options.include_microphone;

        Ok(())
    }

    pub fn pause(&self) -> AppResult<()> {
        let mut state = self.state.lock().unwrap();
        
        if !state.is_recording {
            return Err(AppError::Recording("No recording in progress".to_string()));
        }

        if state.is_paused {
            return Err(AppError::Recording("Recording already paused".to_string()));
        }

        #[cfg(target_os = "macos")]
        {
            // Pause synchronized recording (stops both screen and audio together)
            {
                let mut sync_rec = self.synchronized_recorder.lock().unwrap();
                sync_rec.pause()?;
            }
        }

        state.is_paused = true;
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

        #[cfg(target_os = "macos")]
        {
            // Resume synchronized recording (starts both screen and audio together)
            let screen_file = state.screen_file.clone()
                .ok_or_else(|| AppError::Recording("No screen file path".to_string()))?;
            let audio_file = state.audio_file.clone()
                .ok_or_else(|| AppError::Recording("No audio file path".to_string()))?;
            {
                let mut sync_rec = self.synchronized_recorder.lock().unwrap();
                sync_rec.resume(&screen_file, &audio_file, state.include_microphone)?;
            }
        }

        state.is_paused = false;
        Ok(())
    }

    pub fn stop(&self) -> AppResult<(PathBuf, PathBuf)> {
        let mut state = self.state.lock().unwrap();
        
        if !state.is_recording {
            return Err(AppError::Recording("No recording in progress".to_string()));
        }

        #[cfg(target_os = "macos")]
        {
            // Stop synchronized recording (stops both screen and audio together)
            let (screen_file, audio_file) = {
                let mut sync_rec = self.synchronized_recorder.lock().unwrap();
                sync_rec.stop()?
            };

            state.is_recording = false;
            state.is_paused = false;
            state.start_time = None;
            state.paused_duration = Duration::ZERO;

            Ok((screen_file, audio_file))
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

        #[cfg(target_os = "macos")]
        {
            // Toggle microphone: use the new toggle_microphone method which sets volume filter
            // This avoids gaps by keeping the recording process running
            let mut sync_rec = self.synchronized_recorder.lock().unwrap();
            sync_rec.toggle_microphone(enabled)?;
        }

        Ok(())
    }
}
