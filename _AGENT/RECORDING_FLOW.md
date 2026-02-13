# Recording Flow (Current Code)

This document reflects the current implementation in `src-tauri/src/services/platform/screencapturekit_recorder`.

## Start

1. Frontend runs a 3-second countdown.
2. `start_recording` command starts a ScreenCaptureKit recording session.
3. Session creates temporary files:
   - `sck_video_<id>.mp4` (screen video, H.264)
   - `sck_sysaudio_<id>.raw` (system audio, s16le PCM)
   - `sck_mic_<id>.raw` (mic audio, s16le PCM, if mic enabled)
4. Capture pipelines start in parallel:
   - Screen video: ScreenCaptureKit -> raw BGRA -> FFmpeg stdin -> temp MP4
   - System audio: ScreenCaptureKit -> Float32 -> s16le -> raw file
   - Mic audio: FFmpeg `avfoundation` -> s16le -> raw file
5. Camera preview runs independently through `services/camera.rs`:
   - FFmpeg camera capture -> MJPEG frames -> Tauri event -> camera overlay window.
   - During recording, camera frames are emitted using screen PTS ticks so the on-screen preview remains visually aligned to recorded screen frames.

## During Recording

- Pause/resume:
  - Screen and system audio callbacks stop writing while paused.
  - Mic thread keeps reading but drops data while paused.
- Mute controls:
  - System mute is applied live in system-audio callback (samples zeroed).
  - Mic mute is applied live in mic write thread (samples zeroed before write).
- Sync telemetry captured continuously:
  - First screen frame arrival time
  - First system audio arrival time
  - First mic audio arrival time
  - Audio sample counters

## Stop + Mux

1. Stop ScreenCaptureKit stream.
2. Close writers and wait for FFmpeg processes.
3. Build final MP4 by muxing temp files with FFmpeg:
   - Uses measured first-arrival offsets to align system/mic tracks with video start.
   - Uses mic sample-count vs video duration to estimate clock drift.
   - Applies mic `atempo` correction when drift is detected.
   - Resamples/normalizes final audio timeline (`aresample=async`, `atrim` to video duration).
4. Writes final output to Downloads.
5. Cleans temporary files.

## Important Constraints

- Webcam is not an independently muxed video track. It is visually present in the final file because the camera overlay window is captured as part of screen recording.
- The current architecture is still two-pass (capture -> mux), but with explicit offset and drift correction in the mux stage.
