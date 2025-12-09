use crate::models::AppSettings;
use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::fs;

pub fn get_settings_path() -> AppResult<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| AppError::Settings("Could not find config directory".to_string()))?;
    Ok(config_dir.join("momentum").join("settings.json"))
}

pub fn load_settings() -> AppResult<AppSettings> {
    let settings_path = get_settings_path()?;
    
    if !settings_path.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(&settings_path)?;
    let settings: AppSettings = serde_json::from_str(&content)
        .map_err(|e| AppError::Settings(format!("Failed to parse settings: {}", e)))?;
    
    Ok(settings)
}

pub fn save_settings(settings: &AppSettings) -> AppResult<()> {
    let settings_path = get_settings_path()?;
    
    // Create directory if it doesn't exist
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(settings)
        .map_err(|e| AppError::Settings(format!("Failed to serialize settings: {}", e)))?;
    
    fs::write(&settings_path, content)?;
    Ok(())
}
