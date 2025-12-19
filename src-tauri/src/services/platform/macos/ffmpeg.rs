use std::process::Command;

/// Locate the FFmpeg binary on macOS, falling back to PATH.
pub(crate) fn find_ffmpeg() -> String {
    // Try common macOS FFmpeg locations
    let possible_paths = vec![
        "ffmpeg", // System PATH
        "/opt/homebrew/bin/ffmpeg", // Homebrew on Apple Silicon
        "/usr/local/bin/ffmpeg", // Homebrew on Intel
        "/usr/bin/ffmpeg", // System location
    ];
    
    for path in possible_paths {
        if Command::new(path).arg("-version").output().is_ok() {
            println!("[FFmpeg] Found FFmpeg at: {}", path);
            return path.to_string();
        }
    }
    
    // Default to "ffmpeg" and let it fail with a clear error if not found
    eprintln!("[FFmpeg] WARNING: FFmpeg not found in common locations, trying system PATH");
    "ffmpeg".to_string()
}
