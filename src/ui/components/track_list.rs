use std::{collections::HashMap, rc::Rc, sync::Arc};

use gpui::{App, Div, FontWeight, Image, MouseButton, Window, div, prelude::*, px, rgba, svg};
use uic::{
    assets::LucideIcons,
    components::context_menu::{self, ContextMenu, ContextMenuExt},
};

use crate::{
    services::api::TrackResponse,
    ui::components::media_cover::{self, CoverSource},
};

pub type TrackMenuBuilder =
    Rc<dyn Fn(TrackResponse, &mut Window, &mut App) -> ContextMenu + 'static>;

pub type TrackPlayHandler = Rc<dyn Fn(TrackResponse, &mut Window, &mut App) + 'static>;

pub fn track_list(
    tracks: &[TrackResponse],
    artwork: &HashMap<String, Arc<Image>>,
    menu_builder: TrackMenuBuilder,
    on_play: TrackPlayHandler,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .child(track_list_header())
        .children(tracks.iter().enumerate().map(|(index, track)| {
            track_row(
                index,
                track,
                artwork.get(&track.id).cloned().map(CoverSource::Image),
                menu_builder.clone(),
                on_play.clone(),
            )
        }))
}

fn track_list_header() -> Div {
    div()
        .h(px(34.))
        .px(px(10.))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgba(0xffffff12))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(0xb5b3c0a8))
        .child(div().min_w(px(250.)).flex_1().child("Song"))
        .child(column("Artist", 190.))
        .child(column("Album", 210.))
        .child(
            div()
                .w(px(68.))
                .text_align(gpui::TextAlign::Right)
                .child("Time"),
        )
        .child(div().w(px(32.)))
}

fn track_row(
    index: usize,
    track: &TrackResponse,
    cover: Option<CoverSource>,
    menu_builder: TrackMenuBuilder,
    on_play: TrackPlayHandler,
) -> impl IntoElement {
    let title = track.title.as_deref().unwrap_or("Untitled").to_owned();
    let artist = track
        .artist
        .as_deref()
        .unwrap_or("Unknown Artist")
        .to_owned();
    let album = track.album.as_deref().unwrap_or("—").to_owned();
    let duration = format_duration(track.duration_ms);
    let row_track = track.clone();
    let row_menu_builder = menu_builder.clone();
    let more_track = track.clone();
    let more_menu_builder = menu_builder;
    let play_track = track.clone();

    div()
        .id(("track-list-row", index))
        .h(px(56.))
        .px(px(10.))
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgba(0xffffff0b))
        .cursor_pointer()
        .hover(|row| row.bg(rgba(0xffffff0a)))
        .on_click(move |_, window, cx| on_play(play_track.clone(), window, cx))
        .child(
            div()
                .min_w(px(250.))
                .min_w_0()
                .flex_1()
                .flex()
                .items_center()
                .gap(px(11.))
                .child(media_cover::media_cover(
                    &title,
                    cover,
                    px(38.),
                    px(6.),
                    false,
                ))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(0xeeeef4f0))
                        .child(title),
                ),
        )
        .child(value_column(artist, 190.))
        .child(value_column(album, 210.))
        .child(
            div()
                .w(px(68.))
                .text_align(gpui::TextAlign::Right)
                .text_size(px(13.))
                .text_color(rgba(0xb7b5c0a8))
                .child(duration),
        )
        .child(
            div()
                .id(("track-more-button", index))
                .size(px(30.))
                .flex_none()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|button| button.bg(rgba(0xffffff0e)))
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    let menu = more_menu_builder(more_track.clone(), window, cx);
                    let _ = context_menu::show(menu, event.position, window, cx);
                    cx.stop_propagation();
                })
                .child(
                    svg()
                        .path(LucideIcons::Ellipsis)
                        .size(px(14.))
                        .text_color(rgba(0xb7b5c08f)),
                ),
        )
        .context_menu(move |window, cx| row_menu_builder(row_track.clone(), window, cx))
}

fn column(label: &'static str, width: f32) -> Div {
    div().w(px(width)).px(px(12.)).child(label)
}

fn value_column(value: String, width: f32) -> Div {
    div()
        .w(px(width))
        .min_w_0()
        .px(px(12.))
        .truncate()
        .text_size(px(13.))
        .text_color(rgba(0xc4c2ccb0))
        .child(value)
}

fn format_duration(duration_ms: Option<i64>) -> String {
    let Some(duration_ms) = duration_ms.filter(|duration| *duration >= 0) else {
        return "—".to_owned();
    };
    let seconds = duration_ms / 1_000;
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    if minutes >= 60 {
        format!("{}:{:02}:{:02}", minutes / 60, minutes % 60, seconds)
    } else {
        format!("{minutes}:{seconds:02}")
    }
}
