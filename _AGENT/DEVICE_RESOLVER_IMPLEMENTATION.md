# Device Resolver Implementation

## Overview

This document describes the implementation of a robust device resolution system that dynamically detects AVFoundation device indices at runtime, eliminating the need for hardcoded device indices that break when external devices are connected or when running on different Mac models.

## Problem Solved

**Previous Issue**: The app used hardcoded FFmpeg device indices (e.g., `"3:1"` for screen device 3 and microphone device 1), which caused failures when:

- External devices (cameras, microphones, displays) were connected/disconnected
- Running on different Mac models (MacBook Pro, Air, iMac, etc.)
- Device enumeration order changed

**Solution**: Use native macOS APIs via a Swift resolver script to dynamically detect the correct device indices based on device properties (built-in mic, built-in camera, main display) rather than relying on enumeration order.

## Architecture

### Components

1. **Swift Resolver Script** (`src-tauri/resources/resolve_avf.swift`)

   - Uses AVFoundation and CoreGraphics APIs
   - Detects built-in microphone, built-in camera, and main display
   - Outputs JSON with device indices

2. **Rust Device Resolver Module** (`src-tauri/src/services/platform/device_resolver.rs`)

   - Calls Swift resolver script
   - Parses JSON output
   - Provides typed access to device indices

3. **Integration Points**:
   - `MuxedRecorder::start()` - Uses resolved screen and microphone indices
   - `CameraPreview::start()` - Uses resolved camera index

## Implementation Details

### Swift Resolver Script

The resolver script (`resolve_avf.swift`) uses native macOS APIs to find devices:

- **Built-in Microphone**: Finds audio device with `transportType == .builtIn`
- **Built-in Camera**: Finds video device with `deviceType == .builtInWideAngleCamera` (prefers front camera)
- **Main Display**: Uses `CGMainDisplayID()` and calculates screen index as `camera_count + display_index`
- **System Audio**: Attempts to find BlackHole virtual audio device (for future system audio capture)

**Output Format**:

```json
{
  "audio_index_builtin_mic": 0,
  "video_index_builtin_cam": 0,
  "video_index_main_screen": 1,
  "audio_index_system_audio": null,
  "video_capture_device_count": 1,
  "active_display_index_main": 0
}
```

### Rust Integration

The `device_resolver` module:

- Locates the Swift script in multiple possible locations (app bundle, development paths)
- Executes the script using `swift` command
- Parses JSON output into typed struct
- Provides helper methods to get device indices with error handling

### Path Resolution

The resolver searches for the Swift script in this order:

1. App bundle Resources directory (for bundled app)
2. CARGO_MANIFEST_DIR/resources/ (for development)
3. Current working directory
4. Compile-time path

## Changes Made

### New Files

1. `src-tauri/resources/resolve_avf.swift` - Swift resolver script
2. `src-tauri/src/services/platform/device_resolver.rs` - Rust integration module

### Modified Files

1. `src-tauri/src/services/platform/mod.rs` - Added device_resolver module
2. `src-tauri/src/services/platform/macos.rs`:

   - Updated `MuxedRecorder::start()` to use resolved device indices
   - Removed hardcoded device indices (`"3:1"`)
   - Added device resolution before starting FFmpeg

3. `src-tauri/src/services/camera.rs`:
   - Updated `CameraPreview::start()` to use resolved camera index
   - Removed hardcoded camera index (`"0"`)

## Usage

### Recording

When starting a recording, the system now:

1. Resolves device indices using Swift resolver
2. Uses resolved indices in FFmpeg command: `-i "{screen_index}:{mic_index}"`
3. Logs resolved indices for debugging

### Camera Preview

When starting camera preview:

1. Resolves camera device index
2. Uses resolved index in FFmpeg command: `-i "{camera_index}:"`
3. Falls back to index 0 if resolution fails (with warning)

## Benefits

1. **Robustness**: Works regardless of external device connections
2. **Cross-Model Compatibility**: Works on all Mac models (MacBook Pro, Air, iMac, etc.)
3. **Language Independence**: Doesn't depend on localized device names
4. **Future-Proof**: Easy to extend for additional device types (system audio, external cameras, etc.)

## Requirements

- **Xcode Command Line Tools**: Required to run Swift scripts (`xcode-select --install`)
- **macOS**: Uses macOS-specific APIs (AVFoundation, CoreGraphics)

## Error Handling

- If Swift resolver fails, clear error messages guide users to install Xcode Command Line Tools
- If device resolution fails, the system falls back gracefully where possible
- All errors are logged with detailed information for debugging

## Future Enhancements

1. **System Audio Capture**: Currently detects BlackHole but doesn't use it yet. Future implementation will mix system audio with microphone audio.

2. **Compiled Binary**: For better performance and to avoid requiring Swift at runtime, the resolver could be compiled to a native binary and bundled with the app.

3. **Caching**: Device indices could be cached and only re-resolved when devices change (using macOS notification APIs).

4. **Multiple Displays**: Support for selecting specific displays (currently uses main display).

5. **External Devices**: Support for selecting external cameras/microphones via settings UI.

## Testing

To test the implementation:

1. **Development**: Run the app and check logs for resolved device indices
2. **With External Devices**: Connect/disconnect external cameras, microphones, or displays and verify recording still works
3. **Different Mac Models**: Test on different Mac models to verify cross-compatibility

## Troubleshooting

**Issue**: "Failed to run Swift resolver"

- **Solution**: Install Xcode Command Line Tools: `xcode-select --install`

**Issue**: "Could not find resolve_avf.swift script"

- **Solution**: Ensure the script exists at `src-tauri/resources/resolve_avf.swift` and has execute permissions (`chmod +x`)

**Issue**: Wrong devices selected

- **Solution**: Check resolver output in logs, verify device properties match expected values

