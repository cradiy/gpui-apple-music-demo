mod icons;
mod library;
mod motion;
mod output;
mod queue;
#[cfg(target_os = "android")]
mod system_media;
mod view;

use crate::domain::lyrics::LyricsDocument;
use crate::ui::components::lyrics_panel::{self, LyricsPanelState};
use gpui::{prelude::*, *};
use gpui_media::PlaybackState;
use gpui_media::{
    AudioPlayer as Player, AudioPlayerEvent as PlayerEvent, AudioPlayerOptions as PlayerOptions,
    AudioSource as Source,
};
use library::{Library, Track};
use queue::{QueueEntry, QueueUi};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
use uic::components::{
    input::{InputEvent, TextInput},
    modal,
    slider::{SliderEvent, SliderState},
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Page {
    Home,
    Discover,
    Radio,
    Library,
    NowPlaying,
    Search,
    Collection,
    EditLyrics,
    SongInfo,
}
impl Page {
    fn is_tab(self) -> bool {
        matches!(
            self,
            Self::Home | Self::Discover | Self::Radio | Self::Library | Self::Search
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LibraryTab {
    Songs,
    Albums,
    Playlists,
}

#[derive(Clone)]
enum Sheet {
    Import,
    Song(String),
    Output,
    Remove(String),
}

pub struct MobileApp {
    page: Page,
    page_motion: Option<motion::PageMotion>,
    browse_scenes: std::collections::HashMap<Page, Entity<view::BrowseScene>>,
    player_dismiss: motion::PlayerDismiss,
    return_page: Page,
    library_tab: LibraryTab,
    library: Option<Library>,
    player: Option<Entity<Player>>,
    player_subscription: Option<Subscription>,
    #[cfg(target_os = "android")]
    player_load: Option<Task<()>>,
    #[cfg(target_os = "android")]
    play_when_loaded: bool,
    #[cfg(target_os = "android")]
    system_media: system_media::SystemMedia,
    current: Option<String>,
    lyrics: LyricsDocument,
    show_lyrics: bool,
    show_translation: bool,
    show_queue: bool,
    lyrics_motion: motion::ValueMotion,
    controls_hidden: bool,
    controls_motion: motion::ControlsMotion,
    controls_scroll: motion::LyricsControlsScroll,
    controls_reveal: motion::ControlsRevealGesture,
    controls_idle_since: Instant,
    controls_idle_task: Option<Task<()>>,
    busy: bool,
    error: Option<String>,
    lyric_state: LyricsPanelState,
    visible_artwork: Option<ImageSource>,
    backdrop_elapsed: f32,
    backdrop_sampled_at: Option<Instant>,
    backdrop_frame: Option<Task<()>>,
    #[cfg(target_os = "android")]
    backdrop_prepare_started: bool,
    #[cfg(target_os = "android")]
    backdrop_prepared: bool,
    #[cfg(target_os = "android")]
    backdrop_reveal: motion::ValueMotion,
    window_active: bool,
    window_subscription: Option<Subscription>,
    search: Entity<TextInput>,
    lyrics_editor: Entity<TextInput>,
    timeline_slider: Entity<SliderState>,
    volume_slider: Entity<SliderState>,
    seek_dragging: bool,
    queue: Vec<QueueEntry>,
    queue_current: Option<SharedString>,
    queue_ui: Option<QueueUi>,
    sheet_stack: Vec<Sheet>,
    output: output::OutputState,
    queue_menu_entry: Option<SharedString>,
    recent: Vec<String>,
    repeat: bool,
    shuffle: bool,
    collection_title: String,
    collection: Vec<String>,
    info_track: Option<String>,
    _subscriptions: Vec<Subscription>,
}

const BACKGROUND: u32 = 0x1c202c;
const SURFACE: u32 = 0x282c38;
const TEXT: u32 = 0xf1f1f5;
const SECONDARY: u32 = 0x9295a2;
const ACCENT: u32 = 0xfa2d55;

impl MobileApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        uic::init(cx);
        let search = cx.new(|cx| TextInput::new(cx).placeholder("Songs, artists, albums"));
        let lyrics_editor = cx.new(|cx| TextInput::new(cx).multiline().placeholder("Lyrics"));
        let timeline_slider = cx.new(|cx| SliderState::new(0., 0. ..=1., cx));
        let volume_slider = cx.new(|cx| SliderState::new(1., 0. ..=1., cx));
        let subscriptions = vec![
            cx.observe(&modal::layer(cx), |this: &mut Self, _, cx| {
                if !modal::is_open(cx) {
                    this.output.subscription = None;
                }
                cx.notify();
            }),
            cx.subscribe(&search, |_: &mut Self, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(
                &timeline_slider,
                |this: &mut Self, _, event, cx| match event {
                    SliderEvent::Changing(_) => {
                        this.seek_dragging = true;
                        cx.notify();
                    }
                    SliderEvent::Changed(value) => {
                        this.seek_dragging = false;
                        if let Some(player) = &this.player {
                            if let Some(duration) = player.read(cx).duration() {
                                this.seek_to(duration.mul_f64(value.clamp(0., 1.)), cx);
                            }
                        }
                    }
                },
            ),
            cx.subscribe(&volume_slider, |this: &mut Self, _, event, cx| {
                let (SliderEvent::Changing(value) | SliderEvent::Changed(value)) = event;
                if let Some(player) = &this.player {
                    player.update(cx, |player, cx| player.set_volume(*value, cx));
                }
            }),
        ];
        let io = cx.file_system("dev.romus.demo");
        cx.spawn(async move |this, cx| {
            let result: anyhow::Result<Library> = async {
                let location = io?.location(gpui::gpui_io::SystemLocation::AppData).await?;
                let root = location
                    .path()
                    .ok_or_else(|| anyhow::anyhow!("Private storage is unavailable"))?
                    .join("music");
                cx.background_executor()
                    .spawn(async move { Library::load(root) })
                    .await
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(library) => this.library = Some(library),
                    Err(error) => this.error = Some(error.to_string()),
                }
                this.busy = false;
                cx.notify();
            });
        })
        .detach();
        Self {
            page: Page::Home,
            page_motion: None,
            browse_scenes: std::collections::HashMap::new(),
            player_dismiss: motion::PlayerDismiss::default(),
            return_page: Page::Home,
            library_tab: LibraryTab::Songs,
            library: None,
            player: None,
            player_subscription: None,
            #[cfg(target_os = "android")]
            player_load: None,
            #[cfg(target_os = "android")]
            play_when_loaded: true,
            #[cfg(target_os = "android")]
            system_media: system_media::SystemMedia::default(),
            current: None,
            lyrics: LyricsDocument::empty(),
            show_lyrics: false,
            show_translation: true,
            show_queue: false,
            lyrics_motion: motion::ValueMotion::new(0.),
            controls_hidden: false,
            controls_motion: motion::ControlsMotion::new(),
            controls_scroll: motion::LyricsControlsScroll::default(),
            controls_reveal: motion::ControlsRevealGesture::default(),
            controls_idle_since: Instant::now(),
            controls_idle_task: None,
            busy: true,
            error: None,
            lyric_state: LyricsPanelState::default(),
            visible_artwork: None,
            backdrop_elapsed: 0.,
            backdrop_sampled_at: None,
            backdrop_frame: None,
            #[cfg(target_os = "android")]
            backdrop_prepare_started: false,
            #[cfg(target_os = "android")]
            backdrop_prepared: false,
            #[cfg(target_os = "android")]
            backdrop_reveal: motion::ValueMotion::new(0.),
            window_active: true,
            window_subscription: None,
            search,
            lyrics_editor,
            timeline_slider,
            volume_slider,
            seek_dragging: false,
            queue: Vec::new(),
            queue_current: None,
            queue_ui: None,
            sheet_stack: Vec::new(),
            output: output::OutputState::default(),
            queue_menu_entry: None,
            recent: Vec::new(),
            repeat: false,
            shuffle: false,
            collection_title: String::new(),
            collection: Vec::new(),
            info_track: None,
            _subscriptions: subscriptions,
        }
    }

    fn navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if page == self.page {
            return;
        }
        if self.page == Page::NowPlaying && page == self.return_page {
            self.dismiss_player(window, cx);
            return;
        }
        self.player_dismiss = motion::PlayerDismiss::default();
        if page == Page::NowPlaying && !matches!(self.page, Page::NowPlaying | Page::EditLyrics) {
            self.return_page = self.page;
            self.player_dismiss =
                motion::PlayerDismiss::opening(window.viewport_size().height.as_f32());
            self.page_motion = None;
            self.page = page;
            window.set_back_enabled(true);
            cx.notify();
            return;
        }
        if self.page == Page::EditLyrics && page != Page::EditLyrics {
            window.blur();
        }
        self.page_motion = Some(motion::PageMotion::new(self.page, page));
        self.page = page;
        window.set_back_enabled(!page.is_tab());
        cx.notify();
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if modal::is_open(cx) {
            if self.sheet_stack.len() > 1 {
                self.sheet_stack.pop();
                cx.notify();
                return;
            }
            modal::dismiss(window, cx);
            window.set_back_enabled(!self.page.is_tab());
            cx.notify();
        } else if self.page == Page::NowPlaying && (self.show_lyrics || self.show_queue) {
            self.show_lyrics = false;
            self.show_queue = false;
            cx.notify();
        } else if self.page == Page::NowPlaying {
            self.navigate(self.return_page, window, cx);
        } else if self.page == Page::EditLyrics {
            self.navigate(Page::NowPlaying, window, cx);
        } else {
            self.navigate(Page::Library, window, cx);
        }
    }

    fn import(&mut self, lyrics: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(mut library) = self.library.clone() else {
            return;
        };
        let current = self.current.clone();
        if lyrics && current.is_none() {
            return;
        }
        let selection = cx.prompt_for_files(FilePromptOptions {
            multiple: !lyrics,
            mime_types: if lyrics {
                vec!["*/*".into()]
            } else {
                vec!["audio/*".into()]
            },
            ..Default::default()
        });
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mut errors = Vec::new();
            let result: anyhow::Result<()> = async {
                if let Some(files) = selection.await?? {
                    for file in files {
                        let name = file.name().to_owned();
                        let bytes = match file
                            .read_limited(if lyrics {
                                4 * 1024 * 1024
                            } else {
                                256 * 1024 * 1024
                            })
                            .await
                        {
                            Ok(bytes) => bytes,
                            Err(error) => {
                                errors.push(format!("{name}: {error}"));
                                continue;
                            }
                        };
                        let id = current.clone();
                        let mut imported = library.clone();
                        let (next, result) = cx
                            .background_executor()
                            .spawn(async move {
                                let result = if lyrics {
                                    imported.set_lyrics(id.as_deref().unwrap(), bytes)
                                } else {
                                    imported.import(&name, bytes)
                                };
                                (imported, result)
                            })
                            .await;
                        library = next;
                        if let Err(error) = result {
                            errors.push(error.to_string());
                        }
                    }
                }
                Ok(())
            }
            .await;
            if let Err(error) = result {
                errors.push(error.to_string());
            }
            let _ = this.update(cx, |this, cx| {
                this.library = Some(library);
                this.busy = false;
                this.error = (!errors.is_empty()).then(|| errors.join("\n"));
                this.update_lyrics();
                cx.notify();
            });
        })
        .detach();
    }

    fn track(&self) -> Option<&Track> {
        self.library
            .as_ref()?
            .tracks
            .iter()
            .find(|track| Some(&track.id) == self.current.as_ref())
    }

    fn update_lyrics(&mut self) {
        self.lyrics = LyricsDocument::parse(self.track().and_then(|track| track.lyrics.as_deref()));
        self.lyric_state = LyricsPanelState::default();
    }

    fn play_track(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(track) = self
            .library
            .as_ref()
            .and_then(|library| library.tracks.iter().find(|track| track.id == id))
        else {
            return;
        };
        let source = match Source::from_path(&track.file) {
            Ok(source) => source,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        #[cfg(target_os = "android")]
        {
            self.player_load = None;
            self.play_when_loaded = true;
        }
        self.player_subscription = None;
        self.player = None;
        let options = PlayerOptions {
            volume: self.volume_slider.read(cx).value(),
            timeline_update_interval: Duration::from_millis(250),
            #[cfg(target_os = "android")]
            autoplay: false,
            ..Default::default()
        };
        self.current = Some(id.to_owned());
        if !self.queue.iter().any(|queued| queued.track_id == id) {
            self.queue = self
                .library
                .as_ref()
                .unwrap()
                .tracks
                .iter()
                .map(|track| QueueEntry::new(track.id.clone()))
                .collect();
        }
        self.queue_current = self
            .queue
            .iter()
            .find(|entry| entry.track_id == id)
            .map(|entry| entry.key.clone());
        self.recent.retain(|recent| recent != id);
        self.recent.insert(0, id.to_owned());
        self.update_lyrics();
        self.error = None;
        #[cfg(target_os = "android")]
        {
            let id = id.to_owned();
            let preparation = cx
                .background_executor()
                .spawn(async move { Player::builder(source).options(options).prepare() });
            self.player_load = Some(cx.spawn(async move |this, cx| {
                let result = preparation.await;
                let _ = this.update(cx, |this, cx| {
                    if this.current.as_ref() != Some(&id) {
                        return;
                    }
                    this.player_load = None;
                    match result {
                        Ok(prepared) => {
                            let player = cx.new(|cx| prepared.build(cx));
                            this.attach_player(player, cx);
                        }
                        Err(error) => {
                            this.error = Some(error.to_string());
                            cx.notify();
                        }
                    }
                });
            }));
        }
        #[cfg(not(target_os = "android"))]
        {
            let player = cx
                .new(|cx| Player::new(source, options, cx).expect("failed to create media player"));
            self.attach_player(player, cx);
        }
        cx.notify();
    }

    fn attach_player(&mut self, player: Entity<Player>, cx: &mut Context<Self>) {
        #[cfg(target_os = "android")]
        player.update(cx, |player, cx| {
            player.set_volume(self.volume_slider.read(cx).value(), cx);
            if let Err(error) = player.set_wake_mode(gpui_media::PlaybackWakeMode::Local, cx) {
                log::warn!("Could not enable playback wake lock: {error}");
            }
            if self.play_when_loaded
                && let Err(error) = player.play(cx)
            {
                self.error = Some(error.to_string());
            }
        });
        self.player_subscription = Some(cx.subscribe(&player, |this, _, event, cx| {
            let timeline_visible = this.window_active
                && this.page == Page::NowPlaying
                && !this.controls_hidden
                && !modal::is_open(cx);
            match event {
                PlayerEvent::StateChanged(PlaybackState::Error(error)) => {
                    this.error = Some(error.to_string())
                }
                PlayerEvent::StateChanged(PlaybackState::Ended) => {
                    if this.repeat {
                        this.toggle(cx);
                    } else {
                        let next = this.queue_position().map_or(0, |index| index + 1);
                        if let Some(entry) = this.queue.get(next).cloned() {
                            this.play_queue_entry(&entry, cx);
                        }
                    }
                }
                PlayerEvent::TimelineChanged(timeline) => {
                    if timeline_visible && !this.seek_dragging {
                        let value = timeline
                            .duration()
                            .filter(|duration| !duration.is_zero())
                            .map_or(0., |duration| {
                                timeline.position().as_secs_f64() / duration.as_secs_f64()
                            });
                        this.timeline_slider
                            .update(cx, |slider, cx| slider.set_value(value.clamp(0., 1.), cx));
                    }
                }
                _ => {}
            }
            #[cfg(target_os = "android")]
            this.publish_system_media(!matches!(event, PlayerEvent::TimelineChanged(_)), cx);
            if timeline_visible || !matches!(event, PlayerEvent::TimelineChanged(_)) {
                cx.notify();
            }
        }));
        self.player = Some(player);
        #[cfg(target_os = "android")]
        self.connect_system_media(cx);
        cx.notify();
    }

    fn toggle(&mut self, cx: &mut Context<Self>) {
        if self
            .player
            .as_ref()
            .is_some_and(|player| matches!(player.read(cx).state(), PlaybackState::Error(_)))
        {
            if let Some(id) = self.current.clone() {
                self.play_track(&id, cx);
            }
            return;
        }
        #[cfg(target_os = "android")]
        self.retry_background_playback();
        #[cfg(target_os = "android")]
        if self.player_load.is_some() {
            self.play_when_loaded = !self.play_when_loaded;
        }
        if let Some(player) = &self.player {
            if let Err(error) = player.update(cx, |player, cx| player.toggle_playback(cx)) {
                self.error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    fn skip(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.queue.is_empty() {
            return;
        }
        let index = self
            .queue_position()
            .map_or(if delta < 0 { 0 } else { -1 }, |index| index as isize);
        let index = (index + delta).rem_euclid(self.queue.len() as isize) as usize;
        let entry = self.queue[index].clone();
        self.play_queue_entry(&entry, cx);
    }

    fn queue_position(&self) -> Option<usize> {
        self.queue
            .iter()
            .position(|entry| Some(&entry.key) == self.queue_current.as_ref())
    }

    fn play_queue_entry(&mut self, entry: &QueueEntry, cx: &mut Context<Self>) {
        self.play_track(&entry.track_id, cx);
        self.queue_current = Some(entry.key.clone());
    }

    fn seek_to(&mut self, position: Duration, cx: &mut Context<Self>) {
        if let Some(player) = &self.player {
            if let Err(error) = player.update(cx, |player, cx| {
                player.seek_to(position, gpui_media::SeekMode::Accurate, cx)
            }) {
                self.error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    fn edit_lyrics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self
            .track()
            .and_then(|track| track.lyrics.clone())
            .unwrap_or_default();
        self.lyrics_editor
            .update(cx, |input, cx| input.set_value(value, cx));
        self.navigate(Page::EditLyrics, window, cx);
    }

    fn persist(
        &mut self,
        operation: impl FnOnce(&mut Library) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(mut library) = self.library.clone() else {
            return;
        };
        self.busy = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    operation(&mut library)?;
                    Ok::<_, anyhow::Error>(library)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(library) => {
                        let lyrics_changed = this.track().and_then(|track| track.lyrics.as_ref())
                            != library
                                .tracks
                                .iter()
                                .find(|track| Some(&track.id) == this.current.as_ref())
                                .and_then(|track| track.lyrics.as_ref());
                        this.library = Some(library);
                        if lyrics_changed {
                            this.update_lyrics();
                        }
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                this.busy = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn favorite(&mut self, id: String, cx: &mut Context<Self>) {
        self.persist(
            move |library| {
                let track = library
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == id)
                    .ok_or_else(|| anyhow::anyhow!("Song not found"))?;
                track.favorite = !track.favorite;
                library.save()
            },
            cx,
        );
    }

    fn shuffle_queue(&mut self, cx: &mut Context<Self>) {
        self.shuffle = !self.shuffle;
        let start = self.queue_position().map_or(0, |index| index + 1);
        let upcoming = &mut self.queue[start..];
        if self.shuffle {
            upcoming.sort_by_cached_key(|_| uuid::Uuid::new_v4());
        } else if let Some(library) = &self.library {
            upcoming.sort_by_key(|entry| {
                library
                    .tracks
                    .iter()
                    .position(|track| track.id == entry.track_id)
            });
        }
        cx.notify();
    }

    fn play_all(&mut self, shuffle: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.queue = if self.page == Page::Collection {
            self.collection
                .iter()
                .cloned()
                .map(QueueEntry::new)
                .collect()
        } else {
            self.library.as_ref().map_or(Vec::new(), |library| {
                library
                    .tracks
                    .iter()
                    .map(|track| QueueEntry::new(track.id.clone()))
                    .collect()
            })
        };
        self.shuffle = shuffle;
        if shuffle {
            self.queue.sort_by_cached_key(|_| uuid::Uuid::new_v4());
        }
        if let Some(entry) = self.queue.first().cloned() {
            self.play_queue_entry(&entry, cx);
            self.show_lyrics = false;
            self.navigate(Page::NowPlaying, window, cx);
        }
    }

    fn playing(&self, cx: &App) -> bool {
        #[cfg(target_os = "android")]
        if self.player_load.is_some() {
            return self.play_when_loaded;
        }
        self.player.as_ref().is_some_and(|player| {
            matches!(
                player.read(cx).state(),
                PlaybackState::Playing | PlaybackState::Loading
            )
        })
    }
}
