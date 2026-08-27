use std::{
    collections::{BTreeMap, HashMap},
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    audio_metadata::{AudioMetadata, EmbeddedArtwork, read_audio_metadata},
    services::api::{
        AlbumResponse, ApiError, ArtistResponse, PlaylistDetailResponse, PlaylistItemResponse,
        PlaylistResponse, TrackCredit, TrackMetadataResponse, TrackResponse,
    },
    utils::atomic_write,
};

#[derive(Clone)]
pub struct LocalStore {
    path: PathBuf,
    data: Arc<RwLock<LocalData>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct LocalData {
    #[serde(default)]
    tracks: Vec<LocalTrackRecord>,
    #[serde(default)]
    playlists: Vec<LocalPlaylistRecord>,
    #[serde(default)]
    artwork: HashMap<String, LocalArtworkRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LocalArtworkRecord {
    path: PathBuf,
    content_type: String,
    updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LocalTrackRecord {
    path: PathBuf,
    #[serde(default = "now_seconds")]
    added_at: u64,
    #[serde(default)]
    favorite: bool,
    #[serde(default)]
    metadata: MetadataOverrides,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct MetadataOverrides {
    title: Option<String>,
    artist: Option<String>,
    album_artist: Option<String>,
    album: Option<String>,
    genre: Option<String>,
    composer: Option<String>,
    track_number: Option<i64>,
    disc_number: Option<i64>,
    release_year: Option<i64>,
    lyrics: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LocalPlaylistRecord {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tracks: Vec<PathBuf>,
    #[serde(default = "now_seconds")]
    created_at: u64,
    #[serde(default = "now_seconds")]
    updated_at: u64,
    #[serde(default)]
    cover_path: Option<PathBuf>,
}

#[derive(Clone)]
struct ResolvedTrack {
    record: LocalTrackRecord,
    metadata: AudioMetadata,
    response: TrackResponse,
}

static STORE: OnceLock<Result<LocalStore, String>> = OnceLock::new();

impl LocalStore {
    pub fn shared() -> Result<Self, ApiError> {
        STORE
            .get_or_init(Self::load_default)
            .clone()
            .map_err(ApiError::InvalidResponse)
    }

    fn load_default() -> Result<Self, String> {
        let path = default_config_path()?;
        let data = match fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents)
                .map_err(|error| format!("Unable to read local music config: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => LocalData::default(),
            Err(error) => return Err(format!("Unable to open {}: {error}", path.display())),
        };
        Ok(Self {
            path,
            data: Arc::new(RwLock::new(data)),
        })
    }

    pub fn add_paths(&self, paths: Vec<PathBuf>) -> Result<usize, ApiError> {
        let mut data = self.write()?;
        let mut added = 0;
        for path in paths {
            if !path.is_file() || !is_supported_audio(&path) {
                continue;
            }
            let path = path.canonicalize().unwrap_or(path);
            if data.tracks.iter().any(|track| track.path == path) {
                continue;
            }
            data.tracks.push(LocalTrackRecord {
                path,
                added_at: now_seconds(),
                favorite: false,
                metadata: MetadataOverrides::default(),
            });
            added += 1;
        }
        if added > 0 {
            self.save_locked(&data)?;
        }
        Ok(added)
    }

    pub fn tracks(&self) -> Result<Vec<TrackResponse>, ApiError> {
        Ok(self
            .resolved_tracks()?
            .into_iter()
            .map(|item| item.response)
            .collect())
    }

    pub fn track(&self, id: &str) -> Result<TrackResponse, ApiError> {
        self.resolved_track(id).map(|track| track.response)
    }

    pub fn track_path(&self, id: &str) -> Result<PathBuf, ApiError> {
        Ok(self.resolved_track(id)?.record.path)
    }

    pub fn track_artwork(&self, id: &str) -> Result<EmbeddedArtwork, ApiError> {
        if let Some(artwork) = self.artwork("track", id)? {
            return Ok(artwork);
        }
        self.resolved_track(id)?
            .metadata
            .artwork
            .ok_or_else(|| not_found("artwork"))
    }

    pub fn artwork(&self, kind: &str, id: &str) -> Result<Option<EmbeddedArtwork>, ApiError> {
        let key = artwork_key(kind, id);
        let record = self.read()?.artwork.get(&key).cloned();
        let Some(record) = record else {
            return Ok(None);
        };
        let data = fs::read(&record.path).map_err(io_error)?;
        Ok(Some(EmbeddedArtwork {
            mime_type: record.content_type,
            data,
        }))
    }

    pub fn set_artwork(
        &self,
        kind: &str,
        id: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<LocalArtwork, ApiError> {
        let directory = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("artwork");
        fs::create_dir_all(&directory).map_err(io_error)?;
        let extension = image_extension(content_type);
        let path = directory.join(format!("{}-{id}.{extension}", stable_id(&kind)));
        fs::write(&path, &bytes).map_err(io_error)?;
        let updated_at = now_seconds() as i64 * 1000;
        let mut data = self.write()?;
        data.artwork.insert(
            artwork_key(kind, id),
            LocalArtworkRecord {
                path: path.clone(),
                content_type: content_type.to_owned(),
                updated_at,
            },
        );
        self.save_locked(&data)?;
        Ok(LocalArtwork {
            path,
            content_type: content_type.to_owned(),
            size_bytes: bytes.len() as i64,
            updated_at,
        })
    }

    pub fn delete_artwork(&self, kind: &str, id: &str) -> Result<(), ApiError> {
        let mut data = self.write()?;
        let record = data.artwork.remove(&artwork_key(kind, id));
        self.save_locked(&data)?;
        drop(data);
        if let Some(record) = record {
            match fs::remove_file(record.path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        Ok(())
    }

    pub fn track_metadata(&self, id: &str) -> Result<TrackMetadataResponse, ApiError> {
        let track = self.resolved_track(id)?;
        Ok(TrackMetadataResponse {
            title: track.response.title,
            artist: track.response.artist,
            album_artist: track.response.album_artist,
            album: track.response.album,
            genre: track.response.genre,
            composer: track.response.composer,
            track_number: track.response.track_number,
            disc_number: track.response.disc_number,
            release_year: track.response.release_year,
            lyrics: track.record.metadata.lyrics.or(track.metadata.lyrics),
            updated_at: track.response.updated_at,
        })
    }

    pub fn replace_metadata(
        &self,
        id: &str,
        metadata: MetadataUpdate,
    ) -> Result<TrackMetadataResponse, ApiError> {
        let path = self.track_path(id)?;
        let mut data = self.write()?;
        let record = data
            .tracks
            .iter_mut()
            .find(|track| same_path(&track.path, &path))
            .ok_or_else(|| not_found("track"))?;
        record.metadata = metadata.into();
        self.save_locked(&data)?;
        drop(data);
        self.track_metadata(id)
    }

    pub fn set_lyrics(&self, id: &str, lyrics: Option<String>) -> Result<String, ApiError> {
        let path = self.track_path(id)?;
        let mut data = self.write()?;
        let record = data
            .tracks
            .iter_mut()
            .find(|track| same_path(&track.path, &path))
            .ok_or_else(|| not_found("track"))?;
        record.metadata.lyrics = lyrics.clone();
        self.save_locked(&data)?;
        Ok(lyrics.unwrap_or_default())
    }

    pub fn set_favorite(&self, id: &str, favorite: bool) -> Result<TrackResponse, ApiError> {
        let path = self.track_path(id)?;
        let mut data = self.write()?;
        let record = data
            .tracks
            .iter_mut()
            .find(|track| same_path(&track.path, &path))
            .ok_or_else(|| not_found("track"))?;
        record.favorite = favorite;
        self.save_locked(&data)?;
        drop(data);
        self.track(id)
    }

    pub fn delete_track(&self, id: &str) -> Result<(), ApiError> {
        let path = self.track_path(id)?;
        let mut data = self.write()?;
        data.tracks.retain(|track| !same_path(&track.path, &path));
        for playlist in &mut data.playlists {
            playlist.tracks.retain(|track| !same_path(track, &path));
        }
        self.save_locked(&data)
    }

    pub fn artists(&self) -> Result<Vec<ArtistResponse>, ApiError> {
        let tracks = self.resolved_tracks()?;
        let mut artists: BTreeMap<String, Vec<&ResolvedTrack>> = BTreeMap::new();
        for track in &tracks {
            let name = track
                .response
                .artist
                .clone()
                .unwrap_or_else(|| "Unknown Artist".into());
            artists.entry(name).or_default().push(track);
        }
        Ok(artists
            .into_iter()
            .map(|(name, tracks)| {
                let id = artist_id(&name);
                let artwork_url = if self.artwork("artist", &id).ok().flatten().is_some() {
                    Some(format!("local://artist/{id}/artwork"))
                } else {
                    tracks
                        .iter()
                        .find(|track| track.response.has_artwork)
                        .map(|track| format!("local://track/{}/artwork", track.response.id))
                };
                ArtistResponse {
                    id,
                    sort_name: name.to_lowercase(),
                    name,
                    bio: None,
                    album_count: tracks
                        .iter()
                        .filter_map(|track| track.response.album.as_deref())
                        .collect::<std::collections::HashSet<_>>()
                        .len() as i64,
                    track_count: tracks.len() as i64,
                    duration_ms: tracks
                        .iter()
                        .filter_map(|track| track.response.duration_ms)
                        .sum(),
                    artwork_url,
                    created_at: tracks
                        .iter()
                        .map(|track| track.response.created_at)
                        .min()
                        .unwrap_or(0),
                    updated_at: tracks
                        .iter()
                        .map(|track| track.response.updated_at)
                        .max()
                        .unwrap_or(0),
                }
            })
            .collect())
    }

    pub fn rename_artist(&self, id: &str, name: &str) -> Result<ArtistResponse, ApiError> {
        let mut data = self.write()?;
        let mut changed = false;
        for record in &mut data.tracks {
            let resolved = resolve_track(record.clone());
            if resolved.response.artist_id.as_deref() == Some(id) {
                record.metadata.artist = Some(name.to_owned());
                changed = true;
            }
            if resolved.response.album_artist_id.as_deref() == Some(id) {
                record.metadata.album_artist = Some(name.to_owned());
                changed = true;
            }
        }
        if !changed {
            return Err(not_found("artist"));
        }
        self.save_locked(&data)?;
        drop(data);
        self.artists()?
            .into_iter()
            .find(|artist| artist.name == name)
            .ok_or_else(|| not_found("artist"))
    }

    pub fn albums(&self) -> Result<Vec<AlbumResponse>, ApiError> {
        let tracks = self.resolved_tracks()?;
        let mut albums: BTreeMap<(String, String), Vec<&ResolvedTrack>> = BTreeMap::new();
        for track in &tracks {
            let Some(album) = track.response.album.clone() else {
                continue;
            };
            let artist = track
                .response
                .album_artist
                .clone()
                .or_else(|| track.response.artist.clone())
                .unwrap_or_else(|| "Unknown Artist".into());
            albums.entry((artist, album)).or_default().push(track);
        }
        Ok(albums
            .into_iter()
            .map(|((artist, name), tracks)| {
                let id = album_id(&artist, &name);
                let artwork_url = if self.artwork("album", &id).ok().flatten().is_some() {
                    Some(format!("local://album/{id}/artwork"))
                } else {
                    tracks
                        .iter()
                        .find(|track| track.response.has_artwork)
                        .map(|track| format!("local://track/{}/artwork", track.response.id))
                };
                AlbumResponse {
                    id,
                    sort_name: name.to_lowercase(),
                    album_artist_id: artist_id(&artist),
                    album_artist_name: artist,
                    name,
                    release_year: tracks
                        .iter()
                        .filter_map(|track| track.response.release_year)
                        .next(),
                    track_count: tracks.len() as i64,
                    duration_ms: tracks
                        .iter()
                        .filter_map(|track| track.response.duration_ms)
                        .sum(),
                    artwork_url,
                    created_at: tracks
                        .iter()
                        .map(|track| track.response.created_at)
                        .min()
                        .unwrap_or(0),
                    updated_at: tracks
                        .iter()
                        .map(|track| track.response.updated_at)
                        .max()
                        .unwrap_or(0),
                }
            })
            .collect())
    }

    pub fn patch_album(
        &self,
        id: &str,
        name: Option<&str>,
        release_year: Option<Option<i64>>,
        album_artist_id: Option<&str>,
    ) -> Result<AlbumResponse, ApiError> {
        let artist_name = album_artist_id.and_then(|artist_id| {
            self.artists()
                .ok()?
                .into_iter()
                .find(|artist| artist.id == artist_id)
                .map(|artist| artist.name)
        });
        let mut data = self.write()?;
        let mut changed = false;
        for record in &mut data.tracks {
            let resolved = resolve_track(record.clone());
            if resolved.response.album_id.as_deref() != Some(id) {
                continue;
            }
            if let Some(name) = name {
                record.metadata.album = Some(name.to_owned());
            }
            if let Some(release_year) = release_year {
                record.metadata.release_year = release_year;
            }
            if let Some(artist) = artist_name.as_ref() {
                record.metadata.album_artist = Some(artist.clone());
            }
            changed = true;
        }
        if !changed {
            return Err(not_found("album"));
        }
        self.save_locked(&data)?;
        drop(data);
        let albums = self.albums()?;
        albums
            .into_iter()
            .find(|album| {
                name.is_none_or(|name| album.name == name)
                    && artist_name
                        .as_ref()
                        .is_none_or(|artist| &album.album_artist_name == artist)
            })
            .ok_or_else(|| not_found("album"))
    }

    pub fn playlists(&self) -> Result<Vec<PlaylistResponse>, ApiError> {
        let data = self.read()?;
        Ok(data
            .playlists
            .iter()
            .enumerate()
            .map(|(position, playlist)| {
                let mut response = playlist_response(playlist, position);
                if let Some(artwork) = data.artwork.get(&artwork_key("playlist", &playlist.id)) {
                    response.cover_url = Some(format!(
                        "local://playlist/{}/cover?v={}",
                        playlist.id, artwork.updated_at
                    ));
                }
                response
            })
            .collect())
    }

    pub fn create_playlist(
        &self,
        name: String,
        description: Option<String>,
    ) -> Result<PlaylistResponse, ApiError> {
        let mut data = self.write()?;
        let now = now_seconds();
        let playlist = LocalPlaylistRecord {
            id: format!(
                "playlist-{}",
                stable_id(&(name.clone(), now, data.playlists.len()))
            ),
            name,
            description,
            tracks: Vec::new(),
            created_at: now,
            updated_at: now,
            cover_path: None,
        };
        data.playlists.push(playlist.clone());
        self.save_locked(&data)?;
        Ok(playlist_response(&playlist, data.playlists.len() - 1))
    }

    pub fn replace_playlist(
        &self,
        id: &str,
        name: String,
        description: Option<String>,
    ) -> Result<PlaylistResponse, ApiError> {
        let mut data = self.write()?;
        let position = data
            .playlists
            .iter()
            .position(|item| item.id == id)
            .ok_or_else(|| not_found("playlist"))?;
        let playlist = &mut data.playlists[position];
        playlist.name = name;
        playlist.description = description;
        playlist.updated_at = now_seconds();
        let response = playlist_response(playlist, position);
        self.save_locked(&data)?;
        Ok(response)
    }

    pub fn delete_playlist(&self, id: &str) -> Result<(), ApiError> {
        let mut data = self.write()?;
        data.playlists.retain(|playlist| playlist.id != id);
        self.save_locked(&data)
    }

    pub fn reorder_playlists(&self, ids: &[String]) -> Result<Vec<PlaylistResponse>, ApiError> {
        let mut data = self.write()?;
        let mut old = std::mem::take(&mut data.playlists);
        let mut ordered = Vec::with_capacity(old.len());
        for id in ids {
            if let Some(position) = old.iter().position(|playlist| &playlist.id == id) {
                ordered.push(old.remove(position));
            }
        }
        ordered.extend(old);
        data.playlists = ordered;
        self.save_locked(&data)?;
        drop(data);
        self.playlists()
    }

    pub fn playlist(&self, id: &str) -> Result<PlaylistDetailResponse, ApiError> {
        let data = self.read()?;
        let position = data
            .playlists
            .iter()
            .position(|playlist| playlist.id == id)
            .ok_or_else(|| not_found("playlist"))?;
        let playlist = data.playlists[position].clone();
        drop(data);
        let by_path: HashMap<PathBuf, TrackResponse> = self
            .resolved_tracks()?
            .into_iter()
            .map(|track| (canonical(&track.record.path), track.response))
            .collect();
        let items = playlist
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(position, path)| {
                let track = by_path.get(&canonical(path))?.clone();
                Some(PlaylistItemResponse {
                    id: format!("{}:{}", playlist.id, track.id),
                    position: position as i64,
                    added_at: playlist.updated_at as i64 * 1000,
                    track,
                })
            })
            .collect();
        Ok(PlaylistDetailResponse {
            playlist: playlist_response(&playlist, position),
            items,
        })
    }

    pub fn add_playlist_item(
        &self,
        playlist_id: &str,
        track_id: &str,
    ) -> Result<PlaylistItemResponse, ApiError> {
        let path = self.track_path(track_id)?;
        let mut data = self.write()?;
        let playlist = data
            .playlists
            .iter_mut()
            .find(|playlist| playlist.id == playlist_id)
            .ok_or_else(|| not_found("playlist"))?;
        if !playlist.tracks.iter().any(|item| same_path(item, &path)) {
            playlist.tracks.push(path);
            playlist.updated_at = now_seconds();
        }
        self.save_locked(&data)?;
        drop(data);
        self.playlist(playlist_id)?
            .items
            .into_iter()
            .find(|item| item.track.id == track_id)
            .ok_or_else(|| not_found("playlist item"))
    }

    pub fn remove_playlist_item(&self, playlist_id: &str, item_id: &str) -> Result<(), ApiError> {
        let track_id = item_id.rsplit(':').next().unwrap_or(item_id);
        let path = self.track_path(track_id)?;
        let mut data = self.write()?;
        let playlist = data
            .playlists
            .iter_mut()
            .find(|playlist| playlist.id == playlist_id)
            .ok_or_else(|| not_found("playlist"))?;
        playlist.tracks.retain(|item| !same_path(item, &path));
        playlist.updated_at = now_seconds();
        self.save_locked(&data)
    }

    pub fn reorder_playlist_items(
        &self,
        playlist_id: &str,
        item_ids: &[String],
    ) -> Result<Vec<PlaylistItemResponse>, ApiError> {
        let detail = self.playlist(playlist_id)?;
        let paths: HashMap<String, PathBuf> = detail
            .items
            .iter()
            .filter_map(|item| {
                self.track_path(&item.track.id)
                    .ok()
                    .map(|path| (item.id.clone(), path))
            })
            .collect();
        let mut data = self.write()?;
        let playlist = data
            .playlists
            .iter_mut()
            .find(|playlist| playlist.id == playlist_id)
            .ok_or_else(|| not_found("playlist"))?;
        let mut reordered = item_ids
            .iter()
            .filter_map(|id| paths.get(id).cloned())
            .collect::<Vec<_>>();
        let remaining = playlist
            .tracks
            .iter()
            .filter(|path| !reordered.iter().any(|item| same_path(item, path)))
            .cloned()
            .collect::<Vec<_>>();
        reordered.extend(remaining);
        playlist.tracks = reordered;
        playlist.updated_at = now_seconds();
        self.save_locked(&data)?;
        drop(data);
        Ok(self.playlist(playlist_id)?.items)
    }

    fn resolved_track(&self, id: &str) -> Result<ResolvedTrack, ApiError> {
        self.resolved_tracks()?
            .into_iter()
            .find(|track| track.response.id == id)
            .ok_or_else(|| not_found("track"))
    }

    fn resolved_tracks(&self) -> Result<Vec<ResolvedTrack>, ApiError> {
        let (records, artwork) = {
            let data = self.read()?;
            (data.tracks.clone(), data.artwork.clone())
        };
        Ok(records
            .into_iter()
            .filter(|record| record.path.is_file())
            .map(|record| {
                let mut track = resolve_track(record);
                if artwork.contains_key(&artwork_key("track", &track.response.id)) {
                    track.response.has_artwork = true;
                }
                track
            })
            .collect())
    }

    fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, LocalData>, ApiError> {
        self.data
            .read()
            .map_err(|_| ApiError::InvalidResponse("local library lock is poisoned".into()))
    }

    fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, LocalData>, ApiError> {
        self.data
            .write()
            .map_err(|_| ApiError::InvalidResponse("local library lock is poisoned".into()))
    }

    fn save_locked(&self, data: &LocalData) -> Result<(), ApiError> {
        let contents = toml::to_string_pretty(data)
            .map_err(|error| ApiError::InvalidResponse(error.to_string()))?;
        atomic_write(&self.path, contents.as_bytes()).map_err(io_error)
    }
}

#[derive(Clone, Default)]
pub struct MetadataUpdate {
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

pub struct LocalArtwork {
    pub path: PathBuf,
    pub content_type: String,
    pub size_bytes: i64,
    pub updated_at: i64,
}

impl From<MetadataUpdate> for MetadataOverrides {
    fn from(value: MetadataUpdate) -> Self {
        Self {
            title: value.title,
            artist: value.artist,
            album_artist: value.album_artist,
            album: value.album,
            genre: value.genre,
            composer: value.composer,
            track_number: value.track_number,
            disc_number: value.disc_number,
            release_year: value.release_year,
            lyrics: value.lyrics,
        }
    }
}

fn resolve_track(record: LocalTrackRecord) -> ResolvedTrack {
    let metadata = read_audio_metadata(&record.path).unwrap_or_default();
    let fallback_title = record
        .path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown Song")
        .to_owned();
    let extension = record
        .path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let file_metadata = fs::metadata(&record.path).ok();
    let created_at = (record.added_at as i64).saturating_mul(1000);
    let artist = record
        .metadata
        .artist
        .clone()
        .or_else(|| metadata.artist.clone());
    let album_artist = record
        .metadata
        .album_artist
        .clone()
        .or_else(|| artist.clone());
    let album = record
        .metadata
        .album
        .clone()
        .or_else(|| metadata.album.clone());
    let response = TrackResponse {
        id: track_id(&record.path),
        title: record
            .metadata
            .title
            .clone()
            .or_else(|| metadata.title.clone())
            .or(Some(fallback_title)),
        artist_id: artist.as_deref().map(artist_id),
        album_id: album
            .as_deref()
            .map(|name| album_id(album_artist.as_deref().unwrap_or("Unknown Artist"), name)),
        album_artist_id: album_artist.as_deref().map(artist_id),
        artist,
        album_artist,
        album,
        genre: record.metadata.genre.clone(),
        composer: record.metadata.composer.clone(),
        track_number: record.metadata.track_number,
        disc_number: record.metadata.disc_number,
        release_year: record
            .metadata
            .release_year
            .or_else(|| metadata.year.as_deref().and_then(|year| year.parse().ok())),
        content_type: content_type(&extension).into(),
        container: extension.clone(),
        codec: extension,
        size_bytes: file_metadata
            .as_ref()
            .map(|value| value.len() as i64)
            .unwrap_or(0),
        duration_ms: None,
        sample_rate: None,
        channel_count: None,
        bit_depth: None,
        created_at,
        updated_at: file_metadata
            .and_then(|value| value.modified().ok())
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as i64)
            .unwrap_or(created_at),
        favorite: record.favorite,
        has_artwork: metadata.artwork.is_some(),
        credits: artist_name_credit(
            record
                .metadata
                .artist
                .as_deref()
                .or(metadata.artist.as_deref()),
        ),
    };
    ResolvedTrack {
        record,
        metadata,
        response,
    }
}

fn artist_name_credit(artist: Option<&str>) -> Vec<TrackCredit> {
    artist
        .map(|name| {
            vec![TrackCredit {
                artist_id: artist_id(name),
                name: name.into(),
                role: "primary".into(),
                position: 0,
            }]
        })
        .unwrap_or_default()
}

fn playlist_response(playlist: &LocalPlaylistRecord, position: usize) -> PlaylistResponse {
    PlaylistResponse {
        id: playlist.id.clone(),
        name: playlist.name.clone(),
        description: playlist.description.clone(),
        position: position as i64,
        track_count: playlist.tracks.len() as i64,
        duration_ms: 0,
        cover_url: playlist
            .cover_path
            .as_ref()
            .map(|_| format!("local://playlist/{}/cover", playlist.id)),
        created_at: playlist.created_at as i64 * 1000,
        updated_at: playlist.updated_at as i64 * 1000,
    }
}

fn default_config_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("ROMUS_LOCAL_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    dirs::config_dir()
        .map(|path| path.join("romus-local-player/library.toml"))
        .ok_or_else(|| "The system has no available local config directory".into())
}

fn track_id(path: &Path) -> String {
    format!("track-{}", stable_id(&canonical(path)))
}
fn artist_id(name: &str) -> String {
    format!("artist-{}", stable_id(&name.to_lowercase()))
}
fn album_id(artist: &str, album: &str) -> String {
    format!(
        "album-{}",
        stable_id(&(artist.to_lowercase(), album.to_lowercase()))
    )
}
fn stable_id(value: &impl Hash) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
fn same_path(left: &Path, right: &Path) -> bool {
    canonical(left) == canonical(right)
}
fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn io_error(error: std::io::Error) -> ApiError {
    ApiError::InvalidResponse(error.to_string())
}
fn not_found(resource: &str) -> ApiError {
    ApiError::Http {
        status: 404,
        code: "not_found".into(),
        message: format!("{resource} was not found"),
        request_id: None,
    }
}
fn content_type(extension: &str) -> &'static str {
    match extension {
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

fn artwork_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}
fn image_extension(content_type: &str) -> &'static str {
    match content_type {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    }
}
fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "mp3" | "m4a" | "aac" | "flac" | "wav" | "ogg" | "opus"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_local_paths_and_replaces_the_library_config() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let music_directory = directory.path().join("music");
        fs::create_dir_all(&music_directory).expect("music directory");
        let first = music_directory.join("first.mp3");
        let second = music_directory.join("second.flac");
        fs::write(&first, []).expect("first audio file");
        fs::write(&second, []).expect("second audio file");

        let config_path = directory.path().join("config/library.toml");
        let store = LocalStore {
            path: config_path.clone(),
            data: Arc::new(RwLock::new(LocalData::default())),
        };

        assert_eq!(store.add_paths(vec![first.clone()]).expect("first save"), 1);
        assert_eq!(
            store.add_paths(vec![second.clone()]).expect("second save"),
            1
        );

        let contents = fs::read_to_string(&config_path).expect("saved library config");
        let saved: LocalData = toml::from_str(&contents).expect("valid library config");
        assert_eq!(
            saved
                .tracks
                .into_iter()
                .map(|track| track.path)
                .collect::<Vec<_>>(),
            vec![
                first.canonicalize().expect("canonical first path"),
                second.canonicalize().expect("canonical second path"),
            ]
        );

        let config_entries = fs::read_dir(config_path.parent().expect("config parent"))
            .expect("config directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("config entries");
        assert_eq!(config_entries.len(), 1);
        assert_eq!(config_entries[0].path(), config_path);
    }
}
