pub mod api;
pub mod config;
pub mod local;
#[cfg(target_os = "linux")]
pub mod mpris;
pub mod session;

use crate::services::api::{ApiError, LibraryApi, PlaylistsApi, ProfileApi};

pub fn library_api(cx: &gpui::App) -> Result<LibraryApi, ApiError> {
    let _ = cx;
    LibraryApi::local()
}

pub fn playlists_api(cx: &gpui::App) -> Result<PlaylistsApi, ApiError> {
    let _ = cx;
    PlaylistsApi::local()
}

pub fn profile_api(cx: &gpui::App) -> Result<ProfileApi, ApiError> {
    let _ = cx;
    Ok(ProfileApi)
}
