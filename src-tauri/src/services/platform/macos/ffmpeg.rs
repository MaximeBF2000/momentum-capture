use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{AppError, AppResult};

pub struct FfmpegLocator {
    fallback_paths: Vec<PathBuf>,
}

impl FfmpegLocator {
    pub fn new() -> Self {
        let mut fallback_paths: Vec<PathBuf> = Vec::new();
        if let Ok(custom_path) = std::env::var("FFMPEG_PATH") {
            fallback_paths.push(PathBuf::from(custom_path));
        }

        fallback_paths.extend([
            PathBuf::from("/opt/homebrew/bin/ffmpeg"),
            PathBuf::from("/usr/local/bin/ffmpeg"),
            PathBuf::from("/usr/bin/ffmpeg"),
            PathBuf::from("ffmpeg"),
        ]);

        Self { fallback_paths }
    }

    pub fn resolve(&self) -> AppResult<PathBuf> {
        for path in &self.fallback_paths {
            if is_executable(path) {
                println!("[FFmpeg] Found FFmpeg at: {}", path.display());
                return Ok(path.clone());
            }
        }

        Err(AppError::Recording(
            "FFmpeg not found. Install via Homebrew or set FFMPEG_PATH.".to_string(),
        ))
    }
}

fn is_executable(path: &Path) -> bool {
    Command::new(path).arg("-version").output().is_ok()
}
