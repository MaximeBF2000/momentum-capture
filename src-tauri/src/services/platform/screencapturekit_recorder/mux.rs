use std::path::PathBuf;
use std::process::Command;

use crate::error::{AppError, AppResult};
use crate::services::MIC_VOLUME_GAIN;
use std::path::Path;

pub(super) fn mux_final_video(
    video_path: &PathBuf,
    system_audio_path: &PathBuf,
    mic_audio_path: Option<&PathBuf>,
    output_path: &PathBuf,
    system_audio_sample_rate: Option<u32>,
    system_audio_channels: Option<u32>,
    mic_audio_format: Option<(u32, u32)>,
    ffmpeg_path: &Path,
) -> AppResult<()> {
    let mut cmd = Command::new(ffmpeg_path);
    cmd.args(["-y", "-hide_banner", "-loglevel", "warning"]);

    // Input 0: Video (mp4)
    cmd.args(["-i", video_path.to_str().unwrap()]);

    // Input 1: System audio (raw s16le)
    let sys_audio_size = std::fs::metadata(system_audio_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let has_system_audio = sys_audio_size > 1000; // More than just header

    if has_system_audio {
        let detected_rate = system_audio_sample_rate.filter(|rate| *rate > 0);
        let detected_channels = system_audio_channels.filter(|channels| *channels > 0);
        let sample_rate = detected_rate.unwrap_or(48_000);
        let channel_count = detected_channels.unwrap_or(2).max(1);
        cmd.args([
            "-f",
            "s16le",
            "-ar",
            &sample_rate.to_string(),
            "-ac",
            &channel_count.to_string(),
            "-i",
            system_audio_path.to_str().unwrap(),
        ]);
    }

    // Input 2: Mic audio (if present)
    let has_mic_audio = mic_audio_path.map(|p| p.exists()).unwrap_or(false);
    if has_mic_audio {
        let (mic_rate, mic_channels) = mic_audio_format.unwrap_or((48_000, 1));
        cmd.args([
            "-f",
            "s16le",
            "-ar",
            &mic_rate.to_string(),
            "-ac",
            &mic_channels.to_string(),
            "-i",
            mic_audio_path.unwrap().to_str().unwrap(),
        ]);
    }

    // Mapping depends on what audio we have
    cmd.args(["-map", "0:v"]); // Always map video

    let limiter = "alimiter=limit=0.97";
    let mic_gain_enabled = (MIC_VOLUME_GAIN - 1.0).abs() > f32::EPSILON;

    if has_system_audio && has_mic_audio {
        // Mix both audio sources with mic gain applied before amix
        let mut filter_parts = Vec::new();
        let mut mic_source_label = "2:a".to_string();
        if mic_gain_enabled {
            filter_parts.push(format!("[2:a]volume={:.2}[mic_gain];", MIC_VOLUME_GAIN));
            mic_source_label = "mic_gain".into();
        }
        filter_parts.push(format!(
            "[1:a][{}]amix=inputs=2:duration=first:dropout_transition=3[mix];",
            mic_source_label
        ));
        filter_parts.push(format!("[mix]{}[aout]", limiter));
        let filter_complex = filter_parts.join("");
        cmd.arg("-filter_complex");
        cmd.arg(filter_complex);
        cmd.args(["-map", "[aout]"]);
    } else if has_system_audio {
        cmd.args(["-map", "1:a"]);
    } else if has_mic_audio {
        // Mic only becomes input 1 when system audio is absent
        let mut filter_parts = Vec::new();
        let mic_input_label = "1:a";
        let mut mic_source_label = mic_input_label.to_string();
        if mic_gain_enabled {
            filter_parts.push(format!(
                "[{}]volume={:.2}[mic_gain];",
                mic_input_label, MIC_VOLUME_GAIN
            ));
            mic_source_label = "mic_gain".into();
        }
        filter_parts.push(format!("[{}]{}[aout]", mic_source_label, limiter));
        let filter_complex = filter_parts.join("");
        cmd.arg("-filter_complex");
        cmd.arg(filter_complex);
        cmd.args(["-map", "[aout]"]);
    } else {
        // No audio - just copy video
        cmd.args(["-c:v", "copy"]);
        cmd.arg(output_path.to_str().unwrap());

        println!("[SCK] Muxing: video only (no audio)");
        let status = cmd
            .status()
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
        if has_system_audio {
            "system audio"
        } else {
            "no system audio"
        },
        if has_mic_audio {
            format!("mic (gain x{:.2})", MIC_VOLUME_GAIN)
        } else {
            "no mic".to_string()
        },
        if has_system_audio {
            format!(
                " ({} Hz, {} ch)",
                system_audio_sample_rate
                    .filter(|r| *r > 0)
                    .unwrap_or(48_000),
                system_audio_channels.filter(|c| *c > 0).unwrap_or(2)
            )
        } else {
            String::new()
        }
    );

    let status = cmd
        .status()
        .map_err(|e| AppError::Recording(format!("Mux failed: {}", e)))?;

    if !status.success() {
        return Err(AppError::Recording("Mux process failed".to_string()));
    }

    Ok(())
}
