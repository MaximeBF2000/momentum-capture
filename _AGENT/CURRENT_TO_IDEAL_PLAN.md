# Plan: Current Implementation → Ideal Implementation

This document outlines the step-by-step plan to transition from the current two-pass FFmpeg-based recording architecture to the ideal single-pass real-time composition architecture with perfect synchronization and live controls.

## Goals

1. ✅ **Live mute controls**: Microphone and system audio can be muted/unmuted individually during recording with gain = 0 (applied live, not post-processing)
2. ✅ **Perfect lip sync**: Webcam overlay (captured via screen) and microphone audio are synchronized on the same master timeline
3. ✅ **Webcam capture**: Webcam overlay is captured via screen recording (privacy-friendly: LED only on when overlay visible)
4. ✅ **Perfect sync**: All streams (screen, system audio, mic) are synchronized on a single master timeline (`t0`)
5. ✅ **Native camera preview**: Camera preview uses AVCaptureSession (not FFmpeg) for better performance and sync

---

## Architecture Comparison

### Current Architecture (Two-Pass)

```
┌─────────────────┐     ┌──────────────┐
│ ScreenCaptureKit│────▶│ FFmpeg Video │───┐
│ (Screen + Sys)  │     │   (H.264)    │   │
└─────────────────┘     └──────────────┘   │
                                            │
┌─────────────────┐     ┌──────────────┐   │     ┌─────────────┐
│ ScreenCaptureKit│────▶│ Raw PCM File │───┼────▶│ FFmpeg Mux  │──▶ Final MP4
│ (System Audio)  │     │  (s16le)     │   │     │ (Post-proc) │
└─────────────────┘     └──────────────┘   │     └─────────────┘
                                            │
┌─────────────────┐     ┌──────────────┐   │
│ FFmpeg Mic      │────▶│ M4A File     │───┘
│ (avfoundation)  │     │  (AAC)       │
└─────────────────┘     └──────────────┘

┌─────────────────┐     ┌──────────────┐
│ FFmpeg Camera   │────▶│ JPEG Stream  │──▶ UI Preview Only
│ (Preview Only)  │     │  (Not saved) │   (Captured via screen)
└─────────────────┘     └──────────────┘
```

**Issues**:

- Mic mute applied during muxing (not live)
- Camera preview uses FFmpeg (not native APIs)
- Camera captured via screen but mic audio not synced to screen timeline → **lip sync broken**
- No explicit timeline synchronization (mic audio drifts)
- Multiple independent processes with different timelines

### Ideal Architecture (Single-Pass)

```
Master Timeline (t0 = first screen video frame)
│
├─ ScreenCaptureKit (Screen Video + System Audio)
│  ├─ Screen Video ────────────────────────────┐
│  └─ System Audio ──┐                         │
│                    │                         │
│                    ▼                         │
│            ┌──────────────┐                  │
│            │ Audio Mixer  │                  │
│            │ (Live Gains) │                  │
│            └──────────────┘                  │
│                    │                         │
│                    │                         │
└─ AVAudioEngine (Mic Audio)                   │
   └─ Timestamp aligned to t0 ──┐              │
                                 │              │
                                 ▼              │
                         ┌──────────────┐      │
                         │ Audio Mixer  │      │
                         │ (Live Gains) │      │
                         └──────────────┘      │
                                 │              │
                                 │              │
                                 │              │
                                 │              ▼
                                 │      ┌──────────────┐
                                 └──────▶ AVAssetWriter │──▶ Final MP4
                                        │ (Single-pass) │
                                        └──────────────┘

Camera Preview (Separate from recording):
┌─────────────────┐
│ AVCaptureSession│───▶ Overlay Window ──▶ ScreenCaptureKit captures it
│ (Webcam Preview)│     (Visible/Hidden)    (Already synced to screen timeline)
└─────────────────┘
```

**Key Points**:

- **Screen video** = master timeline (`t0`)
- **System audio** = already synced (same ScreenCaptureKit stream)
- **Mic audio** = **MUST be aligned to t0** (critical for lip sync!)
- **Webcam overlay** = captured via screen → automatically synced
- **Camera preview** = AVCaptureSession for UI overlay (native, not FFmpeg)
- Single timeline with timestamp normalization
- Live gain control for both audio streams
- No video compositing needed (webcam captured via screen)

---

## Critical Architecture Decision: Webcam Capture Strategy

### Why Capture Webcam Via Screen Recording?

**Decision**: Webcam overlay is captured via ScreenCaptureKit (screen recording), not composited separately.

**Benefits**:

1. **Privacy**: Webcam LED only on when overlay window is visible
2. **Simplicity**: No separate video compositing pipeline needed
3. **User Control**: "Camera off" in settings = LED off = no capture
4. **Automatic Sync**: Overlay captured as part of screen → already synced to screen timeline

**Trade-off**:

- Overlay must be visible on screen to be captured (acceptable - user controls visibility)
- No separate webcam track (not needed - overlay is part of screen)

### The Lip Sync Challenge

**Problem**: Webcam overlay (captured via screen) is synced to screen video timeline, but microphone audio may drift relative to that timeline → broken lip sync.

**Solution**: Align microphone audio timestamps to the screen video timeline (`t0`). Since:

- Webcam overlay = captured via screen → synced to screen timeline
- Mic audio = aligned to screen timeline → synced to webcam
- Result: Perfect lip sync!

**Implementation**: Audio mixer must use screen video timestamps as reference when aligning mic audio buffers (see Phase 2.1).

---

## Implementation Plan

### Phase 1: Foundation - Native macOS Module

**Goal**: Create a Swift/Objective-C module that handles all media capture and composition using native macOS APIs.

#### Step 1.1: Create Native Module Structure

**Files to create**:

- `src-tauri/src/services/platform/native_recorder/` (new directory)
- `src-tauri/src/services/platform/native_recorder/mod.rs` - Rust FFI bindings
- `src-tauri/src/services/platform/native_recorder/Recorder.swift` - Main Swift implementation
- `src-tauri/src/services/platform/native_recorder/Bridging-Header.h` - Objective-C bridging

**Tasks**:

1. Set up Rust FFI bindings using `cbindgen` or manual bindings
2. Create Swift module with C-compatible interface
3. Implement basic structure: `start_recording()`, `stop_recording()`, `set_mic_gain()`, `set_system_gain()`, `set_overlay_enabled()`

**Dependencies**:

- Add Swift/Objective-C compilation to `build.rs`
- Link against AVFoundation, ScreenCaptureKit, CoreMedia frameworks

#### Step 1.2: Implement ScreenCaptureKit Capture

**In `Recorder.swift`**:

- Use `SCStream` to capture screen video and system audio
- Store `CMSampleBuffer` objects with their timestamps
- Implement callback handlers for video and audio frames
- Establish `t0` (first video frame timestamp) as master timeline reference

**Key Implementation**:

```swift
class ScreenCaptureHandler: NSObject, SCStreamOutput {
    var t0: CMTime?
    var videoFrames: [(buffer: CVPixelBuffer, timestamp: CMTime)] = []
    var audioBuffers: [(buffer: CMSampleBuffer, timestamp: CMTime)] = []

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        let timestamp = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        if t0 == nil {
            t0 = timestamp // Master timeline reference
        }
        // Store buffer with normalized timestamp
    }
}
```

#### Step 1.3: Implement Microphone Capture

**In `Recorder.swift`**:

- Use `AVAudioEngine` with input node tap
- Capture PCM audio buffers
- Convert to `CMSampleBuffer` with timestamps aligned to master timeline
- Apply live gain control (0.0 when muted, 1.0 when unmuted)

**Key Implementation**:

```swift
class MicCapture {
    var audioEngine: AVAudioEngine
    var inputNode: AVAudioInputNode
    var micGain: Float = 1.0 // Live gain control

    func setupCapture() {
        audioEngine = AVAudioEngine()
        inputNode = audioEngine.inputNode
        let format = inputNode.inputFormat(forBus: 0)

        inputNode.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, time in
            guard let self = self else { return }
            // Apply gain: buffer samples *= self.micGain
            // Convert to CMSampleBuffer with normalized timestamp
        }
    }
}
```

#### Step 1.4: Implement Camera Preview (Separate from Recording)

**Important**: Camera preview is for UI overlay only. The webcam overlay window is captured via ScreenCaptureKit (screen recording), so no separate capture/compositing needed for recording.

**In `camera.rs` (Rust) or separate Swift module**:

- Replace FFmpeg camera preview with `AVCaptureSession`
- Use `AVCaptureVideoDataOutput` to get camera frames
- Convert frames to format suitable for React UI (e.g., JPEG or base64)
- Send frames to overlay window via Tauri events
- **Camera preview runs independently** of recording (can be started/stopped based on overlay visibility)

**Key Implementation** (can be in Rust with FFI or separate Swift module):

```swift
class CameraPreview {
    var captureSession: AVCaptureSession
    var videoOutput: AVCaptureVideoDataOutput

    func setupPreview() {
        captureSession = AVCaptureSession()
        // Configure session for preview quality (640x480, 30fps)
        videoOutput = AVCaptureVideoDataOutput()
        videoOutput.setSampleBufferDelegate(self, queue: DispatchQueue(label: "camera.preview"))
        // Convert frames to JPEG and send to Rust/React
    }
}
```

**Note**: This is separate from the recording pipeline. The overlay window itself is what gets captured by ScreenCaptureKit.

---

### Phase 2: Real-Time Processing Pipeline

**Note**: No video compositor needed! Webcam overlay is captured via screen recording, so ScreenCaptureKit already captures it as part of the screen video stream.

#### Step 2.1: Implement Audio Mixer (Critical for Lip Sync)

**Goal**: Mix system audio + mic audio in real-time with live gain control, ensuring mic audio is aligned to master timeline for perfect lip sync

**Critical for Lip Sync**: Mic audio timestamps must be aligned to the screen video timeline (`t0`). Since webcam overlay is captured via screen (already synced to screen timeline), aligning mic to screen timeline ensures lip sync.

**In `Recorder.swift`**:

- Create `AudioMixer` class
- Resample both inputs to common format (48 kHz, stereo, Float32)
- **CRITICAL**: Align mic audio buffers to screen video timeline
- Apply gains: `systemGain` and `micGain` (0.0 when muted)
- Mix: `output = (system * systemGain) + (mic * micGain)`
- Output continuous stream with stable timestamps normalized to `t0`

**Key Implementation**:

```swift
class AudioMixer {
    var systemGain: Float = 1.0
    var micGain: Float = 1.0
    var outputFormat: AVAudioFormat
    var t0: CMTime? // Master timeline reference (from screen video)
    var micTimestampOffset: CMTime? // Offset to align mic to t0

    func initializeTimeline(masterT0: CMTime, firstMicTimestamp: CMTime) {
        t0 = masterT0
        // Calculate offset to align mic to master timeline
        micTimestampOffset = masterT0 - firstMicTimestamp
    }

    func mix(systemBuffer: CMSampleBuffer,
             micBuffer: CMSampleBuffer?,
             screenVideoTimestamp: CMTime) -> CMSampleBuffer {
        // System audio: already synced (same ScreenCaptureKit stream)
        let systemTimestamp = CMSampleBufferGetPresentationTimeStamp(systemBuffer)
        let normalizedSystemTime = systemTimestamp - t0!

        // Mic audio: align to screen video timeline
        var normalizedMicTime: CMTime?
        if let micBuffer = micBuffer {
            let micTimestamp = CMSampleBufferGetPresentationTimeStamp(micBuffer)
            // Align mic timestamp to screen video timeline
            normalizedMicTime = screenVideoTimestamp - t0! // Use screen timestamp as reference
        }

        // Resample to common format if needed (48 kHz, stereo, Float32)
        // Apply gains (systemGain, micGain)
        // Mix samples: output = (system * systemGain) + (mic * micGain)
        // Fill gaps with silence (don't skip time)
        // Return CMSampleBuffer with normalized timestamp aligned to screen timeline
    }
}
```

**Synchronization Strategy** (from IDEAL_IMPLEMENTATION.md Section 6):

- **Master clock**: Screen video timeline (`t0` = first screen video frame timestamp)
- **System audio**: Already synced (comes from same ScreenCaptureKit stream as video)
- **Mic audio**: **MUST be aligned to screen video timeline** (this is the critical part!)
  - Use screen video timestamps as reference
  - Resample mic audio to match system audio format (48 kHz)
  - Align buffers by timestamp: find mic buffer that matches current screen video timestamp
  - Fill missing ranges with silence (don't skip time)
  - Optionally apply slight time-stretching if drift detected over long recordings
- **Webcam overlay**: Captured via screen → automatically synced to screen timeline
- Normalize all timestamps: `normalizedPTS = PTS - t0`

---

### Phase 3: AVAssetWriter Integration

#### Step 3.1: Set Up AVAssetWriter

**In `Recorder.swift`**:

- Create `AVAssetWriter` with MP4 output
- Add video input: H.264 encoding, 30 FPS
- Add audio input: AAC encoding, 48 kHz stereo
- Start session at `t0` (master timeline reference)

**Key Implementation**:

```swift
class RecordingWriter {
    var assetWriter: AVAssetWriter
    var videoInput: AVAssetWriterInput
    var audioInput: AVAssetWriterInput
    var t0: CMTime?

    func setupWriter(outputURL: URL) {
        assetWriter = AVAssetWriter(url: outputURL, fileType: .mp4)

        // Video input
        let videoSettings: [String: Any] = [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: screenWidth,
            AVVideoHeightKey: screenHeight,
            AVVideoCompressionPropertiesKey: [
                AVVideoAverageBitRateKey: 5000000
            ]
        ]
        videoInput = AVAssetWriterInput(mediaType: .video, outputSettings: videoSettings)
        videoInput.expectsMediaDataInRealTime = true

        // Audio input
        let audioSettings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: 48000,
            AVNumberOfChannelsKey: 2,
            AVEncoderBitRateKey: 128000
        ]
        audioInput = AVAssetWriterInput(mediaType: .audio, outputSettings: audioSettings)
        audioInput.expectsMediaDataInRealTime = true

        assetWriter.add(videoInput)
        assetWriter.add(audioInput)
    }

    func startSession(at time: CMTime) {
        assetWriter.startWriting()
        assetWriter.startSession(atSourceTime: time)
        t0 = time
    }
}
```

#### Step 3.2: Write Frames in Real-Time

**In `Recorder.swift`**:

- Process frames as they arrive:
  1. Video: Screen frames (webcam overlay already included via screen capture) → append to `videoInput`
  2. Audio: Mix system + mic (with mic aligned to screen timeline) → append to `audioInput`
- Use `append(_:withPresentationTimeStamp:)` with normalized timestamps
- Handle backpressure (writer may not be ready immediately)

**Key Implementation**:

```swift
func processVideoFrame(screenBuffer: CVPixelBuffer, timestamp: CMTime) {
    guard videoInput.isReadyForMoreMediaData else { return }

    // Screen buffer already includes webcam overlay (if visible)
    // No compositing needed - ScreenCaptureKit captures the overlay window
    let normalizedTime = timestamp - t0!
    let sampleBuffer = createSampleBuffer(from: screenBuffer, timestamp: normalizedTime)
    videoInput.append(sampleBuffer)
}

func processAudioFrame(systemBuffer: CMSampleBuffer,
                       micBuffer: CMSampleBuffer?,
                       currentScreenTimestamp: CMTime) {
    guard audioInput.isReadyForMoreMediaData else { return }

    // CRITICAL: Pass screen timestamp to mixer for mic alignment
    let mixedBuffer = mixer.mix(
        systemBuffer: systemBuffer,
        micBuffer: micBuffer,
        screenVideoTimestamp: currentScreenTimestamp // Used to align mic to screen timeline
    )

    // Mixed buffer timestamp is already normalized to t0
    audioInput.append(mixedBuffer)
}
```

**Note**: The webcam overlay window is captured as part of the screen video stream by ScreenCaptureKit. When the overlay is visible, it appears in the screen frames automatically. When hidden, it's simply not in the frames. No compositing needed!

---

### Phase 4: Rust Integration & API

#### Step 4.1: Create Rust FFI Bindings

**In `native_recorder/mod.rs`**:

- Define C-compatible structs and functions
- Use `#[repr(C)]` for structs passed to Swift
- Use `extern "C"` for functions called from Swift

**Key Implementation**:

```rust
#[repr(C)]
pub struct RecordingConfig {
    pub output_path: *const c_char,
    pub include_microphone: bool,
    pub include_camera: bool,
}

#[no_mangle]
pub extern "C" fn start_recording(config: RecordingConfig) -> i32 {
    // Call Swift function via FFI
}

#[no_mangle]
pub extern "C" fn set_mic_gain(gain: f32) {
    // 0.0 = muted, 1.0 = unmuted
}

#[no_mangle]
pub extern "C" fn set_system_audio_gain(gain: f32) {
    // 0.0 = muted, 1.0 = unmuted
}

#[no_mangle]
pub extern "C" fn set_camera_overlay_enabled(enabled: bool) {
    // true = show overlay window, false = hide overlay window
    // Note: Overlay visibility doesn't affect recording - ScreenCaptureKit captures whatever is on screen
}
```

#### Step 4.2: Update Rust Recording Service

**In `screencapturekit_recorder.rs`**:

- Replace FFmpeg-based implementation with calls to native module
- Keep same public API (`start_recording()`, `stop_recording()`, etc.)
- Update mute functions to call native module

**Migration Strategy**:

1. Keep old implementation as fallback
2. Add feature flag: `use_native_recorder`
3. Gradually migrate functionality
4. Remove old code once new implementation is stable

#### Step 4.3: Update Tauri Commands

**In `commands/mod.rs`**:

- `set_mic_muted()` → calls native `set_mic_gain(0.0 or 1.0)`
- `set_system_audio_muted()` → calls native `set_system_audio_gain(0.0 or 1.0)`
- `set_camera_overlay_visible()` → calls native `set_camera_overlay_enabled()`
- **Important**: Camera overlay visibility no longer stops capture, just toggles compositing

---

### Phase 5: Frontend Updates

#### Step 5.1: Replace FFmpeg Camera Preview with Native AVCaptureSession

**In `camera.rs`**:

- **Replace FFmpeg camera preview** with native `AVCaptureSession` (via FFI or separate Swift module)
- Use `AVCaptureVideoDataOutput` to get camera frames
- Convert frames to JPEG/base64 for React UI
- Camera preview runs independently of recording
- **Privacy**: When overlay is hidden, camera preview can be stopped (LED off)
- **Recording**: ScreenCaptureKit captures whatever is visible on screen

**Key Changes**:

- Camera preview uses native APIs (not FFmpeg) for better performance
- Camera overlay visibility controls preview start/stop (privacy: LED off when hidden)
- Recording captures overlay window via screen recording (already synced)
- No dependency between camera preview and recording pipeline

#### Step 5.2: Update React Components

**In `ControlBar.tsx`**:

- No major changes needed
- Mute buttons already call correct commands
- Camera toggle behavior: Still shows/hides overlay, but recording continues

**In `CameraOverlayWindow.tsx`**:

- No changes needed (still receives preview frames)

---

### Phase 6: Testing & Validation

#### Step 6.1: Sync Testing

**Test Cases**:

1. Record 10 seconds with all streams enabled
2. **CRITICAL**: Verify lip sync - speak into mic while webcam overlay is visible, check that audio matches lip movement
3. Verify audio/video sync in final file
4. Verify webcam overlay appears at correct position in recording
5. Test mute toggles during recording (mic and system audio independently)
6. Test camera overlay hide/show during recording (overlay appears/disappears in recording)
7. Record long session (30+ minutes) to check for drift (especially mic audio drift)
8. Test camera preview start/stop (LED should turn on/off accordingly)

#### Step 6.2: Performance Testing

**Metrics**:

- CPU usage during recording
- Memory usage
- Frame drop rate
- Encoding quality

**Optimizations**:

- Use Metal for video compositing (GPU acceleration)
- Use hardware H.264 encoding (VideoToolbox)
- Buffer management to handle backpressure

---

## Migration Strategy

### Option A: Big Bang (Replace All at Once)

**Pros**: Clean implementation, no legacy code
**Cons**: High risk, harder to debug

### Option B: Gradual Migration (Recommended)

**Phase 1**: Implement native module alongside existing code
**Phase 2**: Add feature flag to switch between implementations
**Phase 3**: Test native implementation thoroughly
**Phase 4**: Switch default to native implementation
**Phase 5**: Remove old FFmpeg-based code

**Recommended Approach**: Option B

---

## Key Implementation Details

### Timeline Normalization

```swift
// At recording start
let t0 = firstVideoFrameTimestamp

// For all subsequent frames
func normalizeTimestamp(_ timestamp: CMTime) -> CMTime {
    return timestamp - t0
}

// All frames written to AVAssetWriter use normalized timestamps
```

### Live Gain Control

```swift
// System audio
func processSystemAudio(buffer: CMSampleBuffer) {
    let samples = extractSamples(buffer)
    let gain = systemAudioMuted ? 0.0 : 1.0
    let mutedSamples = samples.map { $0 * gain }
    // Continue processing...
}

// Mic audio
func processMicAudio(buffer: CMSampleBuffer) {
    let samples = extractSamples(buffer)
    let gain = micMuted ? 0.0 : 1.0
    let mutedSamples = samples.map { $0 * gain }
    // Continue processing...
}
```

### Mic Audio Timeline Alignment (Critical for Lip Sync)

```swift
// When mic audio buffer arrives
func processMicAudio(micBuffer: AVAudioPCMBuffer, micTimestamp: AVAudioTime) {
    // Get current screen video timestamp (master timeline)
    let currentScreenTimestamp = getCurrentScreenVideoTimestamp()

    // Align mic timestamp to screen timeline
    // Mic audio should match the screen video frame that's being processed
    let alignedMicTimestamp = currentScreenTimestamp - t0!

    // Convert mic buffer to CMSampleBuffer with aligned timestamp
    let micSampleBuffer = createSampleBuffer(
        from: micBuffer,
        timestamp: alignedMicTimestamp
    )

    // Now mic audio is aligned to screen video timeline
    // Since webcam overlay is captured via screen (already synced),
    // mic + webcam will be in sync → lip sync works!
}
```

**Key Insight**:

- Webcam overlay = captured via screen → synced to screen video timeline
- Mic audio = aligned to screen video timeline → synced to webcam
- Result: Perfect lip sync!

---

## Rollback Plan

If native implementation has issues:

1. Keep old FFmpeg implementation as fallback
2. Use feature flag: `use_native_recorder = false`
3. Old implementation continues to work
4. Fix issues in native implementation
5. Re-enable when stable

---

## Success Criteria

✅ **Live Mute Controls**:

- Mic mute: Gain = 0 applied immediately during capture
- System audio mute: Gain = 0 applied immediately during capture
- No post-processing required

✅ **Webcam Synchronization (Lip Sync)**:

- Webcam overlay captured via screen → automatically synced to screen video timeline
- Mic audio aligned to screen video timeline → synced to webcam overlay
- Perfect lip sync: webcam visual matches microphone audio timing
- Camera preview uses native AVCaptureSession (not FFmpeg)

✅ **Perfect Sync**:

- All streams use same master timeline (`t0`)
- No drift over 30+ minute recordings
- Frame-accurate synchronization

✅ **Live Camera Toggle**:

- Camera preview can be started/stopped based on overlay visibility (privacy: LED control)
- Overlay window visibility controls what appears in screen recording
- Toggle reflected immediately in output (overlay appears/disappears in recording)

---

## Estimated Effort

- **Phase 1** (Native Module): 2-3 days
- **Phase 2** (Audio Mixer with Timeline Alignment): 3-4 days (critical for lip sync)
- **Phase 3** (AVAssetWriter): 2-3 days
- **Phase 4** (Rust Integration): 1-2 days
- **Phase 5** (Camera Preview Migration): 1-2 days (FFmpeg → AVCaptureSession)
- **Phase 6** (Testing): 2-3 days (especially lip sync validation)

**Total**: ~2-3 weeks of focused development

**Critical Focus**: Mic audio timeline alignment (Phase 2) is the most important part for achieving lip sync.

---

## Next Steps

1. Review this plan and confirm approach
2. Set up Swift/Objective-C compilation in build system
3. Create native module structure
4. Implement Phase 1 (Foundation)
5. Iterate through remaining phases
6. Test thoroughly before removing old implementation
