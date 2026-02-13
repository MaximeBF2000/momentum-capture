use crate::error::{AppError, AppResult};
use crate::models::AppSettings;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct SettingsStore {
    settings_path: PathBuf,
}

impl SettingsStore {
    pub fn new(base_dir: Option<PathBuf>) -> AppResult<Self> {
        let base_dir = match base_dir {
            Some(dir) => dir,
            None => dirs::config_dir()
                .ok_or_else(|| AppError::Settings("Could not find config directory".to_string()))?,
        };
        Ok(Self {
            settings_path: base_dir.join("momentum").join("settings.json"),
        })
    }

    pub fn load(&self) -> AppResult<AppSettings> {
        if !self.settings_path.exists() {
            return Ok(AppSettings::default());
        }

        let content = fs::read_to_string(&self.settings_path)?;
        let settings: AppSettings = serde_json::from_str(&content)
            .map_err(|e| AppError::Settings(format!("Failed to parse settings: {}", e)))?;
        Ok(settings)
    }

    pub fn save(&self, settings: &AppSettings) -> AppResult<()> {
        if let Some(parent) = self.settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(settings)
            .map_err(|e| AppError::Settings(format!("Failed to serialize settings: {}", e)))?;
        fs::write(&self.settings_path, content)?;
        Ok(())
    }

    #[cfg(test)]
    pub fn path(&self) -> &PathBuf {
        &self.settings_path
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsStore;
    use crate::models::AppSettings;

    #[test]
    fn saves_and_loads_settings() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::new(Some(temp_dir.path().to_path_buf())).expect("store");

        let settings = AppSettings {
            mic_enabled: true,
            camera_enabled: true,
            immersive_shortcut: "Command+Shift+I".to_string(),
            save_location: Some("/tmp".to_string()),
        };

        store.save(&settings).expect("save");
        let loaded = store.load().expect("load");
        assert_eq!(loaded.mic_enabled, true);
        assert_eq!(loaded.camera_enabled, true);
        assert_eq!(loaded.immersive_shortcut, "Command+Shift+I");
        assert_eq!(loaded.save_location.as_deref(), Some("/tmp"));
    }

    #[test]
    fn uses_default_settings_when_missing() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::new(Some(temp_dir.path().to_path_buf())).expect("store");
        let loaded = store.load().expect("load");
        assert_eq!(loaded, AppSettings::default());
    }
}
