mod frame_handler;
mod mux;
mod start;
mod state;
mod stop;

#[allow(unused_imports)]
pub use start::{is_available, start_recording};
#[allow(unused_imports)]
pub use state::{
    is_recording_active,
    recording_paused,
    mic_muted,
    set_mic_muted,
    set_system_audio_muted,
    set_recording_paused,
    system_audio_muted,
};
#[allow(unused_imports)]
pub use stop::stop_recording;
