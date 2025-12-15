 //! Simple ScreenCaptureKit recorder for macOS
//! 
//! Captures: screen video + system audio + microphone
//! Output: single MP4 file via FFmpeg

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use screencapturekit::prelude::*;
use screencapturekit::cv::CVPixelBufferLockFlags;

use crate::error::{AppError, AppResult};
use super::device_resolver;

// Simple global state
static STATE: Mutex<Option<RecordingState>> = Mutex::new(None);

struct RecordingState {
    ffmpeg_process: Child,
    stream: SCStream,
    video_writer: Arc<Mutex<Option<std::process::ChildStdin>>>,
    audio_writer: Arc<Mutex<Option<std::fs::File>>>,
    // Paths
    temp_video_path: PathBuf,
    system_audio_path: PathBuf,
    output_path: PathBuf,
    // Mic recording (separate FFmpeg process)
    mic_process: Option<Child>,
    mic_audio_path: Option<PathBuf>,
}

// Handler for ScreenCaptureKit callbacks
struct FrameHandler {
    video_writer: Arc<Mutex<Option<std::process::ChildStdin>>>,
    audio_writer: Arc<Mutex<Option<std::fs::File>>>,
    video_frame_count: Arc<std::sync::atomic::AtomicU64>,
    audio_frame_count: Arc<std::sync::atomic::AtomicU64>,
}

impl SCStreamOutputTrait for FrameHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        match of_type {
            SCStreamOutputType::Screen => {
                // Write video frame to FFmpeg stdin
                if let Some(ref mut writer) = *self.video_writer.lock().unwrap() {
                    if let Some(buffer) = sample.image_buffer() {
                        if let Ok(guard) = buffer.lock(CVPixelBufferLockFlags::READ_ONLY) {
                            let pixels = guard.as_slice();
                            if writer.write_all(pixels).is_ok() {
                                let count = self.video_frame_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                if count == 0 {
                                    println!("[SCK] First video frame written ({} bytes)", pixels.len());
                                } else if count % 30 == 0 {
                                    println!("[SCK] Video frames: {}", count + 1);
                                }
                            }
                        }
                    }
                }
            }
            SCStreamOutputType::Audio => {
                // Write audio to named pipe (convert Float32 to s16le)
                if let Some(ref mut writer) = *self.audio_writer.lock().unwrap() {
                    if let Some(audio_buffers) = sample.audio_buffer_list() {
                        for buffer in audio_buffers.iter() {
                            let data = buffer.data();
                            let data_size = buffer.data_byte_size();
                            if data_size == 0 { continue; }
                            
                            // Convert Float32 to s16le
                            let num_samples = data_size / 4;
                            let float_samples = unsafe {
                                std::slice::from_raw_parts(data.as_ptr() as *const f32, num_samples)
                            };
                            
                            let mut s16_data = Vec::with_capacity(num_samples * 2);
                            for &s in float_samples {
                                let clamped = s.max(-1.0).min(1.0);
                                let s16 = (clamped * 32767.0) as i16;
                                s16_data.extend_from_slice(&s16.to_le_bytes());
                            }
                            if writer.write_all(&s16_data).is_ok() {
                                let count = self.audio_frame_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                if count == 0 {
                                    println!("[SCK] First audio frame written ({} bytes)", s16_data.len());
                                } else if count % 100 == 0 {
                                    println!("[SCK] Audio frames: {}", count + 1);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn is_recording_active() -> bool {
    STATE.lock().unwrap().is_some()
}

pub fn is_available() -> bool {
    // ScreenCaptureKit is available on macOS 12.3+
    true
}

pub fn start_recording(output_path: &PathBuf, mic_enabled: bool) -> AppResult<()> {
    // TWO-PASS APPROACH:
    // 1. Record video to temp file (no audio) - from SCK via stdin
    // 2. Record system audio to temp WAV file - from SCK callbacks
    // 3. Record mic to temp file (if enabled) - separate FFmpeg process
    // 4. On stop: mux all together into final output
    
    println!("[SCK] Starting recording (two-pass mode)...");
    println!("[SCK]   Final output: {:?}", output_path);
    println!("[SCK]   Mic: {}", mic_enabled);
    
    if is_recording_active() {
        return Err(AppError::Recording("Already recording".to_string()));
    }
    
    // Get screen info
    let content = SCShareableContent::get()
        .map_err(|e| AppError::Recording(format!("Failed to get shareable content: {:?}", e)))?;
    
    let displays = content.displays();
    let display = displays.first()
        .ok_or_else(|| AppError::Recording("No displays found".to_string()))?;
    
    let width = display.width();
    let height = display.height();
    println!("[SCK] Display: {}x{}", width, height);
    
    // Create temp paths
    let temp_dir = std::env::temp_dir();
    let session_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let temp_video_path = temp_dir.join(format!("sck_video_{}.mp4", session_id));
    let system_audio_path = temp_dir.join(format!("sck_sysaudio_{}.raw", session_id));
    let mic_audio_path = temp_dir.join(format!("sck_mic_{}.m4a", session_id));
    
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
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-loglevel", "warning"]);
    cmd.args([
        "-f", "rawvideo",
        "-pix_fmt", "bgra",
        "-s", &format!("{}x{}", width, height),
        "-r", "30",
        "-i", "pipe:0"
    ]);
    cmd.args([
        "-vf", &format!("scale={}:{}", width - (width % 2), height - (height % 2)),
        "-pix_fmt", "yuv420p",
        "-c:v", "libx264",
        "-preset", "ultrafast",
        "-crf", "23",
        "-an",  // No audio in this pass
        "-movflags", "+faststart"
    ]);
    cmd.arg(temp_video_path.to_str().unwrap());
    
    cmd.stdin(Stdio::piped())
       .stdout(Stdio::null())
       .stderr(Stdio::piped());
    
    println!("[SCK] Starting video FFmpeg...");
    let mut ffmpeg = cmd.spawn()
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
    let video_stdin = ffmpeg.stdin.take()
        .ok_or_else(|| AppError::Recording("Failed to get FFmpeg stdin".to_string()))?;
    let video_writer: Arc<Mutex<Option<std::process::ChildStdin>>> = Arc::new(Mutex::new(Some(video_stdin)));
    
    // === SYSTEM AUDIO: Write to file (not pipe!) ===
    let audio_file = std::fs::File::create(&system_audio_path)
        .map_err(|e| AppError::Recording(format!("Failed to create audio file: {}", e)))?;
    let audio_writer: Arc<Mutex<Option<std::fs::File>>> = Arc::new(Mutex::new(Some(audio_file)));
    println!("[SCK] System audio file created");
    
    // === MIC RECORDING: Separate FFmpeg process ===
    let mic_process = if mic_enabled {
        println!("[SCK] Starting mic recording...");
        let mut mic_cmd = Command::new("ffmpeg");
        mic_cmd.args([
            "-y", "-hide_banner", "-loglevel", "warning",
            "-f", "avfoundation",
            "-i", &format!(":{}", mic_index),
            "-c:a", "aac", "-b:a", "128k"
        ]);
        mic_cmd.arg(mic_audio_path.to_str().unwrap());
        mic_cmd.stdout(Stdio::null()).stderr(Stdio::piped());
        
        let mut mic_ffmpeg = mic_cmd.spawn()
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
    let video_frame_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let audio_frame_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    
    // Add video handler
    let handler = FrameHandler {
        video_writer: video_writer.clone(),
        audio_writer: audio_writer.clone(),
        video_frame_count: video_frame_count.clone(),
        audio_frame_count: audio_frame_count.clone(),
    };
    stream.add_output_handler(handler, SCStreamOutputType::Screen);
    
    // Add audio handler for system audio
    let audio_handler = FrameHandler {
        video_writer: Arc::new(Mutex::new(None)),
        audio_writer: audio_writer.clone(),
        video_frame_count: video_frame_count.clone(),
        audio_frame_count: audio_frame_count.clone(),
    };
    stream.add_output_handler(audio_handler, SCStreamOutputType::Audio);
    
    // Start capture
    println!("[SCK] Starting capture...");
    stream.start_capture()
        .map_err(|e| AppError::Recording(format!("Failed to start capture: {:?}", e)))?;
    println!("[SCK] ✓ Capture started");
    
    // Store state
    *STATE.lock().unwrap() = Some(RecordingState {
        ffmpeg_process: ffmpeg,
        stream,
        video_writer,
        audio_writer,
        temp_video_path,
        system_audio_path,
        output_path: output_path.clone(),
        mic_process,
        mic_audio_path: if mic_enabled { Some(mic_audio_path) } else { None },
    });
    
    println!("[SCK] ✓ Recording started successfully");
    Ok(())
}

pub fn stop_recording() -> AppResult<PathBuf> {
    println!("[SCK] === STOP RECORDING START ===");
    let stop_start = std::time::Instant::now();
    
    let mut state_guard = STATE.lock().unwrap();
    let mut state = state_guard.take()
        .ok_or_else(|| AppError::Recording("No active recording".to_string()))?;
    drop(state_guard);
    
    let output_path = state.output_path.clone();
    let temp_video_path = state.temp_video_path.clone();
    let system_audio_path = state.system_audio_path.clone();
    let mic_audio_path = state.mic_audio_path.clone();
    
    // STEP 1: Stop ScreenCaptureKit capture
    println!("[SCK] Stopping ScreenCaptureKit capture...");
    let _ = state.stream.stop_capture();
    println!("[SCK] ✓ Capture stopped");
    
    // STEP 2: Wait briefly for callbacks to finish
    thread::sleep(std::time::Duration::from_millis(100));
    
    // STEP 3: Close writers
    println!("[SCK] Closing writers...");
    {
        let mut guard = state.video_writer.lock().unwrap();
        *guard = None;
    }
    println!("[SCK] ✓ Video writer closed");
    
    {
        let mut guard = state.audio_writer.lock().unwrap();
        *guard = None;
    }
    println!("[SCK] ✓ Audio writer closed");
    
    // STEP 4: Wait for video FFmpeg to finish (should finish quickly since stdin is closed)
    println!("[SCK] Waiting for video FFmpeg to finish...");
    let wait_start = std::time::Instant::now();
    loop {
        match state.ffmpeg_process.try_wait() {
            Ok(Some(status)) => {
                println!("[SCK] ✓ Video FFmpeg exited: {:?} ({:?})", status, wait_start.elapsed());
                break;
            }
            Ok(None) => {
                if wait_start.elapsed() > std::time::Duration::from_secs(5) {
                    println!("[SCK] ⚠ Video FFmpeg timeout, killing...");
                    let _ = state.ffmpeg_process.kill();
                    let _ = state.ffmpeg_process.wait();
                    break;
                }
                thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(_) => {
                let _ = state.ffmpeg_process.kill();
                break;
            }
        }
    }
    
    // STEP 5: Stop mic FFmpeg (if running)
    if let Some(mut mic_proc) = state.mic_process.take() {
        println!("[SCK] Stopping mic FFmpeg...");
        let mic_pid = mic_proc.id();
        let _ = Command::new("kill").args(["-INT", &mic_pid.to_string()]).status();
        
        // Wait for mic FFmpeg
        let mic_wait = std::time::Instant::now();
        loop {
            match mic_proc.try_wait() {
                Ok(Some(status)) => {
                    println!("[SCK] ✓ Mic FFmpeg exited: {:?}", status);
                    break;
                }
                Ok(None) => {
                    if mic_wait.elapsed() > std::time::Duration::from_secs(3) {
                        let _ = mic_proc.kill();
                        let _ = mic_proc.wait();
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => {
                    let _ = mic_proc.kill();
                    break;
                }
            }
        }
    }
    
    // STEP 6: Mux video + audio together
    println!("[SCK] Muxing video + audio...");
    let mux_result = mux_final_video(
        &temp_video_path,
        &system_audio_path,
        mic_audio_path.as_ref(),
        &output_path
    );
    
    // Clean up temp files
    let _ = std::fs::remove_file(&temp_video_path);
    let _ = std::fs::remove_file(&system_audio_path);
    if let Some(mic_path) = &mic_audio_path {
        let _ = std::fs::remove_file(mic_path);
    }
    
    println!("[SCK] === STOP RECORDING COMPLETE in {:?} ===", stop_start.elapsed());
    
    // Check result
    if let Err(e) = mux_result {
        println!("[SCK] ⚠ Mux failed: {}, returning video-only", e);
        // If mux failed, copy video-only
        if temp_video_path.exists() {
            let _ = std::fs::copy(&temp_video_path, &output_path);
        }
    }
    
    if output_path.exists() {
        let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
        println!("[SCK] ✓ Recording saved: {:?} ({} bytes)", output_path, size);
        Ok(output_path)
    } else {
        Err(AppError::Recording(format!("Output file not created: {:?}", output_path)))
    }
}

/// Mux video + audio files into final output
fn mux_final_video(
    video_path: &PathBuf,
    system_audio_path: &PathBuf,
    mic_audio_path: Option<&PathBuf>,
    output_path: &PathBuf,
) -> AppResult<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-loglevel", "warning"]);
    
    // Input 0: Video (mp4)
    cmd.args(["-i", video_path.to_str().unwrap()]);
    
    // Input 1: System audio (raw s16le)
    let sys_audio_size = std::fs::metadata(system_audio_path).map(|m| m.len()).unwrap_or(0);
    let has_system_audio = sys_audio_size > 1000; // More than just header
    
    if has_system_audio {
        cmd.args([
            "-f", "s16le",
            "-ar", "48000",
            "-ac", "2",
            "-i", system_audio_path.to_str().unwrap()
        ]);
    }
    
    // Input 2: Mic audio (if present)
    let has_mic_audio = mic_audio_path.map(|p| p.exists()).unwrap_or(false);
    if has_mic_audio {
        cmd.args(["-i", mic_audio_path.unwrap().to_str().unwrap()]);
    }
    
    // Mapping depends on what audio we have
    cmd.args(["-map", "0:v"]); // Always map video
    
    if has_system_audio && has_mic_audio {
        // Mix both audio sources
        cmd.args([
            "-filter_complex", "[1:a][2:a]amix=inputs=2:duration=first[aout]",
            "-map", "[aout]"
        ]);
    } else if has_system_audio {
        cmd.args(["-map", "1:a"]);
    } else if has_mic_audio {
        cmd.args(["-map", "1:a"]); // mic becomes input 1 if no system audio
    } else {
        // No audio - just copy video
        cmd.args(["-c:v", "copy"]);
        cmd.arg(output_path.to_str().unwrap());
        
        println!("[SCK] Muxing: video only (no audio)");
        let status = cmd.status()
            .map_err(|e| AppError::Recording(format!("Mux failed: {}", e)))?;
        
        if !status.success() {
            return Err(AppError::Recording("Mux process failed".to_string()));
        }
        return Ok(());
    }
    
    // Audio encoding
    cmd.args(["-c:v", "copy", "-c:a", "aac", "-b:a", "128k"]);
    cmd.args(["-movflags", "+faststart"]);
    cmd.arg(output_path.to_str().unwrap());
    
    println!("[SCK] Muxing: video + {} + {}", 
        if has_system_audio { "system audio" } else { "no system audio" },
        if has_mic_audio { "mic" } else { "no mic" });
    
    let status = cmd.status()
        .map_err(|e| AppError::Recording(format!("Mux failed: {}", e)))?;
    
    if !status.success() {
        return Err(AppError::Recording("Mux process failed".to_string()));
    }
    
    Ok(())
}
