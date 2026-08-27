use std::{collections::HashMap, sync::Arc};

use gpui::{
    ClipboardItem, Div, Entity, EventEmitter, FontWeight, Image, ImageFormat, Render, Task, div,
    prelude::*, px, rgba, svg,
};
use gpui_tokio::Tokio;
use uic::{
    assets::LucideIcons,
    components::{
        context_menu::{ContextMenu, ContextMenuExt, ContextMenuItem},
        modal::{self, Modal},
        toast,
    },
};

use super::songs;
use crate::{
    app::music_shell::sidebar::Sidebar,
    domain::Page,
    resources::RomusIcons,
    services::{
        api::{ApiError, PageQuery, PlaylistItemSort, PlaylistResponse, TrackQuery, TrackResponse},
        library_api, playlists_api,
    },
    state::{ImportManager, PlaylistStore, QueueOrigin, imports},
    ui::{
        components::media_cover::{self, CoverSource},
        strings,
        theme::{self, palette},
    },
};

const RECENT_LIMIT: u32 = 24;
const FAVORITE_LIMIT: u32 = 8;
const ALBUM_CARD: f32 = 156.;
const PLAYLIST_CARD: f32 = 148.;

pub enum HomePageEvent {
    Play(Box<TrackResponse>),
    PlayFrom {
        tracks: Vec<TrackResponse>,
        shuffle: bool,
        origin: Option<QueueOrigin>,
    },
    PlayNext(Vec<TrackResponse>),
    Enqueue(Vec<TrackResponse>),
    Navigate(Page),
}

pub struct HomePage {
    playlists: Entity<PlaylistStore>,
    sidebar: Entity<Sidebar>,
    imports: Entity<ImportManager>,
    tracks: Vec<TrackResponse>,
    favorites: Vec<TrackResponse>,
    total_tracks: i64,
    artwork: HashMap<String, Arc<Image>>,
    artwork_tasks: HashMap<String, Task<()>>,
    action_tasks: HashMap<String, Task<()>>,
    request_task: Option<Task<()>>,
    generation: u64,
    loading: bool,
    loaded: bool,
    error: Option<String>,
}

impl EventEmitter<HomePageEvent> for HomePage {}

impl HomePage {
    pub fn new(
        playlists: Entity<PlaylistStore>,
        sidebar: Entity<Sidebar>,
        imports: Entity<ImportManager>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        cx.observe(&playlists, |_, _, cx| cx.notify()).detach();
        Self {
            playlists,
            sidebar,
            imports,
            tracks: Vec::new(),
            favorites: Vec::new(),
            total_tracks: 0,
            artwork: HashMap::new(),
            artwork_tasks: HashMap::new(),
            action_tasks: HashMap::new(),
            request_task: None,
            generation: 0,
            loading: false,
            loaded: false,
            error: None,
        }
    }

    pub fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.loaded && !self.loading {
            self.reload(cx);
        }
    }

    pub fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        self.reload(cx);
    }

    pub fn reset(&mut self, cx: &mut gpui::Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.tracks.clear();
        self.favorites.clear();
        self.total_tracks = 0;
        self.artwork.clear();
        self.artwork_tasks.clear();
        self.action_tasks.clear();
        self.request_task = None;
        self.loading = false;
        self.loaded = false;
        self.error = None;
        cx.notify();
    }

    fn reload(&mut self, cx: &mut gpui::Context<Self>) {
        let Ok(api) = library_api(cx) else {
            self.error = Some("The library service is unavailable".to_owned());
            cx.notify();
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        let request = Tokio::spawn(cx, async move {
            let recent = TrackQuery {
                limit: Some(RECENT_LIMIT),
                offset: Some(0),
                ..Default::default()
            };
            let favorites = PageQuery {
                limit: Some(FAVORITE_LIMIT),
                offset: Some(0),
            };
            tokio::join!(api.tracks(&recent), api.favorites(&favorites))
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
                page.loaded = true;
                match result {
                    Ok((Ok(tracks), Ok(favorites))) => {
                        page.total_tracks = tracks.total;
                        page.tracks = tracks.items;
                        page.favorites = favorites.items;
                        page.error = None;
                        page.sync_artwork(cx);
                    }
                    Ok((Ok(tracks), Err(error))) => {
                        page.total_tracks = tracks.total;
                        page.tracks = tracks.items;
                        page.favorites.clear();
                        page.error = None;
                        log::warn!("unable to load favorites for home: {error}");
                        page.sync_artwork(cx);
                    }
                    Ok((Err(error), _)) => page.error = Some(error.user_message()),
                    Err(error) => page.error = Some(format!("Home request failed: {error}")),
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
            .chain(self.favorites.iter())
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

    fn perform_album_action(
        &mut self,
        album: AlbumCard,
        action: QueueAction,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(album_id) = album.id.clone() else {
            self.dispatch_tracks(vec![album.play_track], action, None, cx);
            return;
        };
        let key = format!("album:{album_id}:{}", action.key());
        if self.action_tasks.contains_key(&key) {
            return;
        }
        let Ok(api) = library_api(cx) else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        let request_album_id = album_id.clone();
        let request = Tokio::spawn(cx, async move {
            let mut tracks = Vec::new();
            let mut offset = 0_u64;
            loop {
                let response = api
                    .album_tracks(
                        &request_album_id,
                        &PageQuery {
                            limit: Some(200),
                            offset: Some(offset),
                        },
                    )
                    .await?;
                let page_len = response.items.len();
                tracks.extend(response.items);
                if page_len == 0 || tracks.len() as i64 >= response.total {
                    break;
                }
                offset = offset.saturating_add(page_len as u64);
            }
            Ok::<_, ApiError>(tracks)
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
                        Ok(Ok(mut tracks)) => {
                            tracks.sort_by_key(|track| {
                                (
                                    track.disc_number.unwrap_or(0),
                                    track.track_number.unwrap_or(0),
                                )
                            });
                            page.dispatch_tracks(
                                tracks,
                                action,
                                Some(QueueOrigin::Album {
                                    id: album_id.clone(),
                                    name: album.name.clone(),
                                }),
                                cx,
                            );
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Album request failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
    }

    fn perform_playlist_action(
        &mut self,
        playlist: PlaylistResponse,
        action: QueueAction,
        cx: &mut gpui::Context<Self>,
    ) {
        let key = format!("playlist:{}:{}", playlist.id, action.key());
        if self.action_tasks.contains_key(&key) {
            return;
        }
        let Ok(api) = playlists_api(cx) else {
            toast::error("The playlist service is unavailable", cx);
            return;
        };
        let playlist_id = playlist.id.clone();
        let request_id = playlist_id.clone();
        let request = Tokio::spawn(cx, async move {
            api.get(&request_id, PlaylistItemSort::Custom).await
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
                        Ok(Ok(detail)) => page.dispatch_tracks(
                            detail.items.into_iter().map(|item| item.track).collect(),
                            action,
                            Some(QueueOrigin::Playlist {
                                id: playlist_id.clone(),
                                name: playlist.name.clone(),
                            }),
                            cx,
                        ),
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Playlist request failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
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
                            page.playlists.update(cx, |store, cx| store.refresh(cx));
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
                                *track = updated.clone();
                            }
                            if liked {
                                if let Some(track) = page
                                    .favorites
                                    .iter_mut()
                                    .find(|track| track.id == updated.id)
                                {
                                    *track = updated;
                                } else {
                                    page.favorites.insert(0, updated);
                                    page.favorites.truncate(FAVORITE_LIMIT as usize);
                                }
                            } else {
                                page.favorites.retain(|track| track.id != updated.id);
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
                            page.favorites.retain(|track| track.id != track_id);
                            page.artwork.remove(&track_id);
                            page.total_tracks = page.total_tracks.saturating_sub(1);
                            page.playlists.update(cx, |store, cx| store.refresh(cx));
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

    fn dispatch_tracks(
        &mut self,
        tracks: Vec<TrackResponse>,
        action: QueueAction,
        origin: Option<QueueOrigin>,
        cx: &mut gpui::Context<Self>,
    ) {
        if tracks.is_empty() {
            toast::error("There are no songs to play", cx);
            return;
        }
        match action {
            QueueAction::Play => cx.emit(HomePageEvent::PlayFrom {
                tracks,
                shuffle: false,
                origin,
            }),
            QueueAction::Shuffle => cx.emit(HomePageEvent::PlayFrom {
                tracks,
                shuffle: true,
                origin,
            }),
            QueueAction::PlayNext => cx.emit(HomePageEvent::PlayNext(tracks)),
            QueueAction::Enqueue => cx.emit(HomePageEvent::Enqueue(tracks)),
        }
    }

    fn recent_albums(&self) -> Vec<AlbumCard> {
        let mut seen = Vec::new();
        let mut albums = Vec::new();
        for track in &self.tracks {
            let key = track
                .album_id
                .clone()
                .or_else(|| track.album.clone())
                .unwrap_or_else(|| track.id.clone());
            if seen.iter().any(|existing| existing == &key) {
                continue;
            }
            seen.push(key);
            albums.push(AlbumCard {
                id: track.album_id.clone(),
                name: track
                    .album
                    .clone()
                    .filter(|album| !album.trim().is_empty())
                    .or_else(|| track.title.clone())
                    .unwrap_or_else(|| strings::untitled().to_owned()),
                artist: track
                    .artist
                    .clone()
                    .unwrap_or_else(|| strings::unknown_artist().to_owned()),
                cover_track_id: track.has_artwork.then(|| track.id.clone()),
                play_track: track.clone(),
            });
            if albums.len() == 10 {
                break;
            }
        }
        albums
    }
}

#[derive(Clone)]
struct AlbumCard {
    id: Option<String>,
    name: String,
    artist: String,
    cover_track_id: Option<String>,
    play_track: TrackResponse,
}

#[derive(Clone, Copy)]
enum QueueAction {
    Play,
    Shuffle,
    PlayNext,
    Enqueue,
}

impl QueueAction {
    fn key(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Shuffle => "shuffle",
            Self::PlayNext => "next",
            Self::Enqueue => "enqueue",
        }
    }
}

impl Render for HomePage {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        self.ensure_loaded(cx);
        let p = palette();
        let albums = self.recent_albums();
        let listen_now = if self.favorites.is_empty() {
            self.tracks.iter().take(8).cloned().collect::<Vec<_>>()
        } else {
            self.favorites.iter().take(8).cloned().collect::<Vec<_>>()
        };
        let playlists = self
            .playlists
            .read(cx)
            .playlists()
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>();
        let empty = self.loaded && self.tracks.is_empty();

        div()
            .id("home-page-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(30.))
            .pt(px(5.))
            .pb(px(118.))
            .text_color(p.text)
            .when(self.loading && self.tracks.is_empty(), |page| {
                page.child(status_text(strings::loading()))
            })
            .when_some(self.error.clone(), |page, error| {
                page.child(status_text(error))
            })
            .when(empty, |page| page.child(empty_state(cx)))
            .when(!albums.is_empty(), |page| {
                page.child(section_header(
                    strings::recently_added(),
                    Some(Page::Albums),
                    cx,
                ))
                .child(album_row(&albums, &self.artwork, cx))
            })
            .when(!listen_now.is_empty(), |page| {
                page.child(section_header(strings::listen_now(), None, cx))
                    .child(listen_now_grid(&listen_now, &self.artwork, cx))
            })
            .when(!playlists.is_empty(), |page| {
                page.child(section_header(
                    strings::playlists(),
                    Some(Page::PlayLists(crate::domain::PlayList::All)),
                    cx,
                ))
                .child(playlist_row(&playlists, &self.playlists, cx))
            })
    }
}

fn empty_state(cx: &mut gpui::Context<HomePage>) -> impl IntoElement {
    let p = palette();
    div()
        .mt(px(48.))
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .child(
            div()
                .size(px(88.))
                .rounded(px(26.))
                .bg(rgba(0xffffff0a))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(LucideIcons::Music2)
                        .size(px(34.))
                        .text_color(rgba(0xe8e6eed0)),
                ),
        )
        .child(
            div()
                .mt(px(22.))
                .text_size(px(18.))
                .font_weight(FontWeight::BOLD)
                .child(strings::home_empty_title()),
        )
        .child(
            div()
                .mt(px(8.))
                .max_w(px(360.))
                .text_size(px(12.5))
                .text_color(p.text_muted)
                .child(strings::home_empty_body()),
        )
        .child(
            div()
                .id("home-empty-add-music")
                .mt(px(22.))
                .h(px(36.))
                .px(px(18.))
                .rounded_full()
                .bg(rgba(0xf2f0f6ff))
                .text_color(rgba(0x171923ff))
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|button| button.bg(rgba(0xffffffee)))
                .on_click(cx.listener(|page, _, window, cx| {
                    imports::show_picker(page.imports.clone(), window, cx);
                }))
                .child(strings::add_music()),
        )
}

fn section_header(
    title: &'static str,
    see_all: Option<Page>,
    cx: &mut gpui::Context<HomePage>,
) -> impl IntoElement {
    let p = palette();
    div()
        .mt(px(26.))
        .mb(px(14.))
        .h(px(28.))
        .flex()
        .items_end()
        .justify_between()
        .child(
            div()
                .text_size(px(20.))
                .font_weight(FontWeight::BOLD)
                .child(title),
        )
        .when_some(see_all, |header, page| {
            header.child(
                div()
                    .id(title)
                    .px(px(4.))
                    .py(px(2.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgba(0xff6684d8))
                    .hover(|link| link.text_color(p.accent))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(HomePageEvent::Navigate(page.clone()));
                    }))
                    .child(strings::see_all()),
            )
        })
}

fn album_row(
    albums: &[AlbumCard],
    artwork: &HashMap<String, Arc<Image>>,
    cx: &mut gpui::Context<HomePage>,
) -> impl IntoElement {
    let entity = cx.entity();
    div()
        .id("home-recent-albums")
        .w_full()
        .overflow_x_scroll()
        .flex()
        .gap(px(16.))
        .children(albums.iter().cloned().enumerate().map(|(index, album)| {
            let cover = album
                .cover_track_id
                .as_ref()
                .and_then(|id| artwork.get(id))
                .cloned()
                .map(CoverSource::Image);
            let play_track = album.play_track.clone();
            let album_id = album.id.clone();
            let menu_album = album.clone();
            let menu_entity = entity.clone();
            div()
                .id(("home-album", index))
                .w(px(ALBUM_CARD))
                .flex_none()
                .flex()
                .flex_col()
                .cursor_pointer()
                .group("home-album-card")
                .on_click(cx.listener(move |_, _, _, cx| {
                    if let Some(album_id) = album_id.clone() {
                        cx.emit(HomePageEvent::Navigate(Page::Album(album_id.into())));
                    } else {
                        cx.emit(HomePageEvent::Play(Box::new(play_track.clone())));
                    }
                }))
                .context_menu(move |_, _| album_menu(menu_entity.clone(), menu_album.clone()))
                .child(
                    div()
                        .relative()
                        .child(media_cover::media_cover(
                            &album.name,
                            cover,
                            px(ALBUM_CARD),
                            px(12.),
                            true,
                        ))
                        .child(play_overlay(index, album.clone(), cx)),
                )
                .child(
                    div()
                        .mt(px(9.))
                        .truncate()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(album.name.clone()),
                )
                .child(
                    div()
                        .mt(px(2.))
                        .truncate()
                        .text_size(px(11.))
                        .text_color(palette().text_muted)
                        .child(album.artist.clone()),
                )
        }))
}

fn listen_now_grid(
    tracks: &[TrackResponse],
    artwork: &HashMap<String, Arc<Image>>,
    cx: &mut gpui::Context<HomePage>,
) -> impl IntoElement {
    let p = palette();
    let entity = cx.entity();
    div().w_full().flex().flex_wrap().gap_x(px(18.)).children(
        tracks.iter().cloned().enumerate().map(|(index, track)| {
            let title = track
                .title
                .clone()
                .unwrap_or_else(|| strings::untitled().to_owned());
            let artist = track
                .artist
                .clone()
                .unwrap_or_else(|| strings::unknown_artist().to_owned());
            let cover = artwork.get(&track.id).cloned().map(CoverSource::Image);
            let play_track = track.clone();
            let menu_track = track.clone();
            let menu_entity = entity.clone();
            div()
                .id(("home-listen", index))
                .min_w(px(280.))
                .w_full()
                .max_w(px(420.))
                .h(px(58.))
                .px(px(8.))
                .rounded(px(12.))
                .flex()
                .items_center()
                .gap(px(12.))
                .cursor_pointer()
                .hover(|row| row.bg(rgba(0xffffff08)))
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(HomePageEvent::Play(Box::new(play_track.clone())));
                }))
                .context_menu(move |_, cx| track_menu(menu_entity.clone(), menu_track.clone(), cx))
                .child(media_cover::media_cover(
                    &title,
                    cover,
                    px(42.),
                    px(8.),
                    false,
                ))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .mt(px(2.))
                                .truncate()
                                .text_size(px(11.))
                                .text_color(p.text_muted)
                                .child(artist),
                        ),
                )
                .child(
                    svg()
                        .path(RomusIcons::MusicPlay)
                        .size(px(14.))
                        .text_color(rgba(0xffffff55)),
                )
        }),
    )
}

fn playlist_row(
    playlists: &[crate::services::api::PlaylistResponse],
    store: &Entity<PlaylistStore>,
    cx: &mut gpui::Context<HomePage>,
) -> impl IntoElement {
    let covers = store.read(cx);
    let entity = cx.entity();
    div()
        .id("home-playlists")
        .w_full()
        .overflow_x_scroll()
        .flex()
        .gap(px(16.))
        .children(
            playlists
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, playlist)| {
                    let cover = covers.cover(&playlist.id).map(CoverSource::Path);
                    let page = Page::PlayLists(crate::domain::PlayList::Custom(
                        playlist.id.clone().into(),
                    ));
                    let menu_playlist = playlist.clone();
                    let menu_entity = entity.clone();
                    div()
                        .id(("home-playlist", index))
                        .w(px(PLAYLIST_CARD))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .cursor_pointer()
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(HomePageEvent::Navigate(page.clone()));
                        }))
                        .context_menu(move |_, _| {
                            playlist_menu(menu_entity.clone(), menu_playlist.clone())
                        })
                        .child(media_cover::media_cover(
                            &playlist.name,
                            cover,
                            px(PLAYLIST_CARD),
                            px(12.),
                            true,
                        ))
                        .child(
                            div()
                                .mt(px(9.))
                                .truncate()
                                .text_size(px(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(playlist.name.clone()),
                        )
                        .child(
                            div()
                                .mt(px(2.))
                                .truncate()
                                .text_size(px(11.))
                                .text_color(palette().text_muted)
                                .child(format!("{} songs", playlist.track_count)),
                        )
                }),
        )
}

fn play_overlay(
    index: usize,
    album: AlbumCard,
    cx: &mut gpui::Context<HomePage>,
) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .rounded(px(12.))
        .flex()
        .items_end()
        .justify_end()
        .p(px(10.))
        .opacity(0.)
        .bg(rgba(0x080a1088))
        .group_hover("home-album-card", |overlay| overlay.opacity(1.))
        .child(
            div()
                .id(("home-album-play", index))
                .size(px(34.))
                .rounded_full()
                .bg(rgba(0xfa2d55ff))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .shadow(vec![
                    gpui::BoxShadow::new(px(0.), px(6.), rgba(0x00000055).into())
                        .blur_radius(px(12.)),
                ])
                .on_mouse_down(gpui::MouseButton::Left, {
                    let album = album.clone();
                    cx.listener(move |page, _, _, cx| {
                        page.perform_album_action(album.clone(), QueueAction::Play, cx);
                        cx.stop_propagation();
                    })
                })
                .child(
                    svg()
                        .path(RomusIcons::MusicPlay)
                        .size(px(15.))
                        .text_color(rgba(0xffffffff)),
                ),
        )
}

fn track_menu(entity: Entity<HomePage>, track: TrackResponse, cx: &gpui::App) -> ContextMenu {
    let (playlist_items, sidebar) = {
        let page = entity.read(cx);
        (
            page.playlists.read(cx).playlists().to_vec(),
            page.sidebar.clone(),
        )
    };
    let mut recent_playlists = playlist_items.clone();
    recent_playlists.sort_by_key(|playlist| std::cmp::Reverse(playlist.updated_at));
    recent_playlists.truncate(3);
    let mut add_menu = ContextMenu::new()
        .action_with(
            |_, _| playlist_create_label(),
            move |window, cx| {
                sidebar.update(cx, |sidebar, cx| {
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

    let should_like = !track.favorite;
    let like_label = if should_like { "Like" } else { "Unlike" };
    let like_track_id = track.id.clone();
    let info_track = track.clone();
    let next_track = track.clone();
    let enqueue_track = track.clone();
    let copy_track = track.clone();
    let delete_track = track.clone();
    let artist_id = track.artist_id.clone();
    let album_id = track.album_id.clone();
    let mut menu = ContextMenu::new()
        .item(ContextMenuItem::submenu_with(
            |_, _| menu_label(LucideIcons::ListMusic, "Add to Playlist"),
            add_menu,
        ))
        .separator()
        .action_with(
            |_, _| menu_label(LucideIcons::BadgeInfo, "Get Info"),
            move |window, cx| songs::show_track_info(info_track.clone(), window, cx),
        )
        .action_with(move |_, _| menu_label(LucideIcons::Heart, like_label), {
            let entity = entity.clone();
            move |_, cx| {
                entity.update(cx, |page, cx| {
                    page.set_liked(like_track_id.clone(), should_like, cx);
                });
            }
        })
        .separator()
        .action_with(
            |_, _| menu_label(LucideIcons::ListPlus, strings::play_next()),
            {
                let entity = entity.clone();
                move |_, cx| {
                    entity.update(cx, |page, cx| {
                        page.dispatch_tracks(
                            vec![next_track.clone()],
                            QueueAction::PlayNext,
                            None,
                            cx,
                        );
                    });
                }
            },
        )
        .action_with(
            |_, _| menu_label(LucideIcons::ListEnd, strings::add_to_queue()),
            {
                let entity = entity.clone();
                move |_, cx| {
                    entity.update(cx, |page, cx| {
                        page.dispatch_tracks(
                            vec![enqueue_track.clone()],
                            QueueAction::Enqueue,
                            None,
                            cx,
                        );
                    });
                }
            },
        )
        .separator()
        .action_with(
            |_, _| menu_label(LucideIcons::Copy, "Copy"),
            move |_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(songs::song_clipboard_text(
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
                    cx.emit(HomePageEvent::Navigate(Page::Artist(
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
                    cx.emit(HomePageEvent::Navigate(Page::Album(
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
                    confirm_delete(entity.clone(), delete_track.clone(), window, cx);
                },
            )
            .danger(),
        ),
    )
}

fn add_playlist_action(
    menu: ContextMenu,
    entity: &Entity<HomePage>,
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

fn album_menu(entity: Entity<HomePage>, album: AlbumCard) -> ContextMenu {
    let play_album = album.clone();
    let shuffle_album = album.clone();
    let next_album = album.clone();
    let enqueue_album = album.clone();
    let album_id = album.id.clone();
    let mut menu = ContextMenu::new()
        .action_with(|_, _| menu_label(LucideIcons::Play, "Play Album"), {
            let entity = entity.clone();
            move |_, cx| {
                entity.update(cx, |page, cx| {
                    page.perform_album_action(play_album.clone(), QueueAction::Play, cx)
                });
            }
        })
        .action_with(|_, _| menu_label(LucideIcons::Shuffle, "Shuffle Album"), {
            let entity = entity.clone();
            move |_, cx| {
                entity.update(cx, |page, cx| {
                    page.perform_album_action(shuffle_album.clone(), QueueAction::Shuffle, cx)
                });
            }
        })
        .separator()
        .action_with(
            |_, _| menu_label(LucideIcons::ListPlus, strings::play_next()),
            {
                let entity = entity.clone();
                move |_, cx| {
                    entity.update(cx, |page, cx| {
                        page.perform_album_action(next_album.clone(), QueueAction::PlayNext, cx)
                    });
                }
            },
        )
        .action_with(
            |_, _| menu_label(LucideIcons::ListEnd, strings::add_to_queue()),
            {
                let entity = entity.clone();
                move |_, cx| {
                    entity.update(cx, |page, cx| {
                        page.perform_album_action(enqueue_album.clone(), QueueAction::Enqueue, cx)
                    });
                }
            },
        );
    if let Some(album_id) = album_id {
        menu = menu.separator().action_with(
            |_, _| menu_label(LucideIcons::DiscAlbum, "Go to Album"),
            move |_, cx| {
                entity.update(cx, |_, cx| {
                    cx.emit(HomePageEvent::Navigate(Page::Album(
                        album_id.clone().into(),
                    )));
                });
            },
        );
    }
    theme::style_context_menu(menu)
}

fn playlist_menu(entity: Entity<HomePage>, playlist: PlaylistResponse) -> ContextMenu {
    let play_playlist = playlist.clone();
    let shuffle_playlist = playlist.clone();
    let next_playlist = playlist.clone();
    let enqueue_playlist = playlist.clone();
    let page = Page::PlayLists(crate::domain::PlayList::Custom(playlist.id.clone().into()));
    theme::style_context_menu(
        ContextMenu::new()
            .action_with(|_, _| menu_label(LucideIcons::Play, "Play Playlist"), {
                let entity = entity.clone();
                move |_, cx| {
                    entity.update(cx, |home, cx| {
                        home.perform_playlist_action(play_playlist.clone(), QueueAction::Play, cx)
                    });
                }
            })
            .action_with(
                |_, _| menu_label(LucideIcons::Shuffle, "Shuffle Playlist"),
                {
                    let entity = entity.clone();
                    move |_, cx| {
                        entity.update(cx, |home, cx| {
                            home.perform_playlist_action(
                                shuffle_playlist.clone(),
                                QueueAction::Shuffle,
                                cx,
                            )
                        });
                    }
                },
            )
            .separator()
            .action_with(
                |_, _| menu_label(LucideIcons::ListPlus, strings::play_next()),
                {
                    let entity = entity.clone();
                    move |_, cx| {
                        entity.update(cx, |home, cx| {
                            home.perform_playlist_action(
                                next_playlist.clone(),
                                QueueAction::PlayNext,
                                cx,
                            )
                        });
                    }
                },
            )
            .action_with(
                |_, _| menu_label(LucideIcons::ListEnd, strings::add_to_queue()),
                {
                    let entity = entity.clone();
                    move |_, cx| {
                        entity.update(cx, |home, cx| {
                            home.perform_playlist_action(
                                enqueue_playlist.clone(),
                                QueueAction::Enqueue,
                                cx,
                            )
                        });
                    }
                },
            )
            .separator()
            .action_with(
                |_, _| menu_label(LucideIcons::FolderOpen, "Open Playlist"),
                move |_, cx| {
                    entity.update(cx, |_, cx| {
                        cx.emit(HomePageEvent::Navigate(page.clone()));
                    });
                },
            ),
    )
}

fn confirm_delete(
    entity: Entity<HomePage>,
    track: TrackResponse,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    let title = track.title.as_deref().unwrap_or("Untitled").to_owned();
    let body_title = title.clone();
    let track_id = track.id;
    let appearance = theme::dark_modal_appearance().ok_button(
        theme::dark_modal_appearance()
            .ok_button
            .background(rgba(0xe73558ff).into())
            .border(rgba(0xe73558ff).into())
            .hover_background(rgba(0xf24868ff).into()),
    );
    modal::show(
        theme::style_modal(
            Modal::new(move |_, _| {
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
                entity.update(cx, |page, cx| page.delete_track(track_id.clone(), cx));
                true
            })
            .appearance(appearance)
            .w(px(420.)),
        ),
        window,
        cx,
    );
}

fn menu_label(icon: LucideIcons, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xf2f0f6f2)))
        .child(label)
}

fn status_text(message: impl Into<String>) -> impl IntoElement {
    div()
        .mt(px(18.))
        .text_size(px(12.))
        .text_color(palette().text_muted)
        .child(message.into())
}
