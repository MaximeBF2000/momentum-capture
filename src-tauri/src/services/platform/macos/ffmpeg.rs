use std::process::Command;

/// Locate the FFmpeg binary on macOS, falling back to PATH.
pub(crate) fn find_ffmpeg() -> String {
    // Allow overriding via env var for custom installs packaged with the app
    let mut possible_paths: Vec<String> = Vec::new();
    if let Ok(custom_path) = std::env::var("FFMPEG_PATH") {
        possible_paths.push(custom_path);
    }

    // Try common macOS FFmpeg locations
    possible_paths.extend([
        "ffmpeg".to_string(),                   // System PATH
        "/opt/homebrew/bin/ffmpeg".to_string(), // Homebrew on Apple Silicon
        "/usr/local/bin/ffmpeg".to_string(),    // Homebrew on Intel
        "/usr/bin/ffmpeg".to_string(),          // System location
    ]);
    
    for path in possible_paths {
        if Command::new(&path).arg("-version").output().is_ok() {
            println!("[FFmpeg] Found FFmpeg at: {}", path);
            return path;
        }
    }
    
    // Default to "ffmpeg" and let it fail with a clear error if not found
    eprintln!("[FFmpeg] WARNING: FFmpeg not found in common locations, trying system PATH");
    "ffmpeg".to_string()
}
