use std::time::Duration;

use gpui::{
    AnyElement, Div, FontWeight, ObjectFit, Stateful, div, img, prelude::*, px, relative, rgb,
    rgba, svg,
};
use gpui_media::SeekMode;
use uic::assets::LucideIcons;

use crate::{app::AppleMusicApp, theme::ACCENT};

fn small_icon_button(id: &'static str, icon: LucideIcons, enabled: bool) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(27.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xffffff0f)))
        .child(svg().path(icon).size(px(14.)).text_color(if enabled {
            rgba(0xd5d5ddeb)
        } else {
            rgba(0xa8a9b34d)
        }))
}

fn icon_button(id: &'static str, icon: LucideIcons, enabled: bool) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(27.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xffffff0f)))
        .child(svg().path(icon).size_5().text_color(if enabled {
            rgba(0xd5d5ddeb)
        } else {
            rgba(0xa8a9b34d)
        }))
}

pub fn render(
    app: &mut AppleMusicApp,
    cx: &mut gpui::Context<AppleMusicApp>,
    compact: bool,
) -> impl IntoElement {
    let playing = app.playback.is_playing();
    let progress = app
        .playback
        .timeline
        .progress()
        .unwrap_or_default()
        .clamp(0., 1.) as f32;
    let position = app.playback.timeline.position();
    let elapsed = format_time(position);
    let remaining = app
        .playback
        .timeline
        .duration()
        .map(|duration| format_time(duration.saturating_sub(position)))
        .unwrap_or_else(|| "--".to_owned());
    let title = app.playback.track.title.clone();
    let artist = app.playback.track.artist.clone();
    let error = app.playback.error.clone();
    let artwork = app
        .playback
        .track
        .artwork
        .as_ref()
        .map(|artwork| artwork.image.clone());

    div()
        .relative()
        .w(px(if compact { 500. } else { 600. }))
        .px(px(10.))
        .m_10()
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(24.))
        .bg(rgba(0x181a23c4))
        .backdrop_blur(px(22.))
        .border_1()
        .border_color(rgba(0xffffff0e))
        .shadow(vec![
            gpui::BoxShadow::new(px(0.), px(9.), rgba(0x00000051).into())
                .blur_radius(px(22.))
                .spread_radius(px(-7.)),
        ])
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(small_icon_button("shuffle", LucideIcons::Shuffle, false))
                .child(
                    icon_button("skip-back", LucideIcons::SkipBack, true).on_click(cx.listener(
                        |app, _, _, cx| {
                            let result = app.player.update(cx, |player, cx| {
                                player.skip_backward(
                                    Duration::from_secs(10),
                                    SeekMode::Accurate,
                                    cx,
                                )
                            });
                            if let Err(error) = result {
                                app.playback.error = Some(error.to_string().into());
                                cx.notify();
                            }
                        },
                    )),
                )
                .child(
                    div()
                        .id("play-pause")
                        .size(px(31.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .cursor_pointer()
                        .hover(|style| style.bg(rgba(0xffffff13)))
                        .on_click(cx.listener(|app, _, _, cx| {
                            let result = app
                                .player
                                .update(cx, |player, cx| player.toggle_playback(cx));
                            if let Err(error) = result {
                                app.playback.error = Some(error.to_string().into());
                                cx.notify();
                            }
                        }))
                        .child(
                            svg()
                                .path(if playing {
                                    LucideIcons::Pause
                                } else {
                                    LucideIcons::Play
                                })
                                .size_6()
                                .text_color(if playing {
                                    rgba(0xffffffff)
                                } else {
                                    rgba(0xb9bac35d)
                                }),
                        ),
                )
                .child(
                    icon_button("skip-forward", LucideIcons::SkipForward, true).on_click(
                        cx.listener(|app, _, _, cx| {
                            let result = app.player.update(cx, |player, cx| {
                                player.skip_forward(Duration::from_secs(10), SeekMode::Accurate, cx)
                            });
                            if let Err(error) = result {
                                app.playback.error = Some(error.to_string().into());
                                cx.notify();
                            }
                        }),
                    ),
                )
                .child(small_icon_button("repeat", LucideIcons::Repeat, false)),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .m_2()
                        .items_center()
                        .justify_start()
                        .rounded(px(12.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(11.))
                                .group_hover("player-progress-hover", |style| style.opacity(0.15))
                                .when_some(artwork, |metadata, artwork| {
                                    metadata.child(
                                        div()
                                            .m_2()
                                            .id("open-now-playing")
                                            .relative()
                                            .size(px(32.))
                                            .flex_none()
                                            .overflow_hidden()
                                            .rounded(px(9.))
                                            .group("player-cover-hover")
                                            .cursor_pointer()
                                            .border_1()
                                            .border_color(rgba(0xffffff18))
                                            .shadow(vec![
                                                gpui::BoxShadow::new(
                                                    px(0.),
                                                    px(4.),
                                                    rgba(0x00000048).into(),
                                                )
                                                .blur_radius(px(10.))
                                                .spread_radius(px(-3.)),
                                            ])
                                            .child(
                                                img(artwork)
                                                    .size_full()
                                                    .rounded(px(9.))
                                                    .object_fit(ObjectFit::Cover),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .top_0()
                                                    .right_0()
                                                    .bottom_0()
                                                    .left_0()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded(px(9.))
                                                    .opacity(0.)
                                                    .group_hover("player-cover-hover", |style| {
                                                        style.opacity(1.).bg(rgba(0x090a0fc2))
                                                    })
                                                    .child(
                                                        svg()
                                                            .path(LucideIcons::Maximize2)
                                                            .size(px(18.))
                                                            .text_color(rgba(0xffffffff)),
                                                    ),
                                            )
                                            .on_click(cx.listener(|app, _, _, cx| {
                                                app.now_playing = true;
                                                app.effects_viewer = None;
                                                cx.notify();
                                            })),
                                    )
                                })
                                .child(
                                    div()
                                        .max_w(px(if compact { 125. } else { 230. }))
                                        .flex()
                                        .flex_col()
                                        .items_start()
                                        .child(
                                            div()
                                                .w_full()
                                                .truncate()
                                                .text_size(px(12.))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgba(0xf1f1f4e8))
                                                .child(error.unwrap_or(title)),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .mt(px(2.))
                                                .truncate()
                                                .font_weight(FontWeight::BOLD)
                                                .text_size(px(9.5))
                                                .text_color(rgba(0xb7b8c288))
                                                .child(artist),
                                        ),
                                ),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .h_4()
                        .w_full()
                        .flex()
                        .group("player-progress-hover-zone")
                        .bottom_0()
                        .left_0()
                        .items_end()
                        .justify_center()
                        .right_0()
                        .text_color(gpui::white())
                        .child(
                            div()
                                .w_full()
                                .absolute()
                                .bottom_0()
                                .flex()
                                .flex_col()
                                // .h_3()
                                .cursor_pointer()
                                .child(
                                    div()
                                        .h(px(4.))
                                        .flex()
                                        .absolute()
                                        .bottom(px(6.))
                                        .items_center()
                                        .w_full()
                                        .flex()
                                        .opacity(0.)
                                        .group_hover("player-progress-hover-zone", |style| {
                                            style
                                                .opacity(1.)
                                                .bg(rgba(0x11131aad))
                                                .backdrop_blur(px(18.))
                                        })
                                        .child(
                                            div()
                                                .text_size(px(10.))
                                                .mr(px(4.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgba(0xffffffe5))
                                                .child(elapsed),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .flex()
                                                .justify_start()
                                                .items_center()
                                                .child(
                                                    div()
                                                        .bg(rgba(0xffffffff))
                                                        .w(relative(progress))
                                                        .h(px(4.))
                                                        .rounded_full(),
                                                ),
                                        )
                                        .child(
                                            div()
                                                // .w(px(10.))
                                                .text_size(px(10.))
                                                .ml(px(4.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgba(0xffffffe5))
                                                .child(remaining),
                                        ),
                                )
                                .child(
                                    div()
                                        .h(px(1.))
                                        .absolute()
                                        .bottom(px(6.))
                                        .ml_4()
                                        .mr_4()
                                        .bg(gpui::white())
                                        .group_hover("player-progress-hover-zone", |this| {
                                            this.opacity(0.0)
                                        })
                                        .w(relative(progress)),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_4()
                .child(
                    svg()
                        .path(LucideIcons::MicVocal)
                        .size_5()
                        .text_color(rgb(ACCENT)),
                )
                .child(icon_button("queue", LucideIcons::ListMusic, true))
                .child(
                    icon_button(
                        "volume",
                        if app.playback.muted {
                            LucideIcons::VolumeX
                        } else {
                            LucideIcons::Volume2
                        },
                        true,
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.player.update(cx, |player, cx| player.toggle_muted(cx));
                    })),
                ),
        )
}

pub fn render_now_playing(
    app: &mut AppleMusicApp,
    cx: &mut gpui::Context<AppleMusicApp>,
) -> AnyElement {
    let playing = app.playback.is_playing();
    let progress = app
        .playback
        .timeline
        .progress()
        .unwrap_or_default()
        .clamp(0., 1.) as f32;
    let position = app.playback.timeline.position();
    let duration = app.playback.timeline.duration();
    let elapsed = format_time(position);
    let remaining = duration
        .map(|duration| format!("−{}", format_time(duration.saturating_sub(position))))
        .unwrap_or_else(|| "−:--".to_owned());

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .relative()
                .w_full()
                .h(px(16.))
                .cursor_pointer()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(7.))
                        .h(px(4.))
                        .rounded_full()
                        .bg(rgba(0xffffff4c)),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(7.))
                        .w(relative(progress))
                        .h(px(4.))
                        .rounded_full()
                        .bg(rgba(0xffffffe0)),
                ),
        )
        .child(
            div()
                .mt(px(-1.))
                .flex()
                .justify_between()
                .text_size(px(9.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(0xffffff72))
                .child(elapsed)
                .child(remaining),
        )
        .child(
            div()
                .mt(px(5.))
                .h(px(48.))
                .flex()
                .items_center()
                .justify_between()
                .child(now_playing_icon_button(
                    "now-playing-shuffle",
                    LucideIcons::Shuffle,
                    17.,
                    false,
                ))
                .child(
                    now_playing_icon_button(
                        "now-playing-skip-back",
                        LucideIcons::SkipBack,
                        24.,
                        true,
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        let result = app.player.update(cx, |player, cx| {
                            player.skip_backward(Duration::from_secs(10), SeekMode::Accurate, cx)
                        });
                        if let Err(error) = result {
                            app.playback.error = Some(error.to_string().into());
                            cx.notify();
                        }
                    })),
                )
                .child(
                    div()
                        .id("now-playing-play-pause")
                        .size(px(46.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .cursor_pointer()
                        .hover(|style| style.bg(rgba(0xffffff12)))
                        .on_click(cx.listener(|app, _, _, cx| {
                            let result = app
                                .player
                                .update(cx, |player, cx| player.toggle_playback(cx));
                            if let Err(error) = result {
                                app.playback.error = Some(error.to_string().into());
                                cx.notify();
                            }
                        }))
                        .child(
                            svg()
                                .path(if playing {
                                    LucideIcons::Pause
                                } else {
                                    LucideIcons::Play
                                })
                                .size(px(31.))
                                .text_color(rgba(0xffffffff)),
                        ),
                )
                .child(
                    now_playing_icon_button(
                        "now-playing-skip-forward",
                        LucideIcons::SkipForward,
                        24.,
                        true,
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        let result = app.player.update(cx, |player, cx| {
                            player.skip_forward(Duration::from_secs(10), SeekMode::Accurate, cx)
                        });
                        if let Err(error) = result {
                            app.playback.error = Some(error.to_string().into());
                            cx.notify();
                        }
                    })),
                )
                .child(now_playing_icon_button(
                    "now-playing-repeat",
                    LucideIcons::Repeat,
                    17.,
                    false,
                )),
        )
        .into_any_element()
}

fn now_playing_icon_button(
    id: &'static str,
    icon: LucideIcons,
    icon_size: f32,
    enabled: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(42.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xffffff12)))
        .child(svg().path(icon).size(px(icon_size)).text_color(if enabled {
            rgba(0xffffffff)
        } else {
            rgba(0xffffffb0)
        }))
}

fn format_time(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}
