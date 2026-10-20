use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{audio_metadata::read_audio_metadata, utils::atomic_write};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub file: PathBuf,
    pub artwork: Option<PathBuf>,
    pub lyrics: Option<String>,
    #[serde(default)]
    pub favorite: bool,
}

#[derive(Clone)]
pub(super) struct Library {
    root: PathBuf,
    pub tracks: Vec<Track>,
}

impl Library {
    pub fn load(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        let tracks = match fs::read(root.join("library.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("Cannot read saved library")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { root, tracks })
    }

    pub fn save(&self) -> Result<()> {
        atomic_write(
            &self.root.join("library.json"),
            &serde_json::to_vec(&self.tracks)?,
        )?;
        Ok(())
    }

    pub fn import(&mut self, name: &str, bytes: Vec<u8>) -> Result<()> {
        let extension = std::path::Path::new(name)
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "mp3" | "m4a" | "flac" | "ogg" | "wav" | "aac"
        ) {
            bail!("Unsupported audio file: {name}");
        }
        let id = uuid::Uuid::new_v4().to_string();
        let folder = self.root.join(&id);
        fs::create_dir(&folder)?;
        let result = (|| -> Result<Track> {
            let file = folder.join(format!("audio.{extension}"));
            atomic_write(&file, &bytes)?;
            let metadata = read_audio_metadata(&file)?;
            let artwork = metadata
                .artwork
                .map(|art| -> Result<PathBuf> {
                    let path = folder.join("cover");
                    atomic_write(&path, &art.data)?;
                    Ok(path)
                })
                .transpose()?;
            Ok(Track {
                id,
                title: metadata.title.unwrap_or_else(|| {
                    std::path::Path::new(name)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                }),
                artist: metadata.artist.unwrap_or_else(|| "Unknown Artist".into()),
                album: metadata.album.unwrap_or_else(|| "Unknown Album".into()),
                file,
                artwork,
                lyrics: metadata.lyrics,
                favorite: false,
            })
        })();
        match result {
            Ok(track) => {
                self.tracks.push(track);
                if let Err(error) = self.save() {
                    self.tracks.pop();
                    let _ = fs::remove_dir_all(&folder);
                    return Err(error);
                }
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_dir_all(folder);
                Err(error)
            }
        }
    }

    pub fn set_lyrics(&mut self, id: &str, bytes: Vec<u8>) -> Result<()> {
        let text = String::from_utf8(bytes).context("Lyrics must use UTF-8 encoding")?;
        let index = self
            .tracks
            .iter()
            .position(|track| track.id == id)
            .context("Song is no longer in the library")?;
        let previous = self.tracks[index].lyrics.replace(text);
        if let Err(error) = self.save() {
            self.tracks[index].lyrics = previous;
            return Err(error);
        }
        Ok(())
    }
}
