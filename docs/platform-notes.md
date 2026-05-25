# Platform Notes

Momentum is currently macOS-specific. The Tauri app can be packaged for multiple targets in config, but the recording implementation relies on macOS APIs and AVFoundation device names.

## Native Dependencies

Runtime dependencies:

- macOS ScreenCaptureKit
- macOS AVFoundation
- FFmpeg
- Swift command-line runtime / Xcode Command Line Tools for the AVFoundation resolver script

Rust dependencies of note:

- `tauri` v2 with `macos-private-api`
- `screencapturekit` pinned to `=1.3.0`
- `objc`, `core-foundation`, and `core-media`

Frontend dependencies of note:

- React 19
- Zustand
- Tailwind CSS
- Lucide React

## Permissions

The bundle declares usage descriptions in `src-tauri/resources/Info.plist`:

- camera
- microphone
- screen capture

The macOS entitlements in `src-tauri/entitlements/momentum.entitlements` include:

- `com.apple.security.device.camera`
- `com.apple.security.device.microphone`
- `com.apple.security.personal-information.screen-recording`

At runtime, the app requires permission to capture the screen, camera, and microphone. Without these permissions, ScreenCaptureKit or FFmpeg AVFoundation capture can fail.

## FFmpeg Resolution

`FfmpegLocator` tries paths in this order:

1. `FFMPEG_PATH` environment variable
2. `/opt/homebrew/bin/ffmpeg`
3. `/usr/local/bin/ffmpeg`
4. `/usr/bin/ffmpeg`
5. `ffmpeg` from PATH

The implementation verifies candidates by running `ffmpeg -version`.

FFmpeg is used for:

- encoding raw BGRA screen frames into temporary H.264 MP4
- capturing microphone raw PCM from AVFoundation
- capturing camera preview MJPEG frames from AVFoundation
- final muxing and audio filtering

## Swift AVFoundation Resolver

`src-tauri/resources/resolve_avf.swift` is run by `device_resolver::resolve_avf_indices()`.

It returns JSON containing:

- built-in microphone index
- built-in camera index
- main screen video index
- BlackHole/system-audio index if found
- camera count and active display index

Current usage:

- Built-in microphone index is used when starting mic FFmpeg capture.
- Built-in camera index is used when starting camera preview.
- System-audio/BlackHole index is logged but not used by the active ScreenCaptureKit system-audio path.
- Main screen FFmpeg index is not used by the active ScreenCaptureKit screen-video path.

This distinction matters because some older notes and the project `README.md` mention system audio devices differently. The current code captures system audio through ScreenCaptureKit with `config.set_captures_audio(true)`.

## Build and Test Commands

Common commands from `package.json`:

```bash
npm run dev
npm run build
npm run test
npm run tauri dev
npm run tauri build
```

There is also a `build-app` script that resets macOS TCC permissions, removes the Tauri target directory, and builds the app.

## Bundled Resources

Tauri config points at:

- entitlements: `src-tauri/entitlements/momentum.entitlements`
- Info.plist: `src-tauri/resources/Info.plist`
- icons under `src-tauri/icons/`

The Swift resolver is expected to be available from one of several locations:

- app bundle Resources directory
- `CARGO_MANIFEST_DIR/resources/`
- current working directory under `src-tauri/resources/`
- compile-time manifest path

## Current Platform Limitations

- Device selection is not exposed to users.
- Built-in camera and built-in mic are preferred.
- Screen target selection is not implemented.
- Camera preview capture uses a separate FFmpeg process and base64 JPEG events, which is simple but expensive compared with a native shared texture or direct compositing path.
- The app requires FFmpeg at runtime; it is not currently documented as bundled.
