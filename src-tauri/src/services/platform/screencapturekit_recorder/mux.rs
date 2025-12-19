use std::path::PathBuf;
use std::process::Command;

use crate::error::{AppError, AppResult};

pub(super) fn mux_final_video(
    video_path: &PathBuf,
    system_audio_path: &PathBuf,
    mic_audio_path: Option<&PathBuf>,
    output_path: &PathBuf,
    system_audio_sample_rate: Option<u32>,
    system_audio_channels: Option<u32>,
) -> AppResult<()> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-loglevel", "warning"]);
    
    // Input 0: Video (mp4)
    cmd.args(["-i", video_path.to_str().unwrap()]);
    
    // Input 1: System audio (raw s16le)
    let sys_audio_size = std::fs::metadata(system_audio_path).map(|m| m.len()).unwrap_or(0);
    let has_system_audio = sys_audio_size > 1000; // More than just header
    
    if has_system_audio {
        let detected_rate = system_audio_sample_rate.filter(|rate| *rate > 0);
        let detected_channels = system_audio_channels.filter(|channels| *channels > 0);
        let sample_rate = detected_rate.unwrap_or(48_000);
        let channel_count = detected_channels.unwrap_or(2).max(1);
        cmd.args([
            "-f", "s16le",
            "-ar", &sample_rate.to_string(),
            "-ac", &channel_count.to_string(),
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
    cmd.args(["-c:v", "copy", "-c:a", "aac", "-b:a", "128k", "-shortest"]);
    cmd.args(["-movflags", "+faststart"]);
    cmd.arg(output_path.to_str().unwrap());
    
    println!(
        "[SCK] Muxing: video + {} + {}{}",
        if has_system_audio { "system audio" } else { "no system audio" },
        if has_mic_audio { "mic" } else { "no mic" },
        if has_system_audio {
            format!(
                " ({} Hz, {} ch)",
                system_audio_sample_rate
                    .filter(|r| *r > 0)
                    .unwrap_or(48_000),
                system_audio_channels
                    .filter(|c| *c > 0)
                    .unwrap_or(2)
            )
        } else {
            String::new()
        }
    );
    
    let status = cmd.status()
        .map_err(|e| AppError::Recording(format!("Mux failed: {}", e)))?;
    
    if !status.success() {
        return Err(AppError::Recording("Mux process failed".to_string()));
    }
    
    Ok(())
}
