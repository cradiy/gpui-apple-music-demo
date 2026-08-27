use std::{
    fs,
    path::{Path, PathBuf},
    process,
    sync::Arc,
};

use crate::audio_metadata::{AudioMetadata, EmbeddedArtwork};
use gpui::{Image, ImageFormat, SharedString};

use super::lyrics::LyricsDocument;
use crate::ui::strings;

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

impl ArtworkFile {
    pub fn from_bytes(bytes: Vec<u8>, content_type: &str, key: &str) -> Option<Arc<Self>> {
        let format = ImageFormat::from_mime_type(content_type).unwrap_or(ImageFormat::Jpeg);
        let extension = match format {
            ImageFormat::Png => "png",
            ImageFormat::Webp => "webp",
            _ => "jpg",
        };
        let path = std::env::temp_dir().join(format!("romus-client-cover-{key}.{extension}"));
        if let Err(error) = fs::write(&path, &bytes) {
            log::warn!("unable to expose cover art: {error}");
            return None;
        }
        let image = Arc::new(Image::from_bytes(format, bytes));
        let url = format!("file://{}", path.display()).into();
        Some(Arc::new(Self { url, image, path }))
    }
}

impl TrackInfo {
    pub fn empty() -> Self {
        Self {
            title: strings::not_playing().into(),
            artist: strings::choose_a_track().into(),
            album: None,
            artwork: None,
            lyrics: LyricsDocument::parse(None),
        }
    }

    pub fn from_library(title: Option<&str>, artist: Option<&str>, album: Option<&str>) -> Self {
        Self {
            title: nonempty(title).unwrap_or(strings::untitled()).into(),
            artist: nonempty(artist).unwrap_or(strings::unknown_artist()).into(),
            album: nonempty(album).map(Into::into),
            artwork: None,
            lyrics: LyricsDocument::parse(None),
        }
    }

    /// Reads tags, sidecar `.lrc`, and embedded art from a local audio file.
    /// Kept for future local playback.
    pub fn from_metadata(path: &Path, mut metadata: AudioMetadata) -> Self {
        let file_stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(strings::unknown_track())
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
                .unwrap_or(strings::unknown_artist())
                .to_owned()
                .into(),
            album: metadata.album.map(Into::into),
            artwork,
            lyrics: LyricsDocument::parse(lyrics.as_deref()),
        }
    }
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn read_sidecar_lyrics(audio_path: &Path) -> Option<String> {
    let lyrics_path = audio_path.with_extension("lrc");
    match fs::read_to_string(&lyrics_path) {
        Ok(lyrics) if !lyrics.trim().is_empty() => Some(lyrics),
        Ok(_) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            log::warn!(
                "unable to read sidecar lyrics from {}: {error}",
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
        "romus-client-cover-{}.{}",
        process::id(),
        extension
    ));
    if let Err(error) = fs::write(&path, &artwork.data) {
        log::warn!("unable to expose embedded cover art: {error}");
        return None;
    }
    let image = Arc::new(Image::from_bytes(format, artwork.data));
    let url = format!("file://{}", path.display()).into();
    Some(Arc::new(ArtworkFile { url, image, path }))
}
