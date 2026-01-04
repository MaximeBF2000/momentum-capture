use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::state;
use crate::services::camera;
use crate::services::time::cm_time_to_ns;
use screencapturekit::cv::CVPixelBufferLockFlags;
use screencapturekit::prelude::*;

static SCREEN_PTS_COUNTER: AtomicU64 = AtomicU64::new(0);

// Handler for ScreenCaptureKit callbacks
pub(super) struct FrameHandler {
    pub(super) video_writer: Arc<Mutex<Option<std::process::ChildStdin>>>,
    pub(super) audio_writer: Arc<Mutex<Option<std::fs::File>>>,
    pub(super) video_frame_count: Arc<std::sync::atomic::AtomicU64>,
    pub(super) audio_frame_count: Arc<std::sync::atomic::AtomicU64>,
    pub(super) audio_sample_rate: Arc<AtomicU32>,
    pub(super) audio_channel_count: Arc<AtomicU32>,
    pub(super) audio_layout_logged: Arc<AtomicBool>,
    pub(super) audio_samples_written: Arc<AtomicU64>,
}

impl SCStreamOutputTrait for FrameHandler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        match of_type {
            SCStreamOutputType::Screen => {
                let screen_pts_ns = cm_time_to_ns(sample.presentation_timestamp());
                let duration_ns = cm_time_to_ns(sample.duration());
                let tick = SCREEN_PTS_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
                if tick <= 5 || tick % 60 == 0 {
                    println!(
                        "[CameraSync] Screen frame #{} pts={}ns duration={}ns",
                        tick, screen_pts_ns, duration_ns
                    );
                }
                camera::emit_camera_frame_for_screen_pts(screen_pts_ns);
                if state::recording_paused() {
                    return;
                }
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
                self.capture_audio_metadata(&sample);
                if state::recording_paused() {
                    return;
                }
                let mut writer_guard = self.audio_writer.lock().unwrap();
                if let Some(ref mut writer) = *writer_guard {
                    if let Some(audio_buffers) = sample.audio_buffer_list() {
                        let planes: Vec<AudioPlane<'_>> = audio_buffers
                            .iter()
                            .map(|buffer| {
                                let data = buffer.data();
                                let float_samples = unsafe {
                                    std::slice::from_raw_parts(
                                        data.as_ptr() as *const f32,
                                        data.len() / 4,
                                    )
                                };
                                AudioPlane {
                                    channels: buffer.number_channels,
                                    samples: float_samples,
                                    bytes: data.len(),
                                }
                            })
                            .collect();

                        if planes.is_empty() {
                            return;
                        }

                        let planar_layout =
                            planes.len() > 1 && planes.iter().all(|plane| plane.channels == 1);
                        let frames_per_channel = if planar_layout {
                            planes
                                .iter()
                                .map(|plane| plane.samples.len())
                                .min()
                                .unwrap_or(0)
                        } else {
                            planes
                                .first()
                                .map(|plane| {
                                    if plane.channels > 0 {
                                        plane.samples.len() / plane.channels as usize
                                    } else {
                                        plane.samples.len()
                                    }
                                })
                                .unwrap_or(0)
                        };

                        self.log_audio_layout_once(
                            planar_layout,
                            &planes,
                            frames_per_channel,
                            sample.presentation_timestamp(),
                            sample.duration(),
                        );

                        let mut s16_data = if planar_layout {
                            convert_planar_buffers(&planes)
                        } else {
                            convert_interleaved_buffers(&planes)
                        };

                        if s16_data.is_empty() {
                            return;
                        }

                        if state::system_audio_muted() {
                            s16_data.iter_mut().for_each(|b| *b = 0);
                        }

                        if writer.write_all(&s16_data).is_ok() {
                            self.audio_samples_written
                                .fetch_add(frames_per_channel as u64, Ordering::Relaxed);
                            let count = self.audio_frame_count.fetch_add(1, Ordering::Relaxed);
                            if count == 0 {
                                println!(
                                    "[SCK] First audio frame written ({} bytes, planar={})",
                                    s16_data.len(),
                                    planar_layout
                                );
                            } else if count % 100 == 0 {
                                println!("[SCK] Audio frames: {}", count + 1);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl FrameHandler {
    fn capture_audio_metadata(&self, sample: &CMSampleBuffer) {
        let current_rate = self.audio_sample_rate.load(std::sync::atomic::Ordering::Relaxed);
        let current_channels = self.audio_channel_count.load(std::sync::atomic::Ordering::Relaxed);
        if current_rate > 0 && current_channels > 0 {
            return;
        }

        if let Some(format_desc) = sample.format_description() {
            if current_rate == 0 {
                if let Some(rate_hz) = format_desc.audio_sample_rate() {
                    let detected_rate = rate_hz.round() as u32;
                    if detected_rate > 0
                        && self
                            .audio_sample_rate
                            .compare_exchange(
                                0,
                                detected_rate,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            )
                            .is_ok()
                    {
                        println!(
                            "[SCK] Detected system audio sample rate: {} Hz",
                            detected_rate
                        );
                    }
                }
            }

            if current_channels == 0 {
                if let Some(detected_channels) = format_desc.audio_channel_count() {
                    if detected_channels > 0
                        && self
                            .audio_channel_count
                            .compare_exchange(
                                0,
                                detected_channels,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            )
                            .is_ok()
                    {
                        println!(
                            "[SCK] Detected system audio channels: {}",
                            detected_channels
                        );
                    }
                }
            }
        }
    }

    fn log_audio_layout_once(
        &self,
        planar_layout: bool,
        planes: &[AudioPlane<'_>],
        frames_per_channel: usize,
        pts: CMTime,
        duration: CMTime,
    ) {
        if self
            .audio_layout_logged
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let pts_seconds = cm_time_seconds(pts);
        let duration_seconds = cm_time_seconds(duration);
        let plane_summary = planes
            .iter()
            .enumerate()
            .map(|(idx, plane)| format!("#{}:{}ch/{}B", idx, plane.channels, plane.bytes))
            .collect::<Vec<_>>()
            .join(", ");

        if planar_layout {
            println!(
                "[SCK] Audio layout: planar ({} frames per channel, planes [{}]), pts={:.6}s, duration={:.6}s",
                frames_per_channel, plane_summary, pts_seconds, duration_seconds
            );
        } else {
            println!(
                "[SCK] Audio layout: interleaved ({} frames per block, planes [{}]), pts={:.6}s, duration={:.6}s",
                frames_per_channel, plane_summary, pts_seconds, duration_seconds
            );
        }
    }
}

fn convert_interleaved_buffers(planes: &[AudioPlane<'_>]) -> Vec<u8> {
    if planes.is_empty() {
        return Vec::new();
    }

    // Treat all buffers as sequential interleaved data.
    let mut result = Vec::new();
    for plane in planes {
        if plane.samples.is_empty() {
            continue;
        }

        if result.capacity() < result.len() + plane.samples.len() * 2 {
            result.reserve(plane.samples.len() * 2);
        }

        for &sample in plane.samples {
            let s16 = float_to_s16(sample);
            result.extend_from_slice(&s16.to_le_bytes());
        }
    }
    result
}

fn convert_planar_buffers(planes: &[AudioPlane<'_>]) -> Vec<u8> {
    if planes.is_empty() {
        return Vec::new();
    }

    let frames_per_channel = planes
        .iter()
        .map(|plane| plane.samples.len())
        .min()
        .unwrap_or(0);
    if frames_per_channel == 0 {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(frames_per_channel * planes.len() * 2);
    for frame_idx in 0..frames_per_channel {
        for plane in planes {
            let sample = plane.samples[frame_idx];
            let s16 = float_to_s16(sample);
            result.extend_from_slice(&s16.to_le_bytes());
        }
    }
    result
}

#[inline]
fn float_to_s16(sample: f32) -> i16 {
    let clamped = sample.max(-1.0).min(1.0);
    (clamped * 32767.0) as i16
}

#[inline]
fn cm_time_seconds(time: CMTime) -> f64 {
    if time.timescale == 0 {
        return 0.0;
    }
    time.value as f64 / time.timescale as f64
}

struct AudioPlane<'a> {
    channels: u32,
    samples: &'a [f32],
    bytes: usize,
}
