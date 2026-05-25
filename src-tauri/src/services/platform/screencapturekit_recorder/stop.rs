use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

use super::mux::mux_final_video;
use super::state::RecordingState;

pub fn stop_recording(
    state: &Mutex<Option<RecordingState>>,
    recording_paused: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> AppResult<PathBuf> {
    println!("[SCK] === STOP RECORDING START ===");
    let stop_start = Instant::now();

    let mut state = state
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| AppError::Recording("No active recording".to_string()))?;
    recording_paused.store(false, Ordering::Relaxed);

    let output_path = state.output_path.clone();
    let temp_video_path = state.temp_video_path.clone();
    let system_audio_path = state.system_audio_path.clone();
    let mic_audio_path = state.mic_audio_path.clone();
    let system_audio_sample_rate = state.system_audio_sample_rate.load(Ordering::Relaxed);
    let system_audio_channel_count = state.system_audio_channel_count.load(Ordering::Relaxed);
    let system_audio_samples = state.audio_samples_written.load(Ordering::Relaxed);
    let mic_audio_samples = state.mic_samples_written.load(Ordering::Relaxed);
    let first_screen_arrival_ns = state.first_screen_frame_arrival_ns.load(Ordering::Relaxed);
    let first_system_audio_arrival_ns = state.first_system_audio_arrival_ns.load(Ordering::Relaxed);
    let first_mic_audio_arrival_ns = state.first_mic_audio_arrival_ns.load(Ordering::Relaxed);
    let mic_sample_rate = state.mic_sample_rate;
    let mic_channel_count = state.mic_channel_count;

    // STEP 1: Stop ScreenCaptureKit capture
    println!("[SCK] Stopping ScreenCaptureKit capture...");
    let _ = state.stream.stop_capture();
    println!("[SCK] ✓ Capture stopped");

    // STEP 2: Wait briefly for callbacks to finish
    thread::sleep(Duration::from_millis(100));

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
    let wait_start = Instant::now();
    loop {
        match state.ffmpeg_process.try_wait() {
            Ok(Some(status)) => {
                println!(
                    "[SCK] ✓ Video FFmpeg exited: {:?} ({:?})",
                    status,
                    wait_start.elapsed()
                );
                break;
            }
            Ok(None) => {
                if wait_start.elapsed() > Duration::from_secs(5) {
                    println!("[SCK] ⚠ Video FFmpeg timeout, killing...");
                    let _ = state.ffmpeg_process.kill();
                    let _ = state.ffmpeg_process.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(100));
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
        let _ = Command::new("kill")
            .args(["-INT", &mic_pid.to_string()])
            .status();

        // Wait for mic FFmpeg
        let mic_wait = Instant::now();
        loop {
            match mic_proc.try_wait() {
                Ok(Some(status)) => {
                    println!("[SCK] ✓ Mic FFmpeg exited: {:?}", status);
                    break;
                }
                Ok(None) => {
                    if mic_wait.elapsed() > Duration::from_secs(3) {
                        let _ = mic_proc.kill();
                        let _ = mic_proc.wait();
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => {
                    let _ = mic_proc.kill();
                    break;
                }
            }
        }
    }

    let video_frames = state.video_frame_count.load(Ordering::Relaxed);
    let audio_packets = state.audio_frame_count.load(Ordering::Relaxed);
    let audio_samples = system_audio_samples;
    let approx_video_seconds = if state.requested_fps > 0 {
        video_frames as f64 / state.requested_fps as f64
    } else {
        0.0
    };
    let approx_audio_seconds = if system_audio_sample_rate > 0 {
        audio_samples as f64 / system_audio_sample_rate as f64
    } else {
        0.0
    };
    println!(
        "[SCK] Frame stats: video={} (~{:.2}s @ {} fps), audio_packets={} samples={} (~{:.2}s @ {} Hz)",
        video_frames,
        approx_video_seconds,
        state.requested_fps,
        audio_packets,
        audio_samples,
        approx_audio_seconds,
        system_audio_sample_rate
    );
    let capture_elapsed_ms = state.capture_started_at.elapsed().as_millis();
    println!(
        "[SCK] Timeline markers (from recorder start): screen={}ms system={}ms mic={}ms total={}ms",
        first_screen_arrival_ns / 1_000_000,
        first_system_audio_arrival_ns / 1_000_000,
        first_mic_audio_arrival_ns / 1_000_000,
        capture_elapsed_ms
    );

    let audio_anchor_ns = first_screen_arrival_ns;
    let system_audio_offset_seconds = if audio_anchor_ns > 0 && first_system_audio_arrival_ns > 0 {
        Some((first_system_audio_arrival_ns as f64 - audio_anchor_ns as f64) / 1_000_000_000.0)
    } else {
        None
    };
    let mic_audio_offset_seconds = if audio_anchor_ns > 0 && first_mic_audio_arrival_ns > 0 {
        Some((first_mic_audio_arrival_ns as f64 - audio_anchor_ns as f64) / 1_000_000_000.0)
    } else {
        None
    };

    // STEP 6: Mux video + audio together
    println!("[SCK] Muxing video + audio...");
    let mux_start = Instant::now();
    let mux_result = mux_final_video(
        &temp_video_path,
        &system_audio_path,
        mic_audio_path.as_ref(),
        &output_path,
        if system_audio_sample_rate > 0 {
            Some(system_audio_sample_rate)
        } else {
            None
        },
        if system_audio_channel_count > 0 {
            Some(system_audio_channel_count)
        } else {
            None
        },
        mic_sample_rate.zip(mic_channel_count),
        system_audio_samples,
        mic_audio_samples,
        approx_video_seconds,
        system_audio_offset_seconds,
        mic_audio_offset_seconds,
        &state.ffmpeg_path,
    );
    let mux_elapsed = mux_start.elapsed();

    if let Err(e) = mux_result {
        println!("[SCK] ⚠ Mux failed after {:?}: {}", mux_elapsed, e);
        println!("[SCK] Attempting video-only fallback...");
        if temp_video_path.exists() {
            std::fs::copy(&temp_video_path, &output_path).map_err(|copy_err| {
                AppError::Recording(format!(
                    "Mux failed ({}) and video-only fallback copy failed ({}). Temp files preserved: video={:?}, system_audio={:?}, mic_audio={:?}",
                    e, copy_err, temp_video_path, system_audio_path, mic_audio_path
                ))
            })?;
            println!("[SCK] Video-only fallback copied to {:?}", output_path);
        } else {
            return Err(AppError::Recording(format!(
                "Mux failed ({}) and temp video is missing. Temp files preserved: video={:?}, system_audio={:?}, mic_audio={:?}",
                e, temp_video_path, system_audio_path, mic_audio_path
            )));
        }
    } else {
        println!("[SCK] Mux completed in {:?}", mux_elapsed);
    }

    if output_path.exists() {
        let size = std::fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0);
        println!(
            "[SCK] ✓ Recording saved: {:?} ({} bytes)",
            output_path, size
        );
        cleanup_temp_files(
            &temp_video_path,
            &system_audio_path,
            mic_audio_path.as_ref(),
        );
        println!(
            "[SCK] === STOP RECORDING COMPLETE in {:?} ===",
            stop_start.elapsed()
        );
        Ok(output_path)
    } else {
        Err(AppError::Recording(format!(
            "Output file not created: {:?}. Temp files preserved: video={:?}, system_audio={:?}, mic_audio={:?}",
            output_path, temp_video_path, system_audio_path, mic_audio_path
        )))
    }
}

fn cleanup_temp_files(
    temp_video_path: &PathBuf,
    system_audio_path: &PathBuf,
    mic_audio_path: Option<&PathBuf>,
) {
    remove_temp_file(temp_video_path, "video");
    remove_temp_file(system_audio_path, "system audio");
    if let Some(path) = mic_audio_path {
        remove_temp_file(path, "mic audio");
    }
}

fn remove_temp_file(path: &PathBuf, label: &str) {
    match std::fs::remove_file(path) {
        Ok(_) => println!("[SCK] Removed temp {} file: {:?}", label, path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => eprintln!(
            "[SCK] Failed to remove temp {} file {:?}: {}",
            label, path, err
        ),
    }
}
