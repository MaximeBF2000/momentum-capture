## MOMUENTUM SCREEN RECORDER – IMPLEMENTATION ROADMAP (NATIVE-ONLY CAPTURE)

Goal: Build “Momuentum screen recorder” as a Tauri desktop app with:

- Backend: Rust (Tauri), using native OS APIs for:

  - Screen recording
  - Microphone capture
  - Camera capture
  - Camera preview window
  - Saving file to Downloads

- Frontend: React + TypeScript + Tailwind CSS + Lucide Icons
- UI:

  - Overlay control bar window (top-right, frameless, draggable, always-on-top)
  - Camera overlay window (bottom-right, frameless, draggable, always-on-top)

- Zero use of `getUserMedia`, MediaRecorder, or any browser media API.

This roadmap is written for an LLM or developer to implement step by step.

---

0. PROJECT INITIALIZATION

---

0.1. Create base project

- Initialize a Tauri app with:

  - React + TypeScript frontend
  - Rust backend

- Confirm that:

  - Frontend builds with Vite or CRA equivalent.
  - `tauri.conf.json` (or `tauri.conf.json` in src-tauri) exists.

    0.2. Install frontend dependencies

- Install Tailwind CSS and configure:

  - Tailwind config
  - PostCSS config
  - Global CSS imports

- Install Lucide React for icons: `lucide-react`.

  0.3. Structure the repo

- `src/` – frontend
- `src-tauri/` – Rust backend

  - `src/main.rs`
  - Other Rust modules as needed.

---

1. TAURI WINDOW ARCHITECTURE

---

You will create two Tauri windows:

1. Overlay window (label: "overlay")
2. Camera overlay window (label: "camera-overlay")

1.1. Configure windows in Tauri config

In Tauri config, define:

Overlay window:

- label: "overlay"
- decorations: false (frameless)
- alwaysOnTop: true
- transparent: true (if platform allows)
- resizable: false
- shadow: enabled if supported
- initial size: compact horizontal bar (e.g. 480x72)
- position: top-right with margin (platform dependent, can be done from Rust)

Camera overlay window:

- label: "camera-overlay"
- decorations: false
- alwaysOnTop: true
- transparent: true
- resizable: false
- initial size: small rectangle for camera preview (e.g. 280x180)
- position: bottom-right with margin
- visible: false by default (visibility controlled by settings)

  1.2. Draggable frameless windows

In the frontend HTML/React:

- For both windows, mark a container area as draggable using Tauri drag region:

  - Set `data-tauri-drag-region="true"` on the bar (overlay) and on a top frame area of the camera window.

- Ensure the control buttons do NOT have the drag region attribute so clicks are handled normally.

  1.3. Window show/hide control

Implement Rust helpers and Tauri commands:

- `set_camera_overlay_visible(visible: bool)`:

  - Lookup the "camera-overlay" window.
  - Call `show()` or `hide()` accordingly.

- Optionally:

  - `center_overlay_window()` or `position_overlay_top_right()` to re-position.

---

2. FRONTEND ARCHITECTURE

---

2.1. File structure (frontend)

Suggested structure:

src
main.tsx / index.tsx
App.tsx
windows/
OverlayWindow.tsx
CameraOverlayWindow.tsx
components/
recording/
ControlBar.tsx
Countdown.tsx
TimerDisplay.tsx
RecordingButtons.tsx
camera/
CameraFrame.tsx
state/
recordingStore.ts
settingsStore.ts
tauri/
commands.ts (typed wrappers around Tauri commands)
events.ts (event names and subscription helpers)
styles/
globals.css

2.2. Window-specific entry points

- Overlay window:

  - Render `<OverlayWindow />` which contains the control bar and timer.

- Camera overlay window:

  - Render `<CameraOverlayWindow />` which only shows the camera preview frame and possibly a subtle camera status indicator.

Each window can share the same bundle or have separate entrypoints as configured in Tauri. Either is acceptable.

---

3. FRONTEND STATE MANAGEMENT

---

Use a small state management library (Zustand recommended) or React context.

3.1. Recording state model

Define a store with:

- `recordingState: "idle" | "countdown" | "recording" | "paused" | "stopping"`
- `elapsedTimeMs: number`
- `countdownSecondsRemaining: number | null`
- `isMicEnabled: boolean`
- `isCameraEnabled: boolean`
- `errorMessage: string | null`

Actions:

- `startCountdown()`
- `tickCountdown()`
- `setRecordingState(state)`
- `setElapsedTime(ms)`
- `toggleMic()`
- `toggleCamera()`
- `setError(message | null)`

  3.2. Timer logic

- In the overlay window, when `recordingState === "recording"`, use `setInterval` (1000 ms or 500 ms) to increment `elapsedTimeMs` in state.
- Pause interval when `recordingState` is "paused", "idle", or "stopping".
- Clear interval on component unmount.

  3.3. Connection to backend (events and commands)

- In a shared module, define:

  - `startRecording(options): Promise<void>`
  - `pauseRecording(): Promise<void>`
  - `resumeRecording(): Promise<void>`
  - `stopRecording(): Promise<void>`
  - `getSettings(): Promise<Settings>`
  - `updateSettings(settings): Promise<void>`
  - `setCameraOverlayVisible(visible: boolean): Promise<void>`

- Subscribe to Tauri events:

  - `"recording-started"`
  - `"recording-paused"`
  - `"recording-resumed"`
  - `"recording-stopped"`
  - `"recording-saved"`
  - `"recording-error"`
  - `"camera-frame"` (for camera preview – see backend section)

Use these events to update the store accordingly.

---

4. OVERLAY WINDOW UI

---

4.1. Visual requirements (ControlBar)

The control bar is a dark pill-shaped horizontal overlay with:

- Background: dark (e.g. Tailwind `bg-neutral-900/90`)
- Border radius: full (pill shape)
- Padding: symmetric (e.g. `px-4 py-2`)
- Shadow: subtle (e.g. `shadow-lg`)
- Border: subtle line (e.g. `border border-neutral-700`)
- Layout: `flex`, `items-center`, `gap-x-4`, `justify-between`

Left section: recording indicator

- Small red dot.
- "RECORDING" uppercase text.
- When idle: dot dimmed and label muted.
- When recording: dot bright red, label maybe more vibrant.

Middle section: elapsed time

- Monospace font: `font-mono`
- Format: `hh:mm:ss` (e.g. `00:12:34`)
- Color: white.

Divider:

- Thin vertical line (e.g. `w-px h-6 bg-neutral-700`).

Right section: control buttons

- Circular buttons of equal size (e.g. `w-8 h-8 rounded-full flex items-center justify-center`).
- Buttons:

  - Pause/Resume:

    - When recording: white circle with black pause icon.
    - When paused: white circle with black play/resume icon.

  - Stop:

    - Red circle with white square icon.
    - Disabled when idle.

  - Mic toggle:

    - Dark circle baseline.
    - When active: green-lit style (e.g. border and glow, icon green).
    - When inactive: greyed.

  - Camera toggle:

    - Dark circle with grey camera icon.
    - When active: slight emphasis (border or glow).

    4.2. Countdown UI

When user initiates recording:

- `recordingState` becomes "countdown".
- Timer area shows a large countdown number (3, 2, 1) or small overlay inside the bar.
- Buttons:

  - Pause and Stop disabled during countdown (unless you implement a cancel).

- When countdown ends at 0:

  - Call backend `start_recording(options)`.
  - Wait for `"recording-started"` before switching to "recording".

---

5. CAMERA OVERLAY WINDOW UI

---

Important constraint: The camera preview must come from native OS APIs via Rust. No `getUserMedia`.

5.1. Camera window layout

`CameraOverlayWindow` should:

- Fill the window with a rounded rectangle:

  - `rounded-2xl`, `overflow-hidden`, `bg-black`, `shadow-lg`.

- The content area displays the camera preview:

  - Implement as a React component `CameraFrame`.

- On top of the frame or in a small corner, you can show a status (e.g. “CAMERA ON/OFF”).

  5.2. Displaying camera frames from native backend

Display pipeline (conceptual):

- Backend (Rust) captures camera frames via native APIs.
- Backend periodically emits frames to frontend via Tauri events:

  - Event name: `"camera-frame"`.
  - Payload: e.g. `{ id: number, width: number, height: number, format: "jpeg", data_base64: string }`.

In frontend:

- Camera overlay window subscribes to `"camera-frame"`.
- On each event:

  - Build a `data:` URL like `data:image/jpeg;base64,<payload.data_base64>`.
  - Store the URL in component state (e.g. `currentFrameUrl`).

- `CameraFrame` renders an `<img>` tag with `src={currentFrameUrl}` styled to cover the container.

Note: This is not the most efficient solution, but it respects the spec: camera capture is purely native. The UI only renders pre-encoded frames.

5.3. Camera enable/disable behavior

- When `isCameraEnabled` becomes `true`:

  - Frontend calls `setCameraOverlayVisible(true)`.
  - Backend starts the native camera capture pipeline.
  - Backend begins emitting `"camera-frame"` events.

- When `isCameraEnabled` becomes `false`:

  - Frontend calls `setCameraOverlayVisible(false)`.
  - Backend stops the native camera capture pipeline.
  - Frontend stops updating frames and shows a placeholder ("Camera off").

---

6. BACKEND ARCHITECTURE (RUST)

---

6.1. Module organization

In `src-tauri/src`:

- `main.rs`
- `recording/`

  - `mod.rs`
  - `screen.rs`
  - `audio.rs`
  - `camera.rs`
  - `pipeline.rs`
  - `platform/`

    - `mod.rs`
    - `macos.rs`
    - `windows.rs`
    - `linux.rs`

- `settings.rs`
- `downloads.rs`
- `events.rs`
- `error.rs`

  6.2. Recording abstraction

Define a trait:

trait Recorder {
fn start(&mut self, options: RecordingOptions) -> Result<(), RecordingError>;
fn pause(&mut self) -> Result<(), RecordingError>;
fn resume(&mut self) -> Result<(), RecordingError>;
fn stop(&mut self) -> Result<RecordingResult, RecordingError>;
}

Where:

- `RecordingOptions` includes:

  - screen capture target
  - include_microphone: bool
  - include_camera: bool
  - output format and path (initially a temp file)

- `RecordingResult` includes:

  - final temp file path
  - duration

Implement platform-specific `Recorder` in `macos.rs`, `windows.rs`, `linux.rs` using native OS APIs:

- macOS: ScreenCaptureKit / AVFoundation
- Windows: Media Foundation
- Linux: PipeWire / GStreamer

  6.3. Camera preview abstraction

Create a `CameraPreview` abstraction:

trait CameraPreview {
fn start(&mut self) -> Result<(), CameraError>;
fn stop(&mut self) -> Result<(), CameraError>;
}

Internal to `start()`:

- Initialize native camera capture.
- Spawn a thread that:

  - Captures frames from the camera.
  - Encodes them (e.g. JPEG).
  - Emits Tauri events `"camera-frame"` with base64-encoded image data at some FPS (e.g. 10-15).

    6.4. Global backend state

Use `tauri::State` to hold:

struct AppState {
recorder: Mutex<Option<Box<dyn Recorder + Send>>>,
camera_preview: Mutex<Option<Box<dyn CameraPreview + Send>>>,
recording_state: Mutex<RecordingState>,
settings: Mutex<AppSettings>,
}

Where `RecordingState` is similar to frontend state:

- Idle, Recording, Paused, Stopping

---

7. TAURI COMMANDS AND EVENTS

---

7.1. Commands

Implement Tauri commands:

- `start_recording(options: RecordingOptionsDto) -> Result<(), Error>`
- `pause_recording() -> Result<(), Error>`
- `resume_recording() -> Result<(), Error>`
- `stop_recording() -> Result<(), Error>`
- `get_settings() -> Result<AppSettings, Error>`
- `update_settings(settings: AppSettings) -> Result<(), Error>`
- `set_camera_overlay_visible(visible: bool) -> Result<(), Error>`

In `set_camera_overlay_visible`:

- Use `tauri::Window` API to show/hide the "camera-overlay" window.
- Start or stop the `CameraPreview` accordingly.

  7.2. Events

Emit events from backend:

- `"recording-started"`
- `"recording-paused"`
- `"recording-resumed"`
- `"recording-stopped"`
- `"recording-saved"` (payload: final file path)
- `"recording-error"` (payload: error message)
- `"camera-frame"` (payload: encoded frame, as described above)

Frontend listens to these and updates state.

---

8. SAVING TO DOWNLOADS DIRECTORY (NATIVE)

---

8.1. Resolve Downloads directory via native APIs

Implement a function:

fn downloads_dir() -> Result<PathBuf, Error>

Using platform-specific implementations:

- Windows: SHGetKnownFolderPath(FOLDERID_Downloads)
- macOS: NSSearchPathDirectory / NSFileManager or equivalent
- Linux: xdg-user-dir DOWNLOAD or similar

  8.2. Finalizing files

In `stop_recording()`:

1. Call `recorder.stop()` to get `RecordingResult` with temp file path.
2. Determine final path:

   - `<Downloads>/momentum-recording-YYYYMMDD-HHMMSS.mp4`

3. Move/rename the temp file to final path.
4. Emit `"recording-saved"` event with final path.

Optional: Trigger a Tauri notification to inform the user.

---

9. SETTINGS PERSISTENCE

---

9.1. Settings structure

Define:

struct AppSettings {
mic_enabled: bool,
camera_enabled: bool,
}

Persist using:

- Tauri plugin `tauri-plugin-store`, or
- a JSON file in app config directory.

  9.2. Lifecycle

On app startup:

- Load settings from disk.
- Initialize backend state.
- If `camera_enabled` is true:

  - Show camera overlay window.
  - Start camera preview in backend.

Frontend:

- On overlay window mount:

  - Call `get_settings()`.
  - Populate `settingsStore`.

- When user toggles mic/camera in idle state:

  - Call `update_settings` with new values.
  - For camera toggle: also call `set_camera_overlay_visible`.

---

10. END-TO-END LOGIC FLOW

---

1. App launch:

- Tauri creates overlay and camera-overlay windows.
- Backend loads settings.
- Backend shows overlay window.
- Backend shows or hides camera overlay window based on `camera_enabled` and starts/stops CameraPreview.

2. Idle state:

- Overlay shows:

  - Dim recording dot
  - Timer “00:00:00”
  - Mic and camera toggles based on settings.

- No recording in progress.

3. Start recording:

- User presses Start (you can treat Stop button as Start in idle, or have a dedicated Start button).
- Frontend sets state to "countdown" and starts a 3-second countdown.
- When countdown hits 0:

  - Frontend calls `start_recording(options)`, where options are derived from:

    - `isMicEnabled`
    - `isCameraEnabled`
    - screen target (full screen for v1).

- Backend:

  - Initializes `Recorder` with those options.
  - Starts screen + mic + camera recording via native APIs.
  - Emits `"recording-started"`.

- Frontend receives `"recording-started"` and:

  - sets state to "recording"
  - starts elapsed time timer.

4. Pause:

- When recording:

  - Pause button calls `pause_recording()`.

- Backend:

  - Pauses recording (or at least audio/video writing) via native APIs.
  - Emits `"recording-paused"`.

- Frontend:

  - Sets state to "paused".
  - Stops timer increments.

5. Resume:

- When paused:

  - Resume button calls `resume_recording()`.

- Backend:

  - Resumes native recording.
  - Emits `"recording-resumed"`.

- Frontend:

  - Sets state to "recording".
  - Resumes timer increments.

6. Stop:

- Stop button calls `stop_recording()`.
- Frontend sets state "stopping".
- Backend:

  - Stops native recording.
  - Finalizes file to temp path.
  - Moves file to Downloads with timestamped name.
  - Emits `"recording-stopped"` and `"recording-saved"` with final path.

- Frontend:

  - On `"recording-stopped"`: can show brief "Saving…" state.
  - On `"recording-saved"`:

    - Reset state to "idle"
    - Reset timer to 0
    - Optionally show a toast with saved file path.

7. Camera preview:

- Controlled solely via:

  - Backend CameraPreview (native APIs)
  - `"camera-frame"` events

- Frontend never calls `getUserMedia` or any browser media API.

8. Error handling:

- Any failure in start/pause/resume/stop or camera operations:

  - Backend emits `"recording-error"` with a user-friendly message.
  - Frontend:

    - Shows error message (toast or inline).
    - Resets to safe state (usually "idle").

---

11. IMPLEMENTATION ORDER (FOR AN LLM)

---

When implementing, follow this order:

1. Initialize Tauri + React + TypeScript + Tailwind + Lucide.
2. Define `overlay` and `camera-overlay` windows in Tauri config (frameless, draggable, always-on-top).
3. Build static overlay UI (ControlBar) with hardcoded states.
4. Build static camera overlay window UI (CameraOverlayWindow) with placeholder preview.
5. Implement frontend state stores and timer logic (still without backend).
6. Define Tauri command signatures and event names in Rust and in TS wrappers.
7. Implement dummy backend Recorder and CameraPreview that:

   - Writes a small dummy file on stop.
   - Emits fake `"camera-frame"` images at intervals.
   - Wire commands and events end-to-end.

8. Replace dummy Recorder with real OS-specific recorders using native screen/mic/camera APIs.
9. Replace dummy CameraPreview with real native camera frame capture.
10. Implement downloads_dir resolution and final file move to Downloads.
11. Implement settings persistence and camera overlay visibility control.
12. Add robust error handling and small UX polish (animations, hover states).
13. Test on each target OS:

    - Recording flow.
    - Mic toggles actually affect audio.
    - Camera preview and camera-in-recording behave as expected.
    - Files are created in Downloads and play correctly.

14. Set up packaging/build scripts for distribution.

---

End of roadmap (native-only capture, no getUserMedia).

If you want, I can now turn this into an actual `.md` or `.txt` file content ready to save, or adapt it into something more “prompt-shaped” for an LLM agent.
