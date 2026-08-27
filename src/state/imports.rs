use std::{collections::HashMap, path::PathBuf, time::Duration};

use gpui::{Entity, EventEmitter, PathPromptOptions, Task, Window};
use gpui_tokio::Tokio;
use uic::components::toast;
use uuid::Uuid;

use crate::services::{
    api::{LibraryApi, TrackQuery, TrackResponse},
    library_api,
    local::LocalStore,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum ImportState {
    Waiting,
    Processing,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImportKind {
    Audio,
    Lyrics,
}

#[derive(Clone, Debug)]
struct ImportItem {
    id: Uuid,
    path: PathBuf,
    state: ImportState,
    kind: ImportKind,
}

enum ImportResult {
    Audio,
    Lyrics { track_id: String, lyrics: String },
    Ignored,
}

pub enum ImportEvent {
    LibraryChanged,
    LyricsChanged { track_id: String, lyrics: String },
}

impl EventEmitter<ImportEvent> for ImportManager {}

pub struct ImportManager {
    items: Vec<ImportItem>,
    workers: HashMap<Uuid, Task<()>>,
}

impl ImportManager {
    pub fn new(_cx: &mut gpui::Context<Self>) -> Self {
        Self {
            items: Vec::new(),
            workers: HashMap::new(),
        }
    }

    fn has_items(&self) -> bool {
        !self.items.is_empty()
    }

    fn is_idle(&self) -> bool {
        self.has_items()
            && self
                .items
                .iter()
                .all(|item| matches!(item.state, ImportState::Completed | ImportState::Failed))
    }

    pub fn select_files(&mut self, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(crate::ui::strings::choose_music_or_lyrics_files().into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |manager, cx| manager.enqueue(paths, cx));
        })
        .detach();
    }

    pub fn clear(&mut self, cx: &mut gpui::Context<Self>) {
        self.workers.clear();
        self.items.clear();
        cx.notify();
    }

    fn enqueue(&mut self, paths: Vec<PathBuf>, cx: &mut gpui::Context<Self>) {
        let mut item_ids = Vec::new();
        for path in paths {
            if !path.is_file() {
                continue;
            }
            let id = Uuid::new_v4();
            let kind = if is_lrc(&path) {
                ImportKind::Lyrics
            } else {
                ImportKind::Audio
            };
            self.items.push(ImportItem {
                id,
                path,
                state: ImportState::Waiting,
                kind,
            });
            item_ids.push(id);
        }
        for item_id in item_ids {
            self.start(item_id, cx);
        }
        cx.notify();
    }

    fn start(&mut self, item_id: Uuid, cx: &mut gpui::Context<Self>) {
        let Some(item) = self.items.iter().find(|item| item.id == item_id) else {
            return;
        };
        if item.kind == ImportKind::Lyrics && self.has_pending_audio() {
            return;
        }
        let path = item.path.clone();
        let kind = item.kind;
        let worker = match kind {
            ImportKind::Audio => {
                let Ok(store) = LocalStore::shared() else {
                    self.fail_to_start(item_id, "The local library is unavailable", cx);
                    return;
                };
                Tokio::spawn(cx, record_file(store, path))
            }
            ImportKind::Lyrics => {
                let Ok(api) = library_api(cx) else {
                    self.fail_to_start(item_id, "The library service is unavailable", cx);
                    return;
                };
                Tokio::spawn(cx, import_lyrics(api, path))
            }
        };
        if let Some(item) = self.items.iter_mut().find(|item| item.id == item_id) {
            item.state = ImportState::Processing;
        }
        let task = cx.spawn(async move |this, cx| {
            let result = worker.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |manager, cx| {
                manager.workers.remove(&item_id);
                let mut changed_lyrics = None;
                let mut ignored = false;
                let result = match result {
                    Ok(result) => result,
                    Err(error) => Err(format!("Import task failed: {error}")),
                };
                if let Some(item) = manager.items.iter_mut().find(|item| item.id == item_id) {
                    match result {
                        Ok(ImportResult::Audio) => {
                            item.state = ImportState::Completed;
                        }
                        Ok(ImportResult::Lyrics { track_id, lyrics }) => {
                            item.state = ImportState::Completed;
                            changed_lyrics = Some((track_id, lyrics));
                        }
                        Ok(ImportResult::Ignored) => ignored = true,
                        Err(error) => {
                            log::warn!("unable to import {}: {error}", item.path.display());
                            item.state = ImportState::Failed;
                        }
                    }
                }
                if ignored {
                    manager.items.retain(|item| item.id != item_id);
                }
                manager.start_waiting_lyrics_if_audio_idle(cx);
                manager.finish_if_idle(cx);
                if let Some((track_id, lyrics)) = changed_lyrics {
                    cx.emit(ImportEvent::LyricsChanged { track_id, lyrics });
                }
                cx.notify();
            });
        });
        self.workers.insert(item_id, task);
        cx.notify();
    }

    fn has_pending_audio(&self) -> bool {
        self.items.iter().any(|item| {
            item.kind == ImportKind::Audio
                && matches!(item.state, ImportState::Waiting | ImportState::Processing)
        })
    }

    fn start_waiting_lyrics_if_audio_idle(&mut self, cx: &mut gpui::Context<Self>) {
        if self.has_pending_audio() {
            return;
        }
        let item_ids = self
            .items
            .iter()
            .filter(|item| item.kind == ImportKind::Lyrics && item.state == ImportState::Waiting)
            .map(|item| item.id)
            .collect::<Vec<_>>();
        for item_id in item_ids {
            self.start(item_id, cx);
        }
    }

    fn fail_to_start(
        &mut self,
        item_id: Uuid,
        message: &'static str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == item_id) {
            item.state = ImportState::Failed;
            log::warn!("unable to import {}: {message}", item.path.display());
        }
        self.start_waiting_lyrics_if_audio_idle(cx);
        self.finish_if_idle(cx);
        cx.notify();
    }

    fn finish_if_idle(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.is_idle() {
            return;
        }
        let completed_audio = self
            .items
            .iter()
            .filter(|item| item.state == ImportState::Completed && item.kind == ImportKind::Audio)
            .count();
        let completed_lyrics = self
            .items
            .iter()
            .filter(|item| item.state == ImportState::Completed && item.kind == ImportKind::Lyrics)
            .count();
        let failed = self
            .items
            .iter()
            .filter(|item| item.state == ImportState::Failed)
            .count();
        if completed_audio > 0 {
            toast::success(
                if completed_audio == 1 {
                    "Song added to your library".to_owned()
                } else {
                    format!("{completed_audio} songs added to your library")
                },
                cx,
            );
            cx.emit(ImportEvent::LibraryChanged);
        }
        if completed_lyrics > 0 {
            toast::success(
                if completed_lyrics == 1 {
                    "Lyrics matched and added".to_owned()
                } else {
                    format!("Lyrics matched and added to {completed_lyrics} songs")
                },
                cx,
            );
        }
        if failed > 0 {
            toast::error(
                if failed == 1 {
                    "1 file failed to process".to_owned()
                } else {
                    format!("{failed} files failed to process")
                },
                cx,
            );
        }
        self.schedule_cleanup(cx);
    }

    fn schedule_cleanup(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.is_idle() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(2400))
                .await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |manager, cx| {
                if manager.is_idle() {
                    manager.items.clear();
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

async fn record_file(store: LocalStore, path: PathBuf) -> Result<ImportResult, String> {
    store
        .add_paths(vec![path])
        .map_err(|error| error.user_message())?;
    Ok(ImportResult::Audio)
}

async fn import_lyrics(api: LibraryApi, path: PathBuf) -> Result<ImportResult, String> {
    let file_stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "The LRC file name cannot be used to match a song".to_owned())?
        .to_owned();
    let lyrics = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| format!("Unable to read the LRC file as UTF-8: {error}"))?;
    if lyrics.trim().is_empty() {
        return Err("The LRC file is empty".to_owned());
    }

    let tracks = all_tracks(&api).await?;
    let track = match match_lyrics_track(&file_stem, &lyrics, &tracks) {
        Ok(track) => track,
        Err(reason) => {
            log::info!("ignoring unmatched LRC file {}: {reason}", path.display());
            return Ok(ImportResult::Ignored);
        }
    };
    api.replace_lyrics(&track.id, lyrics.clone())
        .await
        .map_err(|error| error.user_message())?;
    Ok(ImportResult::Lyrics {
        track_id: track.id.clone(),
        lyrics,
    })
}

async fn all_tracks(api: &LibraryApi) -> Result<Vec<TrackResponse>, String> {
    const PAGE_SIZE: u32 = 200;
    let mut tracks = Vec::new();
    let mut offset = 0_u64;
    loop {
        let page = api
            .tracks(&TrackQuery {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
                ..Default::default()
            })
            .await
            .map_err(|error| error.user_message())?;
        let count = page.items.len();
        tracks.extend(page.items);
        offset = offset.saturating_add(count as u64);
        if count == 0 || offset >= page.total.max(0) as u64 {
            break;
        }
    }
    Ok(tracks)
}

fn match_lyrics_track<'a>(
    file_stem: &str,
    lyrics: &str,
    tracks: &'a [TrackResponse],
) -> Result<&'a TrackResponse, String> {
    let tag_title = lrc_tag(lyrics, "ti");
    let tag_artist = lrc_tag(lyrics, "ar");
    let (filename_artist, filename_title) = filename_hints(file_stem);
    let stem_key = match_key(file_stem);
    let title_keys = [tag_title.as_deref(), Some(filename_title.as_str())]
        .into_iter()
        .flatten()
        .map(match_key)
        .filter(|key| !key.is_empty())
        .collect::<Vec<_>>();
    let artist_hint = tag_artist.as_deref().or(filename_artist.as_deref());
    let artist_key = artist_hint.map(match_key).filter(|key| !key.is_empty());

    let mut matches = tracks
        .iter()
        .filter_map(|track| {
            let title = track.title.as_deref()?;
            let track_title_key = match_key(title);
            let title_match = title_keys.iter().any(|key| key == &track_title_key);
            let track_artist_key = track.artist.as_deref().map(match_key);
            let combined_match = track_artist_key.as_ref().is_some_and(|artist| {
                stem_key == format!("{artist}{track_title_key}")
                    || stem_key == format!("{track_title_key}{artist}")
            });
            if !title_match && !combined_match {
                return None;
            }
            if let Some(expected_artist) = artist_key.as_ref()
                && track_artist_key
                    .as_ref()
                    .is_some_and(|artist| artist != expected_artist)
            {
                return None;
            }

            let mut score = u8::from(title_match) * 40 + u8::from(combined_match) * 20;
            if tag_title
                .as_deref()
                .is_some_and(|title| match_key(title) == track_title_key)
            {
                score += 30;
            }
            if artist_key
                .as_ref()
                .is_some_and(|expected| track_artist_key.as_ref() == Some(expected))
            {
                score += 20;
            }
            Some((score, track))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    let Some((best_score, best)) = matches.first().copied() else {
        return Err(format!(
            "No song matches {file_stem}. Name the file Song.lrc or Artist - Song.lrc"
        ));
    };
    if matches
        .get(1)
        .is_some_and(|(score, _)| *score == best_score)
    {
        return Err(format!(
            "More than one song matches {file_stem}; include the artist in the LRC file name"
        ));
    }
    Ok(best)
}

fn filename_hints(file_stem: &str) -> (Option<String>, String) {
    let file_stem = file_stem.trim();
    for separator in [" - ", " – ", " — "] {
        if let Some((left, right)) = file_stem.rsplit_once(separator) {
            let left = left.trim();
            let right = right.trim();
            if !right.is_empty() {
                if left.chars().all(|character| character.is_ascii_digit()) {
                    return (None, right.to_owned());
                }
                if !left.is_empty() {
                    return (Some(left.to_owned()), right.to_owned());
                }
            }
        }
    }
    (None, file_stem.to_owned())
}

fn lrc_tag(lyrics: &str, expected: &str) -> Option<String> {
    lyrics.lines().find_map(|line| {
        let line = line.trim().trim_start_matches('\u{feff}');
        let content = line.strip_prefix('[')?.strip_suffix(']')?;
        let (tag, value) = content.split_once(':')?;
        (tag.eq_ignore_ascii_case(expected) && !value.trim().is_empty())
            .then(|| value.trim().to_owned())
    })
}

fn match_key(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn is_lrc(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lrc"))
}

pub fn show_picker<C: 'static>(
    manager: Entity<ImportManager>,
    _window: &mut Window,
    cx: &mut gpui::Context<C>,
) {
    manager.update(cx, |manager, cx| manager.select_files(cx));
}
