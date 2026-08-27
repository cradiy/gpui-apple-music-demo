use crate::{
    app::music_shell::sidebar::Sidebar,
    domain::{Page, PlayList},
    services::api::TrackResponse,
    state::{ImportEvent, ImportManager, PlaylistStore, QueueOrigin},
    ui::strings,
};
use gpui::{Entity, div, prelude::*};

pub mod albums;
pub mod artists;
pub mod home;
pub mod liked;
pub mod playlists;
pub mod search;
pub mod songs;

#[derive(Clone)]
pub struct ContentShell {
    pub selected_page: Page,
    home_page: Entity<home::HomePage>,
    search_page: Entity<search::SearchPage>,
    songs_page: Entity<songs::SongsPage>,
    artists_page: Entity<artists::ArtistsPage>,
    albums_page: Entity<albums::AlbumsPage>,
    playlist_page: Entity<playlists::PlaylistPage>,
    liked_page: Entity<liked::LikedPage>,
    playlists: Entity<PlaylistStore>,
    sidebar: Entity<Sidebar>,
}

impl ContentShell {
    pub fn new(
        cx: &mut gpui::Context<Self>,
        selected: Page,
        playlists: Entity<PlaylistStore>,
        sidebar: Entity<Sidebar>,
        imports: Entity<ImportManager>,
    ) -> Self {
        let songs_playlists = playlists.clone();
        let songs_sidebar = sidebar.clone();
        let songs_page = cx.new(move |_| songs::SongsPage::new(songs_playlists, songs_sidebar));
        let detail_playlists = playlists.clone();
        let detail_sidebar = sidebar.clone();
        let playlist_page =
            cx.new(move |cx| playlists::PlaylistPage::new(detail_playlists, detail_sidebar, cx));
        let liked_page = cx.new(|_| liked::LikedPage::new());
        let artists_page = cx.new(artists::ArtistsPage::new);
        let albums_page = cx.new(albums::AlbumsPage::new);
        let search_page = search::entity(playlists.clone(), cx);
        let home_page = cx
            .new(|cx| home::HomePage::new(playlists.clone(), sidebar.clone(), imports.clone(), cx));
        cx.subscribe(&imports, |shell, _, event: &ImportEvent, cx| {
            if matches!(event, ImportEvent::LibraryChanged) {
                shell.refresh_library(cx);
            }
        })
        .detach();
        cx.observe(&songs_page, |_, _, cx| cx.notify()).detach();
        cx.observe(&playlist_page, |_, _, cx| cx.notify()).detach();
        cx.observe(&liked_page, |_, _, cx| cx.notify()).detach();
        cx.observe(&artists_page, |_, _, cx| cx.notify()).detach();
        cx.observe(&albums_page, |_, _, cx| cx.notify()).detach();
        cx.observe(&home_page, |_, _, cx| cx.notify()).detach();
        cx.subscribe(
            &songs_page,
            |shell, _, event: &songs::SongsPageEvent, cx| match event {
                songs::SongsPageEvent::PlayFrom {
                    tracks,
                    start,
                    shuffle,
                } => {
                    cx.emit(ContentShellEvent::PlayFrom {
                        tracks: tracks.clone(),
                        start: *start,
                        shuffle: *shuffle,
                        origin: None,
                    });
                }
                songs::SongsPageEvent::PlayNext(track) => {
                    cx.emit(ContentShellEvent::PlayNext(Box::new(track.clone())));
                }
                songs::SongsPageEvent::Enqueue(track) => {
                    cx.emit(ContentShellEvent::Enqueue(Box::new(track.clone())));
                }
                songs::SongsPageEvent::Navigate(page) => {
                    shell.navigate(page.clone(), cx);
                }
            },
        )
        .detach();
        cx.subscribe(
            &playlist_page,
            |shell, _, event: &playlists::PlaylistPageEvent, cx| match event {
                playlists::PlaylistPageEvent::PlayFrom {
                    tracks,
                    start,
                    shuffle,
                } => {
                    cx.emit(ContentShellEvent::PlayFrom {
                        tracks: tracks.clone(),
                        start: *start,
                        shuffle: *shuffle,
                        origin: shell.playlist_page.read(cx).playing_origin(),
                    });
                }
                playlists::PlaylistPageEvent::PlayNext(track) => {
                    cx.emit(ContentShellEvent::PlayNext(Box::new(track.clone())));
                }
                playlists::PlaylistPageEvent::Enqueue(track) => {
                    cx.emit(ContentShellEvent::Enqueue(Box::new(track.clone())));
                }
            },
        )
        .detach();
        cx.subscribe(
            &liked_page,
            |_, _, event: &liked::LikedPageEvent, cx| match event {
                liked::LikedPageEvent::PlayFrom {
                    tracks,
                    start,
                    shuffle,
                } => {
                    cx.emit(ContentShellEvent::PlayFrom {
                        tracks: tracks.clone(),
                        start: *start,
                        shuffle: *shuffle,
                        origin: Some(QueueOrigin::Playlist {
                            id: "liked".to_owned(),
                            name: strings::like().to_owned(),
                        }),
                    });
                }
                liked::LikedPageEvent::PlayNext(track) => {
                    cx.emit(ContentShellEvent::PlayNext(Box::new(track.clone())));
                }
                liked::LikedPageEvent::Enqueue(track) => {
                    cx.emit(ContentShellEvent::Enqueue(Box::new(track.clone())));
                }
            },
        )
        .detach();
        cx.subscribe(
            &search_page,
            |shell, _, event: &search::SearchPageEvent, cx| match event {
                search::SearchPageEvent::PlayFrom {
                    tracks,
                    start,
                    shuffle,
                } => {
                    cx.emit(ContentShellEvent::PlayFrom {
                        tracks: tracks.clone(),
                        start: *start,
                        shuffle: *shuffle,
                        origin: None,
                    });
                }
                search::SearchPageEvent::Navigate(page) => {
                    shell.navigate(page.clone(), cx);
                }
            },
        )
        .detach();
        cx.subscribe(
            &home_page,
            |shell, _, event: &home::HomePageEvent, cx| match event {
                home::HomePageEvent::Play(track) => {
                    cx.emit(ContentShellEvent::Play(track.clone()));
                }
                home::HomePageEvent::PlayFrom {
                    tracks,
                    shuffle,
                    origin,
                } => {
                    cx.emit(ContentShellEvent::PlayFrom {
                        tracks: tracks.clone(),
                        start: 0,
                        shuffle: *shuffle,
                        origin: origin.clone(),
                    });
                }
                home::HomePageEvent::PlayNext(tracks) => {
                    cx.emit(ContentShellEvent::PlayNextMany(tracks.clone()));
                }
                home::HomePageEvent::Enqueue(tracks) => {
                    cx.emit(ContentShellEvent::EnqueueMany(tracks.clone()));
                }
                home::HomePageEvent::Navigate(page) => {
                    shell.navigate(page.clone(), cx);
                }
            },
        )
        .detach();
        cx.subscribe(
            &artists_page,
            |shell, _, event: &artists::ArtistsPageEvent, cx| match event {
                artists::ArtistsPageEvent::PlayFrom {
                    tracks,
                    start,
                    shuffle,
                } => {
                    cx.emit(ContentShellEvent::PlayFrom {
                        tracks: tracks.clone(),
                        start: *start,
                        shuffle: *shuffle,
                        origin: None,
                    });
                }
                artists::ArtistsPageEvent::PlayNext(track) => {
                    cx.emit(ContentShellEvent::PlayNext(Box::new(track.clone())));
                }
                artists::ArtistsPageEvent::Enqueue(track) => {
                    cx.emit(ContentShellEvent::Enqueue(Box::new(track.clone())));
                }
                artists::ArtistsPageEvent::Navigate(page) => {
                    shell.navigate(page.clone(), cx);
                }
            },
        )
        .detach();
        cx.subscribe(
            &albums_page,
            |shell, _, event: &albums::AlbumsPageEvent, cx| match event {
                albums::AlbumsPageEvent::PlayFrom {
                    tracks,
                    start,
                    shuffle,
                } => {
                    cx.emit(ContentShellEvent::PlayFrom {
                        tracks: tracks.clone(),
                        start: *start,
                        shuffle: *shuffle,
                        origin: shell.albums_page.read(cx).playing_origin(),
                    });
                }
                albums::AlbumsPageEvent::PlayNext(track) => {
                    cx.emit(ContentShellEvent::PlayNext(Box::new(track.clone())));
                }
                albums::AlbumsPageEvent::Enqueue(track) => {
                    cx.emit(ContentShellEvent::Enqueue(Box::new(track.clone())));
                }
                albums::AlbumsPageEvent::Navigate(page) => {
                    shell.navigate(page.clone(), cx);
                }
            },
        )
        .detach();
        Self {
            selected_page: selected,
            home_page,
            search_page,
            songs_page,
            artists_page,
            albums_page,
            playlist_page,
            liked_page,
            playlists,
            sidebar,
        }
    }

    pub fn select_page(&mut self, page: Page, cx: &mut gpui::Context<Self>) {
        self.selected_page = page.clone();
        match &page {
            Page::Search => {
                self.search_page.update(cx, |search, cx| search.refresh(cx));
            }
            Page::Home => {
                self.home_page.update(cx, |home, cx| home.refresh(cx));
                self.playlists
                    .update(cx, |playlists, cx| playlists.refresh(cx));
            }
            Page::Songs => {
                self.songs_page.update(cx, |songs, cx| songs.refresh(cx));
            }
            Page::Artists => {
                self.artists_page.update(cx, |artists, cx| {
                    artists.reset(cx);
                    artists.show_list(cx);
                });
            }
            Page::Artist(artist_id) => {
                let artist_id = artist_id.to_string();
                self.artists_page.update(cx, |artists, cx| {
                    artists.reset(cx);
                    artists.show(artist_id, cx);
                });
            }
            Page::Albums => {
                self.albums_page.update(cx, |albums, cx| {
                    albums.reset(cx);
                    albums.show_list(cx);
                });
            }
            Page::Album(album_id) => {
                let album_id = album_id.to_string();
                self.albums_page.update(cx, |albums, cx| {
                    albums.reset(cx);
                    albums.show(album_id, cx);
                });
            }
            Page::PlayLists(PlayList::All) => {
                self.playlists
                    .update(cx, |playlists, cx| playlists.refresh(cx));
            }
            Page::PlayLists(PlayList::Like) => {
                self.liked_page.update(cx, |liked, cx| liked.reload(cx));
            }
            Page::PlayLists(PlayList::Custom(playlist_id)) => {
                let playlist_id = playlist_id.to_string();
                self.playlist_page
                    .update(cx, |playlist, cx| playlist.refresh(playlist_id, cx));
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn navigate(&mut self, page: Page, cx: &mut gpui::Context<Self>) {
        self.select_page(page.clone(), cx);
        cx.emit(ContentShellEvent::Select(page));
    }

    pub fn signed_in(&mut self, cx: &mut gpui::Context<Self>) {
        self.home_page.update(cx, |home, cx| {
            home.reset(cx);
            if self.selected_page == Page::Home {
                home.ensure_loaded(cx);
            }
        });
        self.songs_page.update(cx, |songs, cx| {
            songs.reset(cx);
            if self.selected_page == Page::Songs {
                songs.ensure_loaded(cx);
            }
        });
        let selected_playlist = match &self.selected_page {
            Page::PlayLists(PlayList::Custom(playlist_id)) => Some(playlist_id.to_string()),
            _ => None,
        };
        self.playlist_page.update(cx, |playlist, cx| {
            playlist.reset(cx);
            if let Some(playlist_id) = selected_playlist {
                playlist.select(playlist_id, cx);
            }
        });
        self.liked_page.update(cx, |liked, cx| {
            liked.reset(cx);
            if self.selected_page == Page::PlayLists(PlayList::Like) {
                liked.reload(cx);
            }
        });
        self.artists_page.update(cx, |artists, cx| {
            artists.reset(cx);
            match &self.selected_page {
                Page::Artists => artists.show_list(cx),
                Page::Artist(artist_id) => artists.show(artist_id.to_string(), cx),
                _ => {}
            }
        });
        self.albums_page.update(cx, |albums, cx| {
            albums.reset(cx);
            match &self.selected_page {
                Page::Albums => albums.show_list(cx),
                Page::Album(album_id) => albums.show(album_id.to_string(), cx),
                _ => {}
            }
        });
    }

    pub fn refresh_library(&mut self, cx: &mut gpui::Context<Self>) {
        self.select_page(self.selected_page.clone(), cx);
    }

    pub fn signed_out(&mut self, cx: &mut gpui::Context<Self>) {
        self.home_page.update(cx, |home, cx| home.reset(cx));
        self.songs_page.update(cx, |songs, cx| songs.reset(cx));
        self.playlist_page
            .update(cx, |playlist, cx| playlist.reset(cx));
        self.liked_page.update(cx, |liked, cx| liked.reset(cx));
        self.artists_page
            .update(cx, |artists, cx| artists.reset(cx));
        self.albums_page.update(cx, |albums, cx| albums.reset(cx));
    }

    pub fn set_search_query(&mut self, query: impl Into<String>, cx: &mut gpui::Context<Self>) {
        self.selected_page = Page::Search;
        let query = query.into();
        self.search_page
            .update(cx, |search, cx| search.set_query(query, cx));
        cx.notify();
    }

    pub fn set_songs_query(&mut self, query: impl Into<String>, cx: &mut gpui::Context<Self>) {
        self.songs_page
            .update(cx, |songs, cx| songs.set_query(query, cx));
    }

    pub fn toggle_songs_sort(&mut self, cx: &mut gpui::Context<Self>) {
        self.songs_page
            .update(cx, |songs, cx| songs.toggle_title_sort(cx));
    }

    pub fn set_playlist_query(&mut self, query: impl Into<String>, cx: &mut gpui::Context<Self>) {
        self.playlist_page
            .update(cx, |playlist, cx| playlist.set_query(query, cx));
    }

    pub fn toggle_playlist_sort(&mut self, cx: &mut gpui::Context<Self>) {
        self.playlist_page
            .update(cx, |playlist, cx| playlist.toggle_title_sort(cx));
    }

    pub fn songs_summary(&self, cx: &gpui::App) -> String {
        self.songs_page.read(cx).summary_text()
    }
}

pub enum ContentShellEvent {
    Select(Page),
    Play(Box<TrackResponse>),
    PlayFrom {
        tracks: Vec<TrackResponse>,
        start: usize,
        shuffle: bool,
        origin: Option<QueueOrigin>,
    },
    PlayNext(Box<TrackResponse>),
    Enqueue(Box<TrackResponse>),
    PlayNextMany(Vec<TrackResponse>),
    EnqueueMany(Vec<TrackResponse>),
}

impl gpui::EventEmitter<ContentShellEvent> for ContentShell {}

impl gpui::Render for ContentShell {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.selected_page {
            Page::Home => self.home_page.clone().into_any_element(),
            Page::Search => self.search_page.clone().into_any_element(),
            Page::Songs => self.songs_page.clone().into_any_element(),
            Page::Artists | Page::Artist(_) => self.artists_page.clone().into_any_element(),
            Page::Albums | Page::Album(_) => self.albums_page.clone().into_any_element(),
            Page::PlayLists(PlayList::All) => {
                playlists::render_all(&self.playlists, &self.sidebar, cx)
            }
            Page::PlayLists(PlayList::Like) => self.liked_page.clone().into_any_element(),
            Page::PlayLists(PlayList::Custom(_)) => self.playlist_page.clone().into_any_element(),
            _ => div().size_full().into_any_element(),
        }
    }
}
