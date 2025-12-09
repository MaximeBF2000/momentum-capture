# Recording System Audit Report

## Executive Summary

This audit evaluates the current recording implementation in the Momentum screen recorder application. The main findings indicate that the recording system uses separate processes for screen and audio capture, then merges them post-recording. This needs to be refactored to use a single muxed recording process.

## Current Architecture

### Recording Flow

1. **Start Recording**: Spawns two separate FFmpeg processes:
   - Screen recorder: `ffmpeg -f avfoundation -i "1:none"` (screen only)
   - Audio recorder: `ffmpeg -f avfoundation -i ":0"` (microphone only)
2. **Pause/Resume**: Kills and restarts both processes (creates gaps)
3. **Stop Recording**: Stops both processes, then merges files using FFmpeg with synchronization offsets
4. **Mic Toggle**: Pauses current recording and resumes with new mic state (creates gaps)

### Camera System

- Camera preview uses separate FFmpeg process for preview only
- Camera stream is **NOT** included in final recording (correct behavior)
- Camera preview can be toggled on/off via settings

### Window Management

- Overlay window: Top-right, frameless, has drag region
- Camera overlay window: Bottom-right, frameless, has drag region
- Both windows configured as draggable but may need improvements

## Issues Identified

### Critical Issues

1. **Separate Recording Processes (Not Muxed)**

   - **Location**: `src-tauri/src/services/platform/macos.rs`
   - **Problem**: Screen and audio are recorded separately, then merged at the end
   - **Impact**:
     - Potential sync issues despite offset calculations
     - More complex error handling
     - Two file I/O operations instead of one
     - Merge step adds processing time
   - **Required Change**: Use single FFmpeg process with both screen and audio inputs muxed together

2. **Microphone Mute Implementation**

   - **Location**: `src-tauri/src/services/platform/macos.rs` (AudioRecorder)
   - **Problem**: When mic is muted, recording pauses and resumes with silence generator
   - **Impact**: Creates gaps in recording, breaks synchronization
   - **Required Change**: Always capture audio stream, but set gain to 0 when muted (use `-af "volume=0"`)

3. **Camera Toggle Behavior**
   - **Location**: `src-tauri/src/services/camera.rs`
   - **Problem**: Camera preview starts/stops but doesn't properly handle toggling during recording
   - **Impact**: Camera state may not reflect user's intent
   - **Required Change**: Actually start/stop camera stream when toggled

### Medium Priority Issues

4. **Pause/Resume Creates Gaps**

   - **Location**: `src-tauri/src/services/platform/macos.rs`
   - **Problem**: Pause kills processes, resume creates new ones (overwrites files)
   - **Impact**: Lost recording segments during pause
   - **Note**: This is acceptable for MVP but should be noted

5. **Window Draggability**
   - **Location**: `src/windows/OverlayWindow.tsx`, `src/windows/CameraOverlayWindow.tsx`
   - **Problem**: Drag regions exist but entire windows may not be draggable
   - **Impact**: Poor UX if users can't reposition windows
   - **Required Change**: Ensure entire window surface is draggable (except interactive elements)

### Low Priority / Code Quality

6. **Error Handling**

   - Some error messages could be more user-friendly
   - FFmpeg process failures could be handled more gracefully

7. **Code Organization**
   - Recording logic is split across multiple files (good separation)
   - Could benefit from better abstraction for platform-specific code

## Architecture Recommendations

### New Recording Architecture

**Single Muxed FFmpeg Process:**

```bash
ffmpeg \
  -f avfoundation -framerate 30 -i "1:none" \  # Screen
  -f avfoundation -i ":0" \                      # Microphone
  -c:v libx264 -preset ultrafast -crf 23 \
  -c:a aac -b:a 128k \
  -af "volume=0" \                               # When mic muted
  output.mp4
```

**Benefits:**

- Single process = better synchronization
- No merge step needed
- Simpler error handling
- Faster final output (no post-processing)

**Mic Mute Implementation:**

- Always include microphone input
- Use `-af "volume=0"` filter when muted
- Can toggle filter dynamically using FFmpeg filter_complex or by restarting with different args

**Camera Toggle:**

- Start camera preview stream when enabled
- Stop camera preview stream when disabled
- Camera stream never included in recording (correct)

## Implementation Plan

1. Refactor `SynchronizedRecorder` to use single muxed FFmpeg process
2. Update `AudioRecorder` to always capture mic but apply volume filter when muted
3. Improve camera toggle to actually start/stop camera stream
4. Enhance window draggability for both overlay windows
5. Remove merge logic from `stop_recording` command (no longer needed)
6. Update pause/resume to handle muxed recording

## Testing Checklist

- [ ] Recording starts with single muxed process
- [ ] Mic mute sets gain to 0 without gaps
- [ ] Camera toggle starts/stops camera preview
- [ ] Windows are fully draggable
- [ ] Pause/resume works correctly
- [ ] Stop recording produces single file (no merge step)
- [ ] File is saved to Downloads directory
- [ ] Error handling works for all failure cases
