#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub mod device_resolver;

#[cfg(target_os = "macos")]
pub mod screencapturekit_recorder;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;
