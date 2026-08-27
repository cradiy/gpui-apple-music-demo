use std::{collections::HashMap, rc::Rc, sync::Arc};

use gpui::{
    ClipboardItem, Entity, EventEmitter, FontWeight, Image, ImageFormat, Render, SharedString,
    Task, div, prelude::*, px, rgba, svg,
};
use gpui_tokio::Tokio;
use uic::{
    assets::LucideIcons,
    components::{
        context_menu::{ContextMenu, ContextMenuItem},
        toast,
    },
};

use crate::{
    resources::RomusIcons,
    services::{
        api::{PageQuery, TrackResponse},
        library_api,
    },
    ui::{
        components::{
            media_cover,
            track_list::{self, TrackMenuBuilder, TrackPlayHandler},
        },
        strings, theme,
    },
};

const PAGE_SIZE: u32 = 100;

pub enum LikedPageEvent {
    PlayFrom {
        tracks: Vec<TrackResponse>,
        start: usize,
        shuffle: bool,
    },
    PlayNext(TrackResponse),
    Enqueue(TrackResponse),
}

pub struct LikedPage {
    tracks: Vec<TrackResponse>,
    total: i64,
    artwork: HashMap<String, Arc<Image>>,
    artwork_tasks: HashMap<String, Task<()>>,
    action_tasks: HashMap<String, Task<()>>,
    request_task: Option<Task<()>>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    generation: u64,
}

impl EventEmitter<LikedPageEvent> for LikedPage {}

impl LikedPage {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            total: 0,
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
            self.reload(cx);
        }
    }

    pub fn reload(&mut self, cx: &mut gpui::Context<Self>) {
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
        self.artwork.clear();
        self.artwork_tasks.clear();
        self.action_tasks.clear();
        self.request_task = None;
        self.loading = false;
        self.loaded = false;
        self.error = None;
        cx.notify();
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
        let query = PageQuery {
            limit: Some(PAGE_SIZE),
            offset: Some(self.tracks.len() as u64),
        };
        let generation = self.generation;
        self.loading = true;
        self.error = None;
        let request = Tokio::spawn(cx, async move { api.favorites(&query).await });
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
                        page.error = Some(format!("Liked songs request failed: {error}"));
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

    fn unlike(&mut self, track_id: String, cx: &mut gpui::Context<Self>) {
        let key = format!("unlike:{track_id}");
        if self.action_tasks.contains_key(&key) {
            return;
        }
        let Ok(api) = library_api(cx) else {
            toast::error("The library service is unavailable", cx);
            return;
        };
        let request_id = track_id.clone();
        let request = Tokio::spawn(cx, async move { api.unfavorite(&request_id).await });
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
                            page.tracks.retain(|track| track.id != track_id);
                            page.artwork.remove(&track_id);
                            page.total = page.total.saturating_sub(1);
                            toast::success("Song unliked", cx);
                        }
                        Ok(Err(error)) => toast::error(error.user_message(), cx),
                        Err(error) => toast::error(format!("Unlike request failed: {error}"), cx),
                    }
                    cx.notify();
                });
            }),
        );
    }

    fn track_menu(entity: Entity<Self>, track: TrackResponse) -> ContextMenu {
        let copy_track = track.clone();
        let unlike_entity = entity;
        let unlike_track_id = track.id;
        theme::style_context_menu(
            ContextMenu::new()
                .action_with(
                    |_, _| menu_label(LucideIcons::ListPlus, crate::ui::strings::play_next()),
                    {
                        let entity = unlike_entity.clone();
                        let track = copy_track.clone();
                        move |_, cx| {
                            entity.update(cx, |_, cx| {
                                cx.emit(LikedPageEvent::PlayNext(track.clone()));
                            });
                        }
                    },
                )
                .action_with(
                    |_, _| menu_label(LucideIcons::ListEnd, crate::ui::strings::add_to_queue()),
                    {
                        let entity = unlike_entity.clone();
                        let track = copy_track.clone();
                        move |_, cx| {
                            entity.update(cx, |_, cx| {
                                cx.emit(LikedPageEvent::Enqueue(track.clone()));
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
                )
                .separator()
                .item(
                    ContextMenuItem::action_with(
                        |_, _| menu_label(LucideIcons::Heart, "Unlike"),
                        move |_, cx| {
                            unlike_entity
                                .update(cx, |page, cx| page.unlike(unlike_track_id.clone(), cx));
                        },
                    )
                    .danger(),
                ),
        )
    }
}

impl Render for LikedPage {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        self.ensure_loaded(cx);
        let has_more = self.loaded && (self.tracks.len() as i64) < self.total;
        let empty = self.tracks.is_empty();
        let duration_ms = self
            .tracks
            .iter()
            .filter_map(|track| track.duration_ms)
            .sum::<i64>();
        let entity = cx.entity();
        let menu_builder: TrackMenuBuilder =
            Rc::new(move |track, _, _| LikedPage::track_menu(entity.clone(), track));
        let play_entity = cx.entity();
        let on_play: TrackPlayHandler = Rc::new(move |track, _, cx| {
            play_entity.update(cx, |page, cx| {
                let start = page
                    .tracks
                    .iter()
                    .position(|item| item.id == track.id)
                    .unwrap_or(0);
                cx.emit(LikedPageEvent::PlayFrom {
                    tracks: page.tracks.clone(),
                    start,
                    shuffle: false,
                });
            });
        });

        div()
            .id("liked-page-scroll")
            .size_full()
            .overflow_y_scroll()
            .px(px(28.))
            .pt(px(20.))
            .pb(px(118.))
            .text_color(rgba(0xf1eff5ef))
            .child(liked_header(
                self.total.max(self.tracks.len() as i64),
                duration_ms,
                self.tracks.clone(),
                cx,
            ))
            .when(self.loading && empty, |page| {
                page.child(status_text(strings::loading_liked()))
            })
            .when_some(self.error.clone(), |page, error| {
                page.child(status_text(error))
            })
            .when(self.loaded && empty && self.error.is_none(), |page| {
                page.child(status_text(strings::liked_empty()))
            })
            .when(!empty, |page| {
                page.child(div().mt(px(28.)).child(track_list::track_list(
                    &self.tracks,
                    &self.artwork,
                    menu_builder,
                    on_play,
                )))
            })
            .when(has_more, |page| {
                page.child(
                    div()
                        .id("load-more-liked")
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
                            strings::loading()
                        } else {
                            "Load More"
                        })
                        .on_click(cx.listener(|page, _, _, cx| page.load_more(cx))),
                )
            })
    }
}

fn liked_header(
    track_count: i64,
    duration_ms: i64,
    tracks: Vec<TrackResponse>,
    cx: &mut gpui::Context<LikedPage>,
) -> impl IntoElement {
    div()
        .min_h(px(212.))
        .flex()
        .items_end()
        .gap(px(25.))
        .child(media_cover::liked_playlist_cover(px(212.), px(14.)))
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
                        .child(strings::like()),
                )
                .child(
                    div()
                        .mt(px(12.))
                        .max_w(px(620.))
                        .text_size(px(14.))
                        .text_color(rgba(0xb8b5c0a6))
                        .child(strings::liked_description()),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .text_size(px(13.))
                        .text_color(rgba(0xa8a5b18f))
                        .child(format!(
                            "{} songs · {}",
                            track_count,
                            format_duration(duration_ms)
                        )),
                )
                .child(
                    div()
                        .mt(px(17.))
                        .flex()
                        .items_center()
                        .gap(px(9.))
                        .child(play_context_button(
                            "liked-play",
                            RomusIcons::MusicPlay,
                            "Play",
                            tracks.clone(),
                            false,
                            cx,
                        ))
                        .child(play_context_button(
                            "liked-shuffle",
                            LucideIcons::Shuffle,
                            "Shuffle",
                            tracks,
                            true,
                            cx,
                        )),
                ),
        )
}

fn play_context_button(
    id: &'static str,
    icon: impl Into<SharedString>,
    label: &'static str,
    tracks: Vec<TrackResponse>,
    shuffle: bool,
    cx: &mut gpui::Context<LikedPage>,
) -> impl IntoElement {
    let enabled = !tracks.is_empty();
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
        .when(enabled, |button| button.cursor_pointer())
        .on_click(cx.listener(move |_, _, _, cx| {
            if tracks.is_empty() {
                return;
            }
            cx.emit(LikedPageEvent::PlayFrom {
                tracks: tracks.clone(),
                start: 0,
                shuffle,
            });
        }))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xff5576d0)))
        .child(label)
}

fn menu_label(icon: LucideIcons, label: &'static str) -> impl IntoElement {
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
