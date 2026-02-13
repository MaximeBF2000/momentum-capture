use serde::{Deserialize, Serialize};

fn default_immersive_shortcut() -> String {
    "Option+I".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingOptions {
    pub include_microphone: bool,
    pub include_camera: bool,
    pub screen_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub mic_enabled: bool,
    pub camera_enabled: bool,
    #[serde(default = "default_immersive_shortcut")]
    pub immersive_shortcut: String,
    pub save_location: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mic_enabled: false,
            camera_enabled: false,
            immersive_shortcut: default_immersive_shortcut(),
            save_location: None,
        }
    }
}
