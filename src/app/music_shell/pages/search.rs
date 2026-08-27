use std::{collections::HashMap, sync::Arc, time::Duration};

use gpui::{
    AppContext, Entity, EventEmitter, FontWeight, Image, ImageFormat, Render, Task, div,
    prelude::*, px, relative, rgba,
};
use gpui_tokio::Tokio;

use crate::{
    domain::Page,
    services::api::{
        AlbumResponse, ArtistResponse, BrowseQuery, LibraryApi, TrackQuery, TrackResponse,
    },
    state::PlaylistStore,
    ui::components::media_cover::{self, CoverSource},
};

pub enum SearchPageEvent {
    PlayFrom {
        tracks: Vec<TrackResponse>,
        start: usize,
        shuffle: bool,
    },
    Navigate(Page),
}

pub struct SearchPage {
    playlists: Entity<PlaylistStore>,
    query: String,
    artists: Vec<ArtistResponse>,
    albums: Vec<AlbumResponse>,
    tracks: Vec<TrackResponse>,
    artwork: HashMap<String, Arc<Image>>,
    artwork_tasks: HashMap<String, Task<()>>,
    request_task: Option<Task<()>>,
    generation: u64,
    loading: bool,
    error: Option<String>,
}

impl EventEmitter<SearchPageEvent> for SearchPage {}

impl SearchPage {
    pub fn new(playlists: Entity<PlaylistStore>) -> Self {
        Self {
            playlists,
            query: String::new(),
            artists: Vec::new(),
            albums: Vec::new(),
            tracks: Vec::new(),
            artwork: HashMap::new(),
            artwork_tasks: HashMap::new(),
            request_task: None,
            generation: 0,
            loading: false,
            error: None,
        }
    }

    pub fn set_query(&mut self, query: impl Into<String>, cx: &mut gpui::Context<Self>) {
        let query = query.into();
        if self.query == query {
            return;
        }
        self.query = query;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.request_task = None;
        if self.query.trim().is_empty() {
            self.artists.clear();
            self.albums.clear();
            self.tracks.clear();
            self.loading = false;
            self.error = None;
            cx.notify();
            return;
        }
        self.loading = true;
        self.error = None;
        let query = self.query.trim().to_owned();
        self.request_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(220))
                .await;
            let Some(this) = this.upgrade() else {
                return;
            };
            this.update(cx, |page, cx| {
                if page.generation == generation {
                    page.search(query, generation, cx);
                }
            });
        }));
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let query = self.query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.request_task = None;
        self.loading = true;
        self.error = None;
        self.search(query, generation, cx);
        cx.notify();
    }

    fn search(&mut self, query: String, generation: u64, cx: &mut gpui::Context<Self>) {
        let Ok(api) = library_api(cx) else {
            self.loading = false;
            self.error = Some("The library service is unavailable".to_owned());
            cx.notify();
            return;
        };
        let artist_api = api.clone();
        let album_api = api.clone();
        let track_query = query.clone();
        let request = Tokio::spawn(cx, async move {
            let browse_query = BrowseQuery {
                q: Some(query),
                limit: Some(12),
                offset: Some(0),
                ..Default::default()
            };
            let tracks_query = TrackQuery {
                q: Some(track_query),
                limit: Some(30),
                offset: Some(0),
                ..Default::default()
            };
            tokio::join!(
                artist_api.artists(&browse_query),
                album_api.albums(&browse_query),
                api.tracks(&tracks_query)
            )
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
                    Ok((Ok(artists), Ok(albums), Ok(tracks))) => {
                        page.artists = artists.items;
                        page.albums = albums.items;
                        page.tracks = tracks.items;
                        page.error = None;
                        page.sync_artwork(cx);
                    }
                    Ok((Err(error), _, _)) | Ok((_, Err(error), _)) | Ok((_, _, Err(error))) => {
                        page.error = Some(error.user_message())
                    }
                    Err(error) => page.error = Some(format!("Search request failed: {error}")),
                }
                cx.notify();
            });
        }));
    }

    fn sync_artwork(&mut self, cx: &mut gpui::Context<Self>) {
        let Ok(api) = library_api(cx) else {
            return;
        };
        let artist_ids = self
            .artists
            .iter()
            .filter(|artist| artist.artwork_url.is_some())
            .map(|artist| artist.id.clone())
            .collect::<Vec<_>>();
        for artist_id in artist_ids {
            self.fetch_artwork(artist_id, ArtKind::Artist, &api, cx);
        }
        let album_ids = self
            .albums
            .iter()
            .filter(|album| album.artwork_url.is_some())
            .map(|album| album.id.clone())
            .collect::<Vec<_>>();
        for album_id in album_ids {
            self.fetch_artwork(album_id, ArtKind::Album, &api, cx);
        }
        let track_ids = self
            .tracks
            .iter()
            .filter(|track| track.has_artwork)
            .map(|track| track.id.clone())
            .collect::<Vec<_>>();
        for track_id in track_ids {
            self.fetch_artwork(track_id, ArtKind::Track, &api, cx);
        }
    }

    fn fetch_artwork(
        &mut self,
        id: String,
        kind: ArtKind,
        api: &crate::services::api::LibraryApi,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.artwork.contains_key(&id) || self.artwork_tasks.contains_key(&id) {
            return;
        }
        let request_api = api.clone();
        let request_id = id.clone();
        let request = Tokio::spawn(cx, async move {
            match kind {
                ArtKind::Artist => request_api.artist_artwork(&request_id, None).await,
                ArtKind::Album => request_api.album_artwork(&request_id, None).await,
                ArtKind::Track => request_api.artwork(&request_id, None).await,
            }
        });
        let task_id = id.clone();
        self.artwork_tasks.insert(
            id,
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
                        Ok(Err(error)) => {
                            log::warn!("unable to load search artwork: {error}")
                        }
                        Err(error) => log::warn!("search artwork task failed: {error}"),
                    }
                    cx.notify();
                });
            }),
        );
    }
}

#[derive(Clone, Copy)]
enum ArtKind {
    Artist,
    Album,
    Track,
}

impl Render for SearchPage {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let query_empty = self.query.trim().is_empty();
        let normalized_query = self.query.trim().to_lowercase();
        let playlist_store = self.playlists.read(cx);
        let matching_playlists = if normalized_query.is_empty() {
            Vec::new()
        } else {
            playlist_store
                .playlists()
                .iter()
                .filter(|playlist| playlist.name.to_lowercase().contains(&normalized_query))
                .map(|playlist| {
                    (
                        playlist.name.clone(),
                        playlist_store.cover(&playlist.id).map(CoverSource::Path),
                    )
                })
                .collect::<Vec<_>>()
        };
        let no_results = !query_empty
            && !self.loading
            && self.artists.is_empty()
            && self.albums.is_empty()
            && self.tracks.is_empty()
            && matching_playlists.is_empty();
        let artists = self.artists.clone();
        let albums = self.albums.clone();
        let artwork = self.artwork.clone();
        div()
            .id("search-results-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(30.))
            .pt(px(22.))
            .pb(px(118.))
            .text_color(rgba(0xf1eff5ef))
            .when(query_empty && matching_playlists.is_empty(), |page| {
                page.child(empty_state("Search your music library"))
            })
            .when(self.loading, |page| page.child(status_text("Searching…")))
            .when_some(self.error.clone(), |page, error| {
                page.child(status_text(error))
            })
            .when(no_results, |page| {
                page.child(empty_state("No matching music found"))
            })
            .when(!artists.is_empty(), |page| {
                page.child(section_title("Artists")).child(
                    div()
                        .mt(px(13.))
                        .mb(px(34.))
                        .flex()
                        .flex_wrap()
                        .gap(px(16.))
                        .children(artists.into_iter().map(|artist| {
                            let page_id = Page::Artist(artist.id.clone().into());
                            let cover = artwork.get(&artist.id).cloned().map(CoverSource::Image);
                            let name = artist.name.clone();
                            div()
                                .id(artist.id.clone())
                                .w(px(132.))
                                .flex()
                                .flex_col()
                                .items_center()
                                .cursor_pointer()
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(SearchPageEvent::Navigate(page_id.clone()));
                                }))
                                .child(media_cover::media_cover(
                                    &name,
                                    cover,
                                    px(132.),
                                    px(66.),
                                    false,
                                ))
                                .child(
                                    div()
                                        .mt(px(8.))
                                        .w_full()
                                        .truncate()
                                        .text_align(gpui::TextAlign::Center)
                                        .text_size(px(11.5))
                                        .font_weight(FontWeight::BOLD)
                                        .child(name),
                                )
                        })),
                )
            })
            .when(!albums.is_empty(), |page| {
                page.child(section_title("Albums")).child(
                    div()
                        .mt(px(13.))
                        .mb(px(34.))
                        .flex()
                        .flex_wrap()
                        .gap(px(16.))
                        .children(albums.into_iter().map(|album| {
                            let page_id = Page::Album(album.id.clone().into());
                            let artist = album.album_artist_name.clone();
                            let cover = artwork.get(&album.id).cloned().map(CoverSource::Image);
                            let name = album.name.clone();
                            div()
                                .id(album.id.clone())
                                .w(px(154.))
                                .flex()
                                .flex_col()
                                .cursor_pointer()
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(SearchPageEvent::Navigate(page_id.clone()));
                                }))
                                .child(media_cover::media_cover(
                                    &name,
                                    cover,
                                    px(154.),
                                    px(11.),
                                    true,
                                ))
                                .child(
                                    div()
                                        .mt(px(8.))
                                        .truncate()
                                        .text_size(px(11.5))
                                        .font_weight(FontWeight::BOLD)
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .mt(px(2.))
                                        .truncate()
                                        .text_size(px(9.5))
                                        .text_color(rgba(0xb3afbb9c))
                                        .child(artist),
                                )
                        })),
                )
            })
            .when(!matching_playlists.is_empty(), |page| {
                page.child(section_title("Playlists")).child(
                    div()
                        .mt(px(13.))
                        .mb(px(34.))
                        .flex()
                        .flex_wrap()
                        .gap(px(16.))
                        .children(matching_playlists.into_iter().map(|(name, cover)| {
                            div()
                                .w(px(154.))
                                .flex()
                                .flex_col()
                                .child(media_cover::media_cover(
                                    &name,
                                    cover,
                                    px(154.),
                                    px(11.),
                                    true,
                                ))
                                .child(
                                    div()
                                        .mt(px(8.))
                                        .truncate()
                                        .text_size(px(11.5))
                                        .font_weight(FontWeight::BOLD)
                                        .child(name),
                                )
                        })),
                )
            })
            .when(!self.tracks.is_empty(), |page| {
                let play_entity = cx.entity();
                let artwork = self.artwork.clone();
                page.child(section_title("Songs")).child(
                    div()
                        .mt(px(10.))
                        .flex()
                        .flex_wrap()
                        .gap_x(px(18.))
                        .children(self.tracks.iter().cloned().enumerate().map(
                            move |(index, track)| {
                                let title =
                                    track.title.clone().unwrap_or_else(|| "Untitled".into());
                                let artist = track
                                    .artist
                                    .clone()
                                    .unwrap_or_else(|| "Unknown Artist".into());
                                let cover = artwork.get(&track.id).cloned().map(CoverSource::Image);
                                let clicked = track.clone();
                                let play_entity = play_entity.clone();
                                div()
                                    .id(("search-track", index))
                                    .min_w(px(245.))
                                    .w(relative(0.31))
                                    .flex_1()
                                    .h(px(57.))
                                    .border_b_1()
                                    .border_color(rgba(0xffffff0b))
                                    .flex()
                                    .items_center()
                                    .gap(px(11.))
                                    .cursor_pointer()
                                    .hover(|row| row.bg(rgba(0xffffff0a)))
                                    .on_click(move |_, _, cx| {
                                        play_entity.update(cx, |page, cx| {
                                            let start = page
                                                .tracks
                                                .iter()
                                                .position(|item| item.id == clicked.id)
                                                .unwrap_or(index);
                                            cx.emit(SearchPageEvent::PlayFrom {
                                                tracks: page.tracks.clone(),
                                                start,
                                                shuffle: false,
                                            });
                                        });
                                    })
                                    .child(media_cover::media_cover(
                                        &title,
                                        cover,
                                        px(40.),
                                        px(7.),
                                        false,
                                    ))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_size(px(11.5))
                                                    .font_weight(FontWeight::BOLD)
                                                    .child(title),
                                            )
                                            .child(
                                                div()
                                                    .mt(px(3.))
                                                    .truncate()
                                                    .text_size(px(9.5))
                                                    .text_color(rgba(0xb3afbb9c))
                                                    .child(artist),
                                            ),
                                    )
                            },
                        )),
                )
            })
    }
}

fn library_api<C: 'static>(cx: &gpui::Context<C>) -> Result<LibraryApi, String> {
    let _ = cx;
    LibraryApi::local().map_err(|error| error.to_string())
}

fn section_title(title: &'static str) -> impl IntoElement {
    div()
        .text_size(px(17.))
        .font_weight(FontWeight::BOLD)
        .child(title)
}

fn empty_state(message: impl Into<String>) -> impl IntoElement {
    div()
        .h(px(220.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.))
        .text_color(rgba(0xb3afbb8a))
        .child(message.into())
}

fn status_text(message: impl Into<String>) -> impl IntoElement {
    div()
        .mb(px(18.))
        .text_size(px(11.))
        .text_color(rgba(0xb3afbbb0))
        .child(message.into())
}

pub fn entity(
    playlists: Entity<PlaylistStore>,
    cx: &mut gpui::Context<super::ContentShell>,
) -> Entity<SearchPage> {
    cx.new(move |_| SearchPage::new(playlists))
}
