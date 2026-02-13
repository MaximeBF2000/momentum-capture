use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::error::{AppError, AppResult};
use crate::services::camera::CameraSyncHandle;
use crate::services::platform::device_resolver;
use screencapturekit::prelude::*;

use super::frame_handler::FrameHandler;
use super::state::RecordingState;

pub fn start_recording(
    state: &Mutex<Option<RecordingState>>,
    mic_muted: &Arc<AtomicBool>,
    system_audio_muted: &Arc<AtomicBool>,
    recording_paused: &Arc<AtomicBool>,
    output_path: &PathBuf,
    mic_enabled: bool,
    ffmpeg_path: &Path,
    camera_sync: Option<Arc<CameraSyncHandle>>,
) -> AppResult<()> {
    // TWO-PASS APPROACH:
    // 1. Record video to temp file (no audio) - from SCK via stdin
    // 2. Record system audio to temp WAV file - from SCK callbacks
    // 3. Record mic to temp file (if enabled) - separate FFmpeg process
    // 4. On stop: mux all together into final output
    recording_paused.store(false, std::sync::atomic::Ordering::Relaxed);
    let capture_started_at = Instant::now();

    println!("[SCK] Starting recording (two-pass mode)...");
    println!("[SCK]   Final output: {:?}", output_path);
    println!("[SCK]   Mic: {}", mic_enabled);

    if state.lock().unwrap().is_some() {
        return Err(AppError::Recording("Already recording".to_string()));
    }

    // Get screen info
    let content = SCShareableContent::get()
        .map_err(|e| AppError::Recording(format!("Failed to get shareable content: {:?}", e)))?;

    let displays = content.displays();
    let display = displays
        .first()
        .ok_or_else(|| AppError::Recording("No displays found".to_string()))?;

    let width = display.width();
    let height = display.height();
    println!("[SCK] Display: {}x{}", width, height);

    // Create temp paths
    let temp_dir = std::env::temp_dir();
    let session_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let temp_video_path = temp_dir.join(format!("sck_video_{}.mp4", session_id));
    let system_audio_path = temp_dir.join(format!("sck_sysaudio_{}.raw", session_id));
    let mic_audio_path = temp_dir.join(format!("sck_mic_{}.raw", session_id));

    println!("[SCK] Temp video: {:?}", temp_video_path);
    println!("[SCK] Temp system audio: {:?}", system_audio_path);

    // Resolve mic device if needed
    let mic_index = if mic_enabled {
        let resolved = device_resolver::resolve_avf_indices()?;
        let idx = resolved.audio_index_builtin_mic.unwrap_or(0);
        println!("[SCK] Mic device index: {}", idx);
        idx
    } else {
        0
    };

    // === PASS 1: VIDEO ONLY FFmpeg ===
    let mut cmd = Command::new(ffmpeg_path);
    cmd.args(["-y", "-hide_banner", "-loglevel", "warning"]);
    cmd.args([
        "-f",
        "rawvideo",
        "-pix_fmt",
        "bgra",
        "-s",
        &format!("{}x{}", width, height),
        "-r",
        "30",
        "-i",
        "pipe:0",
    ]);
    cmd.args([
        "-vf",
        &format!("scale={}:{}", width - (width % 2), height - (height % 2)),
        "-pix_fmt",
        "yuv420p",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-crf",
        "23",
        "-an", // No audio in this pass
        "-movflags",
        "+faststart",
    ]);
    cmd.arg(temp_video_path.to_str().unwrap());

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    println!("[SCK] Starting video FFmpeg...");
    let mut ffmpeg = cmd
        .spawn()
        .map_err(|e| AppError::Recording(format!("Failed to start FFmpeg: {}", e)))?;

    let ffmpeg_pid = ffmpeg.id();
    println!("[SCK] Video FFmpeg started (PID: {})", ffmpeg_pid);

    // Capture FFmpeg stderr
    if let Some(stderr) = ffmpeg.stderr.take() {
        thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if !line.is_empty() {
                        println!("[FFmpeg-Video] {}", line);
                    }
                }
            }
        });
    }

    // Get stdin for video
    let video_stdin = ffmpeg
        .stdin
        .take()
        .ok_or_else(|| AppError::Recording("Failed to get FFmpeg stdin".to_string()))?;
    let video_writer: Arc<Mutex<Option<std::process::ChildStdin>>> =
        Arc::new(Mutex::new(Some(video_stdin)));

    // === SYSTEM AUDIO: Write to file (not pipe!) ===
    let audio_file = std::fs::File::create(&system_audio_path)
        .map_err(|e| AppError::Recording(format!("Failed to create audio file: {}", e)))?;
    let audio_writer: Arc<Mutex<Option<std::fs::File>>> = Arc::new(Mutex::new(Some(audio_file)));
    println!("[SCK] System audio file created");

    // === MIC RECORDING: Separate FFmpeg process ===
    let mut mic_format: Option<(u32, u32)> = None;
    let mic_samples_written = Arc::new(AtomicU64::new(0));
    let first_mic_audio_arrival_ns = Arc::new(AtomicU64::new(0));
    let mic_process = if mic_enabled {
        println!("[SCK] Starting mic recording...");
        let mic_sample_rate = 48_000u32;
        let mic_channel_count = 2u32;
        let mic_writer = std::fs::File::create(&mic_audio_path)
            .map_err(|e| AppError::Recording(format!("Failed to create mic audio file: {}", e)))?;
        let mut mic_cmd = Command::new(ffmpeg_path);
        mic_cmd.args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "warning",
            "-f",
            "avfoundation",
            "-i",
            &format!(":{}", mic_index),
            "-ac",
            &mic_channel_count.to_string(),
            "-ar",
            &mic_sample_rate.to_string(),
            "-f",
            "s16le",
            "-",
        ]);
        mic_cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut mic_ffmpeg = mic_cmd
            .spawn()
            .map_err(|e| AppError::Recording(format!("Failed to start mic FFmpeg: {}", e)))?;

        println!("[SCK] Mic FFmpeg started (PID: {})", mic_ffmpeg.id());

        // Log mic FFmpeg stderr
        if let Some(stderr) = mic_ffmpeg.stderr.take() {
            thread::spawn(move || {
                use std::io::{BufRead, BufReader};
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        if !line.is_empty() {
                            println!("[FFmpeg-Mic] {}", line);
                        }
                    }
                }
            });
        }

        if let Some(stdout) = mic_ffmpeg.stdout.take() {
            let mut writer = std::io::BufWriter::new(mic_writer);
            let mic_muted = mic_muted.clone();
            let recording_paused = recording_paused.clone();
            let mic_samples_written = mic_samples_written.clone();
            let first_mic_audio_arrival_ns = first_mic_audio_arrival_ns.clone();
            let mic_channels_usize = mic_channel_count as usize;
            let capture_started_at = capture_started_at;
            thread::spawn(move || {
                let mut reader = stdout;
                let mut buffer = vec![0u8; 8192];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => {
                            let _ = writer.flush();
                            break;
                        }
                        Ok(len) => {
                            let now_ns = capture_started_at.elapsed().as_nanos() as u64;
                            let _ = first_mic_audio_arrival_ns.compare_exchange(
                                0,
                                now_ns,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            );
                            if recording_paused.load(std::sync::atomic::Ordering::Relaxed) {
                                continue;
                            }
                            if mic_muted.load(std::sync::atomic::Ordering::Relaxed) {
                                buffer[..len].fill(0);
                            }
                            if let Err(err) = writer.write_all(&buffer[..len]) {
                                eprintln!("[SCK] Mic writer error: {}", err);
                                break;
                            }
                            let bytes_per_frame = 2usize.saturating_mul(mic_channels_usize.max(1));
                            let frames = len / bytes_per_frame;
                            mic_samples_written.fetch_add(frames as u64, Ordering::Relaxed);
                        }
                        Err(err) => {
                            eprintln!("[SCK] Mic reader error: {}", err);
                            break;
                        }
                    }
                }
            });
        } else {
            return Err(AppError::Recording(
                "Failed to capture mic stdout".to_string(),
            ));
        }

        mic_format = Some((mic_sample_rate, mic_channel_count));
        Some(mic_ffmpeg)
    } else {
        None
    };

    // Configure ScreenCaptureKit
    let filter = SCContentFilter::builder()
        .display(display)
        .exclude_windows(&[])
        .build();

    let mut config = SCStreamConfiguration::new();
    config.set_width(width);
    config.set_height(height);
    config.set_minimum_frame_interval(&CMTime::new(1, 30));
    config.set_pixel_format(PixelFormat::BGRA);
    config.set_captures_audio(true);
    config.set_sample_rate(48000);
    config.set_channel_count(2);

    // Create stream
    let mut stream = SCStream::new(&filter, &config);

    // Frame counters for debugging
    let video_frame_count = Arc::new(AtomicU64::new(0));
    let audio_frame_count = Arc::new(AtomicU64::new(0));
    let audio_samples_written = Arc::new(AtomicU64::new(0));
    let system_audio_sample_rate = Arc::new(AtomicU32::new(0));
    let system_audio_channel_count = Arc::new(AtomicU32::new(0));
    let audio_layout_logged = Arc::new(AtomicBool::new(false));
    let first_screen_frame_arrival_ns = Arc::new(AtomicU64::new(0));
    let first_system_audio_arrival_ns = Arc::new(AtomicU64::new(0));

    // Add video handler
    let handler = FrameHandler {
        video_writer: video_writer.clone(),
        audio_writer: Arc::new(Mutex::new(None)),
        video_frame_count: video_frame_count.clone(),
        audio_frame_count: audio_frame_count.clone(),
        audio_sample_rate: system_audio_sample_rate.clone(),
        audio_channel_count: system_audio_channel_count.clone(),
        audio_layout_logged: audio_layout_logged.clone(),
        audio_samples_written: audio_samples_written.clone(),
        system_audio_muted: system_audio_muted.clone(),
        recording_paused: recording_paused.clone(),
        capture_started_at,
        first_screen_frame_arrival_ns: first_screen_frame_arrival_ns.clone(),
        first_system_audio_arrival_ns: first_system_audio_arrival_ns.clone(),
        camera_sync: camera_sync.clone(),
    };
    stream.add_output_handler(handler, SCStreamOutputType::Screen);

    // Add audio handler for system audio
    let audio_handler = FrameHandler {
        video_writer: Arc::new(Mutex::new(None)),
        audio_writer: audio_writer.clone(),
        video_frame_count: video_frame_count.clone(),
        audio_frame_count: audio_frame_count.clone(),
        audio_sample_rate: system_audio_sample_rate.clone(),
        audio_channel_count: system_audio_channel_count.clone(),
        audio_layout_logged: audio_layout_logged.clone(),
        audio_samples_written: audio_samples_written.clone(),
        system_audio_muted: system_audio_muted.clone(),
        recording_paused: recording_paused.clone(),
        capture_started_at,
        first_screen_frame_arrival_ns: first_screen_frame_arrival_ns.clone(),
        first_system_audio_arrival_ns: first_system_audio_arrival_ns.clone(),
        camera_sync: camera_sync.clone(),
    };
    stream.add_output_handler(audio_handler, SCStreamOutputType::Audio);

    // Start capture
    println!("[SCK] Starting capture...");
    stream
        .start_capture()
        .map_err(|e| AppError::Recording(format!("Failed to start capture: {:?}", e)))?;
    println!("[SCK] ✓ Capture started");

    // Store state
    *state.lock().unwrap() = Some(RecordingState {
        ffmpeg_process: ffmpeg,
        stream,
        video_writer,
        audio_writer,
        temp_video_path,
        system_audio_path,
        output_path: output_path.clone(),
        mic_process,
        mic_audio_path: if mic_enabled {
            Some(mic_audio_path)
        } else {
            None
        },
        system_audio_sample_rate,
        system_audio_channel_count,
        video_frame_count,
        audio_frame_count,
        audio_samples_written,
        mic_samples_written,
        capture_started_at,
        first_screen_frame_arrival_ns,
        first_system_audio_arrival_ns,
        first_mic_audio_arrival_ns,
        requested_fps: 30,
        mic_sample_rate: mic_format.map(|f| f.0),
        mic_channel_count: mic_format.map(|f| f.1),
        ffmpeg_path: ffmpeg_path.to_path_buf(),
    });

    println!("[SCK] ✓ Recording started successfully");
    Ok(())
}
