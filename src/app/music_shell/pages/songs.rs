use std::{collections::HashMap, rc::Rc, sync::Arc, time::Duration};

use gpui::{
    ClipboardItem, Div, Entity, EventEmitter, FontWeight, Image, ImageFormat, Render, Task, div,
    prelude::*, px, rgba, svg,
};
use gpui_tokio::Tokio;
use uic::{
    assets::LucideIcons,
    components::{
        context_menu::{ContextMenu, ContextMenuItem},
        modal::{self, Modal},
        toast,
    },
};

use crate::{
    app::music_shell::sidebar::Sidebar,
    domain::Page,
    services::api::{LibraryApi, PlaylistResponse, PlaylistsApi, TrackQuery, TrackResponse},
    state::PlaylistStore,
    ui::components::track_list::{self, TrackMenuBuilder, TrackPlayHandler},
    ui::theme,
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
                move |window, cx| show_track_info(info_track.clone(), window, cx),
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
        .text_size(px(13.))
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
        .text_size(px(12.5))
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
        .text_size(px(12.5))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xdedce5d8)))
        .child(label)
}

pub(crate) fn show_track_info(track: TrackResponse, window: &mut gpui::Window, cx: &mut gpui::App) {
    let title = track.title.as_deref().unwrap_or("Untitled").to_owned();
    let artist = track
        .artist
        .as_deref()
        .unwrap_or("Unknown Artist")
        .to_owned();
    let mut details = vec![
        ("Album", track.album.unwrap_or_else(|| "—".to_owned())),
        (
            "Album Artist",
            track.album_artist.unwrap_or_else(|| "—".to_owned()),
        ),
        ("Genre", track.genre.unwrap_or_else(|| "—".to_owned())),
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
            .footer(|_, _| {
                div().w_full().flex().justify_end().child(
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

fn library_api<C: 'static>(cx: &gpui::Context<C>) -> Result<LibraryApi, String> {
    let _ = cx;
    LibraryApi::local().map_err(|error| error.to_string())
}

fn playlists_api<C: 'static>(cx: &gpui::Context<C>) -> Result<PlaylistsApi, String> {
    let _ = cx;
    PlaylistsApi::local().map_err(|error| error.to_string())
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
