use std::path::PathBuf;
use std::process::Command;

use crate::error::{AppError, AppResult};

pub(super) fn mux_final_video(
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
