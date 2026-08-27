use std::{path::PathBuf, sync::Arc};

use gpui::{
    Background, Div, Image, ObjectFit, Pixels, div, img, linear_color_stop, linear_gradient,
    prelude::*, px, rgba,
};

#[derive(Clone)]
pub enum CoverSource {
    Path(PathBuf),
    Image(Arc<Image>),
}

pub fn named_gradient(name: &str) -> Background {
    const PALETTES: [(u32, u32); 12] = [
        (0xf6b6c8, 0x34313f),
        (0xff8b9e, 0x6b4fd7),
        (0x8ce6ee, 0x1558a0),
        (0xf1c0a7, 0x3345a2),
        (0xb5a6ff, 0x341b75),
        (0xf0838e, 0x6b204b),
        (0x7cd8c8, 0x164a68),
        (0xf5d6a7, 0x4f416e),
        (0x9eb8ff, 0x1b2d70),
        (0xe9a9d0, 0x57336e),
        (0xb8d6a4, 0x28485a),
        (0xd5c5ff, 0x4556a8),
    ];
    let hash = name
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    let (from, to) = PALETTES[(hash as usize) % PALETTES.len()];
    let angle = 118. + ((hash >> 16) % 48) as f32;
    linear_gradient(
        angle,
        linear_color_stop(gpui::rgb(from), 0.),
        linear_color_stop(gpui::rgb(to), 1.),
    )
}

pub fn media_cover(
    name: impl Into<String>,
    source: Option<CoverSource>,
    size: Pixels,
    radius: Pixels,
    show_fallback_name: bool,
) -> Div {
    let name = name.into();
    let has_source = source.is_some();
    div()
        .relative()
        .size(size)
        .flex_none()
        .overflow_hidden()
        .rounded(radius)
        .bg(named_gradient(&name))
        .when_some(source, |cover, source| match source {
            CoverSource::Path(path) => cover.child(
                img(path)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded(radius),
            ),
            CoverSource::Image(image) => cover.child(
                img(image)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .rounded(radius),
            ),
        })
        .when(!has_source && show_fallback_name, |cover| {
            cover
                .child(div().absolute().inset_0().bg(linear_gradient(
                    180.,
                    linear_color_stop(rgba(0xffffff14), 0.),
                    linear_color_stop(rgba(0x0000002f), 1.),
                )))
                .child(
                    div()
                        .absolute()
                        .top(px(13.))
                        .left(px(14.))
                        .right(px(12.))
                        .text_size(px((size.as_f32() * 0.105).clamp(11., 22.)))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(rgba(0x11121ae8))
                        .line_height(px((size.as_f32() * 0.13).clamp(14., 27.)))
                        .child(name),
                )
        })
}

pub fn liked_playlist_cover(size: Pixels, radius: Pixels) -> Div {
    div()
        .size(size)
        .flex_none()
        .overflow_hidden()
        .rounded(radius)
        .bg(gpui::rgb(0xffffff))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .text_size(px((size.as_f32() * 0.37).clamp(34., 68.)))
                .line_height(size)
                .text_color(rgba(0xff2d55ff))
                .child("♥"),
        )
}
