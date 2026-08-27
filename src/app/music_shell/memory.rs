use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{MusicShell, PlaybackPanel};
use crate::{
    services::session::{AuthSession, SessionStore},
    utils::atomic_write,
};

const MUSIC_SHELL_MEMORY_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MusicShellMemory {
    version: u32,
    server_url: String,
    user_id: String,
    playback_panel: PlaybackPanel,
}

impl MusicShellMemory {
    fn new(session: &AuthSession, playback_panel: PlaybackPanel) -> Self {
        Self {
            version: MUSIC_SHELL_MEMORY_VERSION,
            server_url: session.server_url.clone(),
            user_id: session.user.id.clone(),
            playback_panel,
        }
    }

    fn belongs_to(&self, session: &AuthSession) -> bool {
        self.version == MUSIC_SHELL_MEMORY_VERSION
            && self.server_url == session.server_url
            && self.user_id == session.user.id
    }
}

#[derive(Clone, Debug)]
pub(super) struct MusicShellMemoryStore {
    path: PathBuf,
}

impl MusicShellMemoryStore {
    pub(super) fn load_default() -> Result<Self> {
        let state_directory = dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .context("unable to determine the user's local state directory")?;
        Ok(Self {
            path: state_directory.join("romus/music-shell.json"),
        })
    }

    fn load(&self, session: &AuthSession) -> Result<Option<MusicShellMemory>> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to read music shell memory {}", self.path.display())
                });
            }
        };
        let memory: MusicShellMemory = serde_json::from_str(&contents).with_context(|| {
            format!(
                "failed to decode music shell memory {}",
                self.path.display()
            )
        })?;
        Ok(memory.belongs_to(session).then_some(memory))
    }

    fn save(&self, memory: &MusicShellMemory) -> Result<()> {
        let bytes =
            serde_json::to_vec_pretty(memory).context("failed to serialize shell memory")?;
        atomic_write(&self.path, &bytes)
            .with_context(|| format!("failed to save shell memory {}", self.path.display()))
    }

    fn clear(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!(
                    "failed to remove music shell memory {}",
                    self.path.display()
                )
            }),
        }
    }
}

impl MusicShell {
    pub(super) fn restore_ui_memory(&mut self, cx: &gpui::App) {
        let Some(store) = &self.ui_memory else {
            return;
        };
        let Some(session) = cx.global::<SessionStore>().current() else {
            return;
        };
        match store.load(&session) {
            Ok(Some(memory)) => self.playback_panel = memory.playback_panel,
            Ok(None) => {}
            Err(error) => log::warn!("unable to restore music shell memory: {error:#}"),
        }
    }

    pub(super) fn save_ui_memory(&self, cx: &gpui::App) {
        let Some(store) = &self.ui_memory else {
            return;
        };
        let Some(session) = cx.global::<SessionStore>().current() else {
            return;
        };
        if let Err(error) = store.save(&MusicShellMemory::new(&session, self.playback_panel)) {
            log::warn!("unable to save music shell memory: {error:#}");
        }
    }

    pub(super) fn clear_ui_memory(&mut self) {
        self.playback_panel = PlaybackPanel::None;
        if let Some(store) = &self.ui_memory
            && let Err(error) = store.clear()
        {
            log::warn!("unable to clear music shell memory: {error:#}");
        }
    }
}
