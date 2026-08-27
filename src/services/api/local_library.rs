use std::fs;

use super::{
    AlbumPageResponse, AlbumResponse, ApiError, ArtistPageResponse, ArtistResponse,
    ArtworkResponse, BinaryMetadata, BinaryResponse, BrowseQuery, ByteRange, GenrePageResponse,
    GenreResponse, LyricsResponse, PageQuery, PatchAlbumRequest, PatchArtistRequest,
    PatchTrackMetadataRequest, ReplaceTrackMetadataRequest, TrackMetadataResponse,
    TrackPageResponse, TrackQuery, TrackResponse,
};
use crate::services::local::{LocalStore, MetadataUpdate};

#[derive(Clone)]
pub struct LibraryApi {
    store: LocalStore,
}

impl LibraryApi {
    pub fn local() -> Result<Self, ApiError> {
        Ok(Self {
            store: LocalStore::shared()?,
        })
    }

    pub async fn tracks(&self, query: &TrackQuery) -> Result<TrackPageResponse, ApiError> {
        let mut items = self.store.tracks()?;
        if let Some(value) = normalized(query.q.as_deref()) {
            items.retain(|track| track_matches(track, &value));
        }
        if let Some(id) = query.artist_id.as_deref() {
            items.retain(|track| track.artist_id.as_deref() == Some(id));
        }
        if let Some(id) = query.album_id.as_deref() {
            items.retain(|track| track.album_id.as_deref() == Some(id));
        }
        if let Some(id) = query.album_artist_id.as_deref() {
            items.retain(|track| track.album_artist_id.as_deref() == Some(id));
        }
        if let Some(genre) = normalized(query.genre.as_deref()) {
            items.retain(|track| normalized(track.genre.as_deref()).as_deref() == Some(&genre));
        }
        Ok(track_page(items, query.limit, query.offset))
    }

    pub async fn track(&self, track_id: &str) -> Result<TrackResponse, ApiError> {
        self.store.track(track_id)
    }

    pub async fn local_path(&self, track_id: &str) -> Result<std::path::PathBuf, ApiError> {
        self.store.track_path(track_id)
    }

    pub async fn delete_track(&self, track_id: &str) -> Result<(), ApiError> {
        self.store.delete_track(track_id)
    }

    pub async fn artists(&self, query: &BrowseQuery) -> Result<ArtistPageResponse, ApiError> {
        let mut items = self.store.artists()?;
        if let Some(value) = normalized(query.q.as_deref()) {
            items.retain(|artist| artist.name.to_lowercase().contains(&value));
        }
        let total = items.len() as i64;
        let (items, limit, offset) = page(items, query.limit, query.offset);
        Ok(ArtistPageResponse {
            items,
            total,
            limit,
            offset,
        })
    }

    pub async fn artist(&self, artist_id: &str) -> Result<ArtistResponse, ApiError> {
        self.store
            .artists()?
            .into_iter()
            .find(|artist| artist.id == artist_id)
            .ok_or_else(|| not_found("artist"))
    }

    pub async fn patch_artist(
        &self,
        artist_id: &str,
        request: &PatchArtistRequest,
    ) -> Result<ArtistResponse, ApiError> {
        if let Some(name) = request.name.as_deref() {
            self.store.rename_artist(artist_id, name)
        } else {
            self.artist(artist_id).await
        }
    }

    pub async fn delete_artist(&self, artist_id: &str) -> Result<(), ApiError> {
        let tracks = self
            .tracks(&TrackQuery {
                artist_id: Some(artist_id.to_owned()),
                limit: Some(u32::MAX),
                ..Default::default()
            })
            .await?;
        for track in tracks.items {
            self.store.delete_track(&track.id)?;
        }
        Ok(())
    }

    pub async fn artist_albums(
        &self,
        artist_id: &str,
        query: &PageQuery,
    ) -> Result<AlbumPageResponse, ApiError> {
        let items = self
            .store
            .albums()?
            .into_iter()
            .filter(|album| album.album_artist_id == artist_id)
            .collect();
        Ok(album_page(items, query.limit, query.offset))
    }

    pub async fn artist_tracks(
        &self,
        artist_id: &str,
        query: &PageQuery,
    ) -> Result<TrackPageResponse, ApiError> {
        self.tracks(&TrackQuery {
            artist_id: Some(artist_id.to_owned()),
            limit: query.limit,
            offset: query.offset,
            ..Default::default()
        })
        .await
    }

    pub async fn artist_artwork(
        &self,
        artist_id: &str,
        range: Option<ByteRange>,
    ) -> Result<BinaryResponse, ApiError> {
        if let Some(artwork) = self.store.artwork("artist", artist_id)? {
            return Ok(binary_response(artwork.mime_type, artwork.data, range));
        }
        let track = self
            .store
            .tracks()?
            .into_iter()
            .find(|track| track.artist_id.as_deref() == Some(artist_id) && track.has_artwork)
            .ok_or_else(|| not_found("artist artwork"))?;
        self.artwork(&track.id, range).await
    }

    pub async fn replace_artist_artwork(
        &self,
        artist_id: &str,
        _filename: String,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<ArtworkResponse, ApiError> {
        let artwork = self
            .store
            .set_artwork("artist", artist_id, content_type, bytes)?;
        Ok(ArtworkResponse {
            content_type: artwork.content_type,
            size_bytes: artwork.size_bytes,
            updated_at: artwork.updated_at,
        })
    }
    pub async fn delete_artist_artwork(&self, artist_id: &str) -> Result<(), ApiError> {
        self.store.delete_artwork("artist", artist_id)
    }

    pub async fn albums(&self, query: &BrowseQuery) -> Result<AlbumPageResponse, ApiError> {
        let mut items = self.store.albums()?;
        if let Some(value) = normalized(query.q.as_deref()) {
            items.retain(|album| {
                album.name.to_lowercase().contains(&value)
                    || album.album_artist_name.to_lowercase().contains(&value)
            });
        }
        if let Some(id) = query.artist_id.as_deref() {
            items.retain(|album| album.album_artist_id == id);
        }
        Ok(album_page(items, query.limit, query.offset))
    }

    pub async fn album(&self, album_id: &str) -> Result<AlbumResponse, ApiError> {
        self.store
            .albums()?
            .into_iter()
            .find(|album| album.id == album_id)
            .ok_or_else(|| not_found("album"))
    }

    pub async fn patch_album(
        &self,
        album_id: &str,
        request: &PatchAlbumRequest,
    ) -> Result<AlbumResponse, ApiError> {
        self.store.patch_album(
            album_id,
            request.name.as_deref(),
            request.release_year,
            request.album_artist_id.as_deref(),
        )
    }

    pub async fn delete_album(&self, album_id: &str) -> Result<(), ApiError> {
        let tracks = self
            .tracks(&TrackQuery {
                album_id: Some(album_id.to_owned()),
                limit: Some(u32::MAX),
                ..Default::default()
            })
            .await?;
        for track in tracks.items {
            self.store.delete_track(&track.id)?;
        }
        Ok(())
    }

    pub async fn album_tracks(
        &self,
        album_id: &str,
        query: &PageQuery,
    ) -> Result<TrackPageResponse, ApiError> {
        self.tracks(&TrackQuery {
            album_id: Some(album_id.to_owned()),
            limit: query.limit,
            offset: query.offset,
            ..Default::default()
        })
        .await
    }

    pub async fn album_artwork(
        &self,
        album_id: &str,
        range: Option<ByteRange>,
    ) -> Result<BinaryResponse, ApiError> {
        if let Some(artwork) = self.store.artwork("album", album_id)? {
            return Ok(binary_response(artwork.mime_type, artwork.data, range));
        }
        let track = self
            .store
            .tracks()?
            .into_iter()
            .find(|track| track.album_id.as_deref() == Some(album_id) && track.has_artwork)
            .ok_or_else(|| not_found("album artwork"))?;
        self.artwork(&track.id, range).await
    }

    pub async fn replace_album_artwork(
        &self,
        album_id: &str,
        _filename: String,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<ArtworkResponse, ApiError> {
        let artwork = self
            .store
            .set_artwork("album", album_id, content_type, bytes)?;
        Ok(ArtworkResponse {
            content_type: artwork.content_type,
            size_bytes: artwork.size_bytes,
            updated_at: artwork.updated_at,
        })
    }
    pub async fn delete_album_artwork(&self, album_id: &str) -> Result<(), ApiError> {
        self.store.delete_artwork("album", album_id)
    }

    pub async fn genres(&self, query: &BrowseQuery) -> Result<GenrePageResponse, ApiError> {
        let mut counts = std::collections::BTreeMap::<String, i64>::new();
        for track in self.store.tracks()? {
            if let Some(genre) = track.genre {
                *counts.entry(genre).or_default() += 1;
            }
        }
        let mut items = counts
            .into_iter()
            .map(|(name, track_count)| GenreResponse {
                name,
                track_count,
                duration_ms: 0,
            })
            .collect::<Vec<_>>();
        if let Some(value) = normalized(query.q.as_deref()) {
            items.retain(|genre| genre.name.to_lowercase().contains(&value));
        }
        let total = items.len() as i64;
        let (items, limit, offset) = page(items, query.limit, query.offset);
        Ok(GenrePageResponse {
            items,
            total,
            limit,
            offset,
        })
    }

    pub async fn metadata(&self, track_id: &str) -> Result<TrackMetadataResponse, ApiError> {
        self.store.track_metadata(track_id)
    }

    pub async fn replace_metadata(
        &self,
        track_id: &str,
        request: &ReplaceTrackMetadataRequest,
    ) -> Result<TrackMetadataResponse, ApiError> {
        self.store.replace_metadata(
            track_id,
            MetadataUpdate {
                title: request.title.clone(),
                artist: request.artist.clone(),
                album_artist: request.album_artist.clone(),
                album: request.album.clone(),
                genre: request.genre.clone(),
                composer: request.composer.clone(),
                track_number: request.track_number,
                disc_number: request.disc_number,
                release_year: request.release_year,
                lyrics: request.lyrics.clone(),
            },
        )
    }

    pub async fn patch_metadata(
        &self,
        track_id: &str,
        request: &PatchTrackMetadataRequest,
    ) -> Result<TrackMetadataResponse, ApiError> {
        let current = self.store.track_metadata(track_id)?;
        self.store.replace_metadata(
            track_id,
            MetadataUpdate {
                title: request.title.clone().unwrap_or(current.title),
                artist: request.artist.clone().unwrap_or(current.artist),
                album_artist: request.album_artist.clone().unwrap_or(current.album_artist),
                album: request.album.clone().unwrap_or(current.album),
                genre: request.genre.clone().unwrap_or(current.genre),
                composer: request.composer.clone().unwrap_or(current.composer),
                track_number: request.track_number.unwrap_or(current.track_number),
                disc_number: request.disc_number.unwrap_or(current.disc_number),
                release_year: request.release_year.unwrap_or(current.release_year),
                lyrics: request.lyrics.clone().unwrap_or(current.lyrics),
            },
        )
    }

    pub async fn lyrics(&self, track_id: &str) -> Result<LyricsResponse, ApiError> {
        let metadata = self.store.track_metadata(track_id)?;
        Ok(LyricsResponse {
            lyrics: metadata.lyrics,
            updated_at: metadata.updated_at,
        })
    }

    pub async fn replace_lyrics(
        &self,
        track_id: &str,
        lyrics: String,
    ) -> Result<LyricsResponse, ApiError> {
        let lyrics = self.store.set_lyrics(track_id, Some(lyrics))?;
        Ok(LyricsResponse {
            lyrics: Some(lyrics),
            updated_at: now_millis(),
        })
    }

    pub async fn delete_lyrics(&self, track_id: &str) -> Result<LyricsResponse, ApiError> {
        self.store.set_lyrics(track_id, None)?;
        Ok(LyricsResponse {
            lyrics: None,
            updated_at: now_millis(),
        })
    }

    pub async fn artwork(
        &self,
        track_id: &str,
        range: Option<ByteRange>,
    ) -> Result<BinaryResponse, ApiError> {
        let artwork = self.store.track_artwork(track_id)?;
        Ok(binary_response(artwork.mime_type, artwork.data, range))
    }

    pub async fn artwork_metadata(&self, track_id: &str) -> Result<BinaryMetadata, ApiError> {
        Ok(self.artwork(track_id, None).await?.metadata)
    }

    pub async fn replace_artwork(
        &self,
        track_id: &str,
        _filename: String,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<ArtworkResponse, ApiError> {
        let artwork = self
            .store
            .set_artwork("track", track_id, content_type, bytes)?;
        Ok(ArtworkResponse {
            content_type: artwork.content_type,
            size_bytes: artwork.size_bytes,
            updated_at: artwork.updated_at,
        })
    }
    pub async fn delete_artwork(&self, track_id: &str) -> Result<(), ApiError> {
        self.store.delete_artwork("track", track_id)
    }
    pub async fn favorite(&self, track_id: &str) -> Result<TrackResponse, ApiError> {
        self.store.set_favorite(track_id, true)
    }
    pub async fn unfavorite(&self, track_id: &str) -> Result<TrackResponse, ApiError> {
        self.store.set_favorite(track_id, false)
    }

    pub async fn favorites(&self, query: &PageQuery) -> Result<TrackPageResponse, ApiError> {
        let items = self
            .store
            .tracks()?
            .into_iter()
            .filter(|track| track.favorite)
            .collect();
        Ok(track_page(items, query.limit, query.offset))
    }

    pub async fn stream(
        &self,
        track_id: &str,
        range: Option<ByteRange>,
    ) -> Result<BinaryResponse, ApiError> {
        let path = self.store.track_path(track_id)?;
        let bytes =
            fs::read(&path).map_err(|error| ApiError::InvalidResponse(error.to_string()))?;
        let content_type = content_type(&path).to_owned();
        Ok(binary_response(content_type, bytes, range))
    }

    pub async fn stream_metadata(&self, track_id: &str) -> Result<BinaryMetadata, ApiError> {
        let path = self.store.track_path(track_id)?;
        let length = fs::metadata(&path)
            .map_err(|error| ApiError::InvalidResponse(error.to_string()))?
            .len();
        Ok(BinaryMetadata {
            status: 200,
            content_type: content_type(&path).into(),
            content_length: Some(length),
            content_range: None,
            accept_ranges: Some("bytes".into()),
            etag: None,
            last_modified: None,
        })
    }
}

fn track_page(
    items: Vec<TrackResponse>,
    limit: Option<u32>,
    offset: Option<u64>,
) -> TrackPageResponse {
    let total = items.len() as i64;
    let (items, limit, offset) = page(items, limit, offset);
    TrackPageResponse {
        items,
        total,
        limit,
        offset,
    }
}

fn album_page(
    items: Vec<AlbumResponse>,
    limit: Option<u32>,
    offset: Option<u64>,
) -> AlbumPageResponse {
    let total = items.len() as i64;
    let (items, limit, offset) = page(items, limit, offset);
    AlbumPageResponse {
        items,
        total,
        limit,
        offset,
    }
}

fn page<T>(items: Vec<T>, limit: Option<u32>, offset: Option<u64>) -> (Vec<T>, u32, u64) {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(50);
    let items = items
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    (items, limit, offset)
}

fn normalized(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}
fn track_matches(track: &TrackResponse, query: &str) -> bool {
    [
        track.title.as_deref(),
        track.artist.as_deref(),
        track.album.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(query))
}
fn not_found(resource: &str) -> ApiError {
    ApiError::Http {
        status: 404,
        code: "not_found".into(),
        message: format!("{resource} was not found"),
        request_id: None,
    }
}
fn now_millis() -> i64 {
    crate::utils::utc_now_millis()
}
fn content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

fn binary_response(
    content_type: String,
    bytes: Vec<u8>,
    range: Option<ByteRange>,
) -> BinaryResponse {
    let full_length = bytes.len() as u64;
    let (bytes, status, content_range) = match range {
        Some(ByteRange::From(start)) if start < full_length => (
            bytes[start as usize..].to_vec(),
            206,
            Some(format!("bytes {start}-{}/{full_length}", full_length - 1)),
        ),
        Some(ByteRange::Inclusive { start, end }) if start < full_length => {
            let end = end.min(full_length - 1);
            (
                bytes[start as usize..=end as usize].to_vec(),
                206,
                Some(format!("bytes {start}-{end}/{full_length}")),
            )
        }
        Some(ByteRange::Suffix(length)) if length > 0 => {
            let start = full_length.saturating_sub(length);
            (
                bytes[start as usize..].to_vec(),
                206,
                Some(format!(
                    "bytes {start}-{}/{full_length}",
                    full_length.saturating_sub(1)
                )),
            )
        }
        _ => (bytes, 200, None),
    };
    BinaryResponse {
        metadata: BinaryMetadata {
            status,
            content_type,
            content_length: Some(bytes.len() as u64),
            content_range,
            accept_ranges: Some("bytes".into()),
            etag: None,
            last_modified: None,
        },
        bytes,
    }
}
