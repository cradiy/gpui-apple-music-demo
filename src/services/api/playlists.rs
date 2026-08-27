use serde::{Deserialize, Serialize};

use super::library::TrackResponse;

#[derive(Clone, Debug, Serialize)]
pub struct PlaylistWriteRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaylistSort {
    #[default]
    Custom,
    CreatedAtAsc,
    CreatedAtDesc,
    NameAsc,
    NameDesc,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaylistItemSort {
    #[default]
    Custom,
    AddedAtAsc,
    AddedAtDesc,
    NameAsc,
    NameDesc,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlaylistResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub position: i64,
    pub track_count: i64,
    pub duration_ms: i64,
    pub cover_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlaylistCoverResponse {
    pub cover_url: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlaylistItemResponse {
    pub id: String,
    pub position: i64,
    pub added_at: i64,
    pub track: TrackResponse,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlaylistDetailResponse {
    pub playlist: PlaylistResponse,
    pub items: Vec<PlaylistItemResponse>,
}
