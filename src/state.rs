use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::Arc,
};

use gpui::{Image, ImageFormat, SharedString};
use gpui_media::{PlaybackState as MediaPlaybackState, PlaybackTimeline};

use crate::{
    audio_metadata::{AudioMetadata, EmbeddedArtwork},
    lyrics::LyricsDocument,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Search,
    Home,
    RecentlyAdded,
    Artists,
    Albums,
    Songs,
}

pub struct NavigationState {
    pub page: Page,
    pub search_query: SharedString,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            page: Page::Search,
            search_query: "".into(),
        }
    }
}

pub struct PlaybackState {
    pub status: MediaPlaybackState,
    pub timeline: PlaybackTimeline,
    pub muted: bool,
    pub volume: f64,
    pub track: TrackInfo,
    pub error: Option<SharedString>,
}

impl PlaybackState {
    pub fn new(track: TrackInfo) -> Self {
        Self {
            status: MediaPlaybackState::Loading,
            timeline: PlaybackTimeline::default(),
            muted: false,
            volume: 1.,
            track,
            error: None,
        }
    }

    pub fn is_playing(&self) -> bool {
        matches!(
            self.status,
            MediaPlaybackState::Loading | MediaPlaybackState::Playing
        )
    }
}

#[derive(Clone)]
pub struct TrackInfo {
    pub title: SharedString,
    pub artist: SharedString,
    pub album: Option<SharedString>,
    pub artwork: Option<Arc<ArtworkFile>>,
    pub lyrics: LyricsDocument,
}

pub struct ArtworkFile {
    pub url: SharedString,
    pub image: Arc<Image>,
    path: PathBuf,
}

impl Drop for ArtworkFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl TrackInfo {
    pub fn from_metadata(path: &Path, mut metadata: AudioMetadata) -> Self {
        let file_stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Unknown Track")
            .trim();
        let (file_artist, file_title) = file_stem
            .split_once(" - ")
            .map(|(artist, title)| (Some(artist.trim()), title.trim()))
            .unwrap_or((None, file_stem));

        let sidecar_lyrics = read_sidecar_lyrics(path);
        let lyrics = sidecar_lyrics.or_else(|| metadata.lyrics.take());
        let artwork = metadata.artwork.and_then(materialize_artwork);
        Self {
            title: metadata
                .title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or(file_title)
                .to_owned()
                .into(),
            artist: metadata
                .artist
                .as_deref()
                .filter(|artist| !artist.trim().is_empty())
                .or(file_artist)
                .unwrap_or("Unknown Artist")
                .to_owned()
                .into(),
            album: metadata.album.map(Into::into),
            artwork,
            lyrics: LyricsDocument::parse(lyrics.as_deref()),
        }
    }
}

fn read_sidecar_lyrics(audio_path: &Path) -> Option<String> {
    let lyrics_path = audio_path.with_extension("lrc");
    match fs::read_to_string(&lyrics_path) {
        Ok(lyrics) if !lyrics.trim().is_empty() => Some(lyrics),
        Ok(_) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!(
                "Unable to read sidecar lyrics from {}: {error}",
                lyrics_path.display()
            );
            None
        }
    }
}

fn materialize_artwork(artwork: EmbeddedArtwork) -> Option<Arc<ArtworkFile>> {
    let format = ImageFormat::from_mime_type(&artwork.mime_type).unwrap_or(ImageFormat::Jpeg);
    let extension = match format {
        ImageFormat::Png => "png",
        ImageFormat::Webp => "webp",
        _ => "jpg",
    };
    let path = std::env::temp_dir().join(format!(
        "gpui-apple-music-demo-cover-{}.{}",
        process::id(),
        extension
    ));
    if let Err(error) = fs::write(&path, &artwork.data) {
        eprintln!("Unable to expose embedded cover art: {error}");
        return None;
    }
    let image = Arc::new(Image::from_bytes(format, artwork.data));
    let url = format!("file://{}", path.display()).into();
    Some(Arc::new(ArtworkFile { url, image, path }))
}
