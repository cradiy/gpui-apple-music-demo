pub mod app;
pub mod audio_metadata;
pub mod components;
pub mod lyrics;
#[cfg(target_os = "linux")]
pub mod mpris;
pub mod state;
pub mod theme;
pub mod window;

static FONT_FAMILY: &str = "Noto Sans CJK SC";
