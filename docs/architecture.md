# Architecture

Momentum is a multi-window desktop app with a React UI layer and a Rust-native media backend.

## High-Level Components

```mermaid
flowchart TB
  subgraph FE["Frontend (React)"]
    app["App.tsx\nwindow router"]
    overlay["OverlayWindow + ControlBar"]
    settings["SettingsWindow"]
    camWin["CameraOverlayWindow"]
    recStore["recordingStore"]
    setStore["settingsStore"]
    tsApi["tauri/commands + tauri/events"]
  end

  subgraph BE["Backend (Tauri Rust)"]
    setup["lib.rs setup"]
    cmd["commands/mod.rs"]
    rec["Recorder"]
    cam["CameraPreview"]
    imm["ImmersiveMode"]
    cfg["SettingsStore"]
    drive["google_drive service"]
  end

  subgraph Media["Media Layer"]
    sck["ScreenCaptureKit stream"]
    ffv["FFmpeg video encode"]
    ffm["FFmpeg mic capture"]
    ffc["FFmpeg camera preview"]
    mux["Final mux"]
  end

  app --> overlay
  app --> settings
  app --> camWin

  overlay --> recStore
  settings --> setStore
  overlay --> tsApi
  settings --> tsApi
  camWin --> tsApi

  tsApi --> cmd
  setup --> cmd
  cmd --> rec
  cmd --> cam
  cmd --> imm
  cmd --> cfg
  cmd --> drive

  rec --> sck
  sck --> ffv
  rec --> ffm
  cam --> ffc
  rec --> mux
```

## Window Model

Defined in `src-tauri/tauri.conf.json`:

- `overlay` (`88x330`): main recording controls, non-focusable, always-on-top.
- `camera-overlay` (`200x200`): live webcam preview surface, non-focusable.
- `settings` (`700x760`): focusable floating settings UI.

`src/App.tsx` inspects window label (`getCurrentWindow().label`) and renders the matching root component.

## Why The Windows Behave Differently

- Overlay/camera are non-focusable so users can interact quickly without stealing app focus in normal use.
- Settings is focusable so text/select inputs are editable.
- Settings is draggable via `data-tauri-drag-region` on shell-only regions; interactive controls explicitly disable drag region.

## Backend State Ownership

Managed in `lib.rs` via `app.manage(...)`:

- `Recorder`: recording lifecycle + elapsed timer task + start/stop/pause/resume facade.
- `Mutex<CameraPreview>`: camera preview FFmpeg process and frame emission.
- `Arc<Mutex<ImmersiveMode>>`: runtime immersive flag.
- `SettingsStore`: persisted JSON settings source of truth.

This keeps React as orchestration/UI and Rust as execution/state authority for media operations.

## Core Flows

### 1) Recording Flow

```mermaid
sequenceDiagram
  participant UI as OverlayWindow
  participant CMD as commands::start_recording
  participant REC as Recorder
  participant SCK as ScreenCaptureKitRecorder

  UI->>CMD: invoke start_recording(options)
  CMD-->>UI: invoke resolves quickly
  CMD->>REC: start_with_output_path(...)
  REC->>SCK: start(...)
  SCK-->>REC: started info
  CMD-->>UI: emit recording-started
```

### 2) Stop + Finalize + Upload Flow

```mermaid
sequenceDiagram
  participant UI as OverlayWindow
  participant CMD as commands::stop_recording
  participant REC as Recorder
  participant FS as finalize_recording_file
  participant DR as google_drive::upload_video

  UI->>CMD: invoke stop_recording()
  CMD-->>UI: invoke resolves quickly
  CMD->>REC: stop()
  CMD-->>UI: emit recording-stopped
  CMD->>FS: finalize local file
  FS-->>UI: emit recording-saved
  alt Google Drive enabled
    FS-->>UI: emit drive-upload-pending
    FS->>DR: upload + make public + readiness poll
    DR-->>UI: emit drive-upload-complete or drive-upload-error
  end
```

### 3) Settings Flow

- `SettingsWindow` loads three resources in parallel on mount:
  - persisted settings (`get_settings`)
  - canonical defaults (`get_default_settings`)
  - capture devices (`list_capture_devices`)
- UI keeps `draft`, `committed`, and `defaults` state.
- `Save settings` persists `draft`.
- `Reset to defaults` applies backend defaults and device resolution fallback.

## Immersive Mode Behavior

```mermaid
flowchart LR
  toggle["Menu or hotkey toggle"] --> setImm["set immersive flag"]
  setImm --> overlay["overlay window hide/show"]
  setImm --> camRule["recompute camera visibility"]
  camRule --> camWindow["camera-overlay hide/show"]
  setImm --> event["emit immersive-mode-changed"]
```

Immersive mode is runtime-only. The persisted setting is not "immersive on/off"; persisted setting is the shortcut and webcam-hide preference while immersive is active.
