use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gpui::{
    ClipboardItem, Div, Entity, EventEmitter, FontWeight, Image, ImageFormat, ObjectFit,
    PathPromptOptions, Render, SharedString, Task, div, img, prelude::*, px, rgba, svg,
};
use gpui_tokio::Tokio;
use uic::{
    assets::LucideIcons,
    components::{
        context_menu::{ContextMenu, ContextMenuItem},
        input::{Input, InputEvent, InputMode, TextInput},
        modal::{self, Modal},
        scrollbar::ScrollbarAppearance,
    },
};

use crate::ui::components::feedback as toast;
use crate::{
    app::music_shell::sidebar::Sidebar,
    domain::Page,
    services::{
        api::{
            LibraryApi, PlaylistResponse, ReplaceTrackMetadataRequest, TrackQuery, TrackResponse,
        },
        library_api, playlists_api,
    },
    state::PlaylistStore,
    ui::{
        components::track_list::{self, TrackMenuBuilder, TrackPlayHandler},
        theme,
    },
};

pub enum SongsPageEvent {
    PlayFrom {
        tracks: Vec<TrackResponse>,
        start: usize,
        shuffle: bool,
    },
    PlayNext(TrackResponse),
    Enqueue(TrackResponse),
    Navigate(Page),
}

const PAGE_SIZE: u32 = 100;

pub struct SongsPage {
    playlists: Entity<PlaylistStore>,
    sidebar: Entity<Sidebar>,
    tracks: Vec<TrackResponse>,
    total: i64,
    query: String,
    title_descending: bool,
    artwork: HashMap<String, Arc<Image>>,
    artwork_tasks: HashMap<String, Task<()>>,
    action_tasks: HashMap<String, Task<()>>,
    request_task: Option<Task<()>>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    generation: u64,
}

impl EventEmitter<SongsPageEvent> for SongsPage {}

impl SongsPage {
    pub fn new(playlists: Entity<PlaylistStore>, sidebar: Entity<Sidebar>) -> Self {
        Self {
            playlists,
            sidebar,
            tracks: Vec::new(),
            total: 0,
            query: String::new(),
            title_descending: false,
            artwork: HashMap::new(),
            artwork_tasks: HashMap::new(),
            action_tasks: HashMap::new(),
            request_task: None,
            loading: false,
            loaded: false,
            error: None,
            generation: 0,
        }
    }

    pub fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.loaded && !self.loading {
            self.load_more(cx);
        }
    }

    pub fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.tracks.clear();
        self.total = 0;
        self.artwork.clear();
        self.artwork_tasks.clear();
        self.request_task = None;
        self.loading = false;
        self.loaded = false;
        self.error = None;
        self.load_more(cx);
    }

    pub fn reset(&mut self, cx: &mut gpui::Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.tracks.clear();
        self.total = 0;
        self.query.clear();
        self.artwork.clear();
        self.artwork_tasks.clear();
        self.action_tasks.clear();
        self.request_task = None;
        self.loading = false;
        self.loaded = false;
        self.error = None;
        cx.notify();
    }

    pub fn set_query(&mut self, query: impl Into<String>, cx: &mut gpui::Context<Self>) {
        let query = query.into().trim().to_owned();
        if self.query == query {
            return;
        }
        self.query = query;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.tracks.clear();
        self.total = 0;
        self.artwork.clear();
        self.artwork_tasks.clear();
        self.request_task = None;
        self.loaded = false;
        self.loading = false;
        self.error = None;
        if self.query.is_empty() {
            self.load_more(cx);
        } else {
            self.loading = true;
            self.request_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(220))
                    .await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |page, cx| {
                    if page.generation == generation {
                        page.loading = false;
                        page.load_more(cx);
                    }
                });
            }));
        }
        cx.notify();
    }

    pub fn toggle_title_sort(&mut self, cx: &mut gpui::Context<Self>) {
        self.title_descending = !self.title_descending;
        self.sort_tracks();
        cx.notify();
    }

    pub fn summary_text(&self) -> String {
        if self.loading && self.tracks.is_empty() {
            return "Loading songs…".to_owned();
        }
        if !self.query.is_empty() {
            return format!("{} results", self.total);
        }
        let duration_ms = self
            .tracks
            .iter()
            .filter_map(|track| track.duration_ms)
            .sum::<i64>();
        let size_bytes = self
            .tracks
            .iter()
            .map(|track| track.size_bytes.max(0))
            .sum::<i64>();
        if self.loaded && self.tracks.len() as i64 == self.total {
            format!(
                "{} songs, {} total time, {}",
                self.total,
                format_total_duration(duration_ms),
                format_bytes(size_bytes)
            )
        } else if self.loaded {
            format!("{} of {} songs loaded", self.tracks.len(), self.total)
        } else {
            "Songs".to_owned()
        }
    }

    fn sort_tracks(&mut self) {
        self.tracks.sort_by(|left, right| {
            let left = left.title.as_deref().unwrap_or_default().to_lowercase();
            let right = right.title.as_deref().unwrap_or_default().to_lowercase();
            if self.title_descending {
                right.cmp(&left)
            } else {
                left.cmp(&right)
            }
        });
    }

    fn load_more(&mut self, cx: &mut gpui::Context<Self>) {
        if self.loading || self.loaded && self.tracks.len() as i64 >= self.total {
            return;
        }
        let Ok(api) = library_api(cx) else {
            self.error = Some("The library service is unavailable".to_owned());
            cx.notify();
            return;
        };
        let query = TrackQuery {
            q: (!self.query.is_empty()).then(|| self.query.clone()),
            limit: Some(PAGE_SIZE),
            offset: Some(self.tracks.len() as u64),
            ..Default::default()
        };
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        let request = Tokio::spawn(cx, async move { api.tracks(&query).await });
        self.request_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |page, cx| {
                if page.generation != generation {
                    return;
                }
                page.request_task = None;
                page.loading = false;
                match result {
                    Ok(Ok(response)) => {
                        page.total = response.total;
                        page.tracks.extend(response.items);
                        page.sort_tracks();
                        page.loaded = true;
                        page.error = None;
                        page.sync_artwork(cx);
                    }
                    Ok(Err(error)) => {
                        page.loaded = true;
                        page.error = Some(error.user_message());
                    }
                    Err(error) => {
                        page.loaded = true;
                        page.error = Some(format!("Songs request failed: {error}"));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn sync_artwork(&mut self, cx: &mut gpui::Context<Self>) {
        let Ok(api) = library_api(cx) else {
            return;
        };
        let track_ids = self
            .tracks
            .iter()
            .filter(|track| track.has_artwork)
            .map(|track| track.id.clone())
            .collect::<Vec<_>>();
        for track_id in track_ids {
            if self.artwork.contains_key(&track_id) || self.artwork_tasks.contains_key(&track_id) {
                continue;
            }
            let request_api = api.clone();
            let request_id = track_id.clone();
            let request = Tokio::spawn(
                cx,
                async move { request_api.artwork(&request_id, None).await },
            );
            let task_id = track_id.clone();
            self.artwork_tasks.insert(
                track_id,
                cx.spawn(async move |this, cx| {
                    let result = request.await;
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    this.update(cx, |page, cx| {
                        page.artwork_tasks.remove(&task_id);
                        match result {
                            Ok(Ok(response)) => {
                                if let Some(format) =
                                    ImageFormat::from_mime_type(&response.metadata.content_type)
                                {
                                    page.artwork.insert(
                                        task_id,
                                        Arc::new(Image::from_bytes(format, response.bytes)),
                                    );
                                }
                            }
                            Ok(Err(error)) => log::warn!("unable to load song artwork: {error}"),
                            Err(error) => log::warn!("song artwork task failed: {error}"),
                        }
                        cx.notify();
                    });
                }),
            );
        }
    }

    fn menu(
        entity: Entity<Self>,
        playlists: Entity<PlaylistStore>,
        sidebar: Entity<Sidebar>,
        track: TrackResponse,
        cx: &gpui::App,
    ) -> ContextMenu {
        let playlist_items = playlists.read(cx).playlists().to_vec();
        let mut recent_playlists = playlist_items.clone();
        recent_playlists.sort_by_key(|right| std::cmp::Reverse(right.updated_at));
        recent_playlists.truncate(3);

        let create_sidebar = sidebar;
        let mut add_menu = ContextMenu::new()
            .action_with(
                |_, _| playlist_create_label(),
                move |window, cx| {
                    create_sidebar.update(cx, |sidebar, cx| {
                        sidebar.open_playlist_modal(window, cx);
                    });
                },
            )
            .separator()
            .item(playlist_section("Recents"));
        if recent_playlists.is_empty() {
            add_menu = add_menu.item(playlist_empty_label("No recent playlists"));
        } else {
            for playlist in recent_playlists {
                add_menu = add_playlist_action(add_menu, &entity, &track.id, playlist);
            }
        }
        add_menu = add_menu.separator().item(playlist_section("All Playlists"));
        if playlist_items.is_empty() {
            add_menu = add_menu.item(playlist_empty_label("No playlists yet"));
        } else {
            for playlist in playlist_items {
                add_menu = add_playlist_action(add_menu, &entity, &track.id, playlist);
            }
        }

        let like_entity = entity.clone();
        let like_track_id = track.id.clone();
        let should_like = !track.favorite;
        let like_label = if should_like { "Like" } else { "Unlike" };
        let info_track = track.clone();
        let info_entity = entity.clone();
        let copy_track = track.clone();
        let artist_id = track.artist_id.clone();
        let album_id = track.album_id.clone();
        let delete_entity = entity.clone();
        let delete_label_track = track;

        let mut menu = ContextMenu::new()
            .item(ContextMenuItem::submenu_with(
                |_, _| menu_label(LucideIcons::ListMusic, "Add to Playlist"),
                add_menu,
            ))
            .separator()
            .action_with(
                |_, _| menu_label(LucideIcons::BadgeInfo, "Get Info"),
                move |window, cx| {
                    open_track_editor(info_track.clone(), Some(info_entity.clone()), window, cx)
                },
            )
            .action_with(
                move |_, _| menu_label(LucideIcons::Heart, like_label),
                move |_, cx| {
                    like_entity.update(cx, |page, cx| {
                        page.set_liked(like_track_id.clone(), should_like, cx);
                    });
                },
            )
            .separator()
            .action_with(
                |_, _| menu_label(LucideIcons::ListPlus, crate::ui::strings::play_next()),
                {
                    let entity = entity.clone();
                    let track = delete_label_track.clone();
                    move |_, cx| {
                        entity.update(cx, |_, cx| {
                            cx.emit(SongsPageEvent::PlayNext(track.clone()));
                        });
                    }
                },
            )
            .action_with(
                |_, _| menu_label(LucideIcons::ListEnd, crate::ui::strings::add_to_queue()),
                {
                    let entity = entity.clone();
                    let track = delete_label_track.clone();
                    move |_, cx| {
                        entity.update(cx, |_, cx| {
                            cx.emit(SongsPageEvent::Enqueue(track.clone()));
                        });
                    }
                },
            )
            .separator()
            .action_with(
                |_, _| menu_label(LucideIcons::Copy, "Copy"),
                move |_, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(song_clipboard_text(
                        &copy_track,
                    )));
                    toast::success("Song info copied", cx);
                },
            );
        if let Some(artist_id) = artist_id {
            let artist_entity = entity.clone();
            menu = menu.action_with(
                |_, _| menu_label(LucideIcons::MicVocal, "Go to Artist"),
                move |_, cx| {
                    artist_entity.update(cx, |_, cx| {
                        cx.emit(SongsPageEvent::Navigate(Page::Artist(
                            artist_id.clone().into(),
                        )));
                    });
                },
            );
        }
        if let Some(album_id) = album_id {
            let album_entity = entity.clone();
            menu = menu.action_with(
                |_, _| menu_label(LucideIcons::DiscAlbum, "Go to Album"),
                move |_, cx| {
                    album_entity.update(cx, |_, cx| {
                        cx.emit(SongsPageEvent::Navigate(Page::Album(
                            album_id.clone().into(),
                        )));
                    });
                },
            );
        }

        theme::style_context_menu(
            menu.separator().item(
                ContextMenuItem::action_with(
                    |_, _| menu_label(LucideIcons::Trash2, "Delete from Library"),
                    move |window, cx| {
                        confirm_delete(
                            delete_entity.clone(),
                            delete_label_track.clone(),
                            window,
                            cx,
                        );
                    },
                )
                .danger(),
            ),
        )
    }

    fn add_to_playlist(
        &mut self,
        track_id: String,
        playlist_id: String,
        playlist_name: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = format!("add:{playlist_id}:{track_id}");
        if self.action_tasks.contains_key(&key) {
            return;
        }
        let Ok(api) = playlists_api(cx) else {
            toast::error("The playlist service is unavailable", cx);
            return;
        };
        let request_playlist_id = playlist_id.clone();
        let request_track_id = track_id.clone();
        let request = Tokio::spawn(cx, async move {
            api.add_item(&request_playlist_id, &request_track_id).await
        });
        let task_key = key.clone();
        self.action_tasks.insert(
            key,
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |page, cx| {
                    page.action_tasks.remove(&task_key);
                    match result {
                        Ok(Ok(_)) => {
                            page.playlists
                                .update(cx, |playlists, cx| playlists.refresh(cx));
                            toast::success(format!("Added to {playlist_name}"), cx);
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Add to playlist failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
    }

    fn set_liked(&mut self, track_id: String, liked: bool, cx: &mut gpui::Context<Self>) {
        let key = format!("like:{track_id}");
        if self.action_tasks.contains_key(&key) {
            return;
        }
        let Ok(api) = library_api(cx) else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        let request_id = track_id.clone();
        let request = Tokio::spawn(cx, async move {
            if liked {
                api.favorite(&request_id).await
            } else {
                api.unfavorite(&request_id).await
            }
        });
        let task_key = key.clone();
        self.action_tasks.insert(
            key,
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |page, cx| {
                    page.action_tasks.remove(&task_key);
                    match result {
                        Ok(Ok(updated)) => {
                            if let Some(track) =
                                page.tracks.iter_mut().find(|track| track.id == updated.id)
                            {
                                *track = updated;
                            }
                            toast::success(if liked { "Song liked" } else { "Song unliked" }, cx);
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Like request failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
    }

    fn delete_track(&mut self, track_id: String, cx: &mut gpui::Context<Self>) {
        let key = format!("delete:{track_id}");
        if self.action_tasks.contains_key(&key) {
            return;
        }
        let Ok(api) = library_api(cx) else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        let request_id = track_id.clone();
        let request = Tokio::spawn(cx, async move { api.delete_track(&request_id).await });
        let task_key = key.clone();
        self.action_tasks.insert(
            key,
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |page, cx| {
                    page.action_tasks.remove(&task_key);
                    match result {
                        Ok(Ok(())) => {
                            page.tracks.retain(|track| track.id != track_id);
                            page.artwork.remove(&track_id);
                            page.total = page.total.saturating_sub(1);
                            toast::success("Song deleted from library", cx);
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Delete request failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
    }
}

impl Render for SongsPage {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        self.ensure_loaded(cx);
        let has_more = self.loaded && (self.tracks.len() as i64) < self.total;
        let entity = cx.entity();
        let menu_playlists = self.playlists.clone();
        let menu_sidebar = self.sidebar.clone();
        let menu_builder: TrackMenuBuilder = Rc::new(move |track, _, cx| {
            SongsPage::menu(
                entity.clone(),
                menu_playlists.clone(),
                menu_sidebar.clone(),
                track,
                cx,
            )
        });
        let play_entity = cx.entity();
        let on_play: TrackPlayHandler = Rc::new(move |track, _, cx| {
            play_entity.update(cx, |page, cx| {
                let start = page
                    .tracks
                    .iter()
                    .position(|item| item.id == track.id)
                    .unwrap_or(0);
                cx.emit(SongsPageEvent::PlayFrom {
                    tracks: page.tracks.clone(),
                    start,
                    shuffle: false,
                });
            });
        });
        div()
            .id("songs-page-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(28.))
            .pt(px(18.))
            .pb(px(118.))
            .text_color(rgba(0xeeeef4f0))
            .when_some(self.error.clone(), |page, error| {
                page.child(status_text(error))
            })
            .when(self.loading && self.tracks.is_empty(), |page| {
                page.child(status_text("Loading songs…"))
            })
            .when(self.loaded && self.tracks.is_empty(), |page| {
                page.child(empty_state("Your music library is empty"))
            })
            .when(!self.tracks.is_empty(), |page| {
                page.child(track_list::track_list(
                    &self.tracks,
                    &self.artwork,
                    menu_builder,
                    on_play,
                ))
            })
            .when(has_more, |page| {
                page.child(
                    div()
                        .id("load-more-songs")
                        .mt(px(18.))
                        .mx_auto()
                        .w(px(112.))
                        .h(px(34.))
                        .rounded(px(17.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .bg(rgba(0xffffff0d))
                        .hover(|button| button.bg(rgba(0xffffff16)))
                        .text_size(px(10.5))
                        .child(if self.loading {
                            "Loading…"
                        } else {
                            "Load More"
                        })
                        .on_click(cx.listener(|page, _, _, cx| page.load_more(cx))),
                )
            })
    }
}

fn add_playlist_action(
    menu: ContextMenu,
    entity: &Entity<SongsPage>,
    track_id: &str,
    playlist: PlaylistResponse,
) -> ContextMenu {
    let action_entity = entity.clone();
    let action_track_id = track_id.to_owned();
    let playlist_id = playlist.id;
    let playlist_name = playlist.name;
    let label_name = playlist_name.clone();
    menu.action_with(
        move |_, _| playlist_action_label(label_name.clone()),
        move |_, cx| {
            action_entity.update(cx, |page, cx| {
                page.add_to_playlist(
                    action_track_id.clone(),
                    playlist_id.clone(),
                    playlist_name.clone(),
                    cx,
                );
            });
        },
    )
}

fn playlist_create_label() -> Div {
    div()
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .child("New Playlist")
}

fn playlist_section(label: &'static str) -> ContextMenuItem {
    ContextMenuItem::action_with(
        move |_, _| {
            div()
                .text_size(px(10.5))
                .font_weight(FontWeight::BOLD)
                .child(label)
        },
        |_, _| {},
    )
    .disabled(true)
}

fn playlist_empty_label(label: &'static str) -> ContextMenuItem {
    ContextMenuItem::action_with(
        move |_, _| div().pl(px(24.)).text_size(px(11.)).child(label),
        |_, _| {},
    )
    .disabled(true)
}

fn playlist_action_label(label: String) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(11.))
        .text_size(px(12.))
        .child(
            svg()
                .path(LucideIcons::Music2)
                .size(px(16.))
                .flex_none()
                .text_color(rgba(0xe3e1e9d9)),
        )
        .child(div().min_w_0().truncate().child(label))
}

fn menu_label(icon: LucideIcons, label: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(9.))
        .text_size(px(12.))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xdedce5d8)))
        .child(label)
}

pub(crate) fn show_track_info(track: TrackResponse, window: &mut gpui::Window, cx: &mut gpui::App) {
    open_track_editor(track, None, window, cx);
}

#[allow(dead_code)]
fn show_track_info_for_page(
    track: TrackResponse,
    songs_page: Option<Entity<SongsPage>>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    let title = track.title.as_deref().unwrap_or("Untitled").to_owned();
    let artist = track
        .artist
        .as_deref()
        .unwrap_or("Unknown Artist")
        .to_owned();
    let mut details = vec![
        (
            "Album",
            track.album.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            "Album Artist",
            track.album_artist.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            "Genre",
            track.genre.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            "Duration",
            track
                .duration_ms
                .map(format_total_duration)
                .unwrap_or_else(|| "—".to_owned()),
        ),
        (
            "Format",
            format!("{} · {}", track.container.to_uppercase(), track.codec),
        ),
        ("File Size", format_bytes(track.size_bytes)),
    ];
    if let Some(year) = track.release_year {
        details.insert(3, ("Year", year.to_string()));
    }
    if let Some(sample_rate) = track.sample_rate {
        details.push(("Sample Rate", format_sample_rate(sample_rate)));
    }
    if let Some(channel_count) = track.channel_count {
        details.push(("Channels", format_channel_count(channel_count)));
    }
    if let Some(bit_depth) = track.bit_depth {
        details.push(("Bit Depth", format!("{bit_depth}-bit")));
    }

    modal::show(
        theme::style_modal(
            Modal::new(move |_, _| {
                div()
                    .w_full()
                    .py(px(5.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .pb(px(16.))
                            .mb(px(4.))
                            .border_b_1()
                            .border_color(rgba(0xffffff16))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgba(0xffffffff))
                                    .child(title.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(rgba(0xbebbc8bd))
                                    .child(artist.clone()),
                            ),
                    )
                    .children(
                        details
                            .iter()
                            .map(|(label, value)| info_row((*label).to_owned(), value.clone())),
                    )
            })
            .title_text("Song Info")
            .footer(move |_, _| {
                let edit_track = track.clone();
                let edit_page = songs_page.clone();
                div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .id("edit-song-info")
                            .w(px(88.))
                            .h(px(36.))
                            .rounded(px(10.))
                            .bg(rgba(0xffffff12))
                            .hover(|button| button.bg(rgba(0xffffff20)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Edit")
                            .on_click(move |_, window, cx| {
                                modal::dismiss(window, cx);
                                open_track_editor(
                                    edit_track.clone(),
                                    edit_page.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .child(
                        div()
                            .id("close-song-info")
                            .w(px(88.))
                            .h(px(36.))
                            .rounded(px(10.))
                            .bg(rgba(0xfa3558ff))
                            .hover(|button| button.bg(rgba(0xff496aff)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Done")
                            .on_click(|_, window, cx| modal::dismiss(window, cx)),
                    )
            })
            .w(px(470.)),
        ),
        window,
        cx,
    );
}

struct TrackEditor {
    track: TrackResponse,
    songs_page: Option<Entity<SongsPage>>,
    title: SharedString,
    artist: SharedString,
    album: SharedString,
    year: SharedString,
    lyrics: SharedString,
    title_input: Entity<TextInput>,
    artist_input: Entity<TextInput>,
    album_input: Entity<TextInput>,
    year_input: Entity<TextInput>,
    lyrics_input: Entity<TextInput>,
    artwork: Option<Arc<Image>>,
    cover_path: String,
    cover_dirty: bool,
    saving: bool,
    active_tab: TrackEditorTab,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrackEditorTab {
    Details,
    Artwork,
    Lyrics,
    File,
}

fn open_track_editor(
    track: TrackResponse,
    songs_page: Option<Entity<SongsPage>>,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    let editor = cx.new(|cx| TrackEditor::new(track, songs_page, cx));
    TrackEditor::load(editor.clone(), cx);
    TrackEditor::show(editor, window, cx);
}

impl TrackEditor {
    fn new(
        track: TrackResponse,
        songs_page: Option<Entity<SongsPage>>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let title = track.title.clone().unwrap_or_default();
        let artist = track.artist.clone().unwrap_or_default();
        let album = track.album.clone().unwrap_or_default();
        let year = track
            .release_year
            .map(|year| year.to_string())
            .unwrap_or_default();
        let title_input = cx.new(|cx| TextInput::new(cx).placeholder("Song Title"));
        let artist_input = cx.new(|cx| TextInput::new(cx).placeholder("Artist"));
        let album_input = cx.new(|cx| TextInput::new(cx).placeholder("Album"));
        let year_input = cx.new(|cx| TextInput::new(cx).placeholder("Year (Optional)"));
        let lyrics_input = cx.new(|cx| {
            TextInput::new(cx)
                .placeholder("Lyrics (Optional)")
                .mode(InputMode::Multiline)
        });
        for (input, field) in [
            (&title_input, 0u8),
            (&artist_input, 1),
            (&album_input, 2),
            (&year_input, 3),
            (&lyrics_input, 4),
        ] {
            cx.subscribe(input, move |editor, _, event: &InputEvent, cx| {
                match field {
                    0 => editor.title = event.text().clone(),
                    1 => editor.artist = event.text().clone(),
                    2 => editor.album = event.text().clone(),
                    3 => editor.year = event.text().clone(),
                    _ => editor.lyrics = event.text().clone(),
                }
                cx.notify();
            })
            .detach();
        }
        title_input.update(cx, |input, cx| input.set_value(title.clone(), cx));
        artist_input.update(cx, |input, cx| input.set_value(artist.clone(), cx));
        album_input.update(cx, |input, cx| input.set_value(album.clone(), cx));
        year_input.update(cx, |input, cx| input.set_value(year.clone(), cx));
        Self {
            track,
            songs_page,
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            year: year.into(),
            lyrics: "".into(),
            title_input,
            artist_input,
            album_input,
            year_input,
            lyrics_input,
            artwork: None,
            cover_path: String::new(),
            cover_dirty: false,
            saving: false,
            active_tab: TrackEditorTab::Details,
        }
    }

    fn load(editor: Entity<Self>, cx: &mut gpui::App) {
        let Ok(api) = LibraryApi::local() else { return };
        let track_id = editor.read(cx).track.id.clone();
        let has_artwork = editor.read(cx).track.has_artwork;
        let request_api = api.clone();
        let request = Tokio::spawn(cx, async move {
            let metadata = request_api.metadata(&track_id).await;
            let artwork = if has_artwork {
                request_api.artwork(&track_id, None).await.ok()
            } else {
                None
            };
            (metadata, artwork)
        });
        cx.spawn(async move |cx| {
            let Ok((metadata, artwork)) = request.await else {
                return;
            };
            editor.update(cx, |editor, cx| {
                if let Ok(metadata) = metadata {
                    let lyrics = metadata.lyrics.unwrap_or_default();
                    editor.lyrics = lyrics.clone().into();
                    editor
                        .lyrics_input
                        .update(cx, |input, cx| input.set_value(lyrics, cx));
                }
                if let Some(response) = artwork
                    && let Some(format) =
                        ImageFormat::from_mime_type(&response.metadata.content_type)
                {
                    editor.artwork = Some(Arc::new(Image::from_bytes(format, response.bytes)));
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn show(editor: Entity<Self>, window: &mut gpui::Window, cx: &mut gpui::App) {
        let content_editor = editor.clone();
        let footer_editor = editor.clone();
        modal::show(
            theme::style_modal(
                Modal::new(move |_, app| {
                    let state = content_editor.read(app);
                    div()
                        .w_full()
                        .overflow_y_hidden()
                        .flex()
                        .flex_col()
                        .child(track_editor_summary(state))
                        .child(track_editor_tabs(content_editor.clone(), state.active_tab))
                        .child(track_editor_page(content_editor.clone(), state))
                })
                .footer(move |_, app| {
                    let save_editor = footer_editor.clone();
                    let disabled = footer_editor.read(app).saving
                        || footer_editor.read(app).title.trim().is_empty();
                    form_footer(
                        disabled,
                        move |window, app| {
                            save_editor.update(app, |editor, cx| editor.save(window, cx))
                        },
                        modal::dismiss,
                    )
                })
                .w(px(640.))
                .ok_on_enter(false),
            )
            .appearance(
                theme::dark_modal_appearance()
                    .body_padding_x(px(0.))
                    .body_padding_y(px(0.)),
            )
            .rounded(px(14.)),
            window,
            cx,
        );
    }

    #[allow(dead_code)]
    fn show_legacy(editor: Entity<Self>, window: &mut gpui::Window, cx: &mut gpui::App) {
        let content_editor = editor.clone();
        let footer_editor = editor.clone();
        modal::show(
            theme::style_modal(
                Modal::new(move |_, app| {
                    let state = content_editor.read(app);
                    let selected_cover = state.cover_path.clone();
                    let cover_for_image = selected_cover.clone();
                    let artwork = state.artwork.clone();
                    let has_cover =
                        !selected_cover.is_empty() || (!state.cover_dirty && artwork.is_some());
                    let pick_editor = content_editor.clone();
                    let remove_editor = content_editor.clone();
                    let export_editor = content_editor.clone();
                    div()
                        .w_full()
                        .px(px(28.))
                        .pt(px(18.))
                        .pb(px(20.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .gap(px(24.))
                                .child(
                                    div()
                                        .w(px(150.))
                                        .flex_none()
                                        .flex()
                                        .flex_col()
                                        .gap(px(10.))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgba(0xaaa7b5cc))
                                                .child("ARTWORK"),
                                        )
                                        .child(
                                            div()
                                                .id("song-cover-picker")
                                                .size(px(150.))
                                                .rounded(px(14.))
                                                .overflow_hidden()
                                                .bg(rgba(0x0f1118ff))
                                                .border_1()
                                                .border_color(rgba(0xffffff20))
                                                .cursor_pointer()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .hover(|cover| cover.border_color(rgba(0xfa3558bb)))
                                                .on_click(move |_, _, app| {
                                                    pick_editor.update(app, |editor, cx| {
                                                        editor.select_cover(cx)
                                                    })
                                                })
                                                .when(!selected_cover.is_empty(), |cover| {
                                                    cover.child(
                                                        img(PathBuf::from(cover_for_image))
                                                            .size_full()
                                                            .object_fit(ObjectFit::Cover),
                                                    )
                                                })
                                                .when(
                                                    selected_cover.is_empty() && !state.cover_dirty,
                                                    |cover| {
                                                        cover.when_some(artwork, |cover, image| {
                                                            cover.child(
                                                                img(image)
                                                                    .size_full()
                                                                    .object_fit(ObjectFit::Cover),
                                                            )
                                                        })
                                                    },
                                                )
                                                .when(!has_cover, |cover| {
                                                    cover.child(
                                                        div()
                                                            .flex()
                                                            .flex_col()
                                                            .items_center()
                                                            .gap(px(7.))
                                                            .text_color(rgba(0xffffff77))
                                                            .child(
                                                                svg()
                                                                    .path(LucideIcons::ImagePlus)
                                                                    .size(px(28.)),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(10.))
                                                                    .child("Choose artwork"),
                                                            ),
                                                    )
                                                }),
                                        )
                                        .when(has_cover, |column| {
                                            column
                                                .child(cover_action(
                                                    "export-song-cover",
                                                    "Export",
                                                    false,
                                                    move |_, _, app| {
                                                        export_editor.update(app, |editor, cx| {
                                                            editor.export_cover(cx)
                                                        })
                                                    },
                                                ))
                                                .child(cover_action(
                                                    "remove-song-cover",
                                                    "Remove",
                                                    true,
                                                    move |_, _, app| {
                                                        remove_editor.update(app, |editor, cx| {
                                                            editor.cover_path.clear();
                                                            editor.cover_dirty = true;
                                                            cx.notify();
                                                        })
                                                    },
                                                ))
                                        }),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .gap(px(12.))
                                        .child(editor_field("SONG", &state.title_input))
                                        .child(editor_field("ARTIST", &state.artist_input))
                                        .child(
                                            div()
                                                .flex()
                                                .gap(px(12.))
                                                .child(div().flex_1().child(editor_field(
                                                    "ALBUM",
                                                    &state.album_input,
                                                )))
                                                .child(div().w(px(118.)).child(editor_field(
                                                    "YEAR",
                                                    &state.year_input,
                                                ))),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(22.))
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgba(0xaaa7b5cc))
                                        .child("LYRICS"),
                                )
                                .child(
                                    theme::style_field(
                                        Input::new(&state.lyrics_input)
                                            .appearance(theme::input_appearance())
                                            .rows(9)
                                            .scrollbar(|bar| {
                                                bar.w(px(12.))
                                                    .h(px(20.))
                                                    .right(px(2.))
                                                    .top(px(6.))
                                                    .bottom(px(6.))
                                            }),
                                    )
                                    .h(px(230.)),
                                ),
                        )
                })
                .title_text("Edit Song Info")
                .footer(move |_, app| {
                    let save_editor = footer_editor.clone();
                    let disabled = footer_editor.read(app).saving
                        || footer_editor.read(app).title.trim().is_empty();
                    form_footer(
                        disabled,
                        move |window, app| {
                            save_editor.update(app, |editor, cx| editor.save(window, cx))
                        },
                        modal::dismiss,
                    )
                })
                .w(px(620.))
                .ok_on_enter(false),
            ),
            window,
            cx,
        );
    }

    fn select_cover(&mut self, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select Cover Image".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
                && path.is_file()
            {
                this.update(cx, |editor, cx| {
                    editor.cover_path = path.to_string_lossy().into_owned();
                    editor.cover_dirty = true;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn export_cover(&mut self, cx: &mut gpui::Context<Self>) {
        let suggested = format!("{}-cover.jpg", safe_file_name(self.title.trim()));
        let receiver = cx.prompt_for_new_path(Path::new("."), Some(&suggested));
        let selected_path = self.cover_path.clone();
        let track_id = self.track.id.clone();
        let Ok(api) = LibraryApi::local() else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        let bytes = Tokio::spawn(cx, async move {
            if !selected_path.is_empty() {
                tokio::fs::read(selected_path).await.map_err(|error| {
                    crate::services::api::ApiError::InvalidResponse(error.to_string())
                })
            } else {
                api.artwork(&track_id, None)
                    .await
                    .map(|response| response.bytes)
            }
        });
        cx.spawn(async move |this, cx| {
            let (destination, bytes) = futures_join(receiver, bytes).await;
            let _ = this.update(cx, |_, cx| match (destination, bytes) {
                (Ok(Ok(Some(path))), Ok(Ok(bytes))) => match std::fs::write(path, bytes) {
                    Ok(()) => toast::success("Cover exported", cx),
                    Err(error) => toast::error(format!("Export cover failed: {error}"), cx),
                },
                (_, Ok(Err(error))) => toast::error(error.user_message(), cx),
                (_, Err(error)) => toast::error(format!("Export cover failed: {error}"), cx),
                _ => {}
            });
        })
        .detach();
    }

    fn save(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        let title = self.title.trim().to_owned();
        if title.is_empty() {
            toast::error("A song title is required", cx);
            return;
        }
        let release_year = if self.year.trim().is_empty() {
            None
        } else {
            match self.year.trim().parse::<i64>() {
                Ok(year) if (1000..=9999).contains(&year) => Some(year),
                _ => {
                    toast::error("Enter a valid year", cx);
                    return;
                }
            }
        };
        let Ok(api) = library_api(cx) else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        self.saving = true;
        let request_id = self.track.id.clone();
        let request = ReplaceTrackMetadataRequest {
            title: Some(title),
            artist: optional_text(&self.artist),
            album_artist: self.track.album_artist.clone(),
            album: optional_text(&self.album),
            genre: self.track.genre.clone(),
            composer: self.track.composer.clone(),
            track_number: self.track.track_number,
            disc_number: self.track.disc_number,
            release_year,
            lyrics: optional_text(&self.lyrics),
        };
        let cover_path = self
            .cover_dirty
            .then(|| PathBuf::from(&self.cover_path))
            .filter(|path| path.is_file());
        let remove_cover = self.cover_dirty && cover_path.is_none() && self.track.has_artwork;
        let songs_page = self.songs_page.clone();
        let task = Tokio::spawn(cx, async move {
            api.replace_metadata(&request_id, &request).await?;
            if let Some(path) = cover_path {
                let bytes = tokio::fs::read(&path).await.map_err(|error| {
                    crate::services::api::ApiError::InvalidResponse(format!(
                        "failed to read cover: {error}"
                    ))
                })?;
                let filename = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("cover")
                    .to_owned();
                api.replace_artwork(&request_id, filename, image_content_type(&path), bytes)
                    .await?;
            } else if remove_cover {
                api.delete_artwork(&request_id).await?;
            }
            api.track(&request_id).await
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |editor, cx| {
                editor.saving = false;
                match result {
                    Ok(Ok(_)) => {
                        if let Some(page) = songs_page {
                            page.update(cx, |page, cx| page.refresh(cx));
                        }
                        toast::success("Song info updated", cx);
                    }
                    Ok(Err(error)) => toast::error(error.user_message(), cx),
                    Err(error) => toast::error(format!("Update song failed: {error}"), cx),
                }
                cx.notify();
            });
        })
        .detach();
        modal::dismiss(window, cx);
    }
}

fn optional_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn track_editor_summary(state: &TrackEditor) -> Div {
    div()
        .h(px(116.))
        .flex_none()
        .px(px(24.))
        .bg(rgba(0xffffff18))
        .flex()
        .items_center()
        .gap(px(16.))
        .child(
            editor_cover_image(state, px(76.))
                .rounded(px(7.))
                .shadow_md(),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .truncate()
                        .text_size(px(19.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(if state.title.trim().is_empty() {
                            "Untitled".to_owned()
                        } else {
                            state.title.to_string()
                        }),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.))
                        .text_color(rgba(0xd5d2dcaa))
                        .child(if state.artist.trim().is_empty() {
                            "Unknown Artist".to_owned()
                        } else {
                            state.artist.to_string()
                        }),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(rgba(0xd5d2dc88))
                        .child(if state.album.trim().is_empty() {
                            "Unknown Album".to_owned()
                        } else {
                            state.album.to_string()
                        }),
                ),
        )
}

fn track_editor_tabs(editor: Entity<TrackEditor>, active: TrackEditorTab) -> Div {
    let tabs = [
        (TrackEditorTab::Details, "Details"),
        (TrackEditorTab::Artwork, "Artwork"),
        (TrackEditorTab::Lyrics, "Lyrics"),
        (TrackEditorTab::File, "File"),
    ];
    div()
        .mx(px(24.))
        .mt(px(14.))
        .mb(px(18.))
        .h(px(32.))
        .flex_none()
        .p(px(3.))
        .gap(px(3.))
        .rounded(px(8.))
        .bg(rgba(0xffffff09))
        .flex()
        .children(tabs.into_iter().enumerate().map(|(index, (tab, label))| {
            let tab_editor = editor.clone();
            div()
                .id(("track-info-tab", index))
                .h_full()
                .flex_1()
                .rounded(px(5.))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .bg(if active == tab {
                    rgba(0xfa1748ff)
                } else {
                    rgba(0x00000000)
                })
                .text_color(if active == tab {
                    rgba(0xffffffff)
                } else {
                    rgba(0xe2dfe8d0)
                })
                .hover(move |item| {
                    if active == tab {
                        item
                    } else {
                        item.bg(rgba(0xffffff0a))
                    }
                })
                .child(label)
                .on_click(move |_, _, app| {
                    tab_editor.update(app, |editor, cx| {
                        editor.active_tab = tab;
                        cx.notify();
                    });
                })
        }))
}

fn track_editor_page(editor: Entity<TrackEditor>, state: &TrackEditor) -> Div {
    let page = div()
        .h(px(392.))
        .flex_none()
        .border_t_1()
        .border_color(rgba(0xffffff0d))
        .overflow_hidden();
    match state.active_tab {
        TrackEditorTab::Details => page.child(
            div()
                .px(px(24.))
                .py(px(24.))
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(editor_section_title("Song Details"))
                .child(
                    div()
                        .w_full()
                        .p(px(20.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(rgba(0xffffff0c))
                        .flex()
                        .flex_col()
                        .gap(px(14.))
                        .child(editor_detail_row("Title", &state.title_input, false))
                        .child(editor_detail_row("Artist", &state.artist_input, false))
                        .child(editor_detail_row("Album", &state.album_input, false))
                        .child(editor_detail_row("Year", &state.year_input, true)),
                ),
        ),
        TrackEditorTab::Artwork => {
            let pick_image = editor.clone();
            let pick_button = editor.clone();
            let export = editor.clone();
            let remove = editor.clone();
            let has_cover =
                !state.cover_path.is_empty() || (!state.cover_dirty && state.artwork.is_some());
            page.child(
                div()
                    .py(px(18.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .child(editor_section_title("Album Artwork"))
                    .child(
                        div()
                            .id("large-song-artwork")
                            .size(px(280.))
                            .flex_none()
                            .rounded(px(9.))
                            .shadow_lg()
                            .cursor_pointer()
                            .on_click(move |_, _, app| {
                                pick_image.update(app, |editor, cx| editor.select_cover(cx))
                            })
                            .child(editor_cover_image(state, px(280.)).rounded(px(9.)).when(
                                !has_cover,
                                |cover| {
                                    cover
                                        .flex_col()
                                        .gap(px(12.))
                                        .text_size(px(12.))
                                        .text_color(rgba(0xaaa7b588))
                                        .child("Add album artwork")
                                },
                            )),
                    )
                    .child(
                        div()
                            .w(px(280.))
                            .flex()
                            .justify_between()
                            .child(cover_action(
                                "add-song-artwork",
                                if has_cover {
                                    "Replace…"
                                } else {
                                    "Add Artwork…"
                                },
                                false,
                                move |_, _, app| {
                                    pick_button.update(app, |editor, cx| editor.select_cover(cx))
                                },
                            ))
                            .when(has_cover, |actions| {
                                actions.child(
                                    div()
                                        .flex()
                                        .gap(px(8.))
                                        .child(cover_action(
                                            "export-song-artwork",
                                            "Export",
                                            false,
                                            move |_, _, app| {
                                                export.update(app, |editor, cx| {
                                                    editor.export_cover(cx)
                                                })
                                            },
                                        ))
                                        .child(cover_action(
                                            "remove-song-artwork",
                                            "Remove",
                                            true,
                                            move |_, _, app| {
                                                remove.update(app, |editor, cx| {
                                                    editor.cover_path.clear();
                                                    editor.cover_dirty = true;
                                                    cx.notify();
                                                })
                                            },
                                        )),
                                )
                            }),
                    ),
            )
        }
        TrackEditorTab::Lyrics => page.child(
            div()
                .p(px(24.))
                .size_full()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(editor_section_title("Lyrics"))
                .child(
                    editor_input(&state.lyrics_input)
                        .rows(12)
                        .scrollbar(|bar| {
                            bar.appearance(
                                ScrollbarAppearance::default()
                                    .thumb(rgba(0xd5d2dc55).into())
                                    .hover_thumb(rgba(0xd5d2dc80).into())
                                    .dragging_thumb(rgba(0xd5d2dcaa).into())
                                    .focus_ring(theme::palette().focus),
                            )
                        })
                        .h_full()
                        .min_h_0()
                        .flex_1()
                        .py(px(10.)),
                ),
        ),
        TrackEditorTab::File => page.child(
            div()
                .p(px(24.))
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(editor_section_title("File Information"))
                .child(
                    div()
                        .px(px(16.))
                        .py(px(4.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(rgba(0xffffff0c))
                        .child(file_row(
                            "Kind",
                            format!("{} audio file", state.track.container.to_uppercase()),
                        ))
                        .child(file_row(
                            "Duration",
                            state
                                .track
                                .duration_ms
                                .map(format_total_duration)
                                .unwrap_or_else(|| "—".into()),
                        ))
                        .child(file_row("Size", format_bytes(state.track.size_bytes)))
                        .child(file_row("Codec", state.track.codec.clone()))
                        .child(file_row(
                            "Sample Rate",
                            state
                                .track
                                .sample_rate
                                .map(format_sample_rate)
                                .unwrap_or_else(|| "—".into()),
                        ))
                        .child(file_row(
                            "Channels",
                            state
                                .track
                                .channel_count
                                .map(format_channel_count)
                                .unwrap_or_else(|| "—".into()),
                        )),
                ),
        ),
    }
}

fn editor_cover_image(state: &TrackEditor, size: gpui::Pixels) -> Div {
    div()
        .size(size)
        .flex_none()
        .overflow_hidden()
        .bg(rgba(0x11131aff))
        .flex()
        .items_center()
        .justify_center()
        .when(!state.cover_path.is_empty(), |cover| {
            cover.child(
                img(PathBuf::from(&state.cover_path))
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            )
        })
        .when(state.cover_path.is_empty() && !state.cover_dirty, |cover| {
            cover.when_some(state.artwork.clone(), |cover, image| {
                cover.child(img(image).size_full().object_fit(ObjectFit::Contain))
            })
        })
        .when(
            state.cover_dirty && state.cover_path.is_empty()
                || !state.cover_dirty && state.artwork.is_none(),
            |cover| {
                cover.child(
                    svg()
                        .path(LucideIcons::Music2)
                        .size(px(34.))
                        .text_color(rgba(0xffffff55)),
                )
            },
        )
}

fn file_row(label: &'static str, value: String) -> Div {
    div()
        .min_h(px(42.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.))
        .text_size(px(13.))
        .child(div().flex_none().text_color(rgba(0xaaa7b5bb)).child(label))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_color(rgba(0xeeeef0e5))
                .child(value),
        )
}

fn editor_section_title(label: &'static str) -> Div {
    div()
        .flex_none()
        .text_size(px(13.))
        .line_height(px(18.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba(0xbcb9c6cc))
        .child(label)
}

fn editor_input(input: &Entity<TextInput>) -> Input {
    theme::style_field(
        Input::new(input).appearance(theme::input_appearance().caret_height(px(18.))),
    )
    .h(px(34.))
    .w_full()
    .px(px(10.))
    .rounded(px(6.))
    .text_size(px(13.))
    .line_height(px(20.))
}

fn editor_detail_row(label: &'static str, input: &Entity<TextInput>, compact: bool) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(16.))
        .child(
            div()
                .w(px(64.))
                .flex_none()
                .text_right()
                .text_size(px(13.))
                .text_color(rgba(0xaaa7b5cc))
                .child(label),
        )
        .child(
            div()
                .min_w_0()
                .when(compact, |field| field.w(px(110.)))
                .when(!compact, |field| field.flex_1())
                .child(editor_input(input)),
        )
}

fn editor_field(label: &'static str, input: &Entity<TextInput>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(0xaaa7b5cc))
                .child(label),
        )
        .child(theme::style_field(
            Input::new(input).appearance(theme::input_appearance()),
        ))
}

fn cover_action(
    id: &'static str,
    label: &'static str,
    danger: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(30.))
        .px(px(12.))
        .flex_none()
        .rounded(px(6.))
        .border_1()
        .border_color(rgba(0xffffff14))
        .text_size(px(12.))
        .text_color(if danger {
            rgba(0xff6684e0)
        } else {
            rgba(0xe5e3ead8)
        })
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .hover(|button| button.bg(rgba(0xffffff0d)))
        .on_click(on_click)
        .child(label)
}

async fn futures_join<A, B>(first: A, second: B) -> (A::Output, B::Output)
where
    A: std::future::Future,
    B: std::future::Future,
{
    tokio::join!(first, second)
}

fn safe_file_name(value: &str) -> String {
    let name = value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character => character,
        })
        .collect::<String>();
    if name.trim().is_empty() {
        "song".to_owned()
    } else {
        name
    }
}

fn form_footer(
    disabled: bool,
    on_save: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
    on_cancel: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .h(px(62.))
        .flex_none()
        .px(px(24.))
        .border_t_1()
        .border_color(rgba(0xffffff0d))
        .flex()
        .items_center()
        .justify_end()
        .gap(px(8.))
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .child(
            div()
                .id("song-edit-cancel")
                .w(px(80.))
                .h(px(30.))
                .rounded(px(6.))
                .bg(rgba(0xffffff12))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .child("Cancel")
                .on_click(move |_, window, app| on_cancel(window, app)),
        )
        .child(
            div()
                .id("song-edit-save")
                .w(px(80.))
                .h(px(30.))
                .rounded(px(6.))
                .bg(if disabled {
                    rgba(0x292d39ff)
                } else {
                    rgba(0xfa3558ff)
                })
                .text_color(if disabled {
                    rgba(0x8c8f9b9c)
                } else {
                    rgba(0xffffffff)
                })
                .flex()
                .items_center()
                .justify_center()
                .when(!disabled, |button| {
                    button
                        .cursor_pointer()
                        .on_click(move |_, window, app| on_save(window, app))
                })
                .child("Save"),
        )
}

fn image_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

fn info_row(label: String, value: String) -> Div {
    div()
        .min_h(px(34.))
        .py(px(7.))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgba(0xffffff0c))
        .child(
            div()
                .w(px(112.))
                .flex_none()
                .text_size(px(10.5))
                .text_color(rgba(0xaaa7b59c))
                .child(label),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_size(px(11.))
                .text_color(rgba(0xeeeef0e5))
                .child(value),
        )
}

fn format_sample_rate(sample_rate: i64) -> String {
    if sample_rate % 1_000 == 0 {
        format!("{} kHz", sample_rate / 1_000)
    } else {
        format!("{:.1} kHz", sample_rate as f64 / 1_000.)
    }
}

fn format_channel_count(channel_count: i64) -> String {
    match channel_count {
        1 => "Mono".to_owned(),
        2 => "Stereo".to_owned(),
        count => format!("{count} channels"),
    }
}

fn confirm_delete(
    entity: Entity<SongsPage>,
    track: TrackResponse,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    let title = track.title.as_deref().unwrap_or("Untitled").to_owned();
    let body_title = title.clone();
    let track_id = track.id;
    let delete_entity = entity;
    let appearance = theme::dark_modal_appearance().ok_button(
        theme::dark_modal_appearance()
            .ok_button
            .background(rgba(0xe73558ff).into())
            .border(rgba(0xe73558ff).into())
            .hover_background(rgba(0xf24868ff).into()),
    );
    modal::show(
        theme::style_modal(Modal::new(move |_, _| {
            div()
                .py(px(8.))
                .text_size(px(12.))
                .text_color(rgba(0xc7c5cfca))
                .child(format!(
                    "Remove “{body_title}” from your local library? The original file will not be deleted."
                ))
        })
        .title_text("Delete Song")
        .ok_label("Delete")
        .cancel_label("Cancel")
        .on_ok(move |_, cx| {
            delete_entity.update(cx, |page, cx| page.delete_track(track_id.clone(), cx));
            true
        })
        .appearance(appearance)
        .w(px(420.))),
        window,
        cx,
    );
}

pub(crate) fn song_clipboard_text(track: &TrackResponse) -> String {
    let title = track.title.as_deref().unwrap_or("Untitled");
    let artist = track.artist.as_deref().unwrap_or("Unknown Artist");
    match track.album.as_deref() {
        Some(album) => format!("{title} — {artist}\nAlbum: {album}"),
        None => format!("{title} — {artist}"),
    }
}

fn format_total_duration(duration_ms: i64) -> String {
    let total_seconds = duration_ms.max(0) / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = total_seconds % 3_600 / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn format_bytes(size_bytes: i64) -> String {
    const MIB: f64 = 1024. * 1024.;
    const GIB: f64 = 1024. * MIB;
    let size_bytes = size_bytes.max(0) as f64;
    if size_bytes >= GIB {
        format!("{:.1} GB", size_bytes / GIB)
    } else {
        format!("{:.0} MB", size_bytes / MIB)
    }
}

fn status_text(message: impl Into<String>) -> Div {
    div()
        .py(px(22.))
        .text_size(px(11.))
        .text_color(rgba(0xb5b3c0a8))
        .child(message.into())
}

fn empty_state(message: impl Into<String>) -> Div {
    div()
        .h(px(240.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.))
        .text_color(rgba(0xb5b3c08a))
        .child(message.into())
}
