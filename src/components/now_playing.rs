use std::{sync::Arc, time::Instant};

use gpui::{
    AnyElement, Div, FontWeight, Image, ImageFormat, ObjectFit, Stateful, div, img, prelude::*, px,
    rgba, svg,
};
use gpui_effects::album_glow;
use uic::assets::LucideIcons;

use crate::{
    app::{AppleMusicApp, EffectsViewer},
    components::{lyrics_panel, player_bar},
};

const FALLBACK_COVER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="800" viewBox="0 0 800 800">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#ff375f"/><stop offset=".5" stop-color="#6e5bff"/><stop offset="1" stop-color="#10131d"/></linearGradient></defs>
<rect width="800" height="800" rx="120" fill="url(#g)"/><path d="M510 175v360c0 74-60 134-134 134s-134-60-134-134 60-134 134-134c28 0 54 9 75 23V232l59-57z" fill="white" fill-opacity=".9"/>
</svg>"##;

pub fn render(
    app: &mut AppleMusicApp,
    window_width: f32,
    window_height: f32,
    cx: &mut gpui::Context<AppleMusicApp>,
) -> AnyElement {
    let artwork = app
        .playback
        .track
        .artwork
        .as_ref()
        .map(|artwork| artwork.image.clone())
        .unwrap_or_else(fallback_cover);
    let (effect_artwork, background_time) = {
        let effect = app.effects_viewer.get_or_insert_with(|| EffectsViewer {
            album_cover: artwork.clone(),
            started_at: Instant::now(),
        });
        (
            effect.album_cover.clone(),
            effect.started_at.elapsed().as_secs_f32(),
        )
    };
    let title = app.playback.track.title.clone();
    let artist = app.playback.track.artist.clone();
    let subtitle = app
        .playback
        .track
        .album
        .as_ref()
        .map(|album| format!("{artist} — {album}"))
        .unwrap_or_else(|| artist.to_string());
    let show_lyrics = app.now_playing_lyrics;
    let cover_size = (window_height - 310.)
        .min(window_width * 0.30)
        .clamp(280., 430.);

    div()
        .relative()
        .size_full()
        .when(cfg!(target_os = "linux"), |this| {
            this.font_family(crate::FONT_FAMILY)
        })
        .overflow_hidden()
        .rounded(px(22.))
        .text_color(rgba(0xffffffff))
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .left_0()
                .overflow_hidden()
                .child(album_glow(effect_artwork).time(background_time).size_full()),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .left_0()
                .bg(rgba(0x080a102b)),
        )
        .child(
            div()
                .relative()
                .size_full()
                .px(px(if show_lyrics { 72. } else { 48. }))
                .pt(px(82.))
                .pb(px(62.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(92.))
                .child(
                    div()
                        .w(px(cover_size))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .size(px(cover_size))
                                .flex_none()
                                .overflow_hidden()
                                .rounded_3xl()
                                .shadow(vec![
                                    gpui::BoxShadow::new(px(0.), px(24.), rgba(0x0000005e).into())
                                        .blur_radius(px(52.))
                                        .spread_radius(px(-15.)),
                                ])
                                .child(
                                    img(artwork)
                                        .size_full()
                                        .rounded_3xl()
                                        .object_fit(ObjectFit::Cover),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .mt(px(18.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(14.))
                                                .font_weight(FontWeight::BOLD)
                                                .child(title),
                                        )
                                        .child(
                                            div()
                                                .mt(px(3.))
                                                .truncate()
                                                .text_size(px(11.5))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgba(0xffffffad))
                                                .child(subtitle),
                                        ),
                                )
                                .child(
                                    div()
                                        .ml(px(16.))
                                        .flex()
                                        .items_center()
                                        .gap(px(7.))
                                        .child(round_icon_button(
                                            "favorite-track",
                                            LucideIcons::Star,
                                            15.,
                                        ))
                                        .child(round_icon_button(
                                            "track-actions",
                                            LucideIcons::Ellipsis,
                                            17.,
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .mt(px(18.))
                                .child(player_bar::render_now_playing(app, cx)),
                        ),
                )
                .when(show_lyrics, |stage| {
                    stage.child(
                        div()
                            .h_full()
                            .flex_1()
                            .min_w(px(390.))
                            .max_w(px(650.))
                            .child(lyrics_panel::render_fullscreen(
                                &app.playback,
                                &mut app.lyrics_panel,
                            )),
                    )
                }),
        )
        .child(render_window_controls(app, cx))
        .child(render_view_switcher(app, cx))
        .into_any_element()
}

fn render_window_controls(
    _app: &mut AppleMusicApp,
    cx: &mut gpui::Context<AppleMusicApp>,
) -> AnyElement {
    div()
        .absolute()
        .top(px(18.))
        .left(px(18.))
        .right(px(18.))
        .h(px(34.))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(traffic_light(0xff5f57))
                        .child(traffic_light(0xfebc2e))
                        .child(traffic_light(0x28c840)),
                )
                .child(
                    div()
                        .h(px(32.))
                        .px(px(7.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded_full()
                        .bg(rgba(0xffffff2a))
                        .border_1()
                        .border_color(rgba(0xffffff1a))
                        .child(
                            top_icon_button("close-now-playing", LucideIcons::X).on_click(
                                cx.listener(|app, _, _, cx| {
                                    app.now_playing = false;
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(top_icon_button(
                            "picture-in-picture",
                            LucideIcons::PictureInPicture2,
                        )),
                ),
        )
        .into_any_element()
}

fn render_view_switcher(
    app: &mut AppleMusicApp,
    cx: &mut gpui::Context<AppleMusicApp>,
) -> AnyElement {
    div()
        .absolute()
        .right(px(18.))
        .bottom(px(16.))
        .h(px(36.))
        .px(px(5.))
        .flex()
        .items_center()
        .gap(px(12.))
        .rounded_full()
        .bg(rgba(0xffffff2a))
        .border_1()
        .border_color(rgba(0xffffff1a))
        .child(
            div()
                .id("toggle-now-playing-lyrics")
                .size(px(27.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .when(app.now_playing_lyrics, |button| button.bg(rgba(0xffffff24)))
                .hover(|style| style.bg(rgba(0xffffff20)))
                .on_click(cx.listener(|app, _, _, cx| {
                    app.now_playing_lyrics = !app.now_playing_lyrics;
                    cx.notify();
                }))
                .child(
                    svg()
                        .path(LucideIcons::MicVocal)
                        .size(px(20.))
                        .text_color(rgba(0xffffffff)),
                ),
        )
        .child(
            div()
                .id("now-playing-queue")
                .size(px(27.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .hover(|style| style.bg(rgba(0xffffff20)))
                .child(
                    svg()
                        .path(LucideIcons::ListMusic)
                        .size(px(20.))
                        .text_color(rgba(0xffffffe3)),
                ),
        )
        .into_any_element()
}

fn round_icon_button(id: &'static str, icon: LucideIcons, icon_size: f32) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .bg(rgba(0xffffff13))
        .hover(|style| style.bg(rgba(0xffffff22)))
        .child(
            svg()
                .path(icon)
                .size(px(icon_size))
                .text_color(rgba(0xffffffe5)),
        )
}

fn top_icon_button(id: &'static str, icon: LucideIcons) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(25.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xffffff16)))
        .child(svg().path(icon).size(px(20.)).text_color(rgba(0xffffffff)))
}

fn traffic_light(color: u32) -> Div {
    div()
        .size(px(12.))
        .rounded_full()
        .bg(rgba((color << 8) | 0xff))
}

fn fallback_cover() -> Arc<Image> {
    Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        FALLBACK_COVER.as_bytes().to_vec(),
    ))
}
