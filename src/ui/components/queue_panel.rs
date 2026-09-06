use std::{cell::Cell, collections::HashMap, rc::Rc, sync::Arc, time::Instant};

use gpui::{
    AnyElement, CursorStyle, DragEnd, DragMoveEvent, FontWeight, Image, ImageFormat, MouseButton,
    Render, SharedString, Window, canvas, div, prelude::*, px, rgb, rgba, svg,
};
use gpui_tokio::Tokio;
use uic::{
    assets::LucideIcons,
    components::context_menu::{self, ContextMenu, ContextMenuExt},
};

use crate::{
    app::music_shell::MusicShell,
    domain::Page,
    resources::RomusIcons,
    services::{api::TrackResponse, library_api},
    ui::{
        components::media_cover::{self, CoverSource},
        strings,
        theme::{self, ACCENT, layout},
    },
};

const ROW_HEIGHT: f32 = 58.0;
const LABEL_HEIGHT: f32 = 28.0;
const ROW_SPRING_OMEGA: f32 = 64.0;

#[derive(Clone, Copy)]
struct SpringSlot {
    current: f32,
    velocity: f32,
    target: f32,
}

impl SpringSlot {
    fn settled(target: f32) -> Self {
        Self {
            current: target,
            velocity: 0.0,
            target,
        }
    }

    fn step(&mut self, dt: f32) -> bool {
        let displacement = self.current - self.target;
        let spring = self.velocity + ROW_SPRING_OMEGA * displacement;
        let decay = (-ROW_SPRING_OMEGA * dt).exp();
        self.current = self.target + (displacement + spring * dt) * decay;
        self.velocity = (self.velocity - ROW_SPRING_OMEGA * spring * dt) * decay;

        if (self.target - self.current).abs() < 0.18 && self.velocity.abs() < 20.0 {
            self.current = self.target;
            self.velocity = 0.0;
            false
        } else {
            true
        }
    }
}

pub struct QueueUiState {
    slots: HashMap<u64, SpringSlot>,
    drag_key: Option<u64>,
    cursor_offset_y: f32,
    last_frame: Instant,
    list_origin_y: Rc<Cell<f32>>,
}

impl Default for QueueUiState {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
            drag_key: None,
            cursor_offset_y: 0.0,
            last_frame: Instant::now(),
            list_origin_y: Rc::new(Cell::new(0.0)),
        }
    }
}

#[derive(Clone)]
struct QueueDrag {
    key: u64,
    title: SharedString,
    artist: SharedString,
    cover: Option<Arc<Image>>,
}

struct QueueDragPreview {
    fullscreen: bool,
    title: SharedString,
    artist: SharedString,
    cover: Option<Arc<Image>>,
}

impl Render for QueueDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let cover = self.cover.clone().map(CoverSource::Image);
        let content = div()
            .w(px(layout::LYRICS_WIDTH - 28.0))
            .h(px(ROW_HEIGHT))
            .px(px(10.))
            .rounded(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .when(!self.fullscreen, |this| {
                this.bg(rgb(0x3a3b42)).shadow(vec![
                    gpui::BoxShadow::new(px(0.), px(8.), rgba(0x00000055).into())
                        .blur_radius(px(18.))
                        .spread_radius(px(-4.)),
                ])
            })
            .child(media_cover::media_cover(
                self.title.clone(),
                cover,
                px(38.),
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
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgba(0xf1eff5ef))
                            .child(self.title.clone()),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .truncate()
                            .text_size(px(11.))
                            .text_color(rgba(if self.fullscreen {
                                0xffffffad
                            } else {
                                0xb3afbb9c
                            }))
                            .child(self.artist.clone()),
                    ),
            );
        if self.fullscreen {
            theme::frosted()
                .appearance(gpui_effects::FrostedGlassAppearance {
                    blur_radius: px(24.),
                    tint: rgba(0xffffff13).into(),
                    brightness: 1.,
                    saturation: 1.,
                    edge: rgba(0xffffff0a).into(),
                    sheen: 0.,
                    ..theme::frosted_appearance()
                })
                .rounded(px(10.))
                .child(content)
                .into_any_element()
        } else {
            content.into_any_element()
        }
    }
}

pub fn render(
    app: &mut MusicShell,
    window: &mut Window,
    cx: &mut gpui::Context<MusicShell>,
) -> impl IntoElement {
    render_panel(app, window, cx, false)
}

pub fn render_now_playing(
    app: &mut MusicShell,
    window: &mut Window,
    cx: &mut gpui::Context<MusicShell>,
) -> impl IntoElement {
    render_panel(app, window, cx, true)
}

fn render_panel(
    app: &mut MusicShell,
    window: &mut Window,
    cx: &mut gpui::Context<MusicShell>,
    fullscreen: bool,
) -> impl IntoElement {
    sync_artwork(app, cx);
    let (items, current_key, current_index, has_upcoming, playing_from) = {
        let playback = app.playback.read(cx);
        (
            playback.queue.items.clone(),
            playback.queue.current_key(),
            playback.queue.index,
            playback.queue.has_next(),
            playback.queue.playing_from().map(str::to_owned),
        )
    };

    step_slots(app, window, &items, current_index);
    let artwork = app.queue_artwork.clone();
    let entity = cx.entity();
    let dragging = app.queue_ui.drag_key;
    let empty = items.is_empty();
    let origin_y = app.queue_ui.list_origin_y.clone();
    let current = current_index.unwrap_or(0);
    let mut list_children: Vec<AnyElement> = Vec::new();
    if current_index.is_some() {
        list_children.push(
            section_label(strings::now_playing(), current as f32 * ROW_HEIGHT).into_any_element(),
        );
        if has_upcoming {
            list_children.push(
                section_label(
                    strings::playing_next(),
                    item_top(current, current) + ROW_HEIGHT,
                )
                .into_any_element(),
            );
        }
    }
    list_children.extend(items.iter().enumerate().map(|(index, item)| {
        let played = current_index.is_some_and(|current| index < current);
        let top = app
            .queue_ui
            .slots
            .get(&item.key)
            .map_or(item_top(index, current), |slot| slot.current);
        queue_row(
            item.key,
            top,
            &item.track,
            current_key == Some(item.key),
            played,
            dragging,
            entity.clone(),
            &artwork,
        )
        .into_any_element()
    }));

    div()
        .id(if fullscreen {
            "now-playing-queue-panel"
        } else {
            "play-queue-panel"
        })
        .relative()
        .h_full()
        .w(px(layout::LYRICS_WIDTH))
        .flex()
        .flex_col()
        .overflow_hidden()
        .when(fullscreen, |panel| panel.w_full())
        .when(!fullscreen, |panel| {
            panel
                .bg(gpui::rgb(0x2c2d32))
                .border_l_1()
                .border_color(rgba(0xffffff10))
        })
        .text_color(rgba(0xf1eff5ef))
        .on_drag_move::<QueueDrag>(cx.listener(|app, event: &DragMoveEvent<QueueDrag>, _, cx| {
            let key = event.drag(cx).key;
            let (count, current) = {
                let playback = app.playback.read(cx);
                (
                    playback.queue.items.len(),
                    playback.queue.index.unwrap_or(0),
                )
            };
            let row_center =
                event.event.position.y.as_f32() - app.queue_ui.cursor_offset_y + ROW_HEIGHT * 0.5;
            let local_y = row_center - app.queue_ui.list_origin_y.get();
            let index = insertion_index(local_y, current, count);
            app.playback.update(cx, |playback, cx| {
                playback.move_queue_key(key, index, cx);
            });
        }))
        .child(
            div()
                .h(px(if playing_from.is_some() { 72. } else { 60. }))
                .px(px(22.))
                .flex()
                .items_center()
                .justify_between()
                .flex_none()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .pr(px(12.))
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(0xffffffe0))
                                .child("QUEUE"),
                        )
                        .when_some(playing_from.clone(), |header, name| {
                            header.child(
                                div()
                                    .mt(px(3.))
                                    .truncate()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgba(0xffffffe6))
                                    .child(format!("{} {name}", strings::playing_from())),
                            )
                        }),
                )
                .when(has_upcoming, |header| {
                    header.child(
                        div()
                            .id("clear-play-queue")
                            .px(px(6.))
                            .py(px(3.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgba(0xff6684d8))
                            .hover(|button| button.bg(rgba(0xffffff10)))
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.playback
                                    .update(cx, |playback, cx| playback.clear_up_next(cx));
                            }))
                            .child(strings::clear()),
                    )
                }),
        )
        .child(
            div()
                .id("play-queue-scroll")
                .flex_1()
                .min_h_0()
                .px(px(8.))
                .pb(px(24.))
                .overflow_y_scroll()
                .when(empty, |list| {
                    list.child(
                        div()
                            .pt(px(48.))
                            .flex()
                            .justify_center()
                            .text_size(px(13.))
                            .text_color(rgba(0xffffffe0))
                            .child(strings::queue_empty()),
                    )
                })
                .when(!items.is_empty(), |list| {
                    list.child(
                        div()
                            .id("queue-track-list")
                            .relative()
                            .w_full()
                            .h(px(list_height(items.len(), current, has_upcoming)))
                            .child(
                                canvas(
                                    move |bounds, _, _| {
                                        origin_y.set(bounds.origin.y.as_f32());
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .children(list_children),
                    )
                }),
        )
}

fn item_top(index: usize, current: usize) -> f32 {
    let mut top = index as f32 * ROW_HEIGHT;
    if index >= current {
        top += LABEL_HEIGHT;
    }
    if index > current {
        top += LABEL_HEIGHT;
    }
    top
}

fn list_height(count: usize, current: usize, has_next: bool) -> f32 {
    if count == 0 {
        return 0.0;
    }
    count as f32 * ROW_HEIGHT
        + LABEL_HEIGHT
        + if has_next && current + 1 < count {
            LABEL_HEIGHT
        } else {
            0.0
        }
}

fn step_slots(
    app: &mut MusicShell,
    window: &mut Window,
    items: &[crate::state::QueueItem],
    current_index: Option<usize>,
) {
    let now = Instant::now();
    let dt = now
        .saturating_duration_since(app.queue_ui.last_frame)
        .as_secs_f32()
        .min(1.0 / 20.0);
    app.queue_ui.last_frame = now;
    let current = current_index.unwrap_or(0);

    let mut moving = false;
    for (index, item) in items.iter().enumerate() {
        let target = item_top(index, current);
        let slot = app
            .queue_ui
            .slots
            .entry(item.key)
            .or_insert_with(|| SpringSlot::settled(target));
        slot.target = target;
        moving |= slot.step(dt);
    }
    app.queue_ui
        .slots
        .retain(|key, _| items.iter().any(|item| item.key == *key));

    if moving || app.queue_ui.drag_key.is_some() {
        window.request_animation_frame();
    }
}

fn insertion_index(local_y: f32, current: usize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let mut best = 0;
    let mut best_dist = f32::MAX;
    for index in 0..count {
        let center = item_top(index, current) + ROW_HEIGHT * 0.5;
        let dist = (local_y - center).abs();
        if dist < best_dist {
            best_dist = dist;
            best = index;
        }
    }
    best
}

fn section_label(label: &'static str, top: f32) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(top))
        .h(px(LABEL_HEIGHT))
        .px(px(10.))
        .flex()
        .items_center()
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba(0xffffffe0))
        .child(label)
}

#[allow(clippy::too_many_arguments)]
fn queue_row(
    key: u64,
    top: f32,
    track: &TrackResponse,
    current: bool,
    played: bool,
    dragging: Option<u64>,
    entity: gpui::Entity<MusicShell>,
    artwork: &HashMap<String, Arc<Image>>,
) -> impl IntoElement {
    let title = track
        .title
        .clone()
        .unwrap_or_else(|| strings::untitled().to_owned());
    let artist = track
        .artist
        .clone()
        .unwrap_or_else(|| strings::unknown_artist().to_owned());
    let cover = artwork.get(&track.id).cloned().map(CoverSource::Image);
    let duration = format_duration(track.duration_ms);
    let is_dragging = dragging == Some(key);
    let group_name = SharedString::from(format!("queue-row-{key}"));
    let menu_track = track.clone();
    let menu_entity = entity.clone();
    let drag = QueueDrag {
        key,
        title: SharedString::from(title.clone()),
        artist: SharedString::from(artist.clone()),
        cover: artwork.get(&track.id).cloned(),
    };

    let row = div()
        .id(("queue-row", key))
        .group(group_name.clone())
        .absolute()
        .left_0()
        .right_0()
        .top(px(top))
        .h(px(ROW_HEIGHT))
        .px(px(10.))
        .rounded(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .when(current, |row| row.bg(rgba(0xffffff12)))
        .cursor(if is_dragging {
            CursorStyle::ClosedHand
        } else {
            CursorStyle::PointingHand
        })
        .hover(|row| {
            if current {
                row.bg(rgba(0xffffff16))
            } else {
                row.bg(rgba(0xffffff0a))
            }
        })
        .opacity(if is_dragging { 0.0 } else { 1.0 })
        .when(current, |row| {
            row.child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(14.))
                    .bottom(px(14.))
                    .w(px(2.))
                    .rounded_full()
                    .bg(rgb(ACCENT)),
            )
        })
        .on_click({
            let entity = entity.clone();
            move |_, _, cx| {
                if cx.has_active_drag() {
                    return;
                }
                entity.update(cx, |app, cx| {
                    app.playback.update(cx, |playback, cx| {
                        playback.play_queue_key(key, cx);
                    });
                });
            }
        })
        .child(
            div()
                .relative()
                .size(px(38.))
                .flex_none()
                .child(media_cover::media_cover(
                    &title,
                    cover,
                    px(38.),
                    px(7.),
                    false,
                ))
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(px(7.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(0.)
                        .group_hover(group_name.clone(), |overlay| {
                            overlay.opacity(1.).bg(rgba(0x090a0fa8))
                        })
                        .child(
                            svg()
                                .path(RomusIcons::MusicPlay)
                                .size(px(13.))
                                .text_color(rgba(0xffffffff)),
                        ),
                ),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(if current {
                            rgba(0xffffffff)
                        } else if played {
                            rgba(0xffffffe6)
                        } else {
                            rgba(0xfffffff5)
                        })
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(2.))
                        .truncate()
                        .text_size(px(11.))
                        .text_color(if played {
                            rgba(0xffffffd9)
                        } else {
                            rgba(0xffffffe6)
                        })
                        .child(artist),
                ),
        )
        .child(
            div()
                .relative()
                .w(px(54.))
                .h(px(32.))
                .flex_none()
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_end()
                        .text_size(px(11.))
                        .text_color(rgba(0xffffffe0))
                        .group_hover(group_name.clone(), |label| label.invisible())
                        .child(duration),
                )
                .child({
                    let menu_entity = menu_entity.clone();
                    let menu_track = menu_track.clone();
                    div()
                        .id(("queue-more", key))
                        .absolute()
                        .right_0()
                        .size(px(32.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .invisible()
                        .cursor_pointer()
                        .group_hover(group_name, |button| button.visible())
                        .hover(|button| button.bg(rgba(0xffffff12)))
                        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                            let menu = queue_item_menu(
                                menu_entity.clone(),
                                key,
                                &menu_track,
                                menu_entity.read(cx).now_playing,
                            );
                            let _ = context_menu::show(menu, event.position, window, cx);
                            cx.stop_propagation();
                        })
                        .child(
                            svg()
                                .path(LucideIcons::EllipsisVertical)
                                .size(px(15.))
                                .text_color(rgba(0xcac8d2d8)),
                        )
                }),
        )
        .context_menu({
            let menu_entity = menu_entity.clone();
            let menu_track = menu_track.clone();
            move |_, cx| {
                queue_item_menu(
                    menu_entity.clone(),
                    key,
                    &menu_track,
                    menu_entity.read(cx).now_playing,
                )
            }
        });

    let drag_entity = entity.clone();
    row.on_drag(drag, move |drag, cursor_offset, _, cx| {
        let fullscreen = drag_entity.update(cx, |app, cx| {
            app.queue_ui.drag_key = Some(drag.key);
            app.queue_ui.cursor_offset_y = cursor_offset.y.as_f32();
            cx.notify();
            app.now_playing
        });
        cx.new(|_| QueueDragPreview {
            fullscreen,
            title: drag.title.clone(),
            artist: drag.artist.clone(),
            cover: drag.cover.clone(),
        })
    })
    .on_drag_end::<QueueDrag>({
        let entity = entity.clone();
        move |_: &DragEnd, _, _, cx| {
            entity.update(cx, |app, cx| {
                app.queue_ui.drag_key = None;
                cx.notify();
            });
        }
    })
}

fn queue_item_menu(
    entity: gpui::Entity<MusicShell>,
    key: u64,
    track: &TrackResponse,
    fullscreen: bool,
) -> ContextMenu {
    let play_next_entity = entity.clone();
    let end_entity = entity.clone();
    let remove_entity = entity.clone();
    let mut menu = ContextMenu::new()
        .action_with(
            |_, _| menu_label(LucideIcons::ListPlus, strings::play_next()),
            move |_, cx| {
                play_next_entity.update(cx, |app, cx| {
                    app.playback.update(cx, |playback, cx| {
                        playback.play_next_queue_key(key, cx);
                    });
                });
            },
        )
        .action_with(
            |_, _| menu_label(LucideIcons::ListEnd, strings::move_to_end()),
            move |_, cx| {
                end_entity.update(cx, |app, cx| {
                    app.playback.update(cx, |playback, cx| {
                        playback.move_queue_key_to_end(key, cx);
                    });
                });
            },
        )
        .action_with(
            |_, _| menu_label(LucideIcons::ListMinus, strings::remove_from_queue()),
            move |_, cx| {
                remove_entity.update(cx, |app, cx| {
                    app.playback.update(cx, |playback, cx| {
                        playback.remove_queue_key(key, cx);
                    });
                });
            },
        );
    if track.album_id.is_some() || track.artist_id.is_some() {
        menu = menu.separator();
        if let Some(album_id) = track.album_id.clone() {
            let album_entity = entity.clone();
            menu = menu.action_with(
                |_, _| menu_label(LucideIcons::DiscAlbum, "Go to Album"),
                move |_, cx| {
                    album_entity.update(cx, |app, cx| {
                        app.navigate(Page::Album(album_id.clone().into()), cx);
                    });
                },
            );
        }
        if let Some(artist_id) = track.artist_id.clone() {
            let artist_entity = entity.clone();
            menu = menu.action_with(
                |_, _| menu_label(LucideIcons::MicVocal, "Go to Artist"),
                move |_, cx| {
                    artist_entity.update(cx, |app, cx| {
                        app.navigate(Page::Artist(artist_id.clone().into()), cx);
                    });
                },
            );
        }
    }
    if fullscreen {
        theme::style_fullscreen_context_menu(menu)
    } else {
        theme::style_context_menu(menu)
    }
}

fn menu_label(icon: LucideIcons, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(svg().path(icon).size(px(15.)).text_color(rgba(0xf2f0f6f2)))
        .child(label)
}

fn format_duration(duration_ms: Option<i64>) -> String {
    let Some(duration_ms) = duration_ms.filter(|duration| *duration >= 0) else {
        return "—".to_owned();
    };
    let seconds = duration_ms / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn sync_artwork(app: &mut MusicShell, cx: &mut gpui::Context<MusicShell>) {
    let Ok(api) = library_api(cx) else {
        return;
    };
    let tracks = {
        let playback = app.playback.read(cx);
        playback
            .queue
            .items
            .iter()
            .map(|item| &item.track)
            .filter(|track| track.has_artwork)
            .map(|track| track.id.clone())
            .collect::<Vec<_>>()
    };
    for track_id in tracks {
        if app.queue_artwork.contains_key(&track_id)
            || app.queue_artwork_tasks.contains_key(&track_id)
        {
            continue;
        }
        let request_api = api.clone();
        let request_id = track_id.clone();
        let request = Tokio::spawn(
            cx,
            async move { request_api.artwork(&request_id, None).await },
        );
        let task_id = track_id.clone();
        app.queue_artwork_tasks.insert(
            track_id,
            cx.spawn(async move |this, cx| {
                let result = request.await;
                let Some(this) = this.upgrade() else {
                    return;
                };
                this.update(cx, |app, cx| {
                    app.queue_artwork_tasks.remove(&task_id);
                    if let Ok(Ok(response)) = result
                        && let Some(format) =
                            ImageFormat::from_mime_type(&response.metadata.content_type)
                    {
                        app.queue_artwork
                            .insert(task_id, Arc::new(Image::from_bytes(format, response.bytes)));
                    }
                    cx.notify();
                });
            }),
        );
    }
}
