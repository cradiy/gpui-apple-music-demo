mod memory;
mod music_client;
pub(crate) mod pages;
mod sidebar;
use std::{collections::HashMap, sync::Arc, time::Instant};

use gpui::{
    AnyWindowHandle, AppContext, Entity, EventEmitter, Image, Render, Task, Window, div,
    prelude::*, px, rgb, rgba,
};
use serde::{Deserialize, Serialize};
use uic::components::{
    input::{InputEvent, TextInput},
    slider::{SliderEvent, SliderState},
};

use crate::{
    app::music_shell::{
        pages::{ContentShell, ContentShellEvent},
        sidebar::{Sidebar, SidebarEvent},
    },
    domain::Page,
    state::{
        ImportEvent, ImportManager, NavigationState, PlaybackController, PlaybackEvent,
        PlaylistStore,
    },
    ui::components::{lyrics_panel, mini_player, now_playing, player_bar, queue_panel, top_bar},
    ui::strings,
    ui::theme::layout,
};

pub enum MusicEvent {
    SignedOut,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackPanel {
    #[default]
    None,
    Queue,
    InlineLyrics,
}

impl PlaybackPanel {
    pub(crate) fn toggle(&mut self, panel: Self) {
        *self = if *self == panel { Self::None } else { panel };
    }
}

pub struct EffectsViewer {
    pub track_id: Option<String>,
    pub album_cover: Arc<Image>,
    pub started_at: Instant,
}

pub struct MusicShell {
    pub navigation: NavigationState,
    pub playback: Entity<PlaybackController>,
    pub effects_viewer: Option<EffectsViewer>,
    pub sidebar: Entity<Sidebar>,
    playlists: Entity<PlaylistStore>,
    pub imports: Entity<ImportManager>,
    pub(crate) timeline_slider: Entity<SliderState>,
    pub search_input: Entity<TextInput>,
    pub songs_search_input: Entity<TextInput>,
    pub playlist_search_input: Entity<TextInput>,
    content_shell: Entity<ContentShell>,
    pub now_playing: bool,
    pub mini_window: Option<AnyWindowHandle>,
    pub queue_artwork: HashMap<String, Arc<Image>>,
    pub queue_artwork_tasks: HashMap<String, Task<()>>,
    pub queue_ui: queue_panel::QueueUiState,
    pub playback_panel: PlaybackPanel,
    pub now_playing_lyrics: bool,
    pub lyrics_panel: lyrics_panel::LyricsPanelState,
    pub user_avatar: Option<Arc<Image>>,
    pub logging_out: bool,
    avatar_task: Option<Task<()>>,
    logout_task: Option<Task<()>>,
    favorite_task: Option<Task<()>>,
    ui_memory: Option<memory::MusicShellMemoryStore>,
}

impl EventEmitter<MusicEvent> for MusicShell {}

impl MusicShell {
    pub fn new(cx: &mut gpui::Context<Self>, _window: &mut Window) -> Self {
        let search_input = cx.new(|cx| TextInput::new(cx).placeholder(strings::search_music()));
        cx.subscribe(&search_input, |app, _, event: &InputEvent, cx| {
            app.navigation.search_query = event.text().clone();
            app.navigation.page = Page::Search;
            app.content_shell.update(cx, |content, cx| {
                content.set_search_query(event.text().to_string(), cx);
            });
            cx.notify();
        })
        .detach();

        let songs_search_input =
            cx.new(|cx| TextInput::new(cx).placeholder(strings::find_in_songs()));
        cx.subscribe(&songs_search_input, |app, _, event: &InputEvent, cx| {
            app.content_shell.update(cx, |content, cx| {
                content.set_songs_query(event.text().to_string(), cx);
            });
            cx.notify();
        })
        .detach();

        let playlist_search_input =
            cx.new(|cx| TextInput::new(cx).placeholder(strings::find_in_playlist()));
        cx.subscribe(&playlist_search_input, |app, _, event: &InputEvent, cx| {
            app.content_shell.update(cx, |content, cx| {
                content.set_playlist_query(event.text().to_string(), cx);
            });
            cx.notify();
        })
        .detach();

        let playlists = cx.new(PlaylistStore::new);
        let imports = cx.new(ImportManager::new);
        cx.observe(&imports, |_, _, cx| cx.notify()).detach();

        let playlists_for_sidebar = playlists.clone();
        let sidebar = cx.new(move |cx| Sidebar::new(cx, Page::Home, playlists_for_sidebar));

        let playlists_for_content = playlists.clone();
        let sidebar_for_content = sidebar.clone();
        let imports_for_content = imports.clone();
        let content = cx.new(move |cx| {
            ContentShell::new(
                cx,
                Page::Home,
                playlists_for_content,
                sidebar_for_content,
                imports_for_content,
            )
        });
        cx.observe(&content, |_, _, cx| cx.notify()).detach();
        cx.subscribe(
            &content,
            |app, _, event: &ContentShellEvent, cx| match event {
                ContentShellEvent::Select(page) => {
                    app.navigation.page = page.clone();
                    if matches!(page, Page::PlayLists(crate::domain::PlayList::Custom(_))) {
                        app.playlist_search_input
                            .update(cx, |input, cx| input.clear(cx));
                    }
                    app.sidebar.update(cx, |sidebar, cx| {
                        sidebar.set_selected_page(page.clone(), cx)
                    });
                    cx.notify();
                }
                ContentShellEvent::Play(track) => {
                    app.playback.update(cx, |playback, cx| {
                        playback.play_track(track.as_ref().clone(), cx)
                    });
                    app.effects_viewer = None;
                    cx.notify();
                }
                ContentShellEvent::PlayFrom {
                    tracks,
                    start,
                    shuffle,
                    origin,
                } => {
                    app.playback.update(cx, |playback, cx| {
                        playback.play_from(tracks.clone(), *start, *shuffle, origin.clone(), cx)
                    });
                    app.effects_viewer = None;
                    cx.notify();
                }
                ContentShellEvent::PlayNext(track) => {
                    app.playback.update(cx, |playback, cx| {
                        playback.play_next(track.as_ref().clone(), cx)
                    });
                    uic::components::toast::success(strings::play_next(), cx);
                    cx.notify();
                }
                ContentShellEvent::Enqueue(track) => {
                    app.playback.update(cx, |playback, cx| {
                        playback.enqueue(track.as_ref().clone(), cx)
                    });
                    uic::components::toast::success(strings::add_to_queue(), cx);
                    cx.notify();
                }
                ContentShellEvent::PlayNextMany(tracks) => {
                    app.playback.update(cx, |playback, cx| {
                        playback.play_next_many(tracks.clone(), cx)
                    });
                    uic::components::toast::success(strings::play_next(), cx);
                    cx.notify();
                }
                ContentShellEvent::EnqueueMany(tracks) => {
                    app.playback
                        .update(cx, |playback, cx| playback.enqueue_many(tracks.clone(), cx));
                    uic::components::toast::success(strings::add_to_queue(), cx);
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe(&sidebar, |app, _, event: &SidebarEvent, cx| match event {
            SidebarEvent::Select(page) => {
                app.navigation.page = page.clone();
                if matches!(page, Page::PlayLists(crate::domain::PlayList::Custom(_))) {
                    app.playlist_search_input
                        .update(cx, |input, cx| input.clear(cx));
                }
                app.content_shell.update(cx, |shell, cx| {
                    shell.select_page(page.clone(), cx);
                });
                cx.notify();
            }
        })
        .detach();

        let timeline_slider = cx.new(|cx| SliderState::new(0., 0.0..=1.0, cx).disabled(true));
        let playback = cx.new(PlaybackController::new);
        cx.subscribe(&imports, |app, _, event: &ImportEvent, cx| {
            if let ImportEvent::LyricsChanged { track_id, lyrics } = event {
                app.playback.update(cx, |playback, cx| {
                    playback.replace_loaded_lyrics(track_id, lyrics, cx);
                });
            }
        })
        .detach();
        let slider_for_playback = timeline_slider.clone();
        cx.observe(&playback, move |_, playback, cx| {
            let timeline = playback.read(cx).snapshot.timeline;
            let duration = timeline.duration();
            let duration_seconds = duration.map_or(1., |duration| duration.as_secs_f64().max(1.));
            let seekable = timeline.is_seekable() && duration.is_some_and(|value| !value.is_zero());
            slider_for_playback.update(cx, |slider, cx| {
                let range = slider.range();
                if *range.start() != 0. || *range.end() != duration_seconds {
                    slider.set_range(0.0..=duration_seconds, cx);
                }
                if !slider.is_dragging() {
                    slider.set_value(timeline.position().as_secs_f64(), cx);
                }
                slider.set_disabled(!seekable, cx);
            });
            cx.notify();
        })
        .detach();
        cx.subscribe(&timeline_slider, |app, _, event: &SliderEvent, cx| {
            if let SliderEvent::Changed(seconds) = event {
                app.playback.update(cx, |playback, cx| {
                    playback.seek_to(std::time::Duration::from_secs_f64(*seconds), cx)
                });
            }
            cx.notify();
        })
        .detach();
        cx.subscribe(&playback, |_, _, event: &PlaybackEvent, cx| match event {
            PlaybackEvent::AuthenticationRequired => {
                cx.emit(MusicEvent::SignedOut);
            }
        })
        .detach();

        Self {
            navigation: NavigationState::default(),
            playback,
            content_shell: content,
            search_input,
            songs_search_input,
            playlist_search_input,
            sidebar,
            playlists,
            imports,
            timeline_slider,
            now_playing: false,
            mini_window: None,
            queue_artwork: HashMap::new(),
            queue_artwork_tasks: HashMap::new(),
            queue_ui: queue_panel::QueueUiState::default(),
            playback_panel: PlaybackPanel::None,
            effects_viewer: None,
            now_playing_lyrics: true,
            lyrics_panel: lyrics_panel::LyricsPanelState::default(),
            user_avatar: None,
            logging_out: false,
            avatar_task: None,
            logout_task: None,
            favorite_task: None,
            ui_memory: memory::MusicShellMemoryStore::load_default()
                .inspect_err(|error| {
                    log::warn!("unable to initialize music shell memory: {error:#}")
                })
                .ok(),
        }
    }

    pub fn play_track(
        &mut self,
        track: crate::services::api::TrackResponse,
        cx: &mut gpui::Context<Self>,
    ) {
        self.playback
            .update(cx, |playback, cx| playback.play_track(track, cx));
        self.effects_viewer = None;
        cx.notify();
    }

    pub fn toggle_playback(&mut self, cx: &mut gpui::Context<Self>) {
        self.playback
            .update(cx, |playback, cx| playback.toggle_playback(cx));
    }

    pub fn toggle_playback_panel(&mut self, panel: PlaybackPanel, cx: &mut gpui::Context<Self>) {
        self.playback_panel.toggle(panel);
        self.save_ui_memory(cx);
        cx.notify();
    }

    pub fn seek_by(
        &mut self,
        delta: std::time::Duration,
        backward: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.playback
            .update(cx, |playback, cx| playback.seek_by(delta, backward, cx));
    }

    pub fn stop_playback(&mut self, cx: &mut gpui::Context<Self>) {
        self.playback
            .update(cx, |playback, cx| playback.stop_playback(cx));
        self.effects_viewer = None;
    }

    pub fn clear_local_session(&mut self, cx: &mut gpui::Context<Self>) {
        self.stop_playback(cx);
        self.playback
            .update(cx, |playback, _| playback.clear_persisted());
        self.clear_ui_memory();
        self.now_playing = false;
        self.close_mini_player(cx);
        self.user_avatar = None;
        self.songs_search_input
            .update(cx, |input, cx| input.clear(cx));
        self.playlist_search_input
            .update(cx, |input, cx| input.clear(cx));
        self.imports.update(cx, |imports, cx| imports.clear(cx));
        self.playlists
            .update(cx, |playlists, cx| playlists.clear(cx));
        self.content_shell
            .update(cx, |content, cx| content.signed_out(cx));
    }

    pub fn songs_summary(&self, cx: &gpui::App) -> String {
        self.content_shell.read(cx).songs_summary(cx)
    }

    pub fn toggle_songs_sort(&mut self, cx: &mut gpui::Context<Self>) {
        self.content_shell
            .update(cx, |content, cx| content.toggle_songs_sort(cx));
    }

    pub fn toggle_playlist_sort(&mut self, cx: &mut gpui::Context<Self>) {
        self.content_shell
            .update(cx, |content, cx| content.toggle_playlist_sort(cx));
    }

    pub fn navigate(&mut self, page: Page, cx: &mut gpui::Context<Self>) {
        self.content_shell.update(cx, |content, cx| {
            content.navigate(page, cx);
        });
    }

    pub fn enter_mini_player(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.mini_window.is_some() {
            return;
        }
        match mini_player::open(
            self.playback.clone(),
            self.timeline_slider.clone(),
            window,
            cx,
        ) {
            Ok(handle) => {
                self.mini_window = Some(handle);
                if let Err(error) = window.set_mapped(false) {
                    log::error!("unable to hide the main window: {error:#}");
                }
            }
            Err(error) => log::error!("unable to open mini player: {error:#}"),
        }
    }

    pub fn close_mini_player(&mut self, cx: &mut gpui::App) {
        if let Some(handle) = self.mini_window.take() {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }
}

impl Render for MusicShell {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        #[cfg(target_os = "macos")]
        window.set_traffic_light_position(if self.now_playing {
            gpui::point(px(17.), px(28.))
        } else {
            gpui::point(px(34.), px(30.))
        });

        let width = window.bounds().size.width;
        let height = window.bounds().size.height;
        if self.now_playing {
            window.request_animation_frame();
            return now_playing::render(self, window, width.as_f32(), height.as_f32(), cx);
        }
        let wide_enough = width >= px(layout::HIDE_LYRICS_BELOW);
        let show_queue = self.playback_panel == PlaybackPanel::Queue && wide_enough;
        let show_inline_lyrics = self.playback_panel == PlaybackPanel::InlineLyrics && wide_enough;
        if show_inline_lyrics {
            window.request_animation_frame();
        }
        let compact = width < px(layout::COMPACT_TOP_BAR_BELOW);
        let playback = self.playback.read(cx).snapshot.clone();

        div()
            .relative()
            .size_full()
            .flex()
            .overflow_hidden()
            .when(cfg!(not(target_os = "windows")), |root| {
                root.rounded(px(22.))
            })
            .bg(rgb(0x20222e))
            .border_1()
            .border_color(rgba(0x00000045))
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(11.), rgba(0x00000042).into())
                    .blur_radius(px(28.))
                    .spread_radius(px(-7.)),
            ])
            .child(div().m_2().flex().flex_col().child(self.sidebar.clone()))
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
                            .child(self.content_shell.clone())
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
                    .when(show_queue, |body| {
                        body.child(queue_panel::render(self, window, cx))
                    })
                    .when(show_inline_lyrics, |body| {
                        body.child(lyrics_panel::render(
                            &playback,
                            &mut self.lyrics_panel,
                            self.playback.clone(),
                        ))
                    }),
            )
            .into_any_element()
    }
}
