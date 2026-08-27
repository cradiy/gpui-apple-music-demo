use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize)]
pub struct TrackQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BrowseQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PageQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrackCredit {
    pub artist_id: String,
    pub name: String,
    pub role: String,
    pub position: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TrackResponse {
    pub id: String,
    pub title: Option<String>,
    pub artist_id: Option<String>,
    pub album_id: Option<String>,
    pub album_artist_id: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub composer: Option<String>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub release_year: Option<i64>,
    pub content_type: String,
    pub container: String,
    pub codec: String,
    pub size_bytes: i64,
    pub duration_ms: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channel_count: Option<i64>,
    pub bit_depth: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub favorite: bool,
    pub has_artwork: bool,
    #[serde(default)]
    pub credits: Vec<TrackCredit>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TrackPageResponse {
    pub items: Vec<TrackResponse>,
    pub total: i64,
    pub limit: u32,
    pub offset: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ArtistResponse {
    pub id: String,
    pub name: String,
    pub sort_name: String,
    pub bio: Option<String>,
    pub album_count: i64,
    pub track_count: i64,
    pub duration_ms: i64,
    pub artwork_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ArtistPageResponse {
    pub items: Vec<ArtistResponse>,
    pub total: i64,
    pub limit: u32,
    pub offset: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AlbumResponse {
    pub id: String,
    pub name: String,
    pub sort_name: String,
    pub album_artist_id: String,
    pub album_artist_name: String,
    pub release_year: Option<i64>,
    pub track_count: i64,
    pub duration_ms: i64,
    pub artwork_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AlbumPageResponse {
    pub items: Vec<AlbumResponse>,
    pub total: i64,
    pub limit: u32,
    pub offset: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GenreResponse {
    pub name: String,
    pub track_count: i64,
    pub duration_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GenrePageResponse {
    pub items: Vec<GenreResponse>,
    pub total: i64,
    pub limit: u32,
    pub offset: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PatchArtistRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<Option<String>>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PatchAlbumRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_year: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReplaceTrackMetadataRequest {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub composer: Option<String>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub release_year: Option<i64>,
    pub lyrics: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PatchTrackMetadataRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composer: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_year: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<Option<String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TrackMetadataResponse {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub composer: Option<String>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub release_year: Option<i64>,
    pub lyrics: Option<String>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReplaceLyricsRequest {
    pub lyrics: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LyricsResponse {
    pub lyrics: Option<String>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ArtworkResponse {
    pub content_type: String,
    pub size_bytes: i64,
    pub updated_at: i64,
}
