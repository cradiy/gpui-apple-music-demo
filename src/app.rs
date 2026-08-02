use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    AppContext, Entity, Image, Render, Subscription, Window, div, prelude::*, px, rgb, rgba,
};
use gpui_media::{
    MediaSource, PlaybackState as MediaPlaybackState, SeekMode, VideoPlayer, VideoPlayerEvent,
    VideoPlayerOptions,
};
use uic::components::input::{InputEvent, TextInput};

#[cfg(target_os = "linux")]
use crate::mpris::{MprisBridge, MprisCommand};
use crate::{
    components::{content, lyrics_panel, now_playing, player_bar, sidebar, top_bar},
    state::{NavigationState, Page, PlaybackState, TrackInfo},
    theme::layout,
};

pub struct EffectsViewer {
    pub album_cover: Arc<Image>,
    pub started_at: Instant,
}

pub struct AppleMusicApp {
    pub navigation: NavigationState,
    pub playback: PlaybackState,
    pub player: Entity<VideoPlayer>,
    pub effects_viewer: Option<EffectsViewer>,
    pub search_input: Entity<TextInput>,
    pub now_playing: bool,
    pub now_playing_lyrics: bool,
    pub lyrics_panel: lyrics_panel::LyricsPanelState,
    #[cfg(target_os = "linux")]
    mpris: Option<MprisBridge>,
    _subscriptions: Vec<Subscription>,
}

impl AppleMusicApp {
    pub fn new(
        source: MediaSource,
        track: TrackInfo,
        cx: &mut gpui::Context<Self>,
        window: &mut Window,
    ) -> Self {
        let search_input = cx.new(|cx| TextInput::new(cx).placeholder("Music"));
        let search_subscription = cx.subscribe(&search_input, |app, _, event: &InputEvent, cx| {
            app.navigation.search_query = event.text().clone();
            app.navigation.page = Page::Search;
            cx.notify();
        });
        cx.focus_view(&search_input, window);

        let player = cx.new(|cx| {
            VideoPlayer::new_in_window(
                source,
                VideoPlayerOptions {
                    timeline_update_interval: Duration::from_millis(33),
                    ..VideoPlayerOptions::default()
                },
                window,
                cx,
            )
            .expect("failed to create audio playback session")
        });
        let playback = PlaybackState::new(track);
        #[cfg(target_os = "linux")]
        let mpris = match MprisBridge::start(&playback) {
            Ok(mpris) => Some(mpris),
            Err(error) => {
                eprintln!("Unable to start Linux MPRIS service: {error}");
                None
            }
        };
        #[cfg(target_os = "linux")]
        if let Some(commands) = mpris.as_ref().map(MprisBridge::commands) {
            cx.spawn(async move |this, cx| {
                while let Ok(command) = commands.recv().await {
                    let Some(this) = this.upgrade() else {
                        break;
                    };
                    this.update(cx, |app, cx| app.handle_mpris_command(command, cx));
                }
            })
            .detach();
        }
        let player_subscription = cx.subscribe(&player, |app, _, event: &VideoPlayerEvent, cx| {
            match event {
                VideoPlayerEvent::StateChanged(status) => {
                    app.playback.status = status.clone();
                    app.playback.error = match status {
                        MediaPlaybackState::Error(error) => Some(error.to_string().into()),
                        _ => None,
                    };
                }
                VideoPlayerEvent::TimelineChanged(timeline) => {
                    app.playback.timeline = *timeline;
                }
                VideoPlayerEvent::VolumeChanged { volume, muted } => {
                    app.playback.volume = *volume;
                    app.playback.muted = *muted;
                }
                _ => {}
            }
            #[cfg(target_os = "linux")]
            if let Some(mpris) = &app.mpris {
                mpris.publish(&app.playback);
            }
            cx.notify();
        });

        Self {
            navigation: NavigationState::default(),
            playback,
            player,
            search_input,
            now_playing: false,
            effects_viewer: None,
            now_playing_lyrics: true,
            lyrics_panel: lyrics_panel::LyricsPanelState::default(),
            #[cfg(target_os = "linux")]
            mpris,
            _subscriptions: vec![search_subscription, player_subscription],
        }
    }

    #[cfg(target_os = "linux")]
    fn handle_mpris_command(&mut self, command: MprisCommand, cx: &mut gpui::Context<Self>) {
        let result = self.player.update(cx, |player, cx| match command {
            MprisCommand::Play => player.play(cx),
            MprisCommand::Pause => player.pause(cx),
            MprisCommand::PlayPause => player.toggle_playback(cx),
            MprisCommand::Stop => player.stop(cx),
            MprisCommand::Seek(offset) => {
                let amount = Duration::from_micros(offset.unsigned_abs());
                if offset >= 0 {
                    player.skip_forward(amount, SeekMode::Accurate, cx)
                } else {
                    player.skip_backward(amount, SeekMode::Accurate, cx)
                }
            }
            MprisCommand::SetPosition(position) => player.seek_to(
                Duration::from_micros(position.max(0) as u64),
                SeekMode::Accurate,
                cx,
            ),
            MprisCommand::SetVolume(volume) => {
                player.set_muted(false, cx);
                player.set_volume(volume, cx);
                Ok(())
            }
            MprisCommand::Next => {
                player.skip_forward(Duration::from_secs(10), SeekMode::Accurate, cx)
            }
            MprisCommand::Previous => {
                player.skip_backward(Duration::from_secs(10), SeekMode::Accurate, cx)
            }
        });
        if let Err(error) = result {
            self.playback.error = Some(error.to_string().into());
            cx.notify();
        }
    }
}

impl Render for AppleMusicApp {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let width = window.bounds().size.width;
        let height = window.bounds().size.height;
        if self.now_playing {
            window.request_animation_frame();
            return now_playing::render(self, width.as_f32(), height.as_f32(), cx);
        }
        let hide_lyrics = width < px(layout::HIDE_LYRICS_BELOW);
        let compact = width < px(layout::COMPACT_TOP_BAR_BELOW);

        div()
            .size_full()
            .font_family(crate::FONT_FAMILY)
            .flex()
            .overflow_hidden()
            .rounded(px(22.))
            .bg(rgb(0x20222e))
            .border_1()
            .border_color(rgba(0x00000045))
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(11.), rgba(0x00000042).into())
                    .blur_radius(px(28.))
                    .spread_radius(px(-7.)),
            ])
            .child(
                div()
                    .m_5()
                    .flex()
                    .flex_col()
                    .child(sidebar::render(self, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .bg(rgb(0x20222d))
                            .child(top_bar::render(self, compact, cx))
                            .child(content::render(self.navigation.page))
                            .child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .bottom(px(14.))
                                    .flex()
                                    .justify_center()
                                    .child(player_bar::render(self, cx, compact)),
                            ),
                    )
                    .when(!hide_lyrics, |body| {
                        body.child(lyrics_panel::render(&self.playback, &mut self.lyrics_panel))
                    }),
            )
            .into_any_element()
    }
}
