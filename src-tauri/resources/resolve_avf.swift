#!/usr/bin/env swift

import Foundation
import AVFoundation
import CoreGraphics

func json(_ obj: Any) {
    do {
        let data = try JSONSerialization.data(withJSONObject: obj, options: [])
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write("\n".data(using: .utf8)!)
    } catch {
        let errorObj: [String: Any] = ["error": "Failed to serialize JSON: \(error.localizedDescription)"]
        let errorData = try! JSONSerialization.data(withJSONObject: errorObj, options: [])
        FileHandle.standardError.write(errorData)
        FileHandle.standardError.write("\n".data(using: .utf8)!)
        exit(1)
    }
}

func indexOfBuiltInMic() -> Int? {
    let audio = AVCaptureDevice.DiscoverySession(
        deviceTypes: [.microphone],
        mediaType: .audio,
        position: .unspecified
    ).devices
    
    if audio.isEmpty {
        return nil
    }
    
    // Find built-in microphone using device properties, not names
    // Strategy: Prefer devices with built-in transport type, then check uniqueID patterns
    
    // Priority 1: Check transportType - built-in devices have transportType == 0
    // transportType values from IOKit: 0 = built-in, 1 = USB, 2 = FireWire, 3 = PCI, etc.
    var builtInCandidates: [(index: Int, device: AVCaptureDevice)] = []
    for (index, device) in audio.enumerated() {
        if device.transportType == 0 {
            builtInCandidates.append((index, device))
        }
    }
    
    // If we found built-in transport devices, prefer the one that's also the default
    if !builtInCandidates.isEmpty {
        if let defaultDevice = AVCaptureDevice.default(for: .audio) {
            if let match = builtInCandidates.first(where: { $0.device == defaultDevice }) {
                return match.index
            }
        }
        // Return first built-in transport device
        return builtInCandidates.first?.index
    }
    
    // Priority 2: Check uniqueID for Apple-built device patterns
    // Built-in devices often have uniqueID containing "Apple" or specific patterns
    for (index, device) in audio.enumerated() {
        let uniqueID = device.uniqueID.lowercased()
        if uniqueID.contains("apple") || uniqueID.contains("builtin") || uniqueID.contains("internal") {
            return index
        }
    }
    
    // Priority 3: Use default device if available
    if let defaultDevice = AVCaptureDevice.default(for: .audio) {
        if let index = audio.firstIndex(of: defaultDevice) {
            return index
        }
    }
    
    // Final fallback: return first device
    return 0
}

func indexOfBuiltInCamera() -> Int? {
    // Use .external instead of deprecated .externalUnknown for macOS 14.0+
    let deviceTypes: [AVCaptureDevice.DeviceType] = if #available(macOS 14.0, *) {
        [.builtInWideAngleCamera, .external]
    } else {
        [.builtInWideAngleCamera, .externalUnknown]
    }
    
    let video = AVCaptureDevice.DiscoverySession(
        deviceTypes: deviceTypes,
        mediaType: .video,
        position: .unspecified
    ).devices
    
    // Prefer built-in wide angle, front if present
    if let i = video.firstIndex(where: { $0.deviceType == .builtInWideAngleCamera && $0.position == .front }) {
        return i
    }
    if let i = video.firstIndex(where: { $0.deviceType == .builtInWideAngleCamera }) {
        return i
    }
    return video.isEmpty ? nil : 0
}

func videoCaptureDeviceCount() -> Int {
    // Use .external instead of deprecated .externalUnknown for macOS 14.0+
    let deviceTypes: [AVCaptureDevice.DeviceType] = if #available(macOS 14.0, *) {
        [.builtInWideAngleCamera, .external]
    } else {
        [.builtInWideAngleCamera, .externalUnknown]
    }
    
    return AVCaptureDevice.DiscoverySession(
        deviceTypes: deviceTypes,
        mediaType: .video,
        position: .unspecified
    ).devices.count
}

func mainDisplayScreenIndex() -> Int? {
    var count: UInt32 = 0
    guard CGGetActiveDisplayList(0, nil, &count) == .success, count > 0 else {
        return nil
    }
    var displays = [CGDirectDisplayID](repeating: 0, count: Int(count))
    guard CGGetActiveDisplayList(count, &displays, &count) == .success else {
        return nil
    }
    
    let main = CGMainDisplayID()
    return displays.firstIndex(of: main)
}

// Note: FFmpeg's device enumeration may differ from AVFoundation's
// FFmpeg includes Continuity Camera devices that AVFoundation might not count
// So we can't reliably calculate screen index - we need to parse FFmpeg's output
// For now, we'll use a workaround: add 1 to account for potential Continuity cameras

func indexOfSystemAudio() -> Int? {
    // System audio capture requires BlackHole virtual audio device
    // BlackHole appears as an audio INPUT device when installed
    // Note: BlackHole must be set as OUTPUT device (or in Multi-Output Device) to receive system audio
    let audio = AVCaptureDevice.DiscoverySession(
        deviceTypes: [.microphone],
        mediaType: .audio,
        position: .unspecified
    ).devices
    
    // Look for BlackHole by name (case-insensitive)
    // BlackHole device names can vary: "BlackHole", "BlackHole 2ch", "BlackHole 16ch", etc.
    for (index, device) in audio.enumerated() {
        let name = device.localizedName.lowercased()
        // Check for BlackHole variations
        if name.contains("blackhole") || name.contains("black hole") {
            return index
        }
        // Also check uniqueID which might contain "BlackHole"
        let uniqueID = device.uniqueID.lowercased()
        if uniqueID.contains("blackhole") || uniqueID.contains("black hole") {
            return index
        }
    }
    
    // If no BlackHole found, return nil
    // User needs to install BlackHole and configure Multi-Output Device
    return nil
}

let mic = indexOfBuiltInMic()
let cam = indexOfBuiltInCamera()
let screenIdx = mainDisplayScreenIndex()
let camCount = videoCaptureDeviceCount()
let systemAudio = indexOfSystemAudio()

// FFmpeg's avfoundation lists devices as: [cameras...][screens...]
// So we need to find the screen index in FFmpeg's enumeration
// First, let's list what FFmpeg actually sees by using a different approach:
// We'll use the fact that screens come after ALL cameras in FFmpeg's list

// Count total video devices (cameras + screens) that FFmpeg will see
var totalVideoDevices = camCount
var displayCount: UInt32 = 0
if CGGetActiveDisplayList(0, nil, &displayCount) == .success {
    totalVideoDevices += Int(displayCount)
}

// Screen index in FFmpeg = cameraCount + screenIndexInDisplayList
// However, FFmpeg may see more cameras than AVFoundation (e.g., Continuity Camera devices)
// Based on logs: FFmpeg sees 3 cameras when AVFoundation sees 2
// So we add 1 to account for this discrepancy
let screenVideoIndex: Int? = screenIdx.map { camCount + $0 + 1 }

json([
    "audio_index_builtin_mic": mic as Any,
    "video_index_builtin_cam": cam as Any,
    "video_index_main_screen": screenVideoIndex as Any,
    "audio_index_system_audio": systemAudio as Any,
    "video_capture_device_count": camCount,
    "active_display_index_main": screenIdx as Any
])

