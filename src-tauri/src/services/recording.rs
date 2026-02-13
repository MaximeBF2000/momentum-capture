use crate::error::{AppError, AppResult};
use crate::models::RecordingOptions;
use crate::services::camera::CameraSyncHandle;
use crate::services::platform::macos::ffmpeg::FfmpegLocator;
use crate::services::platform::screencapturekit_recorder::ScreenCaptureKitRecorder;
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use tokio::sync::watch;
use uuid::Uuid;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStartInfo {
    pub started_at_ms: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPausedInfo {
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResumedInfo {
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStoppedInfo {
    pub elapsed_ms: u64,
}

pub struct RecordingStopResult {
    pub elapsed_ms: u64,
    pub output_path: PathBuf,
}

struct RecorderState {
    is_recording: bool,
    is_paused: bool,
    output_file: Option<PathBuf>,
    include_microphone: bool,
    include_camera: bool,
    elapsed_task: Option<tauri::async_runtime::JoinHandle<()>>,
    elapsed_cancel: Option<watch::Sender<bool>>,
}

impl Default for RecorderState {
    fn default() -> Self {
        Self {
            is_recording: false,
            is_paused: false,
            output_file: None,
            include_microphone: false,
            include_camera: false,
            elapsed_task: None,
            elapsed_cancel: None,
        }
    }
}

#[derive(Debug, Default)]
struct RecordingClock {
    start_instant: Option<Instant>,
    accumulated: Duration,
    running: bool,
}

impl RecordingClock {
    fn start(&mut self) {
        self.start_instant = Some(Instant::now());
        self.accumulated = Duration::ZERO;
        self.running = true;
    }

    fn pause(&mut self) {
        if self.running {
            if let Some(start) = self.start_instant.take() {
                self.accumulated += start.elapsed();
            }
            self.running = false;
        }
    }

    fn resume(&mut self) {
        if !self.running {
            self.start_instant = Some(Instant::now());
            self.running = true;
        }
    }

    fn stop(&mut self) {
        self.start_instant = None;
        self.accumulated = Duration::ZERO;
        self.running = false;
    }

    fn elapsed(&self) -> Duration {
        if self.running {
            if let Some(start) = self.start_instant {
                return self.accumulated + start.elapsed();
            }
        }
        self.accumulated
    }
}

#[derive(Clone)]
pub struct Recorder {
    state: Arc<Mutex<RecorderState>>,
    clock: Arc<Mutex<RecordingClock>>,
    sck_recorder: Arc<ScreenCaptureKitRecorder>,
    camera_sync: Arc<CameraSyncHandle>,
    ffmpeg_locator: Arc<FfmpegLocator>,
}

impl Recorder {
    pub fn new(ffmpeg_locator: Arc<FfmpegLocator>, camera_sync: Arc<CameraSyncHandle>) -> Self {
        Self {
            state: Arc::new(Mutex::new(RecorderState::default())),
            clock: Arc::new(Mutex::new(RecordingClock::default())),
            sck_recorder: Arc::new(ScreenCaptureKitRecorder::new()),
            camera_sync,
            ffmpeg_locator,
        }
    }

    pub fn start(&self, options: RecordingOptions) -> AppResult<RecordingStartInfo> {
        let output_file = self.build_output_path();

        {
            let mut state = self.state.lock().unwrap();
            if state.is_recording {
                return Err(AppError::Recording("Recording already in progress".to_string()));
            }
            state.is_recording = true;
            state.is_paused = false;
            state.output_file = Some(output_file.clone());
            state.include_microphone = options.include_microphone;
            state.include_camera = options.include_camera;
        }

        let ffmpeg_path = self.ffmpeg_locator.resolve()?;
        let camera_sync = if options.include_camera {
            Some(self.camera_sync.clone())
        } else {
            None
        };

        match self.sck_recorder.start(
            &output_file,
            options.include_microphone,
            &ffmpeg_path,
            camera_sync,
        ) {
            Ok(_) => {
                if options.include_camera {
                    self.camera_sync.set_sync_enabled(true);
                }
                self.clock.lock().unwrap().start();
                Ok(RecordingStartInfo {
                    started_at_ms: current_time_ms(),
                    elapsed_ms: 0,
                })
            }
            Err(err) => {
                let mut state = self.state.lock().unwrap();
                state.is_recording = false;
                state.output_file = None;
                Err(err)
            }
        }
    }

    pub fn pause(&self) -> AppResult<RecordingPausedInfo> {
        let mut state = self.state.lock().unwrap();
        if !state.is_recording {
            return Err(AppError::Recording("No recording in progress".to_string()));
        }
        if state.is_paused {
            return Err(AppError::Recording("Recording already paused".to_string()));
        }

        state.is_paused = true;
        self.sck_recorder.set_recording_paused(true);
        self.clock.lock().unwrap().pause();
        Ok(RecordingPausedInfo {
            elapsed_ms: self.elapsed_ms(),
        })
    }

    pub fn resume(&self) -> AppResult<RecordingResumedInfo> {
        let mut state = self.state.lock().unwrap();
        if !state.is_recording {
            return Err(AppError::Recording("No recording in progress".to_string()));
        }
        if !state.is_paused {
            return Err(AppError::Recording("Recording is not paused".to_string()));
        }

        state.is_paused = false;
        self.sck_recorder.set_recording_paused(false);
        self.clock.lock().unwrap().resume();
        Ok(RecordingResumedInfo {
            elapsed_ms: self.elapsed_ms(),
        })
    }

    pub fn stop(&self) -> AppResult<RecordingStopResult> {
        if !self.state.lock().unwrap().is_recording {
            return Err(AppError::Recording("No recording in progress".to_string()));
        }

        let output_path = self.sck_recorder.stop()?;
        self.camera_sync.set_sync_enabled(false);
        self.stop_elapsed_task();

        let elapsed_ms = self.elapsed_ms();

        let mut state = self.state.lock().unwrap();
        state.is_recording = false;
        state.is_paused = false;
        state.output_file = None;
        state.include_microphone = false;
        state.include_camera = false;

        self.clock.lock().unwrap().stop();

        Ok(RecordingStopResult {
            elapsed_ms,
            output_path,
        })
    }

    pub fn set_mic_muted(&self, muted: bool) {
        self.sck_recorder.set_mic_muted(muted);
    }

    pub fn set_system_audio_muted(&self, muted: bool) {
        self.sck_recorder.set_system_audio_muted(muted);
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.clock
            .lock()
            .unwrap()
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    pub fn start_elapsed_task(&self, app: tauri::AppHandle) {
        let mut state = self.state.lock().unwrap();
        if state.elapsed_task.is_some() {
            return;
        }

        let (tx, mut rx) = watch::channel(false);
        let clock = self.clock.clone();

        let handle = tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let _ = app.emit("recording-elapsed", json!({
                "elapsedMs": elapsed_ms_from_clock(&clock)
            }));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let _ = app.emit("recording-elapsed", json!({
                            "elapsedMs": elapsed_ms_from_clock(&clock)
                        }));
                    }
                    _ = rx.changed() => {
                        if *rx.borrow() {
                            break;
                        }
                    }
                }
            }
        });

        state.elapsed_cancel = Some(tx);
        state.elapsed_task = Some(handle);
    }

    pub fn stop_elapsed_task(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(cancel) = state.elapsed_cancel.take() {
            let _ = cancel.send(true);
        }
        if let Some(handle) = state.elapsed_task.take() {
            handle.abort();
        }
    }

    fn build_output_path(&self) -> PathBuf {
        let temp_dir = std::env::temp_dir();
        let recording_id = Uuid::new_v4();
        temp_dir.join(format!("momentum_screen_{}.mp4", recording_id))
    }
}

fn elapsed_ms_from_clock(clock: &Arc<Mutex<RecordingClock>>) -> u64 {
    clock
        .lock()
        .unwrap()
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::RecordingClock;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn recording_clock_tracks_pause_and_resume() {
        let mut clock = RecordingClock::default();
        clock.start();
        thread::sleep(Duration::from_millis(10));
        clock.pause();

        let paused = clock.elapsed();
        thread::sleep(Duration::from_millis(10));
        assert_eq!(clock.elapsed(), paused);

        clock.resume();
        thread::sleep(Duration::from_millis(10));
        assert!(clock.elapsed() > paused);
    }
}
