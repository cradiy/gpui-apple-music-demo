use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    services::{api::TrackResponse, session::AuthSession},
    state::{QueueOrigin, RepeatMode},
    utils::atomic_write,
};

const PLAYBACK_MEMORY_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PlaybackMemory {
    version: u32,
    server_url: String,
    user_id: String,
    pub tracks: Vec<TrackResponse>,
    pub index: usize,
    pub origin: Option<QueueOrigin>,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub position_ms: u64,
    pub volume: f64,
    pub muted: bool,
}

impl PlaybackMemory {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        session: &AuthSession,
        tracks: Vec<TrackResponse>,
        index: usize,
        origin: Option<QueueOrigin>,
        repeat: RepeatMode,
        shuffle: bool,
        position_ms: u64,
        volume: f64,
        muted: bool,
    ) -> Self {
        Self {
            version: PLAYBACK_MEMORY_VERSION,
            server_url: session.server_url.clone(),
            user_id: session.user.id.clone(),
            tracks,
            index,
            origin,
            repeat,
            shuffle,
            position_ms,
            volume: if volume.is_finite() {
                volume.clamp(0., 1.)
            } else {
                1.
            },
            muted,
        }
    }

    fn belongs_to(&self, session: &AuthSession) -> bool {
        self.version == PLAYBACK_MEMORY_VERSION
            && self.server_url == session.server_url
            && self.user_id == session.user.id
    }
}

#[derive(Clone, Debug)]
pub(super) struct PlaybackMemoryStore {
    path: PathBuf,
}

impl PlaybackMemoryStore {
    pub(super) fn load_default() -> Result<Self> {
        let state_directory = dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .context("unable to determine the user's local state directory")?;
        Ok(Self {
            path: state_directory.join("romus/playback.json"),
        })
    }

    pub(super) fn load(&self, session: &AuthSession) -> Result<Option<PlaybackMemory>> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to read playback memory {}", self.path.display())
                });
            }
        };
        let memory: PlaybackMemory = serde_json::from_str(&contents)
            .with_context(|| format!("failed to decode playback memory {}", self.path.display()))?;
        Ok(memory.belongs_to(session).then_some(memory))
    }

    pub(super) fn save(&self, memory: &PlaybackMemory) -> Result<()> {
        let bytes =
            serde_json::to_vec_pretty(memory).context("failed to serialize playback memory")?;
        atomic_write(&self.path, &bytes)
            .with_context(|| format!("failed to save playback memory {}", self.path.display()))
    }

    pub(super) fn clear(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!("failed to remove playback memory {}", self.path.display())
            }),
        }
    }
}
