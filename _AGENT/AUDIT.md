 # Momentum Implementation Audit

Date: 2026-01-25

This audit is based on reading the current Rust (Tauri) and React code in the repository. Content inside `_AGENT/` was not used as input. No code changes were made.

## High-Level Architecture

```
Tauri App (Rust)
  - main.rs -> momentum_lib::run()
  - lib.rs -> setup, window positions, menu, hotkeys, command registration
  - commands/* -> Tauri commands + event emission
  - services/* -> recording, camera preview, settings, hotkeys
  - platform/* -> macOS implementations (ScreenCaptureKit + FFmpeg)

React App (TypeScript)
  - App.tsx -> window router by label
  - windows/* -> Overlay, Camera Overlay, Settings
  - state/* -> Zustand stores
  - tauri/* -> invoke wrappers + event subscriptions
```

## Frontend (React + Zustand)

### Window Routing

- `src/App.tsx` reads the current Tauri window label and returns one of:
  - `OverlayWindow` (main control bar)
  - `CameraOverlayWindow` (webcam overlay)
  - `SettingsWindow`

### State

```
recordingStore
  recordingState: idle | countdown | recording | paused | stopping
  elapsedTimeMs
  countdownSecondsRemaining
  isMicEnabled / isCameraEnabled
  isMicMuted / isSystemAudioMuted
  errorMessage

settingsStore
  micEnabled / cameraEnabled
  immersiveShortcut
  saveLocation (optional)
```

### Control Bar (Overlay)

`src/components/recording/ControlBar.tsx`

- Start button runs a local 3-second countdown timer then calls `start_recording`.
- Pause/resume toggles call `pause_recording` / `resume_recording`.
- Stop calls `stop_recording` and moves to `stopping` state.
- Mic and system audio mute toggles send `set_mic_muted` / `set_system_audio_muted` during recording.
- Camera toggle updates settings and calls `set_camera_overlay_visible`.

### Event Handling

```
Backend -> app.emit("recording-started" | "recording-paused" | ...)
Frontend -> subscribeToRecordingEvents() -> recordingStore updates
```

- `OverlayWindow` listens for recording events and drives `recordingState`.
- Timer is computed locally in the UI every second while `recordingState === recording`.
- `CameraFrame` listens for `camera-frame` events and updates a base64-encoded `<img>`.

### Settings UI

- `SettingsWindow` fetches settings on mount.
- `ImmersiveShortcutForm` captures a keyboard combo, persists via `update_immersive_shortcut`, and listens for updates via the event bus.

## Backend (Rust + Tauri)

### Tauri Startup

`src-tauri/src/lib.rs`

- Manages shared state:
  - `Recorder` (recording control)
  - `CameraPreview` (FFmpeg-based webcam preview)
  - `ImmersiveMode`
- Positions overlay windows at top-right and bottom-right of the primary monitor.
- Loads settings from `~/.config/momentum/settings.json`.
- Initializes camera overlay visibility and preview based on settings.
- Builds app menu with:
  - Toggle Immersive Mode (accelerator from settings)
  - Settings window
- Registers a global hotkey using Carbon APIs.

### Commands

`src-tauri/src/commands/mod.rs`

- `start_recording` / `pause_recording` / `resume_recording` / `stop_recording`.
- `get_settings`, `update_settings`, `update_immersive_shortcut`.
- `set_camera_overlay_visible`, `toggle_immersive_mode`, `set_immersive_mode`.
- `set_mic_muted`, `set_system_audio_muted` directly update ScreenCaptureKit state.

### Recording Core

`src-tauri/src/services/recording.rs`

- `Recorder` holds recording state in an `Arc<Mutex<RecordingState>>`.
- On macOS it delegates to `services::platform::screencapturekit_recorder`.
- Start creates a temp file in the OS temp directory and starts a platform recorder.
- Stop returns `(screen_file, audio_file)` (currently same file in muxed flows).

### macOS Recording Implementation

There are two overlapping implementations:

1) ScreenCaptureKit two-pass (primary path)
2) FFmpeg AVFoundation fallback (legacy / fallback)

#### ScreenCaptureKit Flow

`services/platform/screencapturekit_recorder/*`

```
start_recording
  - create temp video file (mp4)
  - create temp system audio file (raw s16le)
  - optional mic capture using FFmpeg to raw s16le
  - ScreenCaptureKit emits:
      - video frames -> FFmpeg stdin (raw BGRA -> mp4)
      - system audio -> raw file (Float32 -> s16le)
  - state stored in a global Mutex<Option<RecordingState>>

stop_recording
  - stop stream
  - close writers
  - wait for FFmpeg process to finish
  - stop mic FFmpeg
  - mux video + system audio + mic audio into final mp4
```

#### FFmpeg AVFoundation Fallback

`services/platform/macos/muxed_recorder.rs`

- Resolves AVFoundation device indices via a Swift script (`resolve_avf.swift`).
- Runs a single FFmpeg process capturing screen and mic (no system audio if SCK is unavailable).
- Pause/resume stops and restarts FFmpeg (gaps in output).

### Camera Preview

`services/camera.rs`

- Starts FFmpeg `avfoundation` capture to MJPEG pipe.
- Each JPEG frame is base64-encoded and emitted as a `camera-frame` event.
- A camera sync buffer exists to align camera frames with screen frame PTS.

### Settings Persistence

- `services/settings.rs` reads/writes `~/.config/momentum/settings.json`.
- Default settings are defined in Rust and mirrored in TypeScript.

## Core Flows (Textual Graphs)

### Recording Start/Stop

```
[ControlBar]
  -> start_recording (invoke)
      -> commands::start_recording
          -> Recorder::start
              -> screencapturekit_recorder::start_recording
          -> emit "recording-started"

[ControlBar]
  -> stop_recording (invoke)
      -> commands::stop_recording
          -> Recorder::stop
              -> screencapturekit_recorder::stop_recording
          -> emit "recording-stopped"
          -> async copy temp file to Downloads
          -> emit "recording-saved"
```

### Camera Preview

```
FFmpeg (avfoundation) -> MJPEG frames
  -> base64 encode -> camera-frame event
      -> CameraFrame.tsx -> <img src="data:image/jpeg;base64,...">
```

### Immersive Mode

```
Hotkey (Carbon) OR Menu
  -> commands::toggle_immersive_mode
     - hide/show overlay
     - hide/show camera overlay
     - emit "immersive-mode-changed"
```

## Issues / Risks / Technical Debt

This list focuses on correctness, stability, maintainability, and security/privacy.

1) Heavy synchronous work on command handlers
- `start_recording` and `stop_recording` do long-running IO and process coordination on the Tauri command thread.
- Result: UI freezes and risk of deadlocks (several debug comments already hint at this).
- Fix: move recording start/stop orchestration into background tasks and report progress via events.

2) Multiple overlapping recording stacks
- Two different implementations exist (SCK two-pass and FFmpeg muxed). Both are active, with fallbacks and special cases.
- Result: hard to reason about behavior, greater surface for bugs, inconsistent pause/mute semantics.
- Fix: pick a single canonical pipeline (likely ScreenCaptureKit) and remove the fallback or isolate it behind a trait with clean boundaries.

3) Privacy and security logging
- Many logs include file paths and detailed system state (paths, device indices, temp file names).
- This conflicts with the app's privacy expectations.
- Fix: remove or gate logs behind a debug flag, and never log recording paths in production.

4) External command execution
- Commands like `swift`, `ffmpeg`, and `kill` are invoked directly.
- `get_downloads_dir` shells out to `sh -c "echo ~/Downloads"`.
- Result: sandbox/security risks and fragile behavior in packaged apps.
- Fix: use native APIs (tauri::path, macOS APIs, or Rust crates) and bundle/ship ffmpeg if needed.

5) Pause/resume is destructive
- Pause/resume stops FFmpeg and restarts, effectively overwriting or creating gaps in output.
- This is not true pause/resume and may lose data.
- Fix: implement pause/resume in the recorder layer or in post-processing (segment concat).

6) Settings vs. runtime state split
- Settings are stored in `settingsStore`, but the recording store keeps its own `isMicEnabled/isCameraEnabled`.
- These values are manually synced in multiple places.
- Fix: use a single source of truth or derived state; emit explicit events when settings are changed by the backend.

7) Timer is UI-derived, not backend-derived
- UI timer increments every second when `recordingState === recording`.
- Pauses are not explicitly accounted for by the backend; timing can drift or desync.
- Fix: send elapsed time from backend or compute using timestamps when `recording-started/paused/resumed` events fire.

8) Global mutable state without clear ownership
- `screencapturekit_recorder::STATE` is a global `Mutex<Option<RecordingState>>`.
- Camera sync uses a `OnceLock` singleton.
- Result: hard to test, error-prone under concurrency, difficult to reason about lifecycle.
- Fix: move to explicit state structs owned by the Tauri app state, and avoid globals where possible.

9) Screen capture device resolution is fragile
- `resolve_avf.swift` relies on heuristic offsets for screen indices and assumes FFmpeg device ordering.
- System audio depends on BlackHole, but README claims no virtual devices are required.
- Fix: use ScreenCaptureKit for screen and system audio on macOS 12.3+, and remove AVFoundation heuristics for screen capture where possible.

10) Camera preview relies on FFmpeg and base64 events
- Camera preview is a continuous base64 stream emitted over Tauri events.
- High CPU usage and memory pressure for MJPEG + base64; the buffer can grow.
- Fix: consider a native camera preview window or use a shared memory/texture bridge if needed; at minimum, throttle and drop frames more aggressively.

11) Download path and output naming
- Output path is always `~/Downloads/momentum-recording-<timestamp>.mp4`.
- Settings include `saveLocation` but are not used.
- Fix: respect `saveLocation` and use the OS APIs for directories.

12) CSP disabled
- `tauri.conf.json` sets `csp: null`.
- This increases exposure to XSS and injection risks.
- Fix: define a minimal CSP for the app windows.

13) Error handling in UI is shallow
- Most errors are logged; the user only sees a generic `errorMessage` in the store.
- No surfaced error UI beyond a store value.
- Fix: provide explicit error UI states and expose next steps (permissions, missing FFmpeg).

14) Mixed platform intent
- Code paths include Windows/Linux stubs, but the product is macOS-only.
- Some logic attempts cross-platform handling but is incomplete and untested.
- Fix: either commit to macOS-only paths or create platform-specific modules behind traits and feature flags.

15) `CameraPreview` lifecycle is loosely managed
- `CameraPreview::start` spawns a thread; there is no explicit process kill, only a flag check.
- If FFmpeg hangs, it may leak a process.
- Fix: retain process handles and terminate on stop with a timeout.

## Suggested Next Steps

1) Choose the canonical recording pipeline (ScreenCaptureKit recommended) and remove the older FFmpeg flow.
2) Move recording start/stop to background tasks and emit progress events for the UI.
3) Replace shell/command usage with native APIs for directories and device resolution.
4) Consolidate settings and runtime state in a single store and add a structured event model.
5) Implement a clear error UI and reduce logging in production builds.

---

If you want, I can turn this audit into a prioritized remediation plan or open concrete tickets for each item.
