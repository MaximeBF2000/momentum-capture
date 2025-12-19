use std::path::PathBuf;
use std::process::Command;
use std::thread;

use crate::error::{AppError, AppResult};

use super::mux::mux_final_video;
use super::state::take_state;

pub fn stop_recording() -> AppResult<PathBuf> {
    println!("[SCK] === STOP RECORDING START ===");
    let stop_start = std::time::Instant::now();
    
    let mut state = take_state()
        .ok_or_else(|| AppError::Recording("No active recording".to_string()))?;
    
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
