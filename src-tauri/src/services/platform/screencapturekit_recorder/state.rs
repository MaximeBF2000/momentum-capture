use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use screencapturekit::prelude::SCStream;

// Simple global state
pub(super) static STATE: Mutex<Option<RecordingState>> = Mutex::new(None);
static MIC_MUTED: AtomicBool = AtomicBool::new(false);
static SYSTEM_AUDIO_MUTED: AtomicBool = AtomicBool::new(false);

pub(super) struct RecordingState {
    pub ffmpeg_process: Child,
    pub stream: SCStream,
    pub video_writer: Arc<Mutex<Option<std::process::ChildStdin>>>,
    pub audio_writer: Arc<Mutex<Option<std::fs::File>>>,
    // Paths
    pub temp_video_path: PathBuf,
    pub system_audio_path: PathBuf,
    pub output_path: PathBuf,
    // Mic recording (separate FFmpeg process)
    pub mic_process: Option<Child>,
    pub mic_audio_path: Option<PathBuf>,
    pub system_audio_sample_rate: Arc<AtomicU32>,
    pub system_audio_channel_count: Arc<AtomicU32>,
    pub video_frame_count: Arc<AtomicU64>,
    pub audio_frame_count: Arc<AtomicU64>,
    pub audio_samples_written: Arc<AtomicU64>,
    pub requested_fps: u32,
    pub mic_sample_rate: Option<u32>,
    pub mic_channel_count: Option<u32>,
}

pub fn is_recording_active() -> bool {
    STATE.lock().unwrap().is_some()
}

pub(super) fn set_state(state: RecordingState) {
    *STATE.lock().unwrap() = Some(state);
}

pub(super) fn take_state() -> Option<RecordingState> {
    STATE.lock().unwrap().take()
}

pub fn set_mic_muted(muted: bool) {
    let old = MIC_MUTED.swap(muted, Ordering::Relaxed);
    if old != muted {
        println!("[SCK] Microphone mute state updated -> {}", muted);
    }
}

pub fn mic_muted() -> bool {
    MIC_MUTED.load(Ordering::Relaxed)
}

pub fn set_system_audio_muted(muted: bool) {
    let old = SYSTEM_AUDIO_MUTED.swap(muted, Ordering::Relaxed);
    if old != muted {
        println!("[SCK] System audio mute state updated -> {}", muted);
    }
}

pub fn system_audio_muted() -> bool {
    SYSTEM_AUDIO_MUTED.load(Ordering::Relaxed)
}
