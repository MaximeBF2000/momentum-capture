# Current Implementation Documentation

This document provides a detailed explanation of how the screen recording application currently works, focusing on the capture, processing, and synchronization of all input streams.

## Architecture Overview

The application uses a **two-pass recording approach** with FFmpeg for encoding and muxing:

1. **Capture Phase**: Multiple streams are captured simultaneously into separate temporary files
2. **Muxing Phase**: On stop, all streams are combined into a single MP4 file

This differs from the ideal "single-pass" architecture where everything is composited and written in real-time.

---

## Stream Capture Details

### 1. Screen Video Capture

**Technology**: ScreenCaptureKit (macOS native API)

**Implementation** (`screencapturekit_recorder.rs`):

- Uses `SCStream` with `SCContentFilter` to capture the primary display
- Configuration:
  - Format: BGRA pixels (32-bit per pixel)
  - Resolution: Full display resolution
  - Frame rate: 30 FPS (configured via `minimum_frame_interval`)
  - Pixel format: `PixelFormat::BGRA`

**Data Flow**:

1. ScreenCaptureKit delivers video frames via `did_output_sample_buffer` callback with `SCStreamOutputType::Screen`
2. Each frame contains a `CVPixelBuffer` with raw BGRA pixel data
3. Pixels are extracted and written directly to FFmpeg stdin pipe
4. FFmpeg process (`ffmpeg -f rawvideo`) encodes frames to H.264 and writes to temp MP4 file

**FFmpeg Command**:

```bash
ffmpeg -f rawvideo -pix_fmt bgra -s WIDTHxHEIGHT -r 30 -i pipe:0 \
  -vf scale=... -pix_fmt yuv420p -c:v libx264 -preset ultrafast -crf 23 \
  -an -movflags +faststart temp_video.mp4
```

**Key Characteristics**:

- Video-only encoding (no audio in this pass)
- Uses hardware-accelerated H.264 encoding
- Timestamps: Not explicitly managed - FFmpeg uses frame order and frame rate

---

### 2. System Audio Capture

**Technology**: ScreenCaptureKit (same stream as video)

**Implementation**:

- ScreenCaptureKit delivers audio samples via `did_output_sample_buffer` callback with `SCStreamOutputType::Audio`
- Audio configuration:
  - Sample rate: 48 kHz
  - Channels: 2 (stereo)
  - Format: Float32 planar (non-interleaved)

**Data Processing**:

1. Audio buffers arrive as `CMSampleBuffer` containing `AudioBufferList`
2. Planar format: Buffer 0 = all left channel samples, Buffer 1 = all right channel samples
3. Conversion process:
   - Extract Float32 samples from both channels
   - **Apply mute gain**: If `SYSTEM_AUDIO_MUTED` is true, gain = 0.0, else gain = 1.0
   - Interleave channels: L0 R0 L1 R1 L2 R2...
   - Convert Float32 → s16le (signed 16-bit little-endian)
   - Write to raw PCM file (`sck_sysaudio_{session_id}.raw`)

**Mute Control**:

- **Live mute**: Applied during capture (gain multiplication before conversion)
- State stored in `AtomicBool SYSTEM_AUDIO_MUTED`
- Can be toggled via `set_system_audio_muted()` command

**File Format**:

- Raw PCM: s16le, 48 kHz, stereo, interleaved
- No header/metadata - just raw sample data

---

### 3. Microphone Audio Capture

**Technology**: FFmpeg with avfoundation

**Implementation**:

- Separate FFmpeg process spawned for microphone capture
- Uses avfoundation input device (resolved via `device_resolver`)
- Process runs independently of screen capture

**FFmpeg Command**:

```bash
ffmpeg -f avfoundation -i :{mic_index} -c:a aac -b:a 128k temp_mic.m4a
```

**Data Flow**:

1. FFmpeg captures microphone audio directly
2. Encodes to AAC format
3. Saves to temp M4A file
4. Process runs until recording stops (killed via SIGINT)

**Mute Control**:

- **NOT applied during capture** - mic is always captured
- Mute is applied during **muxing phase** (post-processing)
- When muted, FFmpeg volume filter sets gain to 0.0 during muxing

**Key Characteristics**:

- Independent process with its own timeline
- No synchronization with screen video or system audio during capture
- Mute state stored in `AtomicBool MIC_MUTED` but only used during muxing

---

### 4. Webcam Video (Camera Overlay)

**Technology**: FFmpeg with avfoundation (for preview only)

**Current Status**: **NOT captured into final video file**

**Implementation** (`camera.rs`):

- FFmpeg process streams webcam frames as JPEG images
- Frames are base64-encoded and sent to React UI via Tauri events
- Used exclusively for the camera overlay window preview

**FFmpeg Command**:

```bash
ffmpeg -f avfoundation -framerate 30 -video_size 640x480 \
  -i {camera_index}: -vf fps=30 -f image2pipe -vcodec mjpeg -q:v 3 -
```

**Data Flow**:

1. FFmpeg captures webcam frames
2. Encodes each frame as JPEG
3. JPEG data is read from stdout
4. Base64-encoded and emitted as `camera-frame` event
5. React UI displays frames in overlay window

**Toggle Behavior**:

- When camera overlay is hidden: FFmpeg process is **stopped** (`preview.stop()`)
- When camera overlay is shown: FFmpeg process is **started** (`preview.start()`)
- This start/stop cycle causes delays when toggling

**Key Limitation**:

- Webcam frames are **never captured into the recording**
- Only used for live preview overlay
- No synchronization with other streams

---

## Recording State Management

### State Structure (`recording.rs`)

```rust
pub struct RecordingState {
    pub is_recording: bool,
    pub is_paused: bool,
    pub start_time: Option<std::time::Instant>,
    pub paused_duration: Duration,
    pub output_file: Option<PathBuf>,
    pub include_microphone: bool,
}
```

**Note**: Pause/resume is not fully implemented - ScreenCaptureKit doesn't support native pause, would require stop/restart with concatenation.

### Global Mute States (`screencapturekit_recorder.rs`)

```rust
static MIC_MUTED: AtomicBool = AtomicBool::new(false);
static SYSTEM_AUDIO_MUTED: AtomicBool = AtomicBool::new(false);
```

- Both reset to `false` at recording start
- Can be toggled during recording via commands
- System audio mute: Applied live during capture
- Mic mute: Applied during muxing (not live)

---

## Muxing Phase (Stop Recording)

When `stop_recording()` is called:

### Step 1: Stop Capture

- Stop ScreenCaptureKit stream
- Wait 500ms for callbacks to finish
- Flush and close all writers

### Step 2: Wait for Processes

- Wait for video FFmpeg to finish (reads until stdin closes)
- Kill mic FFmpeg process (SIGINT, then force kill if needed)

### Step 3: Mux All Streams

**FFmpeg Muxing Command**:

```bash
ffmpeg -i temp_video.mp4 \
  -f s16le -ar 48000 -ac 2 -i temp_system_audio.raw \
  -i temp_mic.m4a \
  -filter_complex "[1:a]volume={sys_gain}[sys];[2:a]volume={mic_gain}[mic];[sys][mic]amix=inputs=2:duration=shortest:normalize=0[aout]" \
  -map 0:v -map "[aout]" \
  -c:v copy -c:a aac -b:a 128k -shortest \
  -movflags +faststart output.mp4
```

**Mute Application**:

- System audio gain: `1.5` if unmuted, `0.0` if muted
- Mic gain: `1.0` if unmuted, `0.0` if muted
- Both gains applied via FFmpeg volume filters
- Audio streams mixed with `amix` filter

**Key Points**:

- Video is copied (no re-encoding)
- Audio is re-encoded to AAC
- `-shortest` flag ensures output stops when video ends
- Final file: Single MP4 with video + mixed audio track

---

## Synchronization Analysis

### Current Synchronization Approach

**Timeline Management**: **None** - relies on FFmpeg's automatic synchronization

**How It Works**:

1. Screen video: Encoded with 30 FPS frame rate (implicit timeline)
2. System audio: Raw PCM file, duration calculated from file size
3. Mic audio: AAC file with its own timeline
4. FFmpeg muxing: Uses `-shortest` to align streams, but no explicit timestamp alignment

### Synchronization Issues

1. **No Master Timeline**: Each stream has its own implicit timeline
2. **No Timestamp Normalization**: No `t0` reference point established
3. **Drift Risk**: Different capture processes may have slight clock differences
4. **Mic Audio**: Captured independently, may drift relative to system audio
5. **Webcam**: Not captured, so no sync concerns (but also not composited)

### What Works

- System audio and screen video come from same ScreenCaptureKit stream, so they're naturally synchronized
- FFmpeg's `-shortest` flag ensures output duration matches shortest input
- Muxing generally produces acceptable results for typical recording durations

### What Doesn't Work

- Mic audio may drift over long recordings
- No guarantee of frame-accurate sync
- Webcam overlay not synced with anything (not even captured)

---

## Live Control Implementation

### System Audio Mute

**Implementation**: ✅ **Live** (applied during capture)

- State: `SYSTEM_AUDIO_MUTED` AtomicBool
- Applied: In `did_output_sample_buffer` callback for audio
- Method: Gain multiplication (0.0 or 1.0) before Float32→s16le conversion
- Result: Muted audio written as silence to raw PCM file

### Microphone Mute

**Implementation**: ❌ **Post-processing** (applied during muxing)

- State: `MIC_MUTED` AtomicBool
- Applied: In `mux_final_video()` function via FFmpeg volume filter
- Method: FFmpeg `volume=0` filter during muxing
- Result: Mic audio still captured, but set to silence in final file
- **Issue**: Mic is still being captured even when muted (wasteful)

### Camera Overlay Toggle

**Implementation**: ⚠️ **Process start/stop** (causes delays)

- When hidden: FFmpeg camera process is stopped
- When shown: FFmpeg camera process is started
- **Issue**: Start/stop cycle adds delay, camera not captured into video anyway

---

## File Structure

### Temporary Files Created During Recording

1. `sck_video_{session_id}.mp4` - Screen video (H.264)
2. `sck_sysaudio_{session_id}.raw` - System audio (raw PCM s16le)
3. `sck_mic_{session_id}.m4a` - Microphone audio (AAC) - only if mic enabled

### Final Output

- `momentum-recording-{timestamp}.mp4` in Downloads directory
- Single file with video + mixed audio track
- Temporary files cleaned up after successful muxing

---

## Frontend Integration

### React State (`recordingStore.ts`)

- Tracks recording state, mute states, camera enabled state
- UI buttons call Tauri commands to toggle mute states
- Mute toggles immediately update UI, then send command to backend

### Tauri Commands

- `set_mic_muted(muted: bool)` - Updates `MIC_MUTED` AtomicBool
- `set_system_audio_muted(muted: bool)` - Updates `SYSTEM_AUDIO_MUTED` AtomicBool
- `set_camera_overlay_visible(visible: bool)` - Starts/stops camera preview FFmpeg

### Event Flow

1. User clicks mute button → React updates local state
2. React calls Tauri command → Backend updates AtomicBool
3. For system audio: Next callback applies mute immediately
4. For mic: Mute applied during muxing (when recording stops)

---

## Summary of Current Limitations

1. **Webcam Not Captured**: Camera overlay is preview-only, not composited into final video
2. **No Webcam Sync**: Webcam preview not synchronized with microphone or other streams
3. **Mic Mute Not Live**: Microphone mute applied post-processing, not during capture
4. **Camera Toggle Delay**: Starting/stopping camera process causes delays
5. **No Explicit Timeline**: No master timeline or timestamp normalization
6. **Potential Drift**: Independent capture processes may drift over time
7. **Two-Pass Architecture**: Requires muxing step, not single-pass real-time composition

---

## Technical Stack Summary

- **Screen Capture**: ScreenCaptureKit (macOS native)
- **Audio Capture**: ScreenCaptureKit (system) + FFmpeg avfoundation (mic)
- **Camera Preview**: FFmpeg avfoundation (JPEG streaming)
- **Encoding**: FFmpeg (H.264 video, AAC audio)
- **Muxing**: FFmpeg
- **Backend**: Rust + Tauri
- **Frontend**: React + TypeScript + Zustand

---

## Next Steps Needed

To achieve the ideal implementation with live controls and perfect sync:

1. Implement real-time video compositor (screen + webcam overlay)
2. Implement real-time audio mixer (system + mic with live gain control)
3. Establish master timeline with timestamp normalization
4. Capture webcam into recording (not just preview)
5. Apply mic mute live during capture (not post-processing)
6. Keep camera capture running continuously (don't stop/start on toggle)
7. Use AVAssetWriter for single-pass recording (or improve FFmpeg pipeline)
