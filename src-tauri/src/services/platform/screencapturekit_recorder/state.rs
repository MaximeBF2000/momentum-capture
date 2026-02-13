use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::{Arc, Mutex};

use screencapturekit::prelude::SCStream;

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
    pub ffmpeg_path: PathBuf,
}
