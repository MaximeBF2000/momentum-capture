# Momentum

![Momentum screenshot](/public/screenshot.png)

**Momentum** is a native macOS screen recording application focused on speed, simplicity, and local-first recording.

It is designed to make screen recording feel instantaneous: launch, press record, speak — no setup, no cloud, no accounts, no friction.

Momentum is built with **Rust**, **Tauri**, and **React**, and runs entirely on-device.

---

## Overview

Momentum provides a minimal recording experience through two lightweight overlays:

- **Control Bar Overlay**
- **Webcam Overlay**

The app captures:

- Screen
- System audio
- Microphone audio
- Built-in webcam

All recordings are saved **locally** as MP4 files.

There is no cloud processing, no upload step, and no background service.

---

## Key Principles

- **Local-first**: recordings never leave your machine
- **Opinionated defaults**: no configuration required to start recording
- **Minimal UI**: overlays stay out of the way
- **Native performance**: Rust-powered backend
- **One-purpose tool**: record fast, share later however you want

---

## Features

### Recording Controls

The control bar overlay provides:

- Start / stop recording
- Pause / resume recording
- Mute / unmute microphone
- Mute / unmute system audio
- Enable / disable webcam overlay
- Immersive mode toggle (via shortcut)

### Webcam Overlay

- Circular webcam overlay
- Draggable
- Fully optional (can be disabled at any time)
- Hidden automatically in immersive mode

### Immersive Mode

- Toggle with **Option + I**
- Hides all overlays while recording
- Recording continues normally
- Can be enabled or disabled during a recording
- Shortcut-only by design (no UI toggle)

---

## Output

- **Format**: MP4
- **Location**: macOS `Downloads` folder
- **File naming**: automatic
- **Processing**: real-time, no post-processing step

---

## Permissions

Momentum requires the following macOS permissions:

- Screen Recording
- Microphone
- Camera
- System Audio

No additional drivers or virtual audio devices are required.  
Everything works out of the box using native macOS APIs.

---

## Technical Stack

- **Backend**: Rust
- **App Framework**: Tauri
- **Frontend**: React + TypeScript + Tailwind CSS
- **Platform**: macOS only

Momentum is intentionally macOS-specific and optimized for Apple hardware.

---

## Design Constraints (Intentional)

Momentum is an opinionated tool. Some limitations are deliberate:

- No resolution or frame rate configuration
- No external microphone or webcam selection
- Uses built-in Mac devices only
- No editing or timeline
- No cloud sync or sharing

The goal is to remove decision fatigue and reduce setup time to zero.

---

## Known Limitations

Momentum is an early-stage product.

Current known issues:

- Occasional audio drift between webcam and microphone audio
- Limited configuration options
- No multi-device input selection

These issues are actively being worked on.

---

## Roadmap

Planned improvements include:

- Fixing audio drift issues
- Optional configuration UI
- Output folder customization
- Input device selection (microphone, camera)
- General stability improvements

---

## Status

Momentum is under active development and not yet publicly released.

Expect breaking changes, bugs, and rapid iteration.

---

## License

Momentum is a proprietary, closed-source application.

All rights reserved.
