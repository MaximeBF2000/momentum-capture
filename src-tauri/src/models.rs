use serde::{Deserialize, Serialize};

fn default_immersive_shortcut() -> String {
    "Option+I".to_string()
}

fn default_hide_webcam_on_immersive_mode() -> bool {
    true
}

fn default_save_recordings_locally() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingOptions {
    pub include_microphone: bool,
    pub include_camera: bool,
    #[serde(default)]
    pub microphone_device_id: Option<String>,
    #[serde(default)]
    pub camera_device_id: Option<String>,
    pub screen_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureDevice {
    pub id: String,
    pub name: String,
    pub index: i32,
    pub is_default: bool,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureDevices {
    pub microphones: Vec<CaptureDevice>,
    pub cameras: Vec<CaptureDevice>,
    pub selected_microphone_id: Option<String>,
    pub selected_camera_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GoogleDriveSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub folder_name: Option<String>,
    #[serde(default)]
    pub account_email: Option<String>,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub mic_enabled: bool,
    pub camera_enabled: bool,
    #[serde(default)]
    pub microphone_device_id: Option<String>,
    #[serde(default)]
    pub camera_device_id: Option<String>,
    #[serde(default = "default_hide_webcam_on_immersive_mode")]
    pub hide_webcam_on_immersive_mode: bool,
    #[serde(default = "default_immersive_shortcut")]
    pub immersive_shortcut: String,
    #[serde(default = "default_save_recordings_locally")]
    pub save_recordings_locally: bool,
    pub save_location: Option<String>,
    #[serde(default)]
    pub google_drive: GoogleDriveSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mic_enabled: false,
            camera_enabled: false,
            microphone_device_id: None,
            camera_device_id: None,
            hide_webcam_on_immersive_mode: default_hide_webcam_on_immersive_mode(),
            immersive_shortcut: default_immersive_shortcut(),
            save_recordings_locally: default_save_recordings_locally(),
            save_location: None,
            google_drive: GoogleDriveSettings::default(),
        }
    }
}
