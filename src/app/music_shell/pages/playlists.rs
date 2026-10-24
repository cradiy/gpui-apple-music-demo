use gpui_icons::GpuiIcons;
use std::{collections::HashMap, rc::Rc, sync::Arc};

use gpui::{
    AnyElement, ClipboardItem, Entity, EventEmitter, FontWeight, Image, ImageFormat, MouseButton,
    Render, SharedString, Task, div, prelude::*, px, rgba, svg,
};
use gpui_tokio::Tokio;
use uic::components::context_menu::{self, ContextMenu, ContextMenuExt, ContextMenuItem};

use super::ContentShell;
use crate::ui::components::feedback as toast;
use crate::{
    app::music_shell::sidebar::{self, Sidebar},
    domain::{Page, PlayList},
    resources::RomusIcons,
    services::api::{
        LibraryApi, PlaylistDetailResponse, PlaylistItemSort, PlaylistResponse, PlaylistsApi,
        TrackResponse,
    },
    state::{PlaylistStore, QueueOrigin},
    ui::{
        components::{
            media_cover::{self, CoverSource},
            track_list::{self, TrackMenuBuilder, TrackPlayHandler},
        },
        theme,
    },
};

pub enum PlaylistPageEvent {
    PlayFrom {
        tracks: Vec<TrackResponse>,
        start: usize,
        shuffle: bool,
    },
    PlayNext(TrackResponse),
    Enqueue(TrackResponse),
}

pub struct PlaylistPage {
    playlists: Entity<PlaylistStore>,
    sidebar: Entity<Sidebar>,
    playlist_id: Option<String>,
    detail: Option<PlaylistDetailResponse>,
    query: String,
    title_sort_descending: Option<bool>,
    artwork: HashMap<String, Arc<Image>>,
    artwork_tasks: HashMap<String, Task<()>>,
    action_tasks: HashMap<String, Task<()>>,
    request_task: Option<Task<()>>,
    loading: bool,
    error: Option<String>,
    generation: u64,
}

impl EventEmitter<PlaylistPageEvent> for PlaylistPage {}

impl PlaylistPage {
    pub fn new(
        playlists: Entity<PlaylistStore>,
        sidebar: Entity<Sidebar>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        cx.observe(&playlists, |page, store, cx| {
            if let Some(detail) = page.detail.as_mut()
                && let Some(updated) = store
                    .read(cx)
                    .playlists()
                    .iter()
                    .find(|playlist| playlist.id == detail.playlist.id)
                    .cloned()
            {
                detail.playlist = updated;
            }
            cx.notify();
        })
        .detach();
        Self {
            playlists,
            sidebar,
            playlist_id: None,
            detail: None,
            query: String::new(),
            title_sort_descending: None,
            artwork: HashMap::new(),
            artwork_tasks: HashMap::new(),
            action_tasks: HashMap::new(),
            request_task: None,
            loading: false,
            error: None,
            generation: 0,
        }
    }

    pub fn playing_origin(&self) -> Option<QueueOrigin> {
        self.detail.as_ref().map(|detail| QueueOrigin::Playlist {
            id: detail.playlist.id.clone(),
            name: detail.playlist.name.clone(),
        })
    }

    pub fn select(&mut self, playlist_id: String, cx: &mut gpui::Context<Self>) {
        if self.playlist_id.as_deref() == Some(&playlist_id) && self.detail.is_some() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.playlist_id = Some(playlist_id);
        self.detail = None;
        self.query.clear();
        self.artwork.clear();
        self.artwork_tasks.clear();
        self.action_tasks.clear();
        self.request_task = None;
        self.loading = false;
        self.error = None;
        self.load(cx);
    }

    pub fn refresh(&mut self, playlist_id: String, cx: &mut gpui::Context<Self>) {
        if self.playlist_id.as_deref() != Some(playlist_id.as_str()) {
            self.detail = None;
            self.query.clear();
            self.artwork.clear();
            self.artwork_tasks.clear();
            self.action_tasks.clear();
        }
        self.generation = self.generation.wrapping_add(1);
        self.playlist_id = Some(playlist_id);
        self.request_task = None;
        self.loading = false;
        self.error = None;
        self.load(cx);
    }

    pub fn reset(&mut self, cx: &mut gpui::Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.playlist_id = None;
        self.detail = None;
        self.query.clear();
        self.artwork.clear();
        self.artwork_tasks.clear();
        self.action_tasks.clear();
        self.request_task = None;
        self.loading = false;
        self.error = None;
        cx.notify();
    }

    pub fn set_query(&mut self, query: impl Into<String>, cx: &mut gpui::Context<Self>) {
        self.query = query.into().trim().to_owned();
        cx.notify();
    }

    pub fn toggle_title_sort(&mut self, cx: &mut gpui::Context<Self>) {
        self.title_sort_descending = match self.title_sort_descending {
            None => Some(false),
            Some(false) => Some(true),
            Some(true) => None,
        };
        cx.notify();
    }

    fn load(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(playlist_id) = self.playlist_id.clone() else {
            return;
        };
        let Ok(api) = playlists_api(cx) else {
            self.error = Some("The playlist service is unavailable".to_owned());
            cx.notify();
            return;
        };
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        let request = Tokio::spawn(cx, async move {
            api.get(&playlist_id, PlaylistItemSort::Custom).await
        });
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
                    Ok(Ok(detail)) => {
                        page.detail = Some(detail);
                        page.error = None;
                        page.sync_artwork(cx);
                    }
                    Ok(Err(error)) => page.error = Some(error.user_message()),
                    Err(error) => page.error = Some(format!("Playlist request failed: {error}")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn sync_artwork(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let Ok(api) = library_api(cx) else {
            return;
        };
        let track_ids = detail
            .items
            .iter()
            .filter(|item| item.track.has_artwork)
            .map(|item| item.track.id.clone())
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
                        if let Ok(Ok(response)) = result
                            && let Some(format) =
                                ImageFormat::from_mime_type(&response.metadata.content_type)
                        {
                            page.artwork.insert(
                                task_id,
                                Arc::new(Image::from_bytes(format, response.bytes)),
                            );
                        }
                        cx.notify();
                    });
                }),
            );
        }
    }

    fn visible_tracks(&self) -> Vec<TrackResponse> {
        let query = self.query.to_lowercase();
        let mut tracks = self
            .detail
            .as_ref()
            .map(|detail| {
                detail
                    .items
                    .iter()
                    .filter(|item| {
                        query.is_empty()
                            || [&item.track.title, &item.track.artist, &item.track.album]
                                .into_iter()
                                .flatten()
                                .any(|value| value.to_lowercase().contains(&query))
                    })
                    .map(|item| item.track.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(descending) = self.title_sort_descending {
            tracks.sort_by(|left, right| {
                let left = left.title.as_deref().unwrap_or_default().to_lowercase();
                let right = right.title.as_deref().unwrap_or_default().to_lowercase();
                if descending {
                    right.cmp(&left)
                } else {
                    left.cmp(&right)
                }
            });
        }
        tracks
    }

    fn remove_track(&mut self, track_id: String, cx: &mut gpui::Context<Self>) {
        let Some(playlist_id) = self.playlist_id.clone() else {
            return;
        };
        let Some(item_id) = self.detail.as_ref().and_then(|detail| {
            detail
                .items
                .iter()
                .find(|item| item.track.id == track_id)
                .map(|item| item.id.clone())
        }) else {
            return;
        };
        let key = format!("remove:{item_id}");
        if self.action_tasks.contains_key(&key) {
            return;
        }
        let Ok(api) = playlists_api(cx) else {
            toast::error("The playlist service is unavailable", cx);
            return;
        };
        let request_playlist_id = playlist_id;
        let request_item_id = item_id.clone();
        let request = Tokio::spawn(cx, async move {
            api.remove_item(&request_playlist_id, &request_item_id)
                .await
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
                        Ok(Ok(())) => {
                            if let Some(detail) = page.detail.as_mut() {
                                detail.items.retain(|item| item.id != item_id);
                                detail.playlist.track_count = detail.items.len() as i64;
                                detail.playlist.duration_ms = detail
                                    .items
                                    .iter()
                                    .filter_map(|item| item.track.duration_ms)
                                    .sum();
                            }
                            page.artwork.remove(&track_id);
                            page.playlists.update(cx, |store, cx| store.refresh(cx));
                            toast::success("Removed from playlist", cx);
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => {
                            toast::error(format!("Remove from playlist failed: {error}"), cx)
                        }
                    }
                    cx.notify();
                });
            }),
        );
    }

    fn track_menu(entity: Entity<Self>, track: TrackResponse) -> ContextMenu {
        let copy_track = track.clone();
        let remove_entity = entity;
        let remove_track_id = track.id;
        theme::style_context_menu(
            ContextMenu::new()
                .action_with(
                    |_, _| menu_label(GpuiIcons::LucideListPlus, crate::ui::strings::play_next()),
                    {
                        let entity = remove_entity.clone();
                        let track = copy_track.clone();
                        move |_, cx| {
                            entity.update(cx, |_, cx| {
                                cx.emit(PlaylistPageEvent::PlayNext(track.clone()));
                            });
                        }
                    },
                )
                .action_with(
                    |_, _| menu_label(GpuiIcons::LucideListEnd, crate::ui::strings::add_to_queue()),
                    {
                        let entity = remove_entity.clone();
                        let track = copy_track.clone();
                        move |_, cx| {
                            entity.update(cx, |_, cx| {
                                cx.emit(PlaylistPageEvent::Enqueue(track.clone()));
                            });
                        }
                    },
                )
                .separator()
                .action_with(
                    |_, _| menu_label(GpuiIcons::LucideCopy, "Copy"),
                    move |_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(song_clipboard_text(
                            &copy_track,
                        )));
                        toast::success("Song info copied", cx);
                    },
                )
                .separator()
                .item(
                    ContextMenuItem::action_with(
                        |_, _| menu_label(GpuiIcons::LucideListMinus, "Remove from Playlist"),
                        move |_, cx| {
                            remove_entity.update(cx, |page, cx| {
                                page.remove_track(remove_track_id.clone(), cx)
                            });
                        },
                    )
                    .danger(),
                ),
        )
    }
}

impl Render for PlaylistPage {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let tracks = self.visible_tracks();
        let detail = self.detail.clone();
        let cover = detail.as_ref().and_then(|detail| {
            self.playlists
                .read(cx)
                .cover(&detail.playlist.id)
                .map(CoverSource::Path)
        });
        let entity = cx.entity();
        let menu_builder: TrackMenuBuilder =
            Rc::new(move |track, _, _| PlaylistPage::track_menu(entity.clone(), track));
        let play_entity = cx.entity();
        let on_play: TrackPlayHandler = Rc::new(move |track, _, cx| {
            play_entity.update(cx, |page, cx| {
                let tracks = page.visible_tracks();
                let start = tracks
                    .iter()
                    .position(|item| item.id == track.id)
                    .unwrap_or(0);
                cx.emit(PlaylistPageEvent::PlayFrom {
                    tracks,
                    start,
                    shuffle: false,
                });
            });
        });

        div()
            .id("playlist-detail-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(28.))
            .pt(px(20.))
            .pb(px(118.))
            .text_color(rgba(0xf1eff5ef))
            .when(self.loading && detail.is_none(), |page| {
                page.child(status_text("Loading playlist…"))
            })
            .when_some(self.error.clone(), |page, error| {
                page.child(status_text(error))
            })
            .when_some(detail, |page, detail| {
                let playlist = detail.playlist;
                page.child(playlist_header(
                    &playlist,
                    cover,
                    &self.sidebar,
                    &tracks,
                    cx,
                ))
                .child(
                    div()
                        .mt(px(28.))
                        .when(tracks.is_empty(), |list| {
                            list.child(status_text(if self.query.is_empty() {
                                "This playlist is empty"
                            } else {
                                "No matching songs"
                            }))
                        })
                        .when(!tracks.is_empty(), |list| {
                            list.child(track_list::track_list(
                                &tracks,
                                &self.artwork,
                                menu_builder,
                                on_play,
                            ))
                        }),
                )
            })
    }
}

pub fn render_all(
    playlists: &Entity<PlaylistStore>,
    sidebar: &Entity<Sidebar>,
    cx: &mut gpui::Context<ContentShell>,
) -> AnyElement {
    let store = playlists.read(cx);
    let items = store
        .playlists()
        .iter()
        .map(|playlist| (playlist.clone(), store.cover(&playlist.id)))
        .collect::<Vec<_>>();
    let loading = store.loading();
    let error = store.error().map(str::to_owned);

    div()
        .id("all-playlists-scroll")
        .size_full()
        .overflow_y_scroll()
        .px(px(28.))
        .pt(px(24.))
        .pb(px(118.))
        .text_color(rgba(0xf1eff5ef))
        .child(
            div()
                .text_size(px(18.))
                .font_weight(FontWeight::BOLD)
                .child("Playlists"),
        )
        .when(loading && items.is_empty(), |page| {
            page.child(status_text("Loading playlists…"))
        })
        .when_some(error, |page, error| page.child(status_text(error)))
        .child(
            div()
                .mt(px(15.))
                .flex()
                .flex_wrap()
                .gap_x(px(17.))
                .gap_y(px(23.))
                .child(liked_playlist_card(cx))
                .children(items.into_iter().map(|(playlist, cover)| {
                    playlist_card(playlist, cover.map(CoverSource::Path), sidebar, cx)
                })),
        )
        .into_any_element()
}

fn playlist_header(
    playlist: &PlaylistResponse,
    cover: Option<CoverSource>,
    sidebar: &Entity<Sidebar>,
    tracks: &[TrackResponse],
    cx: &mut gpui::Context<PlaylistPage>,
) -> impl IntoElement {
    let description = playlist
        .description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
        .unwrap_or("A personal playlist from your library")
        .to_owned();
    div()
        .min_h(px(212.))
        .flex()
        .items_end()
        .gap(px(25.))
        .child(media_cover::media_cover(
            &playlist.name,
            cover,
            px(212.),
            px(14.),
            true,
        ))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .pb(px(6.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(px(32.))
                        .font_weight(FontWeight::BOLD)
                        .child(playlist.name.clone()),
                )
                .child(
                    div()
                        .mt(px(12.))
                        .max_w(px(620.))
                        .text_size(px(14.))
                        .text_color(rgba(0xb8b5c0a6))
                        .child(description),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .text_size(px(13.))
                        .text_color(rgba(0xa8a5b18f))
                        .child(format!(
                            "{} songs · {}",
                            playlist.track_count,
                            format_duration(playlist.duration_ms)
                        )),
                )
                .child(
                    div()
                        .mt(px(17.))
                        .flex()
                        .items_center()
                        .gap(px(9.))
                        .child(play_context_button(
                            "playlist-play",
                            RomusIcons::MusicPlay,
                            "Play",
                            tracks.to_vec(),
                            false,
                            cx,
                        ))
                        .child(play_context_button(
                            "playlist-shuffle",
                            GpuiIcons::LucideShuffle,
                            "Shuffle",
                            tracks.to_vec(),
                            true,
                            cx,
                        ))
                        .child(playlist_more_button(playlist.clone(), sidebar.clone())),
                ),
        )
}

fn play_context_button(
    id: &'static str,
    icon: impl Into<SharedString>,
    label: &'static str,
    tracks: Vec<TrackResponse>,
    shuffle: bool,
    cx: &mut gpui::Context<PlaylistPage>,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(38.))
        .px(px(18.))
        .rounded_full()
        .bg(rgba(0xffffff0c))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgba(0xff5576d0))
        .cursor_pointer()
        .on_click(cx.listener(move |_, _, _, cx| {
            cx.emit(PlaylistPageEvent::PlayFrom {
                tracks: tracks.clone(),
                start: 0,
                shuffle,
            });
        }))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xff5576d0)))
        .child(label)
}

fn playlist_more_button(playlist: PlaylistResponse, sidebar: Entity<Sidebar>) -> impl IntoElement {
    let menu_sidebar = sidebar.clone();
    let menu_playlist = playlist.clone();
    div()
        .id("playlist-more")
        .size(px(38.))
        .rounded_full()
        .bg(rgba(0xffffff0c))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|button| button.bg(rgba(0xffffff14)))
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let menu = sidebar::playlist_manage_menu(menu_sidebar.clone(), menu_playlist.clone());
            let _ = context_menu::show(menu, event.position, window, cx);
            cx.stop_propagation();
        })
        .context_menu(move |_, _| sidebar::playlist_manage_menu(sidebar.clone(), playlist.clone()))
        .child(
            svg()
                .path(GpuiIcons::LucideEllipsisVertical)
                .size(px(16.))
                .text_color(rgba(0xff5576d0)),
        )
}

fn liked_playlist_card(cx: &gpui::Context<ContentShell>) -> impl IntoElement {
    div()
        .id("liked-playlist-card")
        .w(px(174.))
        .flex()
        .flex_col()
        .cursor_pointer()
        .on_click(
            cx.listener(|shell, _, _, cx| shell.navigate(Page::PlayLists(PlayList::Like), cx)),
        )
        .child(media_cover::liked_playlist_cover(px(174.), px(11.)))
        .child(
            div()
                .mt(px(8.))
                .truncate()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .child("Like"),
        )
}

fn playlist_card(
    playlist: PlaylistResponse,
    cover: Option<CoverSource>,
    sidebar: &Entity<Sidebar>,
    cx: &gpui::Context<ContentShell>,
) -> impl IntoElement {
    let page = Page::PlayLists(PlayList::Custom(playlist.id.clone().into()));
    let sidebar = sidebar.clone();
    let menu_playlist = playlist.clone();
    let name = playlist.name.clone();
    div()
        .id(playlist.id.clone())
        .w(px(174.))
        .flex()
        .flex_col()
        .cursor_pointer()
        .on_click(cx.listener(move |shell, _, _, cx| shell.navigate(page.clone(), cx)))
        .context_menu(move |_, _| {
            sidebar::playlist_manage_menu(sidebar.clone(), menu_playlist.clone())
        })
        .child(media_cover::media_cover(
            &name,
            cover,
            px(174.),
            px(11.),
            true,
        ))
        .child(
            div()
                .mt(px(8.))
                .truncate()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .child(name),
        )
}

fn menu_label(icon: GpuiIcons, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xf2f0f6f2)))
        .child(label)
}

fn song_clipboard_text(track: &TrackResponse) -> String {
    let title = track.title.as_deref().unwrap_or("Untitled");
    let artist = track.artist.as_deref().unwrap_or("Unknown Artist");
    match track.album.as_deref() {
        Some(album) => format!("{title} — {artist}\nAlbum: {album}"),
        None => format!("{title} — {artist}"),
    }
}

fn format_duration(duration_ms: i64) -> String {
    let total_seconds = duration_ms.max(0) / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = total_seconds % 3_600 / 60;
    if hours > 0 {
        format!("{hours} hr {minutes} min")
    } else {
        format!("{minutes} min")
    }
}

fn status_text(message: impl Into<String>) -> impl IntoElement {
    div()
        .mt(px(18.))
        .text_size(px(11.))
        .text_color(rgba(0xb3afbbb0))
        .child(message.into())
}

fn playlists_api<C: 'static>(cx: &gpui::Context<C>) -> Result<PlaylistsApi, String> {
    let _ = cx;
    PlaylistsApi::local().map_err(|error| error.to_string())
}

fn library_api<C: 'static>(cx: &gpui::Context<C>) -> Result<LibraryApi, String> {
    let _ = cx;
    LibraryApi::local().map_err(|error| error.to_string())
}
