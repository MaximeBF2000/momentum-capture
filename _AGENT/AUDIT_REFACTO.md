# Momentum Refactor Summary

Date: 2026-01-25

This document summarizes the refactor work applied after the audit. `_AGENT` legacy content was not used as input.

## Major Changes

### Recording Pipeline (ScreenCaptureKit-only)
- Removed the legacy FFmpeg AVFoundation fallback and muxed recorder stack.
- Centralized ScreenCaptureKit recording into a `ScreenCaptureKitRecorder` struct with explicit state and no global singletons.
- Updated frame handling to use injected pause/mute flags and optional camera sync handles.
- Muxing now uses the resolved FFmpeg path from a shared locator.

### Background Orchestration + Progress Events
- `start_recording` and `stop_recording` now run in background tasks.
- Added `recording-elapsed` event and event payloads for started/paused/resumed/stopped.
- UI timer now tracks backend-provided elapsed time instead of local intervals.

### macOS-Only Simplification
- Removed Windows/Linux code paths in camera preview, hotkeys, and platform modules.
- Simplified platform module exports to macOS-only implementations.

### Settings as Single Source of Truth
- Introduced `SettingsStore` (backend) to load/save settings with a clean API.
- `update_settings` and `update_immersive_shortcut` emit `settings-updated` events.
- Frontend subscribes to `settings-updated` and syncs all windows from one source.

### FFmpeg Resolution and Native Paths
- Added `FfmpegLocator` to resolve bundled FFmpeg (resources/ffmpeg) with PATH fallbacks.
- Output path now uses `dirs::download_dir` or `settings.saveLocation` (no shell execution).

## Frontend Updates

- Removed duplicate mic/camera flags from `recordingStore`; use `settingsStore` instead.
- Control bar uses settings for recording options and camera enable state.
- Overlay window uses backend event payloads for elapsed time and state transitions.
- Added settings event subscription in `App.tsx`.

## Test Infrastructure

- Added Rust unit tests for `SettingsStore` and `RecordingClock`.
- Added Vitest setup with baseline tests for Zustand stores.
- Added `tempfile` dev dependency for backend tests.

## Files Added/Removed (Not Exhaustive)

Added:
- `src-tauri/src/services/recording.rs` (rewritten)
- `src/setupTests.ts`
- `src/state/recordingStore.test.ts`
- `src/state/settingsStore.test.ts`
- `_AGENT/AUDIT_REFACTO.md`

Removed:
- `src-tauri/src/services/platform/macos/audio_recorder.rs`
- `src-tauri/src/services/platform/macos/screen_recorder.rs`
- `src-tauri/src/services/platform/macos/synchronized_recorder.rs`
- `src-tauri/src/services/platform/macos/muxed_recorder.rs`

## Follow-ups to Consider

- Add bundled FFmpeg binary under `src-tauri/resources/ffmpeg` to fully support the locator.
- Expand tests to cover recording event flows and settings persistence at the command layer.
- Add integration tests for hotkey updates and immersive mode behavior.
