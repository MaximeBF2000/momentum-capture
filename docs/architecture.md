# Architecture

Momentum is a single Tauri application with multiple webview windows and a Rust backend that owns all native recording work.

## Runtime Components

```mermaid
flowchart TB
  subgraph frontend["React frontend"]
    app["App.tsx routes by Tauri window label"]
    overlay["OverlayWindow + ControlBar"]
    cameraOverlay["CameraOverlayWindow + CameraFrame"]
    settings["SettingsWindow"]
    stores["Zustand stores"]
    tauriApi["src/tauri command/event wrappers"]
  end

  subgraph tauri["Tauri Rust backend"]
    bootstrap["lib.rs setup"]
    commands["commands/mod.rs"]
    recorder["Recorder"]
    camera["CameraPreview + CameraSyncHandle"]
    settingsStore["SettingsStore"]
    immersive["ImmersiveMode"]
    hotkey["Carbon global hotkey"]
  end

  subgraph platform["macOS media layer"]
    sck["ScreenCaptureKitRecorder"]
    sckStream["SCStream callbacks"]
    ffmpegVideo["FFmpeg video encoder"]
    ffmpegMic["FFmpeg mic capture"]
    ffmpegCam["FFmpeg camera preview"]
    mux["FFmpeg mux"]
    swift["Swift AVFoundation resolver"]
  end

  app --> overlay
  app --> cameraOverlay
  app --> settings
  overlay --> stores
  settings --> stores
  stores --> tauriApi
  tauriApi --> commands
  bootstrap --> commands
  bootstrap --> recorder
  bootstrap --> camera
  bootstrap --> settingsStore
  bootstrap --> immersive
  bootstrap --> hotkey
  commands --> recorder
  commands --> camera
  commands --> settingsStore
  commands --> immersive
  recorder --> sck
  camera --> ffmpegCam
  sck --> sckStream
  sck --> ffmpegVideo
  sck --> ffmpegMic
  sck --> mux
  sck --> swift
  ffmpegCam --> swift
```

## Windows

Tauri defines three windows in `src-tauri/tauri.conf.json`.

| Window label | React component | Purpose |
| --- | --- | --- |
| `overlay` | `OverlayWindow` | Always-on-top control bar for record, pause, stop, mute, and camera toggle. |
| `camera-overlay` | `CameraOverlayWindow` | Circular draggable webcam preview overlay. Hidden until camera is enabled. |
| `settings` | `SettingsWindow` | Settings UI, currently focused on immersive-mode shortcut configuration. |

`src/App.tsx` calls `getCurrentWindow()` and renders the correct window UI based on the label. This means the same React bundle serves all windows.

## Backend State Ownership

Rust state is registered in `lib.rs` during Tauri setup:

| Managed state | Type | Responsibility |
| --- | --- | --- |
| Recorder | `Recorder` | User-facing recording lifecycle, elapsed clock, and delegation to ScreenCaptureKit recorder. |
| Camera preview | `Mutex<CameraPreview>` | FFmpeg camera preview process and frame emission. |
| Immersive mode | `Arc<Mutex<ImmersiveMode>>` | Whether overlay windows should be hidden. |
| Settings | `SettingsStore` | JSON settings persisted under the OS config directory. |

The command layer in `src-tauri/src/commands/mod.rs` is an orchestration layer. It accepts Tauri invokes, calls services, updates windows, and emits events back to React.

## Recording Service Layers

```mermaid
flowchart TB
  control["ControlBar"]
  invoke["startRecording() invoke"]
  command["commands::start_recording"]
  recorder["Recorder::start"]
  sck["ScreenCaptureKitRecorder::start"]
  startMod["start.rs start_recording"]
  handlers["FrameHandler callbacks"]
  stopMod["stop.rs stop_recording"]
  muxMod["mux.rs mux_final_video"]

  control --> invoke --> command --> recorder --> sck --> startMod
  startMod --> handlers
  control --> stopInvoke["stopRecording() invoke"]
  stopInvoke --> stopCommand["commands::stop_recording"]
  stopCommand --> recorderStop["Recorder::stop"]
  recorderStop --> sckStop["ScreenCaptureKitRecorder::stop"]
  sckStop --> stopMod --> muxMod
```

`Recorder` is the public recording facade. It protects higher-level state such as "is recording", "is paused", elapsed time, output path, and whether mic/camera were included at start. It does not directly process media buffers.

`ScreenCaptureKitRecorder` owns platform recording state. It starts the ScreenCaptureKit stream, FFmpeg processes, raw writers, mute flags, pause flag, counters, temporary paths, and final mux.

## Settings and Immersive Mode

Settings are persisted as JSON by `SettingsStore`. Current settings:

- `micEnabled`: whether microphone capture should be included when recording starts.
- `cameraEnabled`: whether camera preview overlay should be shown and included visually in screen capture.
- `immersiveShortcut`: global shortcut string, default `Option+I`.
- `saveLocation`: optional output directory; otherwise recordings are copied to Downloads.

Immersive mode is runtime state, not persisted as an enabled setting. When enabled, it hides the control overlay and camera overlay. Recording continues normally.

The global shortcut uses Carbon APIs in `services/hotkey.rs`. Updating the shortcut changes both the app menu accelerator and the Carbon hotkey registration.

## Output Ownership

Recording first writes a final temporary MP4 path under the OS temp directory, then `commands::save_recording_file` copies it to the configured save location or Downloads using the file name `momentum-recording-<unix_seconds>.mp4`.

This means there are two levels of temporary output:

1. ScreenCaptureKit recorder temp files used during capture and muxing.
2. Recorder-level temporary final MP4 copied into the user-visible output directory.
