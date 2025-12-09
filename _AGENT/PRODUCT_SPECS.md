# Product Specs

Product name: Momuentum screen recorder

Momentum is a desktop app that allows you to record your screen and save videos easily.
It is meant to be a simple and easy to use screen recorder for the everyday user.

## Tech stack

The app is built with Tauri and Rust for the backend, and React and Typescript for the frontend.
The app is styled with Tailwind CSS and uses Lucide Icons for the icons.

## Interface

The interface should be simple and easy to use.

It is just an overlay window that appears on the top right corner of the screen when the user launches the app.

The overlay is a Tauri frameless window that is always on top of the other windows, and is draggable.

### Overlay visual description (actual screenshot available at /\_AGENTS/images/recording-overlay.png)

The interface is a horizontal, rounded rectangle control bar with a dark background.
It contains five main elements arranged from left to right, centered vertically:

Recording indicator (left side)

A small solid red circle icon.

To its right, the word “RECORDING” in uppercase, light gray text.

Elapsed time display

After a small horizontal spacing, the elapsed recording time is shown as white monospace text in this format: 00:12:34.

Vertical divider

A thin, light gray vertical line separating the time display from the control buttons.

Control buttons (center-right)
There are three circular buttons:

Pause button: white circle with a black pause icon (two vertical bars).

Stop button: red circle with a white square icon.

Microphone toggle: dark circle with a glowing green microphone icon. The green indicates that the microphone is active.

Camera button (right side)

A final circular button with a gray camera icon inside a dark circle.

It is visually less emphasized than the active microphone button.

The entire bar has:

A pill-shaped outline (large border radius).

A subtle shadow.

Even padding on all sides.

Elements spaced evenly using consistent gaps.

## Logic flow

The logic flow is as follows:

The user launches the app.
The overlay window appears on the screen in a non recoding state.
The user clicks the start recording button.
The app waits 3 seconds (and a countdown is shown in the overlay window).
The app starts recording the screen/camera/microphone (based on the user's settings).
The app records the screen/camera/microphone.
The user can pause the recording and resume it.
The user stops the recording.
The app stops recording the screen/camera/microphone.
The app saves the recording to the user's downloads directory.

The camera overlay window is shown/hidden on the screen weather in a non recoding state or in a recording state, based on the user's settings.
The camera overlay window is a frameless draggable rounded window that appears on the bottom right corner of the screen and shows the camera feed.

## Other specifications

The app uses native OS APIs to record the screen/camera/microphone.
The app uses native OS APIs to save the recording to the user's downloads directory.
The app uses native OS APIs to show the camera overlay window.
