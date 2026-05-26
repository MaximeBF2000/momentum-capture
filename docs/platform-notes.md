# Platform Notes

Momentum's active recording implementation is macOS-oriented.

## macOS Dependencies

- ScreenCaptureKit (screen + system audio stream)
- AVFoundation (mic/camera device inputs through FFmpeg)
- FFmpeg binary accessible at runtime
- Swift runtime path support for related tooling

## Permission Model

User-facing permission descriptions are declared in:

- `src-tauri/resources/Info.plist`

Entitlements are in:

- `src-tauri/entitlements/momentum.entitlements`

Expected permissions:

- Screen Recording
- Microphone
- Camera

## Window Configuration Notes

From `src-tauri/tauri.conf.json`:

- Overlay and camera overlay are `focusable: false`, `focus: false`.
- Settings is `focusable: true`, `focus: true`.

Why this matters:

- non-focusable overlays reduce UX friction for quick controls
- settings requires keyboard focus for editable inputs

## FFmpeg Resolution Strategy

`src-tauri/src/services/platform/macos/ffmpeg.rs` resolves FFmpeg by probing:

1. `FFMPEG_PATH`
2. `/opt/homebrew/bin/ffmpeg`
3. `/usr/local/bin/ffmpeg`
4. `/usr/bin/ffmpeg`
5. `ffmpeg` on PATH

Each candidate is verified with `-version`.

## Build-Time Google Env Injection

`src-tauri/build.rs` imports the following from shell env or repository `.env` and exposes them as compile-time env for Rust:

- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`

This enables current local OAuth behavior and is intentionally called out as risky for distributed builds.

## Overlay Positioning

At setup, `position_overlay_windows` in `lib.rs`:

- places `overlay` near right edge, vertically centered
- places `camera-overlay` near bottom-right corner

This happens on startup; layout/size tweaks should account for these offsets.

## Device Resolution Notes

The backend resolver returns rich device metadata:

- stable-ish ID string
- display name
- default flag
- built-in flag

Settings UI uses these to show expressive labels and recover from stale stored IDs.
