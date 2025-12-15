# ScreenCaptureKit Recording Implementation Audit Report

## Executive Summary

This audit examines the current ScreenCaptureKit-based recording implementation that attempts to capture screen video and system audio natively on macOS 12.3+. The implementation uses a callback-based approach to pipe video and audio data from ScreenCaptureKit to FFmpeg for encoding. The system is currently experiencing failures where recordings start but then crash/stop unexpectedly.

**Key Findings:**

1. **Architecture**: Uses ScreenCaptureKit callbacks → Rust handlers → FFmpeg via stdin (video) and named pipe (audio)
2. **Critical Issue**: FFmpeg process exits unexpectedly, likely due to blocking/deadlock in pipe handling
3. **Root Causes**: Multiple synchronization issues between ScreenCaptureKit callbacks, pipe I/O, and FFmpeg input processing
4. **Audio Format**: Float32 PCM conversion to s16le appears correct but may have timing issues

## Current Implementation Architecture

### Overview

The implementation (`screencapturekit_recorder.rs`) attempts to:

1. Capture screen video + system audio using ScreenCaptureKit (macOS 12.3+)
2. Pipe video frames to FFmpeg via stdin (raw BGRA format)
3. Pipe audio samples to FFmpeg via a named pipe (FIFO) in s16le format
4. Mix system audio with microphone audio (if enabled) using FFmpeg's `amix` filter
5. Encode everything to MP4

### Component Flow

```
ScreenCaptureKit Stream
    ↓
SCStreamOutputTrait callbacks (video + audio)
    ↓
StreamHandler::did_output_sample_buffer()
    ↓
Video: Write BGRA pixels → FFmpeg stdin
Audio: Convert Float32→s16le → Named pipe (FIFO)
    ↓
FFmpeg Process
    ├─ Input 0: stdin (rawvideo, BGRA, 30fps)
    ├─ Input 1: Named pipe (s16le, 48kHz, stereo) - System audio
    ├─ Input 2: avfoundation (microphone) - if enabled
    └─ Output: MP4 with mixed audio
```

### Key Code Components

#### 1. Stream Configuration

```rust
let config = SCStreamConfiguration::new()
    .with_width(width)
    .with_height(height)
    .with_pixel_format(PixelFormat::BGRA)
    .with_shows_cursor(true)
    .with_captures_audio(true)  // Enable system audio
    .with_sample_rate(48000)
    .with_channel_count(2); // Stereo
```

#### 2. FFmpeg Command Structure

```bash
ffmpeg \
  -f rawvideo -pix_fmt bgra -s WxH -r 30 -i pipe:0 \  # Video from stdin
  -f s16le -ar 48000 -ac 2 -i /path/to/audio.pipe \   # System audio from FIFO
  -f avfoundation -i ":MIC_INDEX" \                    # Microphone (if enabled)
  -filter_complex "[1:a][2:a]amix=inputs=2[aout]" \   # Mix audio
  -map 0:v -map "[aout] \                              # Map outputs
  -c:v libx264 -preset ultrafast -crf 23 \
  -c:a aac -b:a 128k \
  output.mp4
```

#### 3. Audio Format Conversion

- **Input**: ScreenCaptureKit provides Float32 PCM (interleaved stereo)
- **Conversion**: Float32 [-1.0, 1.0] → s16le [-32768, 32767]
- **Output**: s16le format written to named pipe

#### 4. Pipe Synchronization

- Named pipe (FIFO) created with `mkfifo`
- Separate thread opens pipe for writing (blocks until FFmpeg opens for reading)
- Channel (`mpsc`) signals when pipe is ready
- Early audio samples buffered until pipe opens

## Identified Issues

### Issue 1: FFmpeg Process Exit / Deadlock

**Symptom**: Recording starts successfully but FFmpeg process exits unexpectedly after a few seconds, causing recording to stop.

**Root Causes**:

1. **FFmpeg Input Processing Order**:

   - FFmpeg processes inputs sequentially
   - It opens input 0 (stdin/video) first and starts reading
   - Then it tries to open input 1 (audio pipe)
   - If audio pipe isn't ready when FFmpeg tries to open it, FFmpeg may fail or block
   - Current code starts ScreenCaptureKit stream immediately, which sends video frames
   - This causes FFmpeg to start processing video before audio pipe is ready

2. **Named Pipe Blocking Behavior**:

   - Named pipes (FIFOs) block on `open()` until both reader and writer are present
   - Current implementation spawns thread to open pipe for writing
   - This thread blocks until FFmpeg opens pipe for reading
   - FFmpeg won't open audio pipe until it finishes processing video input setup
   - **Race condition**: Video frames arrive → FFmpeg processes video → FFmpeg tries to open audio pipe → Audio pipe thread is waiting → Deadlock potential

3. **FFmpeg Thread Queue Size**:

   - FFmpeg has default thread queue size (typically 8 packets)
   - If video frames arrive faster than FFmpeg can process, queue fills up
   - FFmpeg may block waiting for queue space
   - No `-thread_queue_size` option set in current command
   - This can cause blocking when one stream is faster than another

4. **Missing Error Handling**:
   - FFmpeg stderr is monitored but errors may not be caught in time
   - Process monitoring checks every 1 second, may miss rapid failures
   - No validation that FFmpeg successfully opened all inputs

### Issue 2: Audio Format and Timing

**Potential Issues**:

1. **Audio Format Assumptions**:

   - Code assumes Float32 interleaved stereo
   - ScreenCaptureKit may provide non-interleaved (planar) format
   - No verification of actual audio format from `CMSampleBuffer`
   - Conversion may be incorrect if format differs

2. **Audio Sample Timing**:

   - Audio samples arrive asynchronously via callbacks
   - No timestamp synchronization with video frames
   - FFmpeg relies on sample rate for timing (48kHz)
   - If samples arrive irregularly, audio may desync

3. **Buffer Flushing**:
   - Audio samples flushed after each callback
   - May cause excessive I/O operations
   - Named pipe may not handle rapid small writes efficiently

### Issue 3: Process Management

**Issues**:

1. **Process State Tracking**:

   - FFmpeg process moved to monitoring thread via `Arc<Mutex>`
   - State extraction happens after stream starts
   - If process crashes before state is stored, cleanup may fail

2. **Graceful Shutdown**:
   - `stop_recording()` sends SIGINT then `kill()`
   - May not properly flush FFmpeg buffers
   - Named pipe cleanup happens after process kill
   - May leave orphaned pipes

## Why It Fails

### Primary Failure Mode

The most likely failure sequence:

1. **Startup**:

   - Named pipe created successfully
   - FFmpeg process spawned
   - Video stdin obtained
   - Audio pipe thread spawned (blocks waiting for FFmpeg)

2. **Stream Start**:

   - ScreenCaptureKit stream starts immediately
   - Video frames start flowing to FFmpeg stdin
   - FFmpeg begins processing video input

3. **FFmpeg Input Processing**:

   - FFmpeg reads video frames from stdin
   - FFmpeg's internal queue fills with video packets
   - FFmpeg tries to open audio pipe (input 1)
   - **Problem**: Audio pipe thread is blocked on `File::create()` waiting for FFmpeg to open it
   - **Deadlock**: FFmpeg waiting for pipe to be writable, pipe thread waiting for FFmpeg to open it

4. **Alternative Failure**:

   - FFmpeg opens audio pipe successfully
   - But video frames arrive faster than FFmpeg can process
   - FFmpeg's thread queue fills up
   - FFmpeg blocks waiting for queue space
   - Audio pipe has no data yet (early samples buffered)
   - FFmpeg may timeout or error on audio input

5. **Process Exit**:
   - FFmpeg encounters error (pipe issue, queue full, format mismatch)
   - FFmpeg exits with error code
   - Monitoring thread detects exit
   - Recording stops unexpectedly

### Evidence from Logs

From terminal output:

```
[ScreenCaptureKit] FFmpeg encoder started (PID: 26359)
[ScreenCaptureKit] Waiting for FFmpeg to open audio pipe...
[ScreenCaptureKit] Waiting for audio pipe to be ready...
[FFmpeg INFO] Input #0, rawvideo, from 'pipe:0':  # FFmpeg starts reading video
[ScreenCaptureKit] Warning: Audio pipe not ready after 5 seconds  # Pipe still blocked
[ScreenCaptureKit] Audio pipe opened for writing  # Finally opens, but too late?
```

This sequence suggests:

- FFmpeg starts processing video before audio pipe is ready
- Audio pipe thread blocks for 5+ seconds
- By the time pipe opens, FFmpeg may have already encountered issues

## Recommended Fixes

### Fix 1: Proper Input Synchronization

**Problem**: FFmpeg processes inputs sequentially, but we start sending video before audio pipe is ready.

**Solution**: Ensure audio pipe is ready BEFORE starting ScreenCaptureKit stream.

**Implementation**:

1. Start FFmpeg process
2. Spawn audio pipe thread (will block until FFmpeg opens it)
3. Send a small amount of dummy video data to stdin to trigger FFmpeg to open audio pipe
4. Wait for audio pipe to be ready (with timeout)
5. Only then start ScreenCaptureKit stream

**Alternative**: Use FFmpeg's `-thread_queue_size` option to increase buffer sizes and prevent blocking.

### Fix 2: Increase FFmpeg Thread Queue Sizes

**Problem**: Default queue size (8) may be too small, causing blocking.

**Solution**: Add `-thread_queue_size` options to FFmpeg command.

**Implementation**:

```rust
ffmpeg_cmd.args(&[
    "-thread_queue_size", "512",  // For video input
    "-f", "rawvideo", ...
    "-i", "pipe:0",
    "-thread_queue_size", "512",  // For audio pipe input
    "-f", "s16le", ...
    "-i", audio_pipe_str,
    // ... rest of command
]);
```

### Fix 3: Verify Audio Format

**Problem**: Assumes Float32 interleaved, may be incorrect.

**Solution**: Check actual audio format from `CMSampleBuffer` before conversion.

**Implementation**:

- Use `CMSampleBuffer::format_description()` to get `AudioStreamBasicDescription`
- Verify format flags (interleaved vs. planar)
- Handle both formats correctly
- Log format for debugging

### Fix 4: Better Error Detection

**Problem**: FFmpeg errors may not be caught in time.

**Solution**: Improve error monitoring and validation.

**Implementation**:

1. Check FFmpeg process immediately after spawn (verify it's running)
2. Monitor stderr more aggressively (read in real-time, not buffered)
3. Validate that FFmpeg successfully opened all inputs (check stderr for "Input #" messages)
4. Add timeout for initial FFmpeg setup phase
5. Log all FFmpeg output for debugging

### Fix 5: Alternative Architecture: Use Separate Processes

**Problem**: Complex synchronization between single FFmpeg process and multiple inputs.

**Solution**: Use separate FFmpeg processes or different architecture.

**Options**:

1. **Separate audio encoding**: Encode audio separately, then mux with FFmpeg
2. **Use FFmpeg's `-f concat`**: Record segments, concatenate later
3. **Use ScreenCaptureKit's native recording** (macOS 15+): If available, use `SCRecordingOutput` API
4. **Use `avfoundation` for audio**: Capture system audio via BlackHole (fallback)

### Fix 6: Fix Named Pipe Opening

**Problem**: `File::create()` on FIFO blocks until reader opens it, creating deadlock.

**Solution**: Use non-blocking approach or different synchronization.

**Implementation**:

1. Open pipe with `O_NONBLOCK` flag (requires `nix` crate or `libc`)
2. Use `select()` or `poll()` to wait for pipe to be writable
3. Or: Use a different IPC mechanism (Unix domain socket, shared memory)
4. Or: Pre-open pipe before FFmpeg starts (may not work with FIFOs)

### Fix 7: Audio Sample Batching

**Problem**: Flushing after each sample may be inefficient.

**Solution**: Batch audio samples before writing.

**Implementation**:

- Collect multiple audio samples in buffer
- Write in larger chunks (e.g., 1024 samples = ~21ms at 48kHz)
- Reduces I/O overhead
- May help with pipe performance

## Documentation References

### ScreenCaptureKit Rust Crate (v1.4)

- **Source**: `screencapturekit = "1.4"` crate
- **Documentation**: https://doom-fish.github.io/screencapturekit-rs/
- **Key API**: `SCStreamOutputTrait::did_output_sample_buffer()`
- **Audio Format**: CMSampleBuffer → AudioBufferList → Float32 PCM (typically interleaved)

### FFmpeg Named Pipe Behavior

- **Key Finding**: FFmpeg processes inputs sequentially
- **Blocking**: Named pipes block on open until both reader and writer present
- **Thread Queue**: Default size 8, can be increased with `-thread_queue_size`
- **Multiple Inputs**: FFmpeg opens inputs in order, may block if one input isn't ready

### macOS Audio Format

- **ScreenCaptureKit**: Provides Float32 PCM (interleaved or planar)
- **Format Flags**: `kAudioFormatFlagsNativeFloatPacked` for interleaved
- **Non-Interleaved**: Uses `kAudioFormatFlagIsNonInterleaved` flag
- **Conversion**: Float32 [-1.0, 1.0] → s16le [-32768, 32767] is standard

## Implementation Priority

### High Priority (Critical for Functionality)

1. **Fix Input Synchronization** (Fix 1): Ensure audio pipe ready before stream start
2. **Increase Thread Queue Sizes** (Fix 2): Prevent FFmpeg blocking
3. **Better Error Detection** (Fix 4): Catch failures early

### Medium Priority (Improves Reliability)

4. **Verify Audio Format** (Fix 3): Ensure correct conversion
5. **Fix Named Pipe Opening** (Fix 6): Eliminate deadlock potential

### Low Priority (Optimization)

6. **Audio Sample Batching** (Fix 7): Improve performance
7. **Alternative Architecture** (Fix 5): Consider if above fixes don't work

## Testing Strategy

### Test Cases

1. **Basic Recording**: Start recording, verify FFmpeg doesn't exit
2. **Audio Pipe Timing**: Verify pipe opens before stream starts
3. **FFmpeg Input Validation**: Check stderr for successful input opening
4. **Error Scenarios**: Test with missing permissions, invalid paths, etc.
5. **Long Recording**: Record for extended period, verify stability
6. **Audio Format**: Verify audio format detection and conversion
7. **Microphone Mixing**: Test with microphone enabled/disabled

### Debugging Tools

1. **FFmpeg Verbose Logging**: Use `-loglevel verbose` or `debug`
2. **Pipe Monitoring**: Use `lsof` to check pipe status
3. **Process Monitoring**: Monitor FFmpeg process state continuously
4. **Audio Format Logging**: Log actual audio format from ScreenCaptureKit
5. **Timing Analysis**: Log timestamps for each stage of startup

## Conclusion

The current implementation has a solid architectural foundation but suffers from synchronization issues between ScreenCaptureKit callbacks, named pipe I/O, and FFmpeg's input processing. The primary failure mode is a deadlock/blocking situation where FFmpeg tries to open the audio pipe before it's ready, or where video frames arrive faster than FFmpeg can process them.

**Key Recommendations**:

1. Fix the input synchronization order (audio pipe ready before stream start)
2. Increase FFmpeg thread queue sizes to prevent blocking
3. Improve error detection and logging
4. Verify audio format handling is correct

With these fixes, the implementation should work reliably. The architecture is sound; it just needs proper synchronization and error handling.
