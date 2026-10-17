use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use gpui::{AppContext, Entity, EventEmitter, Task};
use gpui_media::{
    AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioSource,
    PlaybackState as MediaPlaybackState, PlaybackTimeline,
};
use gpui_tokio::Tokio;

use crate::domain::{ArtworkFile, LyricsDocument, PlaybackState, TrackInfo};
use crate::services::api::{ApiError, LibraryApi, TrackResponse};
use crate::services::library_api;
#[cfg(target_os = "linux")]
use crate::services::mpris;
use crate::services::session::SessionStore;
use crate::state::playback_memory::{PlaybackMemory, PlaybackMemoryStore};
use crate::state::queue::{PlayQueue, QueueOrigin, RepeatMode};
use crate::ui::strings;

pub enum PlaybackEvent {
    AuthenticationRequired,
}

const PREVIOUS_RESTART_AFTER: Duration = Duration::from_secs(3);
const PERSIST_INTERVAL: Duration = Duration::from_secs(2);

pub struct PlaybackController {
    pub snapshot: PlaybackState,
    pub queue: PlayQueue,
    player: Option<Entity<AudioPlayer>>,
    playing_track_id: Option<String>,
    play_generation: u64,
    play_task: Option<Task<()>>,
    metadata_task: Option<Task<()>>,
    queue_duration_task: Option<Task<()>>,
    memory_store: Option<PlaybackMemoryStore>,
    persist_task: Option<Task<()>>,
    restored_identity: Option<String>,
    autoplay_on_open: bool,
    pending_seek_position: Option<Duration>,
    #[cfg(target_os = "linux")]
    mpris: Option<mpris::MprisBridge>,
    #[cfg(target_os = "linux")]
    _mpris_commands: Option<Task<()>>,
}

impl EventEmitter<PlaybackEvent> for PlaybackController {}

impl PlaybackController {
    pub(crate) fn artwork_is_loading(&self) -> bool {
        self.metadata_task.is_some()
            && self.snapshot.track.artwork.is_none()
            && self.queue.current().is_some_and(|track| track.has_artwork)
    }

    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        let snapshot = PlaybackState::empty();
        let memory_store = PlaybackMemoryStore::load_default()
            .inspect_err(|error| log::warn!("unable to initialize playback memory: {error:#}"))
            .ok();
        #[cfg(target_os = "linux")]
        let (mpris, mpris_commands) = start_mpris(cx);
        let controller = Self {
            snapshot,
            queue: PlayQueue::default(),
            player: None,
            playing_track_id: None,
            play_generation: 0,
            play_task: None,
            metadata_task: None,
            queue_duration_task: None,
            memory_store,
            persist_task: None,
            restored_identity: None,
            autoplay_on_open: true,
            pending_seek_position: None,
            #[cfg(target_os = "linux")]
            mpris,
            #[cfg(target_os = "linux")]
            _mpris_commands: mpris_commands,
        };
        cx.on_app_quit(|controller, cx| {
            let payload = controller.persistence_payload(cx);
            async move {
                if let Some((store, memory)) = payload
                    && let Err(error) = tokio::task::spawn_blocking(move || match memory {
                        Some(memory) => store.save(&memory),
                        None => store.clear(),
                    })
                    .await
                    .unwrap_or_else(|error| Err(anyhow::anyhow!(error.to_string())))
                {
                    log::warn!("unable to save playback state while quitting: {error:#}");
                }
            }
        })
        .detach();
        controller
    }

    pub fn restore(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(session) = cx.global::<SessionStore>().current() else {
            return;
        };
        let identity = format!("{}\n{}", session.server_url, session.user.id);
        if self.restored_identity.as_deref() == Some(identity.as_str()) {
            return;
        }
        self.restored_identity = Some(identity);
        let Some(store) = self.memory_store.clone() else {
            return;
        };
        let memory = match store.load(&session) {
            Ok(Some(memory)) => memory,
            Ok(None) => return,
            Err(error) => {
                log::warn!("unable to restore playback memory: {error:#}");
                return;
            }
        };
        self.queue.restore(
            memory.tracks,
            memory.index,
            memory.origin,
            memory.repeat,
            memory.shuffle,
        );
        self.snapshot.volume = memory.volume;
        self.snapshot.muted = memory.muted;
        self.refresh_queue_durations(cx);
        let Some(track) = self.queue.current().cloned() else {
            return;
        };
        self.prepare_track(track, Duration::from_millis(memory.position_ms), false, cx);
    }

    pub fn clear_persisted(&mut self) {
        self.queue_duration_task = None;
        self.persist_task = None;
        self.restored_identity = None;
        if let Some(store) = &self.memory_store
            && let Err(error) = store.clear()
        {
            log::warn!("unable to clear playback memory: {error:#}");
        }
    }

    fn refresh_queue_durations(&mut self, cx: &mut gpui::Context<Self>) {
        if !self
            .queue
            .items
            .iter()
            .any(|item| item.track.duration_ms.is_none())
        {
            return;
        }
        let request = cx.background_executor().spawn(async {
            crate::services::local::LocalStore::shared()?
                .tracks()
                .map(|tracks| {
                    tracks
                        .into_iter()
                        .filter_map(|track| track.duration_ms.map(|duration| (track.id, duration)))
                        .collect::<std::collections::HashMap<_, _>>()
                })
        });
        self.queue_duration_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |controller, cx| {
                controller.queue_duration_task = None;
                match result {
                    Ok(durations) => {
                        for item in &mut controller.queue.items {
                            if item.track.duration_ms.is_none() {
                                item.track.duration_ms = durations.get(&item.track.id).copied();
                            }
                        }
                        cx.notify();
                    }
                    Err(error) => log::warn!("Unable to refresh queue durations: {error}"),
                }
            });
        }));
    }

    pub fn play_track(&mut self, track: TrackResponse, cx: &mut gpui::Context<Self>) {
        self.play_from(vec![track], 0, false, None, cx);
    }

    pub fn replace_loaded_lyrics(
        &mut self,
        track_id: &str,
        lyrics: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.playing_track_id.as_deref() != Some(track_id) {
            return;
        }
        self.snapshot.track.lyrics = LyricsDocument::parse(Some(lyrics));
        cx.notify();
    }

    pub fn play_from(
        &mut self,
        tracks: Vec<TrackResponse>,
        start: usize,
        shuffle: bool,
        origin: Option<QueueOrigin>,
        cx: &mut gpui::Context<Self>,
    ) {
        if tracks.is_empty() {
            return;
        }
        let start = start.min(tracks.len() - 1);
        let track = tracks[start].clone();
        self.queue.play_from(tracks, start, shuffle, origin);
        if self.playing_track_id.as_deref() == Some(track.id.as_str()) && self.player.is_some() {
            self.schedule_persist(cx);
            cx.notify();
            return;
        }
        self.start_track(track, cx);
    }

    pub fn play_next(&mut self, track: TrackResponse, cx: &mut gpui::Context<Self>) {
        self.play_next_many(vec![track], cx);
    }

    pub fn play_next_many(&mut self, tracks: Vec<TrackResponse>, cx: &mut gpui::Context<Self>) {
        let Some(first) = tracks.first().cloned() else {
            return;
        };
        let should_start = self.queue.current().is_none();
        self.queue.play_next_many(tracks);
        if should_start {
            self.start_track(first, cx);
        } else {
            self.schedule_persist(cx);
            cx.notify();
        }
    }

    pub fn enqueue(&mut self, track: TrackResponse, cx: &mut gpui::Context<Self>) {
        self.enqueue_many(vec![track], cx);
    }

    pub fn enqueue_many(&mut self, tracks: Vec<TrackResponse>, cx: &mut gpui::Context<Self>) {
        let Some(first) = tracks.first().cloned() else {
            return;
        };
        let should_start = self.queue.current().is_none();
        self.queue.enqueue_many(tracks);
        if should_start {
            self.start_track(first, cx);
        } else {
            self.schedule_persist(cx);
            cx.notify();
        }
    }

    pub fn skip_next(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(track) = self.queue.advance() else {
            return;
        };
        if self.queue.repeat == RepeatMode::One
            && self.playing_track_id.as_deref() == Some(track.id.as_str())
        {
            self.restart_current(cx);
            return;
        }
        self.start_track(track, cx);
    }

    pub fn skip_previous(&mut self, cx: &mut gpui::Context<Self>) {
        let position = self.snapshot.timeline.position();
        if position >= PREVIOUS_RESTART_AFTER || !self.queue.has_previous() {
            self.restart_current(cx);
            return;
        }
        let Some(track) = self.queue.go_previous() else {
            return;
        };
        self.start_track(track, cx);
    }

    pub fn play_queue_index(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        let Some(track) = self.queue.jump(index) else {
            return;
        };
        self.start_track(track, cx);
    }

    pub fn play_queue_key(&mut self, key: u64, cx: &mut gpui::Context<Self>) {
        if self.queue.current_key() == Some(key) {
            self.restart_finished_track(cx);
            return;
        }
        let Some(track) = self.queue.jump_key(key) else {
            return;
        };
        self.start_track(track, cx);
    }

    pub fn remove_queue_index(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        self.queue.remove(index);
        self.schedule_persist(cx);
        cx.notify();
    }

    pub fn remove_queue_key(&mut self, key: u64, cx: &mut gpui::Context<Self>) {
        let was_current = self.queue.current_key() == Some(key);
        self.queue.remove_key(key);
        if was_current {
            if let Some(track) = self.queue.current().cloned() {
                self.start_track(track, cx);
            } else {
                self.stop_player(cx);
            }
        } else {
            self.schedule_persist(cx);
            cx.notify();
        }
    }

    pub fn play_next_queue_key(&mut self, key: u64, cx: &mut gpui::Context<Self>) {
        self.queue.play_next_key(key);
        self.schedule_persist(cx);
        cx.notify();
    }

    pub fn move_queue_key_to_end(&mut self, key: u64, cx: &mut gpui::Context<Self>) {
        self.queue.move_key_to_end(key);
        self.schedule_persist(cx);
        cx.notify();
    }

    pub fn move_queue_key(&mut self, key: u64, to: usize, cx: &mut gpui::Context<Self>) -> bool {
        let changed = self.queue.move_by_key(key, to);
        if changed {
            self.schedule_persist(cx);
            cx.notify();
        }
        changed
    }

    pub fn clear_up_next(&mut self, cx: &mut gpui::Context<Self>) {
        self.queue.clear_up_next();
        self.schedule_persist(cx);
        cx.notify();
    }

    pub fn toggle_shuffle(&mut self, cx: &mut gpui::Context<Self>) {
        self.queue.toggle_shuffle();
        self.schedule_persist(cx);
        cx.notify();
    }

    pub fn cycle_repeat(&mut self, cx: &mut gpui::Context<Self>) {
        self.queue.cycle_repeat();
        self.schedule_persist(cx);
        cx.notify();
    }

    pub fn update_queue_track(&mut self, track: TrackResponse, cx: &mut gpui::Context<Self>) {
        let mut changed = false;
        for item in &mut self.queue.items {
            if item.track.id == track.id {
                item.track = track.clone();
                changed = true;
            }
        }
        if changed {
            self.schedule_persist(cx);
            cx.notify();
        }
    }

    pub fn can_skip_next(&self) -> bool {
        self.queue.has_next()
            || self.queue.repeat == RepeatMode::All
            || self.queue.repeat == RepeatMode::One && self.queue.current().is_some()
    }

    pub fn can_skip_previous(&self) -> bool {
        self.queue.current().is_some()
    }

    fn restart_current(&mut self, cx: &mut gpui::Context<Self>) {
        if self.restart_finished_track(cx) {
            return;
        }
        let Some(player) = self.player.clone() else {
            if let Some(track) = self.queue.current().cloned() {
                self.start_track(track, cx);
            }
            return;
        };
        if !player.read(cx).is_seekable() {
            if let Some(track) = self.queue.current().cloned() {
                self.start_track(track, cx);
            }
            return;
        }
        if let Err(error) = player.update(cx, |player, cx| {
            player.seek_to(Duration::ZERO, gpui_media::SeekMode::Accurate, cx)?;
            player.play(cx)
        }) {
            log::warn!("unable to restart the current track: {error}");
        }
        self.publish_mpris();
        cx.notify();
    }

    fn restart_finished_track(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        if matches!(
            self.snapshot.status,
            MediaPlaybackState::Ended | MediaPlaybackState::Error(_)
        ) && let Some(track) = self.queue.current().cloned()
        {
            self.start_track(track, cx);
            return true;
        }
        false
    }

    fn start_track(&mut self, track: TrackResponse, cx: &mut gpui::Context<Self>) {
        self.prepare_track(track, Duration::ZERO, true, cx);
    }

    fn prepare_track(
        &mut self,
        track: TrackResponse,
        position: Duration,
        autoplay: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.play_generation = self.play_generation.wrapping_add(1);
        let generation = self.play_generation;
        self.player = None;
        self.play_task = None;
        self.metadata_task = None;
        self.playing_track_id = Some(track.id.clone());
        self.autoplay_on_open = autoplay;
        self.pending_seek_position = (position > Duration::ZERO).then_some(position);

        let volume = self.snapshot.volume;
        let muted = self.snapshot.muted;
        let duration = track
            .duration_ms
            .and_then(|millis| u64::try_from(millis).ok())
            .map(Duration::from_millis);
        self.snapshot = PlaybackState {
            status: if autoplay {
                MediaPlaybackState::Loading
            } else {
                MediaPlaybackState::Paused
            },
            timeline: PlaybackTimeline::new(position, duration, false),
            muted,
            volume,
            track: TrackInfo::from_library(
                track.title.as_deref(),
                track.artist.as_deref(),
                track.album.as_deref(),
            ),
            error: None,
        };
        cx.notify();

        self.load_now_playing_assets(&track, generation, cx);
        self.start_stream(track, generation, cx);
        self.schedule_persist(cx);
        self.publish_mpris();
    }

    pub fn seek_by(&mut self, delta: Duration, backward: bool, cx: &mut gpui::Context<Self>) {
        let Some(player) = self.player.clone() else {
            return;
        };
        if !player.read(cx).is_seekable() {
            return;
        }
        let result = player.update(cx, |player, cx| {
            let position = if backward {
                player.position().saturating_sub(delta)
            } else {
                player.position().saturating_add(delta)
            };
            player.seek_to(position, gpui_media::SeekMode::Accurate, cx)
        });
        if let Err(error) = result {
            log::warn!("unable to seek: {error}");
        }
        self.publish_mpris();
    }

    pub fn toggle_playback(&mut self, cx: &mut gpui::Context<Self>) {
        if self.restart_finished_track(cx) {
            return;
        }
        let Some(player) = self.player.clone() else {
            return;
        };
        if let Err(error) = player.update(cx, |player, cx| player.toggle_playback(cx)) {
            self.snapshot.error = Some(error.to_string().into());
            cx.notify();
        }
        self.publish_mpris();
    }

    pub fn set_playing(&mut self, playing: bool, cx: &mut gpui::Context<Self>) {
        if playing && self.restart_finished_track(cx) {
            return;
        }
        let Some(player) = self.player.clone() else {
            return;
        };
        let result = player.update(cx, |player, cx| {
            if playing {
                player.play(cx)
            } else {
                player.pause(cx);
                Ok(())
            }
        });
        if let Err(error) = result {
            self.snapshot.error = Some(error.to_string().into());
            cx.notify();
        }
        self.publish_mpris();
    }

    pub fn seek_to(&mut self, position: Duration, cx: &mut gpui::Context<Self>) {
        let Some(player) = self.player.clone() else {
            return;
        };
        if !player.read(cx).is_seekable() {
            return;
        }
        if let Err(error) = player.update(cx, |player, cx| {
            player.seek_to(position, gpui_media::SeekMode::Accurate, cx)
        }) {
            log::warn!("unable to seek: {error}");
        }
        self.publish_mpris();
    }

    pub fn set_volume(&mut self, volume: f64, cx: &mut gpui::Context<Self>) {
        let volume = volume.clamp(0., 1.);
        if let Some(player) = self.player.clone() {
            player.update(cx, |player, cx| {
                if volume > 0. && player.is_muted() {
                    player.set_muted(false, cx);
                }
                player.set_volume(volume, cx);
            });
        } else {
            self.snapshot.volume = volume;
            if volume > 0. {
                self.snapshot.muted = false;
            }
            self.schedule_persist(cx);
            cx.notify();
        }
        self.publish_mpris();
    }

    pub fn toggle_muted(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(player) = self.player.clone() {
            player.update(cx, |player, cx| player.toggle_muted(cx));
        } else {
            self.snapshot.muted = !self.snapshot.muted;
            self.schedule_persist(cx);
            cx.notify();
        }
        self.publish_mpris();
    }

    pub fn stop_playback(&mut self, cx: &mut gpui::Context<Self>) {
        self.queue.clear();
        self.stop_player(cx);
    }

    fn stop_player(&mut self, cx: &mut gpui::Context<Self>) {
        self.play_generation = self.play_generation.wrapping_add(1);
        self.player = None;
        self.play_task = None;
        self.metadata_task = None;
        self.playing_track_id = None;
        self.autoplay_on_open = true;
        self.pending_seek_position = None;
        let volume = self.snapshot.volume;
        let muted = self.snapshot.muted;
        self.snapshot = PlaybackState::empty();
        self.snapshot.volume = volume;
        self.snapshot.muted = muted;
        self.schedule_persist(cx);
        self.publish_mpris();
        cx.notify();
    }

    #[cfg(target_os = "linux")]
    pub fn handle_mpris_command(
        &mut self,
        command: mpris::MprisCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            mpris::MprisCommand::Play => self.set_playing(true, cx),
            mpris::MprisCommand::Pause => self.set_playing(false, cx),
            mpris::MprisCommand::PlayPause => self.toggle_playback(cx),
            mpris::MprisCommand::Stop => {
                if let Some(player) = self.player.clone() {
                    if let Err(error) = player.update(cx, |player, cx| player.stop(cx)) {
                        log::warn!("unable to stop playback: {error}");
                    }
                } else {
                    self.stop_playback(cx);
                }
            }
            mpris::MprisCommand::Seek(offset_micros) => {
                let Some(player) = self.player.clone() else {
                    return;
                };
                let position = player.read(cx).position();
                let target = if offset_micros >= 0 {
                    position.saturating_add(Duration::from_micros(offset_micros as u64))
                } else {
                    position.saturating_sub(Duration::from_micros(offset_micros.unsigned_abs()))
                };
                self.seek_to(target, cx);
            }
            mpris::MprisCommand::SetPosition(position_micros) => {
                self.seek_to(Duration::from_micros(position_micros.max(0) as u64), cx);
            }
            mpris::MprisCommand::SetVolume(volume) => self.set_volume(volume, cx),
            mpris::MprisCommand::Next => self.skip_next(cx),
            mpris::MprisCommand::Previous => self.skip_previous(cx),
        }
        self.publish_mpris();
        cx.notify();
    }

    fn schedule_persist(&mut self, cx: &mut gpui::Context<Self>) {
        if self.persist_task.is_some() || self.memory_store.is_none() {
            return;
        }
        self.persist_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PERSIST_INTERVAL).await;
            let Some(this) = this.upgrade() else {
                return;
            };
            let payload = this.update(cx, |controller, cx| {
                controller.persist_task = None;
                controller.persistence_payload(cx)
            });
            let Some((store, memory)) = payload else {
                return;
            };
            let result = tokio::task::spawn_blocking(move || match memory {
                Some(memory) => store.save(&memory),
                None => store.clear(),
            })
            .await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => log::warn!("unable to save playback memory: {error:#}"),
                Err(error) => log::warn!("playback memory task failed: {error}"),
            }
        }));
    }

    fn persistence_payload(
        &self,
        cx: &gpui::App,
    ) -> Option<(PlaybackMemoryStore, Option<PlaybackMemory>)> {
        let store = self.memory_store.clone()?;
        let session = cx.global::<SessionStore>().current()?;
        let memory = self.queue.index.map(|index| {
            PlaybackMemory::new(
                &session,
                self.queue
                    .items
                    .iter()
                    .map(|item| item.track.clone())
                    .collect(),
                index,
                self.queue.origin.clone(),
                self.queue.repeat,
                self.queue.shuffle,
                self.snapshot
                    .timeline
                    .position()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
                self.snapshot.volume,
                self.snapshot.muted,
            )
        });
        Some((store, memory))
    }

    fn publish_mpris(&self) {
        #[cfg(target_os = "linux")]
        if let Some(bridge) = &self.mpris {
            bridge.publish(&self.snapshot);
        }
    }

    fn sign_out(&mut self, cx: &mut gpui::Context<Self>) {
        self.stop_playback(cx);
        cx.emit(PlaybackEvent::AuthenticationRequired);
    }

    fn start_stream(
        &mut self,
        track: TrackResponse,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        let api = match library_api(cx) {
            Ok(api) => api,
            Err(error) => {
                self.snapshot.status = MediaPlaybackState::Paused;
                self.snapshot.error = Some(error.user_message().into());
                cx.notify();
                return;
            }
        };
        let track_id = track.id.clone();
        let request = Tokio::spawn(
            cx,
            async move { ensure_cached_audio(&api, &track_id).await },
        );
        self.play_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |controller, cx| {
                if controller.play_generation != generation {
                    return;
                }
                controller.play_task = None;
                match result {
                    Ok(Ok(path)) => {
                        if let Err(error) = controller.open_player(&path, cx) {
                            log::error!("unable to start playback: {error}");
                            controller.snapshot.status = MediaPlaybackState::Paused;
                            controller.snapshot.error = Some(error.into());
                        }
                    }
                    Ok(Err(ApiError::AuthenticationRequired)) => {
                        log::info!("the saved session is no longer valid");
                        controller.sign_out(cx);
                    }
                    Ok(Err(error)) => {
                        log::warn!("unable to prepare playback: {error}");
                        controller.snapshot.status = MediaPlaybackState::Paused;
                        controller.snapshot.error = Some(error.user_message().into());
                    }
                    Err(error) => {
                        log::error!("playback task failed: {error}");
                        controller.snapshot.status = MediaPlaybackState::Paused;
                        controller.snapshot.error =
                            Some(strings::unable_to_start_playback().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn open_player(&mut self, path: &Path, cx: &mut gpui::Context<Self>) -> Result<(), String> {
        let source = AudioSource::from_path(path).map_err(|error| error.to_string())?;
        let options = AudioPlayerOptions {
            autoplay: self.autoplay_on_open && self.pending_seek_position.is_none(),
            volume: self.snapshot.volume,
            muted: self.snapshot.muted,
            ..AudioPlayerOptions::default()
        };
        let player = cx.new(|cx| {
            AudioPlayer::new(source, options, cx).expect("failed to start the audio decoder thread")
        });
        cx.subscribe(&player, |controller, player, event, cx| {
            if controller.player.as_ref() != Some(&player) {
                return;
            }
            match event {
                AudioPlayerEvent::StateChanged(state) => {
                    if let MediaPlaybackState::Error(error) = state
                        && is_device_interrupt(&error.to_string())
                    {
                        controller.snapshot.status = MediaPlaybackState::Paused;
                        controller.snapshot.error = None;
                        if let Some(player) = controller.player.clone() {
                            player.update(cx, |player, cx| player.pause(cx));
                        }
                    } else {
                        let ended = matches!(state, MediaPlaybackState::Ended);
                        controller.snapshot.status = state.clone();
                        controller.snapshot.error = match state {
                            MediaPlaybackState::Error(error) => Some(error.to_string().into()),
                            _ => None,
                        };
                        if ended {
                            controller.skip_next(cx);
                        }
                    }
                }
                AudioPlayerEvent::TimelineChanged(timeline) => {
                    controller.apply_timeline(*timeline);
                }
                AudioPlayerEvent::VolumeChanged { volume, muted } => {
                    controller.snapshot.volume = *volume;
                    controller.snapshot.muted = *muted;
                }
                AudioPlayerEvent::InfoChanged(info) => {
                    if let Some(duration) = info.duration() {
                        controller.apply_timeline(PlaybackTimeline::new(
                            controller.snapshot.timeline.position(),
                            Some(duration),
                            info.is_seekable(),
                        ));
                    }
                    if info.is_seekable()
                        && let Some(position) = controller.pending_seek_position.take()
                    {
                        let autoplay = controller.autoplay_on_open;
                        if let Err(error) = player.update(cx, |player, cx| {
                            player.seek_to(position, gpui_media::SeekMode::Accurate, cx)?;
                            if autoplay {
                                player.play(cx)?;
                            }
                            Ok::<_, gpui_media::MediaError>(())
                        }) {
                            log::warn!("unable to restore playback position: {error}");
                        }
                    }
                }
                _ => {}
            }
            controller.schedule_persist(cx);
            controller.publish_mpris();
            cx.notify();
        })
        .detach();

        self.player = Some(player);
        Ok(())
    }

    fn apply_timeline(&mut self, timeline: PlaybackTimeline) {
        let duration = timeline
            .duration()
            .or_else(|| self.snapshot.timeline.duration());
        self.snapshot.timeline =
            PlaybackTimeline::new(timeline.position(), duration, timeline.is_seekable());
    }

    fn load_now_playing_assets(
        &mut self,
        track: &TrackResponse,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(api) = library_api(cx) else {
            return;
        };
        let track_id = track.id.clone();
        let artwork_key = track.id.clone();
        let has_artwork = track.has_artwork;
        let request = Tokio::spawn(cx, async move {
            let artwork = if has_artwork {
                Some(api.artwork(&track_id, None).await)
            } else {
                None
            };
            let lyrics = api.lyrics(&track_id).await;
            (artwork, lyrics)
        });
        self.metadata_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |controller, cx| {
                if controller.play_generation != generation {
                    return;
                }
                controller.metadata_task = None;
                match result {
                    Ok((artwork, lyrics)) => {
                        if let Some(artwork) = artwork {
                            match artwork {
                                Ok(response) => {
                                    controller.snapshot.track.artwork = ArtworkFile::from_bytes(
                                        response.bytes,
                                        &response.metadata.content_type,
                                        &artwork_key,
                                    );
                                }
                                Err(ApiError::AuthenticationRequired) => {
                                    log::info!("the saved session is no longer valid");
                                    controller.sign_out(cx);
                                    return;
                                }
                                Err(error) => {
                                    log::warn!("unable to load playing artwork: {error}")
                                }
                            }
                        }
                        match lyrics {
                            Ok(response) => {
                                controller.snapshot.track.lyrics =
                                    LyricsDocument::parse(response.lyrics.as_deref());
                            }
                            Err(ApiError::AuthenticationRequired) => {
                                log::info!("the saved session is no longer valid");
                                controller.sign_out(cx);
                                return;
                            }
                            Err(error) => log::warn!("unable to load playing lyrics: {error}"),
                        }
                    }
                    Err(error) => log::warn!("now-playing metadata task failed: {error}"),
                }
                controller.publish_mpris();
                cx.notify();
            });
        }));
    }
}

#[cfg(target_os = "linux")]
fn start_mpris(
    cx: &mut gpui::Context<PlaybackController>,
) -> (Option<mpris::MprisBridge>, Option<Task<()>>) {
    match mpris::MprisBridge::start(&PlaybackState::empty()) {
        Ok(bridge) => {
            let commands = bridge.commands();
            let task = cx.spawn(async move |this, cx| {
                while let Ok(command) = commands.recv().await {
                    let Some(this) = this.upgrade() else {
                        break;
                    };
                    this.update(cx, |controller, cx| {
                        controller.handle_mpris_command(command, cx);
                    });
                }
            });
            (Some(bridge), Some(task))
        }
        Err(error) => {
            log::warn!("MPRIS is unavailable: {error}");
            (None, None)
        }
    }
}

async fn ensure_cached_audio(api: &LibraryApi, track_id: &str) -> Result<PathBuf, ApiError> {
    api.local_path(track_id).await
}

fn is_device_interrupt(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("underrun")
        || message.contains("overrun")
        || message.contains("xrun")
        || message.contains("device not available")
}
