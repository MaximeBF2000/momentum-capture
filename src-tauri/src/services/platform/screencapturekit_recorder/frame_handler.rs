use std::io::Write;
use std::sync::{Arc, Mutex};

use screencapturekit::cv::CVPixelBufferLockFlags;
use screencapturekit::prelude::*;

// Handler for ScreenCaptureKit callbacks
pub(super) struct FrameHandler {
    pub(super) video_writer: Arc<Mutex<Option<std::process::ChildStdin>>>,
    pub(super) audio_writer: Arc<Mutex<Option<std::fs::File>>>,
    pub(super) video_frame_count: Arc<std::sync::atomic::AtomicU64>,
    pub(super) audio_frame_count: Arc<std::sync::atomic::AtomicU64>,
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
