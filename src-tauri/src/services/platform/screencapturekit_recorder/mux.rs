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
    system_audio_samples: u64,
    mic_audio_samples: u64,
    approx_video_seconds: f64,
    system_audio_offset_seconds: Option<f64>,
    mic_audio_offset_seconds: Option<f64>,
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
    let has_mic_audio = mic_audio_path
        .map(|p| p.exists() && std::fs::metadata(p).map(|m| m.len() > 1000).unwrap_or(false))
        .unwrap_or(false);
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

    if !has_system_audio && !has_mic_audio {
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

    let limiter = "alimiter=limit=0.97";
    let mic_gain_enabled = (MIC_VOLUME_GAIN - 1.0).abs() > f32::EPSILON;
    let mut filter_parts: Vec<String> = Vec::new();

    let mut next_audio_input_idx = 1u8;
    let system_audio_input = if has_system_audio {
        let idx = next_audio_input_idx;
        next_audio_input_idx += 1;
        Some(idx)
    } else {
        None
    };
    let mic_audio_input = if has_mic_audio {
        let idx = next_audio_input_idx;
        Some(idx)
    } else {
        None
    };

    let mut system_ready_label: Option<String> = None;
    if let Some(idx) = system_audio_input {
        let input_label = format!("{idx}:a");
        let aligned_label = "sys_aligned";
        push_alignment_filter(
            &mut filter_parts,
            &input_label,
            system_audio_offset_seconds,
            aligned_label,
        );
        system_ready_label = Some(aligned_label.to_string());
    }

    let mut mic_ready_label: Option<String> = None;
    let mut mic_tempo_applied: Option<f64> = None;
    if let Some(idx) = mic_audio_input {
        let input_label = format!("{idx}:a");
        let aligned_label = "mic_aligned";
        push_alignment_filter(
            &mut filter_parts,
            &input_label,
            mic_audio_offset_seconds,
            aligned_label,
        );

        let mut working_label = aligned_label.to_string();
        if let Some((mic_rate, _)) = mic_audio_format {
            if mic_rate > 0 && mic_audio_samples > 0 && approx_video_seconds > 0.0 {
                let mic_duration = mic_audio_samples as f64 / mic_rate as f64;
                let ratio = mic_duration / approx_video_seconds;
                if (ratio - 1.0).abs() > 0.001 {
                    let tempo_chain = build_atempo_chain(ratio);
                    if !tempo_chain.is_empty() {
                        filter_parts.push(format!(
                            "[{}]{}[mic_tempo]",
                            working_label, tempo_chain
                        ));
                        working_label = "mic_tempo".into();
                        mic_tempo_applied = Some(ratio);
                    }
                }
            }
        }

        if mic_gain_enabled {
            filter_parts.push(format!(
                "[{}]volume={:.2}[mic_gain]",
                working_label, MIC_VOLUME_GAIN
            ));
            working_label = "mic_gain".into();
        }

        mic_ready_label = Some(working_label);
    }

    let mix_source_label = match (system_ready_label.as_ref(), mic_ready_label.as_ref()) {
        (Some(system_label), Some(mic_label)) => {
            filter_parts.push(format!(
                "[{}][{}]amix=inputs=2:duration=longest:dropout_transition=0[mix]",
                system_label, mic_label
            ));
            "mix".to_string()
        }
        (Some(system_label), None) => system_label.clone(),
        (None, Some(mic_label)) => mic_label.clone(),
        (None, None) => unreachable!("audio filters requested without any audio source"),
    };

    let mut post_mix_filters = vec!["aresample=async=1000:first_pts=0".to_string()];
    if approx_video_seconds > 0.0 {
        post_mix_filters.push(format!("atrim=duration={:.6}", approx_video_seconds));
    }
    post_mix_filters.push(limiter.to_string());
    filter_parts.push(format!(
        "[{}]{}[aout]",
        mix_source_label,
        post_mix_filters.join(",")
    ));

    cmd.arg("-filter_complex");
    cmd.arg(filter_parts.join(";"));
    cmd.args(["-map", "[aout]"]);

    // Audio encoding
    cmd.args(["-c:v", "copy", "-c:a", "aac", "-b:a", "128k", "-shortest"]);
    cmd.args(["-movflags", "+faststart"]);
    cmd.arg(output_path.to_str().unwrap());

    println!(
        "[SCK] Muxing: video + system={} (offset={:+.3}s, {} samples) + mic={} (offset={:+.3}s, {} samples, tempo={})",
        has_system_audio,
        system_audio_offset_seconds.unwrap_or(0.0),
        system_audio_samples,
        has_mic_audio,
        mic_audio_offset_seconds.unwrap_or(0.0),
        mic_audio_samples,
        mic_tempo_applied
            .map(|v| format!("{:.6}", v))
            .unwrap_or_else(|| "none".to_string())
    );
    println!("[SCK] Mux filter graph: {}", filter_parts.join(";"));

    let status = cmd
        .status()
        .map_err(|e| AppError::Recording(format!("Mux failed: {}", e)))?;

    if !status.success() {
        return Err(AppError::Recording("Mux process failed".to_string()));
    }

    Ok(())
}

fn push_alignment_filter(
    filter_parts: &mut Vec<String>,
    input_label: &str,
    offset_seconds: Option<f64>,
    output_label: &str,
) {
    let offset = offset_seconds.unwrap_or(0.0);
    if offset.abs() < 0.001 {
        filter_parts.push(format!("[{}]anull[{}]", input_label, output_label));
        return;
    }

    if offset > 0.0 {
        let delay_ms = (offset * 1000.0).round().max(1.0) as u64;
        filter_parts.push(format!(
            "[{}]adelay={}:all=1[{}]",
            input_label, delay_ms, output_label
        ));
    } else {
        let trim_start = (-offset).max(0.0);
        filter_parts.push(format!(
            "[{}]atrim=start={:.6},asetpts=PTS-STARTPTS[{}]",
            input_label, trim_start, output_label
        ));
    }
}

fn build_atempo_chain(mut ratio: f64) -> String {
    if !ratio.is_finite() || ratio <= 0.0 {
        return String::new();
    }

    let mut factors: Vec<f64> = Vec::new();
    while ratio > 2.0 {
        factors.push(2.0);
        ratio /= 2.0;
    }
    while ratio < 0.5 {
        factors.push(0.5);
        ratio /= 0.5;
    }

    if (ratio - 1.0).abs() > 0.0001 {
        factors.push(ratio);
    }

    factors
        .into_iter()
        .map(|factor| format!("atempo={:.6}", factor))
        .collect::<Vec<_>>()
        .join(",")
}
