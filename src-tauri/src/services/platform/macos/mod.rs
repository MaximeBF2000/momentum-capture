mod audio_recorder;
mod ffmpeg;
mod muxed_recorder;
mod screen_recorder;
mod synchronized_recorder;

#[allow(unused_imports)]
pub use audio_recorder::AudioRecorder;
#[allow(unused_imports)]
pub use muxed_recorder::MuxedRecorder;
#[allow(unused_imports)]
pub use screen_recorder::ScreenRecorder;
#[allow(unused_imports)]
pub use synchronized_recorder::SynchronizedRecorder;
