/// Global multiplier applied to microphone samples before they are encoded into
/// the final recording. Increase to make mic audio louder, decrease to quiet
/// it down. Keep within a reasonable range (e.g. `0.8..=2.0`) to avoid harsh
/// dynamics.
pub const MIC_VOLUME_GAIN: f32 = 1.8;

pub mod recording;
pub mod camera;
pub mod settings;
pub mod platform;
pub mod immersive;
pub mod hotkey;
pub mod time;

pub use recording::Recorder;
pub use camera::CameraPreview;
