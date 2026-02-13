use crate::error::{AppError, AppResult};
use crate::services::platform::device_resolver;
use crate::services::time::host_time_now_ns;
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::VecDeque;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter};

use crate::services::platform::macos::ffmpeg::FfmpegLocator;

const CAMERA_BUFFER_CAPACITY: usize = 300;
const CAMERA_FRAME_DURATION_NS: u64 = 33_333_333; // ~30 FPS
const CAMERA_TARGET_LAG_NS: u64 = 5_000_000;
const MAX_CAM_DELAY_NS: u64 = 120_000_000;
const BUFFER_REFILL_TARGET: usize = 5;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CameraFramePayload {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
    pub data_base64: String,
    #[serde(rename = "ptsNs")]
    pub pts_ns: u64,
}

#[derive(Debug)]
pub struct SyncedFrameBuffer {
    frames: VecDeque<CameraFramePayload>,
    last_frame: Option<CameraFramePayload>,
    min_queued: usize,
    max_queued: usize,
}

impl SyncedFrameBuffer {
    pub fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(CAMERA_BUFFER_CAPACITY),
            last_frame: None,
            min_queued: usize::MAX,
            max_queued: 0,
        }
    }

    pub fn push(&mut self, frame: CameraFramePayload) {
        self.frames.push_back(frame.clone());
        self.last_frame = Some(frame);
        while self.frames.len() > CAMERA_BUFFER_CAPACITY {
            self.frames.pop_front();
        }
        self.update_stats();
    }

    pub fn pop_for_screen_pts(&mut self, screen_pts_ns: u64) -> Option<CameraFramePayload> {
        let mut candidate_idx: Option<usize> = None;
        for (idx, frame) in self.frames.iter().enumerate() {
            if frame.pts_ns <= screen_pts_ns {
                candidate_idx = Some(idx);
            } else {
                break;
            }
        }

        match candidate_idx {
            Some(idx) => {
                for _ in 0..idx {
                    self.frames.pop_front();
                }
                let frame = self.frames.pop_front();
                self.update_stats();
                frame
            }
            None => None,
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.last_frame = None;
        self.min_queued = usize::MAX;
        self.max_queued = 0;
    }

    pub fn last(&self) -> Option<CameraFramePayload> {
        self.last_frame.clone()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    fn update_stats(&mut self) {
        let len = self.frames.len();
        if len < self.min_queued {
            self.min_queued = len;
        }
        if len > self.max_queued {
            self.max_queued = len;
        }
    }
}

#[derive(Clone)]
pub struct CameraSyncHandle {
    app_handle: Arc<Mutex<Option<AppHandle>>>,
    frame_buffer: Arc<Mutex<SyncedFrameBuffer>>,
    sync_enabled: Arc<AtomicBool>,
    frame_in_count: Arc<AtomicU64>,
    frame_out_count: Arc<AtomicU64>,
    last_emit_delta_ns: Arc<AtomicU64>,
    last_screen_pts_ns: Arc<AtomicU64>,
    screen_tick_count: Arc<AtomicU64>,
    last_emitted_frame: Arc<Mutex<Option<CameraFramePayload>>>,
    target_offset_ns: Arc<AtomicU64>,
    dropped_frames: Arc<AtomicU64>,
    repeated_frames: Arc<AtomicU64>,
}

impl CameraSyncHandle {
    pub fn new() -> Self {
        Self {
            app_handle: Arc::new(Mutex::new(None)),
            frame_buffer: Arc::new(Mutex::new(SyncedFrameBuffer::new())),
            sync_enabled: Arc::new(AtomicBool::new(false)),
            frame_in_count: Arc::new(AtomicU64::new(0)),
            frame_out_count: Arc::new(AtomicU64::new(0)),
            last_emit_delta_ns: Arc::new(AtomicU64::new(0)),
            last_screen_pts_ns: Arc::new(AtomicU64::new(0)),
            screen_tick_count: Arc::new(AtomicU64::new(0)),
            last_emitted_frame: Arc::new(Mutex::new(None)),
            target_offset_ns: Arc::new(AtomicU64::new(30_000_000)), // start ~30 ms
            dropped_frames: Arc::new(AtomicU64::new(0)),
            repeated_frames: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn set_app_handle(&self, app: AppHandle) {
        *self.app_handle.lock().unwrap() = Some(app);
    }

    pub fn set_sync_enabled(&self, enabled: bool) {
        let previous = self.sync_enabled.swap(enabled, Ordering::Relaxed);
        if enabled != previous {
            let in_count = self.frame_in_count.load(Ordering::Relaxed);
            let out_count = self.frame_out_count.load(Ordering::Relaxed);
            println!(
                "[CameraSync] State change -> enabled={} (frames_in={}, frames_out={}, screen_ticks={}, last_delta={}µs, target_offset={}µs)",
                enabled,
                in_count,
                out_count,
                self.screen_tick_count.load(Ordering::Relaxed),
                self.last_emit_delta_ns.load(Ordering::Relaxed) as f64 / 1_000.0,
                self.target_offset_ns.load(Ordering::Relaxed) as f64 / 1_000.0
            );
        }
        if !enabled && previous {
            {
                let mut buffer = self.frame_buffer.lock().unwrap();
                println!(
                    "[CameraSync] Buffer stats before clear: queued={} min={} max={}",
                    buffer.len(),
                    buffer.min_queued,
                    buffer.max_queued
                );
                buffer.clear();
            }
            *self.last_emitted_frame.lock().unwrap() = None;
            self.frame_in_count.store(0, Ordering::Relaxed);
            self.frame_out_count.store(0, Ordering::Relaxed);
            self.screen_tick_count.store(0, Ordering::Relaxed);
            self.dropped_frames.store(0, Ordering::Relaxed);
            self.repeated_frames.store(0, Ordering::Relaxed);
            self.target_offset_ns.store(30_000_000, Ordering::Relaxed);
        }
    }

    pub fn push_frame(&self, frame: CameraFramePayload) {
        let buffered_len = {
            let mut buffer = self.frame_buffer.lock().unwrap();
            buffer.push(frame.clone());
            buffer.frames.len()
        };

        let in_count = self.frame_in_count.fetch_add(1, Ordering::Relaxed) + 1;
        if in_count <= 5 || in_count % 30 == 0 {
            println!(
                "[CameraSync] Buffered frame #{}, pts={}ns (buffer_len={})",
                in_count, frame.pts_ns, buffered_len
            );
        }
        if !self.sync_enabled.load(Ordering::Relaxed) {
            self.emit(frame.clone());
            *self.last_emitted_frame.lock().unwrap() = Some(frame);
        }
    }

    pub fn emit_for_screen_pts(&self, screen_pts_ns: u64) {
        self.last_screen_pts_ns
            .store(screen_pts_ns, Ordering::Relaxed);
        let tick = self.screen_tick_count.fetch_add(1, Ordering::Relaxed) + 1;
        let enabled = self.sync_enabled.load(Ordering::Relaxed);
        if !enabled {
            if tick <= 5 || tick % 60 == 0 {
                println!(
                    "[CameraSync] Screen tick #{} (pts={}ns) ignored because sync not enabled",
                    tick, screen_pts_ns
                );
            }
            return;
        }

        let adjusted_screen_ns =
            screen_pts_ns.saturating_sub(self.target_offset_ns.load(Ordering::Relaxed));

        let (frame, remaining, leading_delay_ns) = {
            let mut buffer = self.frame_buffer.lock().unwrap();
            let best = buffer.pop_for_screen_pts(adjusted_screen_ns);
            let len = buffer.frames.len();
            let leading = buffer
                .last()
                .map(|f| screen_pts_ns.saturating_sub(f.pts_ns))
                .unwrap_or(0);
            (best, len, leading)
        };

        if let Some(frame) = frame {
            let delta = screen_pts_ns.saturating_sub(frame.pts_ns);
            self.last_emit_delta_ns.store(delta, Ordering::Relaxed);
            let out_count = self.frame_out_count.fetch_add(1, Ordering::Relaxed) + 1;
            if out_count <= 5 || out_count % 30 == 0 || delta > 25_000_000 {
                println!(
                    "[CameraSync] Emit frame #{}, screen_pts={}ns cam_pts={}ns Δ={}µs (buffer_len={})",
                    out_count,
                    screen_pts_ns,
                    frame.pts_ns,
                    delta as f64 / 1_000.0,
                    remaining
                );
            }
            self.emit(frame.clone());
            *self.last_emitted_frame.lock().unwrap() = Some(frame);

            // Adjust offset toward target lag, but also account for increasing leading delay
            let desired = CAMERA_TARGET_LAG_NS;
            let mut offset = self.target_offset_ns.load(Ordering::Relaxed);
            if delta > desired {
                let overshoot = delta - desired;
                let adj = ((overshoot) / 8).max(1_000);
                offset = (offset + adj).min(MAX_CAM_DELAY_NS);
            } else {
                let undershoot = desired - delta;
                let adj = (undershoot / 8).max(1_000);
                offset = offset.saturating_sub(adj);
            }
            if leading_delay_ns > MAX_CAM_DELAY_NS / 2 {
                offset = (offset + leading_delay_ns / 16).min(MAX_CAM_DELAY_NS);
            }
            self.target_offset_ns.store(offset, Ordering::Relaxed);
        } else {
            let mut reused = false;
            if let Some(frame) = self.last_emitted_frame.lock().unwrap().clone() {
                println!(
                    "[CameraSync] Reusing last frame for screen pts {}ns (buffer_len={}, offset={}µs)",
                    screen_pts_ns,
                    remaining,
                    self.target_offset_ns.load(Ordering::Relaxed) as f64 / 1_000.0
                );
                self.emit(frame);
                self.repeated_frames.fetch_add(1, Ordering::Relaxed);
                reused = true;
            }
            if !reused {
                println!(
                    "[CameraSync] ⚠️ No camera frame available for screen pts {}ns (buffer_len={})",
                    screen_pts_ns, remaining
                );
                self.dropped_frames.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn clear(&self) {
        let mut guard = self.frame_buffer.lock().unwrap();
        let len = guard.frames.len();
        guard.frames.clear();
        println!("[CameraSync] Cleared frame buffer (dropped {} frames)", len);
        *self.last_emitted_frame.lock().unwrap() = None;
    }

    fn emit(&self, frame: CameraFramePayload) {
        if let Some(app) = self.app_handle.lock().unwrap().as_ref() {
            if let Err(err) = app.emit("camera-frame", &frame) {
                eprintln!("[CameraPreview] Failed to emit camera frame: {}", err);
            }
        }
    }
}

pub struct CameraPreview {
    is_running: Arc<Mutex<bool>>,
    sync_handle: Arc<CameraSyncHandle>,
    ffmpeg_locator: Arc<FfmpegLocator>,
}

impl CameraPreview {
    pub fn new(ffmpeg_locator: Arc<FfmpegLocator>) -> Self {
        let handle = Arc::new(CameraSyncHandle::new());
        Self {
            is_running: Arc::new(Mutex::new(false)),
            sync_handle: handle,
            ffmpeg_locator,
        }
    }

    pub fn sync_handle(&self) -> Arc<CameraSyncHandle> {
        self.sync_handle.clone()
    }

    pub fn set_app_handle(&mut self, app: AppHandle) {
        self.sync_handle.set_app_handle(app);
    }

    pub fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }

    pub fn start(&self) -> AppResult<()> {
        let mut is_running = self.is_running.lock().unwrap();

        if *is_running {
            // Already running, just return success
            return Ok(());
        }

        let ffmpeg_path = self
            .ffmpeg_locator
            .resolve()
            .map_err(|err| AppError::Camera(err.to_string()))?;

        println!(
            "[CameraPreview] Starting camera preview with FFmpeg: {}",
            ffmpeg_path.display()
        );

        let camera_index = match device_resolver::resolve_avf_indices() {
            Ok(devices) => match devices.get_camera_index() {
                Ok(idx) => {
                    println!("[CameraPreview] Resolved built-in camera index: {}", idx);
                    idx
                }
                Err(e) => {
                    eprintln!(
                        "[CameraPreview] Failed to resolve camera index: {}, falling back to 0",
                        e
                    );
                    0
                }
            },
            Err(e) => {
                eprintln!(
                    "[CameraPreview] Failed to resolve device indices: {}, falling back to 0",
                    e
                );
                0
            }
        };

        *is_running = true;

        let is_running_clone = self.is_running.clone();
        let sync_handle_clone = self.sync_handle.clone();

        // Start FFmpeg in a separate thread
        let ffmpeg_path_clone = ffmpeg_path.clone();
        let camera_index_clone = camera_index;
        thread::spawn(move || {
            let mut cmd = Command::new(&ffmpeg_path_clone);

            cmd.args(&[
                "-f",
                "avfoundation",
                "-framerate",
                "30",
                "-video_size",
                "640x480",
                "-i",
                &format!("{}:", camera_index_clone), // Built-in camera, no audio
                "-vf",
                "fps=30", // Keep at 30 fps for smooth preview
                "-f",
                "image2pipe",
                "-vcodec",
                "mjpeg",
                "-q:v",
                "3", // Lower quality number = higher quality but faster encoding
                "-",
            ]);

            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped()); // Capture stderr for debugging

            let mut process = match cmd.spawn() {
                Ok(p) => {
                    println!(
                        "[CameraPreview] FFmpeg process spawned successfully (PID: {})",
                        p.id()
                    );
                    p
                }
                Err(e) => {
                    let error_msg = format!(
                        "Failed to spawn camera FFmpeg process: {}. FFmpeg path used: {}",
                        e,
                        ffmpeg_path_clone.display()
                    );
                    eprintln!("[CameraPreview] ERROR: {}", error_msg);
                    *is_running_clone.lock().unwrap() = false;

                    if let Some(app) = sync_handle_clone.app_handle.lock().unwrap().as_ref() {
                        let _ = app.emit(
                            "camera-error",
                            json!({
                                "message": error_msg
                            }),
                        );
                    }
                    return;
                }
            };

            // Read stderr in a separate thread to capture errors (but don't log everything)
            let stderr = process.stderr.take();
            if let Some(mut stderr) = stderr {
                let is_running_err = is_running_clone.clone();
                std::thread::spawn(move || {
                    let mut buffer = [0u8; 1024];
                    while *is_running_err.lock().unwrap() {
                        if let Ok(n) = stderr.read(&mut buffer) {
                            if n > 0 {
                                let error_msg = String::from_utf8_lossy(&buffer[..n]);
                                // Only log actual errors, not warnings or info
                                if error_msg.contains("Error") || error_msg.contains("error") {
                                    eprintln!("Camera FFmpeg error: {}", error_msg);
                                }
                            }
                        }
                    }
                });
            }

            let mut stdout = process.stdout.take().unwrap();
            let mut frame_id = 0u64;
            let mut jpeg_data = Vec::with_capacity(50000); // Pre-allocate for typical JPEG size
            let mut buffer = [0u8; 65536]; // Larger buffer for better performance
            let mut found_start = false;
            let mut last_frame_time = std::time::Instant::now();

            while *is_running_clone.lock().unwrap() {
                match stdout.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        for i in 0..n {
                            let byte = buffer[i];

                            // Look for JPEG start marker (FF D8)
                            if !found_start
                                && i < n - 1
                                && buffer[i] == 0xFF
                                && buffer[i + 1] == 0xD8
                            {
                                found_start = true;
                                jpeg_data.clear();
                                jpeg_data.push(byte);
                            } else if found_start {
                                jpeg_data.push(byte);

                                // Check for JPEG end marker (FF D9)
                                if jpeg_data.len() >= 2
                                    && jpeg_data[jpeg_data.len() - 2] == 0xFF
                                    && jpeg_data[jpeg_data.len() - 1] == 0xD9
                                {
                                    // Complete JPEG frame found
                                    // Only emit if enough time has passed (throttle to ~30 FPS max)
                                    let now = std::time::Instant::now();
                                    if now.duration_since(last_frame_time).as_millis() >= 33 {
                                        let base64_frame =
                                            general_purpose::STANDARD.encode(&jpeg_data);
                                        let pts_ns = host_time_now_ns();

                                        sync_handle_clone.push_frame(CameraFramePayload {
                                            id: frame_id,
                                            width: 640,
                                            height: 480,
                                            format: "jpeg",
                                            data_base64: base64_frame,
                                            pts_ns,
                                        });

                                        frame_id += 1;
                                        last_frame_time = now;
                                    }

                                    found_start = false;
                                    jpeg_data.clear();
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(())
    }

    pub fn stop(&self) -> AppResult<()> {
        let mut is_running = self.is_running.lock().unwrap();

        if !*is_running {
            // Already stopped, return success
            return Ok(());
        }

        *is_running = false;
        self.sync_handle.set_sync_enabled(false);
        self.sync_handle.clear();

        // Note: The FFmpeg process will detect is_running=false and exit naturally
        // We don't need to kill it explicitly as the thread checks the flag

        Ok(())
    }
}
