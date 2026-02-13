mod frame_handler;
mod mux;
mod start;
mod state;
mod stop;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::services::camera::CameraSyncHandle;

use state::RecordingState;

pub struct ScreenCaptureKitRecorder {
    state: Mutex<Option<RecordingState>>,
    mic_muted: Arc<AtomicBool>,
    system_audio_muted: Arc<AtomicBool>,
    recording_paused: Arc<AtomicBool>,
}

impl ScreenCaptureKitRecorder {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
            mic_muted: Arc::new(AtomicBool::new(false)),
            system_audio_muted: Arc::new(AtomicBool::new(false)),
            recording_paused: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_active(&self) -> bool {
        self.state.lock().unwrap().is_some()
    }

    pub fn start(
        &self,
        output_path: &PathBuf,
        mic_enabled: bool,
        ffmpeg_path: &Path,
        camera_sync: Option<Arc<CameraSyncHandle>>,
    ) -> AppResult<()> {
        if self.is_active() {
            return Err(AppError::Recording("Already recording".to_string()));
        }

        self.set_recording_paused(false);
        start::start_recording(
            &self.state,
            &self.mic_muted,
            &self.system_audio_muted,
            &self.recording_paused,
            output_path,
            mic_enabled,
            ffmpeg_path,
            camera_sync,
        )
    }

    pub fn stop(&self) -> AppResult<PathBuf> {
        self.set_recording_paused(false);
        stop::stop_recording(&self.state, &self.recording_paused)
    }

    pub fn set_mic_muted(&self, muted: bool) {
        let old = self.mic_muted.swap(muted, Ordering::Relaxed);
        if old != muted {
            println!("[SCK] Microphone mute state updated -> {}", muted);
        }
    }

    pub fn set_system_audio_muted(&self, muted: bool) {
        let old = self.system_audio_muted.swap(muted, Ordering::Relaxed);
        if old != muted {
            println!("[SCK] System audio mute state updated -> {}", muted);
        }
    }

    pub fn set_recording_paused(&self, paused: bool) {
        let old = self.recording_paused.swap(paused, Ordering::Relaxed);
        if old != paused {
            println!("[SCK] Recording pause state -> {}", paused);
        }
    }

    pub fn is_recording_paused(&self) -> bool {
        self.recording_paused.load(Ordering::Relaxed)
    }
}
