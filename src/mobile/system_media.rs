use super::*;
use gpui::gpui_notifications::{
    MediaArtwork, MediaCommand, MediaMetadata, MediaPlayback, MediaSessionOptions,
    MediaSessionState, SystemMediaSession,
};
use std::io::Read;

#[derive(Default)]
pub(super) struct SystemMedia {
    session: Option<Rc<SystemMediaSession>>,
    connect: Option<Task<()>>,
    commands: Option<Task<()>>,
    artwork: Option<Task<()>>,
    background: Option<Task<()>>,
    background_error: Option<String>,
    last_update: Option<Instant>,
}

impl MobileApp {
    pub(super) fn connect_system_media(&mut self, cx: &mut Context<Self>) {
        self.retry_background_playback();
        if self.system_media.session.is_some() {
            self.publish_system_media(true, cx);
            self.load_system_artwork(cx);
            return;
        }
        if self.system_media.connect.is_some() {
            return;
        }
        let request = cx.system_media_session(MediaSessionOptions {
            app_id: "dev.romus.demo".into(),
            app_name: "Romus demo".into(),
        });
        self.system_media.connect = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |this, cx| {
                this.system_media.connect = None;
                match result {
                    Ok(mut session) => {
                        if let Some(commands) = session.take_commands() {
                            this.system_media.commands = Some(cx.spawn(async move |this, cx| {
                                while let Ok(command) = commands.recv().await {
                                    if this
                                        .update(cx, |this, cx| {
                                            this.system_media_command(command, cx);
                                        })
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }));
                        }
                        this.system_media.session = Some(Rc::new(session));
                        this.publish_system_media(true, cx);
                        this.load_system_artwork(cx);
                    }
                    Err(error) => this.error = Some(format!("Media controls: {error}")),
                }
                cx.notify();
            });
        }));
    }

    pub(super) fn publish_system_media(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(session) = self.system_media.session.clone() else {
            return;
        };
        if !force
            && self
                .system_media
                .last_update
                .is_some_and(|last| last.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        let (Some(track), Some(player)) = (self.track(), self.player.as_ref()) else {
            return;
        };
        let player = player.read(cx);
        let playback = match player.state() {
            PlaybackState::Playing => MediaPlayback::Playing,
            PlaybackState::Paused => MediaPlayback::Paused,
            PlaybackState::Loading | PlaybackState::Seeking => MediaPlayback::Buffering,
            _ => MediaPlayback::Stopped,
        };
        let result = session.update(MediaSessionState {
            metadata: MediaMetadata {
                track_id: track.id.clone(),
                title: track.title.clone(),
                artist: Some(track.artist.clone()),
                album: Some(track.album.clone()),
            },
            playback,
            position: player.position(),
            duration: player.duration(),
            rate: player.playback_rate(),
            volume: if player.is_muted() {
                0.
            } else {
                player.volume()
            },
            seekable: player.is_seekable(),
            can_next: self.queue.len() > 1,
            can_previous: self.queue.len() > 1,
            ..Default::default()
        });
        self.system_media.last_update = Some(Instant::now());
        if let Err(error) = result {
            self.error = Some(format!("Media controls: {error}"));
            self.system_media = SystemMedia::default();
            return;
        }
        if matches!(playback, MediaPlayback::Playing | MediaPlayback::Buffering)
            && self.window_active
            && self.system_media.background.is_none()
            && self.system_media.background_error.is_none()
        {
            self.system_media.background =
                Some(cx.spawn(
                    async move |this, cx| match session.start_background_playback().await {
                        Ok(mut lease) => {
                            log::debug!("Background playback lease acquired");
                            let _ = lease.stopped().await;
                            let _ = this.update(cx, |this, cx| {
                                if !this.system_media.session.as_ref().is_some_and(|current| {
                                    Rc::ptr_eq(current, &session)
                                }) {
                                    return;
                                }
                                this.system_media_command(MediaCommand::Pause, cx);
                                this.system_media.background = None;
                            });
                        }
                        Err(error) => {
                            log::warn!("Could not start background playback: {error}");
                            let _ = this.update(cx, |this, cx| {
                                if !this.system_media.session.as_ref().is_some_and(|current| {
                                    Rc::ptr_eq(current, &session)
                                }) {
                                    return;
                                }
                                this.system_media.background = None;
                                let message = "Background playback is unavailable. Return to the app and try playing again.".to_owned();
                                if this.window_active {
                                    this.error = Some(message.clone());
                                }
                                this.system_media.background_error = Some(message);
                                cx.notify();
                            });
                        }
                    },
                ));
        }
    }

    pub(super) fn retry_background_playback(&mut self) {
        if self.window_active
            && let Some(error) = self.system_media.background_error.take()
            && self.error.as_ref() == Some(&error)
        {
            self.error = None;
        }
    }

    fn load_system_artwork(&mut self, cx: &mut Context<Self>) {
        self.system_media.artwork = None;
        let (Some(session), Some(track)) = (self.system_media.session.clone(), self.track()) else {
            return;
        };
        let id = track.id.clone();
        let path = track.artwork.clone();
        let Some(path) = path else {
            if let Err(error) = session.set_artwork(None) {
                log::warn!("Could not clear system artwork: {error}");
            }
            return;
        };
        self.system_media.artwork = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    const LIMIT: u64 = 16 * 1024 * 1024;
                    let mut bytes = Vec::new();
                    std::fs::File::open(path)?
                        .take(LIMIT + 1)
                        .read_to_end(&mut bytes)?;
                    anyhow::ensure!(bytes.len() as u64 <= LIMIT, "Cover exceeds 16 MiB");
                    MediaArtwork::from_encoded(&bytes)
                })
                .await;
            let _ = this.update(cx, |this, _| {
                if this.current.as_ref() != Some(&id)
                    || !this
                        .system_media
                        .session
                        .as_ref()
                        .is_some_and(|current| Rc::ptr_eq(current, &session))
                {
                    return;
                }
                match result.and_then(|artwork| session.set_artwork(Some(artwork))) {
                    Ok(()) => {}
                    Err(error) => {
                        log::warn!("Could not publish system artwork: {error}");
                        let _ = session.set_artwork(None);
                    }
                }
            });
        }));
    }

    fn system_media_command(&mut self, command: MediaCommand, cx: &mut Context<Self>) {
        if matches!(command, MediaCommand::Play) {
            self.retry_background_playback();
        }
        match command {
            MediaCommand::Next => self.skip(1, cx),
            MediaCommand::Previous => self.skip(-1, cx),
            MediaCommand::Toggle => self.toggle(cx),
            MediaCommand::SeekTo(position) => self.seek_to(position, cx),
            MediaCommand::SeekBy(seconds) => {
                if let Some(player) = &self.player {
                    let position = player.read(cx).position().as_secs_f64() + seconds;
                    if seconds.is_finite()
                        && let Ok(position) = Duration::try_from_secs_f64(position.max(0.))
                    {
                        self.seek_to(position, cx);
                    }
                }
            }
            MediaCommand::SetVolume(volume) if volume.is_finite() => {
                let volume = volume.clamp(0., 1.);
                self.volume_slider
                    .update(cx, |slider, cx| slider.set_value(volume, cx));
                if let Some(player) = &self.player {
                    player.update(cx, |player, cx| player.set_volume(volume, cx));
                }
            }
            MediaCommand::Play | MediaCommand::Pause | MediaCommand::Stop => {
                if let Some(player) = &self.player {
                    if let Err(error) = player.update(cx, |player, cx| match command {
                        MediaCommand::Play => player.play(cx),
                        MediaCommand::Stop if player.is_seekable() => player.stop(cx),
                        _ => {
                            player.pause(cx);
                            Ok(())
                        }
                    }) {
                        self.error = Some(error.to_string());
                    }
                }
            }
            _ => {}
        }
        self.publish_system_media(true, cx);
        cx.notify();
    }
}
