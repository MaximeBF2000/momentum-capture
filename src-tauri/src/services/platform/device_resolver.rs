use crate::error::{AppError, AppResult};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Deserialize, Clone)]
pub struct AvfResolved {
    pub audio_index_builtin_mic: Option<i32>,
    pub video_index_builtin_cam: Option<i32>,
    pub video_index_main_screen: Option<i32>,
    pub audio_index_system_audio: Option<i32>,
    pub video_capture_device_count: Option<i32>,
    pub active_display_index_main: Option<i32>,
}

impl AvfResolved {
    pub fn get_mic_index(&self) -> AppResult<i32> {
        self.audio_index_builtin_mic
            .ok_or_else(|| AppError::Recording("Built-in microphone not found".to_string()))
    }

    pub fn get_camera_index(&self) -> AppResult<i32> {
        self.video_index_builtin_cam
            .ok_or_else(|| AppError::Recording("Built-in camera not found".to_string()))
    }

    pub fn get_screen_index(&self) -> AppResult<i32> {
        self.video_index_main_screen
            .ok_or_else(|| AppError::Recording("Main screen not found".to_string()))
    }

    pub fn get_system_audio_index(&self) -> Option<i32> {
        self.audio_index_system_audio
    }
}

pub fn resolve_avf_indices() -> AppResult<AvfResolved> {
    // Get the path to the Swift resolver script
    // In Tauri, resources are bundled, but during development we need to find it
    let resolver_path = get_resolver_path()?;
    
    println!("[DeviceResolver] Resolving AVFoundation device indices using: {}", resolver_path.display());
    
    let output = Command::new("swift")
        .arg(&resolver_path)
        .output()
        .map_err(|e| {
            AppError::Recording(format!(
                "Failed to run Swift resolver: {}. Make sure Xcode Command Line Tools are installed (xcode-select --install)",
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Recording(format!(
            "Swift resolver failed: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("[DeviceResolver] Resolver output: {}", stdout.trim());

    let parsed: AvfResolved = serde_json::from_str(stdout.trim())
        .map_err(|e| {
            AppError::Recording(format!(
                "Failed to parse resolver output: {}. Output was: {}",
                e, stdout
            ))
        })?;

    println!("[DeviceResolver] Resolved indices:");
    println!("[DeviceResolver]   Built-in mic: {:?}", parsed.audio_index_builtin_mic);
    println!("[DeviceResolver]   Built-in camera: {:?}", parsed.video_index_builtin_cam);
    println!("[DeviceResolver]   Main screen: {:?}", parsed.video_index_main_screen);
    println!("[DeviceResolver]   System audio (BlackHole): {:?}", parsed.audio_index_system_audio);

    Ok(parsed)
}

fn get_resolver_path() -> AppResult<PathBuf> {
    // Try multiple locations in order of preference
    
    // 1. Try relative to the executable (for bundled app)
    // In macOS app bundles, structure is: App.app/Contents/MacOS/App
    // Resources are in: App.app/Contents/Resources/
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Go up: MacOS -> Contents -> App.app -> Contents -> Resources
            let bundled_path = exe_dir.parent()
                .and_then(|contents| contents.parent())
                .map(|app| app.join("Contents").join("Resources").join("resolve_avf.swift"));
            
            if let Some(path) = bundled_path {
                if path.exists() {
                    println!("[DeviceResolver] Found resolver in app bundle: {:?}", path);
                    return Ok(path);
                }
            }
            
            // Also try: MacOS -> Contents -> Resources (alternative bundle structure)
            let alt_bundled_path = exe_dir.parent()
                .map(|contents| contents.join("Resources").join("resolve_avf.swift"));
            
            if let Some(path) = alt_bundled_path {
                if path.exists() {
                    println!("[DeviceResolver] Found resolver in app bundle (alt): {:?}", path);
                    return Ok(path);
                }
            }
        }
    }

    // 2. Try relative to CARGO_MANIFEST_DIR (for development)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev_path = PathBuf::from(manifest_dir)
            .join("resources")
            .join("resolve_avf.swift");
        if dev_path.exists() {
            println!("[DeviceResolver] Found resolver in manifest dir: {:?}", dev_path);
            return Ok(dev_path);
        }
    }

    // 3. Try relative to current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_path = cwd.join("src-tauri").join("resources").join("resolve_avf.swift");
        if cwd_path.exists() {
            println!("[DeviceResolver] Found resolver in CWD: {:?}", cwd_path);
            return Ok(cwd_path);
        }
        
        // Also try without src-tauri prefix
        let cwd_path_alt = cwd.join("resources").join("resolve_avf.swift");
        if cwd_path_alt.exists() {
            println!("[DeviceResolver] Found resolver in CWD (alt): {:?}", cwd_path_alt);
            return Ok(cwd_path_alt);
        }
    }

    // 4. Try absolute path from project root
    let absolute_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("resolve_avf.swift");
    if absolute_path.exists() {
        println!("[DeviceResolver] Found resolver at compile-time path: {:?}", absolute_path);
        return Ok(absolute_path);
    }

    Err(AppError::Recording(
        format!(
            "Could not find resolve_avf.swift script. Searched in:\n\
            - App bundle Resources directory\n\
            - CARGO_MANIFEST_DIR/resources/\n\
            - Current working directory\n\
            - Compile-time path: {:?}\n\
            Please ensure the script exists at: src-tauri/resources/resolve_avf.swift",
            absolute_path
        )
    ))
}

