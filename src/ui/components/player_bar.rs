use std::time::Duration;

use gpui::{
    AnyElement, CursorStyle, Div, FontWeight, ObjectFit, Rgba, SharedString, Stateful, div, img,
    prelude::*, px, rgb, rgba, svg,
};

use uic::{
    assets::LucideIcons,
    components::slider::{Slider, SliderAppearance, SliderThumbVisibility, SliderTooltipPlacement},
};

use crate::{
    app::music_shell::{MusicShell, PlaybackPanel},
    resources::RomusIcons,
    state::RepeatMode,
    ui::{
        theme::{self, ACCENT},
        timeline_tooltip,
    },
    utils::format_time,
};

fn mode_button(id: &'static str, icon: LucideIcons, active: bool) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .size(px(27.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .when(active, |button| button.bg(rgba(0xfa355814)))
        .hover(|style| style.bg(rgba(0xffffff0f)))
        .child(svg().path(icon).size_5().text_color(if active {
            rgb(ACCENT)
        } else {
            rgba(0xd5d5ddeb)
        }))
}

fn icon_button(id: &'static str, icon: impl Into<SharedString>, enabled: bool) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(27.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|style| style.bg(rgba(0xffffff0f)))
        .child(svg().path(icon).size_6().text_color(if enabled {
            rgba(0xd5d5ddeb)
        } else {
            rgba(0xa8a9b34d)
        }))
}

pub fn render(
    app: &mut MusicShell,
    cx: &mut gpui::Context<MusicShell>,
    compact: bool,
) -> impl IntoElement {
    let playback = app.playback.read(cx);
    let playing = playback.snapshot.is_playing();
    let timeline = playback.snapshot.timeline;
    let position = timeline.duration().map_or_else(
        || timeline.position(),
        |_| Duration::from_secs_f64(app.timeline_slider.read(cx).value()),
    );
    let elapsed = format_time(position);
    let remaining = playback
        .snapshot
        .timeline
        .duration()
        .map(|duration| format_time(duration.saturating_sub(position)))
        .unwrap_or_else(|| "--".to_owned());
    let title = playback.snapshot.track.title.clone();
    let artist = playback.snapshot.track.artist.clone();
    let error = playback.snapshot.error.clone();
    let muted = playback.snapshot.muted;
    let shuffle = playback.queue.shuffle;
    let repeat = playback.queue.repeat;
    let can_next = playback.can_skip_next();
    let can_previous = playback.can_skip_previous();
    let artwork = playback
        .snapshot
        .track
        .artwork
        .as_ref()
        .map(|artwork| artwork.image.clone());
    let _ = playback;

    theme::frosted()
        .appearance(
            theme::frosted_appearance()
                .blur_radius(px(22.))
                .tint(rgba(0x181a23c4).into()),
        )
        .relative()
        .w(px(if compact { 500. } else { 600. }))
        .px(px(10.))
        .pt_2()
        .pb_2()
        .m_10()
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(32.))
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
                .when(!compact, |this| {
                    this.child(
                        mode_button("shuffle", LucideIcons::Shuffle, shuffle).on_click(
                            cx.listener(|app, _, _, cx| {
                                app.playback
                                    .update(cx, |playback, cx| playback.toggle_shuffle(cx));
                            }),
                        ),
                    )
                })
                .child(
                    icon_button("skip-back", RomusIcons::MusicPrevious, can_previous).on_click(
                        cx.listener(|app, _, _, cx| {
                            app.playback
                                .update(cx, |playback, cx| playback.skip_previous(cx));
                        }),
                    ),
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
                            app.toggle_playback(cx);
                        }))
                        .child(
                            svg()
                                .path(if playing {
                                    RomusIcons::MusicPause
                                } else {
                                    RomusIcons::MusicPlay
                                })
                                .size_7()
                                .text_color(rgba(0xffffffff)),
                        ),
                )
                .child(
                    icon_button("skip-forward", RomusIcons::MusicNext, can_next).on_click(
                        cx.listener(|app, _, _, cx| {
                            app.playback
                                .update(cx, |playback, cx| playback.skip_next(cx));
                        }),
                    ),
                )
                .child(
                    mode_button("repeat", LucideIcons::Repeat, repeat != RepeatMode::Off)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.playback
                                .update(cx, |playback, cx| playback.cycle_repeat(cx));
                        }))
                        .when(repeat == RepeatMode::One, |button| {
                            button.child(
                                div()
                                    .absolute()
                                    .bottom(px(0.))
                                    .right(px(2.))
                                    .text_size(px(8.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(ACCENT))
                                    .child("1"),
                            )
                        }),
                ),
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
                        .h(px(16.))
                        .w_full()
                        .flex()
                        .group("player-progress-hover-zone")
                        .bottom(px(-5.))
                        .left_0()
                        .items_end()
                        .justify_center()
                        .right_0()
                        .text_color(gpui::white())
                        .child(
                            div()
                                .w_full()
                                .h_full()
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .w(px(36.))
                                        .flex_none()
                                        .text_right()
                                        .opacity(0.)
                                        .group_hover("player-progress-hover-zone", |style| {
                                            style.opacity(1.)
                                        })
                                        .text_size(px(10.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgba(0xffffffe5))
                                        .child(elapsed),
                                )
                                .child(
                                    Slider::new(&app.timeline_slider)
                                        .label("Playback position")
                                        .cursor(CursorStyle::PointingHand)
                                        .appearance(
                                            SliderAppearance::default()
                                                .active_track(rgba(0xffffffff).into())
                                                .h(px(1.))
                                                .rounded_full()
                                                .bg(rgba(0xffffff00)),
                                        )
                                        .thumb_visibility(SliderThumbVisibility::Never)
                                        .hover_track_height(px(4.))
                                        .value_tooltip(
                                            SliderTooltipPlacement::Top,
                                            |value, _, _| timeline_tooltip(value),
                                        )
                                        .h(px(16.))
                                        .mx(px(4.))
                                        .flex_1(),
                                )
                                .child(
                                    div()
                                        .w(px(36.))
                                        .flex_none()
                                        .opacity(0.)
                                        .group_hover("player-progress-hover-zone", |style| {
                                            style.opacity(1.)
                                        })
                                        .text_size(px(10.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgba(0xffffffe5))
                                        .child(remaining),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_4()
                .when(!compact, |this| {
                    this.child(
                        div()
                            .id("toggle-mini-player")
                            .size(px(27.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .cursor_pointer()
                            .hover(|button| button.bg(rgba(0xffffff0f)))
                            .on_click(cx.listener(|app, _, w, cx| {
                                app.enter_mini_player(w, cx);
                            }))
                            .child(
                                svg()
                                    .path(LucideIcons::PictureInPicture2)
                                    .size_5()
                                    .text_color(rgba(0xd5d5ddeb)),
                            ),
                    )
                })
                .child(
                    div()
                        .id("toggle-inline-lyrics")
                        .size(px(27.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .cursor_pointer()
                        .when(
                            app.playback_panel == PlaybackPanel::InlineLyrics,
                            |button| button.bg(rgba(0xfa355814)),
                        )
                        .hover(|button| button.bg(rgba(0xffffff0f)))
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.toggle_playback_panel(PlaybackPanel::InlineLyrics, cx);
                        }))
                        .child(svg().path(LucideIcons::MicVocal).size_5().text_color(
                            if app.playback_panel == PlaybackPanel::InlineLyrics {
                                rgb(ACCENT)
                            } else {
                                rgba(0xd5d5ddeb)
                            },
                        )),
                )
                .child(
                    mode_button(
                        "queue",
                        LucideIcons::ListMusic,
                        app.playback_panel == PlaybackPanel::Queue,
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.toggle_playback_panel(PlaybackPanel::Queue, cx);
                    })),
                )
                .when(!compact, |this| {
                    this.child(
                        icon_button(
                            "volume",
                            if muted {
                                LucideIcons::VolumeX
                            } else {
                                LucideIcons::Volume2
                            },
                            true,
                        )
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.playback
                                .update(cx, |playback, cx| playback.toggle_muted(cx));
                        })),
                    )
                }),
        )
        .child(super::feedback::layer(cx))
}

pub fn render_now_playing(
    app: &mut MusicShell,
    cx: &mut gpui::Context<MusicShell>,
    active_color: Rgba,
) -> AnyElement {
    let playback = app.playback.read(cx);
    let playing = playback.snapshot.is_playing();
    let shuffle = playback.queue.shuffle;
    let repeat = playback.queue.repeat;
    let can_previous = playback.can_skip_previous();
    let can_next = playback.can_skip_next();
    let timeline = playback.snapshot.timeline;
    let duration = timeline.duration();
    let position = duration.map_or_else(
        || timeline.position(),
        |_| Duration::from_secs_f64(app.timeline_slider.read(cx).value()),
    );
    let _ = playback;
    let elapsed = format_time(position);
    let remaining = duration
        .map(|duration| format!("−{}", format_time(duration.saturating_sub(position))))
        .unwrap_or_else(|| "−:--".to_owned());

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            Slider::new(&app.timeline_slider)
                .label("Playback position")
                .cursor(CursorStyle::PointingHand)
                .appearance(
                    SliderAppearance::default()
                        .active_track(rgba(0xffffffe0).into())
                        .h(px(4.))
                        .rounded_full()
                        .bg(rgba(0xffffff4c)),
                )
                .thumb_visibility(SliderThumbVisibility::Never)
                .value_tooltip(SliderTooltipPlacement::Top, |value, _, _| {
                    timeline_tooltip(value)
                })
                .h(px(16.)),
        )
        .child(
            div()
                .mt(px(-1.))
                .flex()
                .justify_between()
                .text_size(px(9.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(0xffffffe0))
                .child(elapsed)
                .child(remaining),
        )
        .child(
            div()
                .mt(px(9.))
                .h(px(66.))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    now_playing_icon_button(
                        "now-playing-shuffle",
                        LucideIcons::Shuffle,
                        23.,
                        true,
                        shuffle,
                        Some(active_color),
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.playback
                            .update(cx, |playback, cx| playback.toggle_shuffle(cx));
                    })),
                )
                .child(
                    now_playing_icon_button(
                        "now-playing-skip-back",
                        RomusIcons::MusicPrevious,
                        31.,
                        can_previous,
                        false,
                        Some(active_color),
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.playback
                            .update(cx, |playback, cx| playback.skip_previous(cx));
                    })),
                )
                .child(
                    div()
                        .id("now-playing-play-pause")
                        .size(px(60.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .cursor_pointer()
                        .bg(rgba(0xffffff14))
                        .hover(|style| style.bg(rgba(0xffffff24)))
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.toggle_playback(cx);
                        }))
                        .child(
                            svg()
                                .path(if playing {
                                    RomusIcons::MusicPause
                                } else {
                                    RomusIcons::MusicPlay
                                })
                                .size(px(36.))
                                .text_color(rgba(0xffffffff)),
                        ),
                )
                .child(
                    now_playing_icon_button(
                        "now-playing-skip-forward",
                        RomusIcons::MusicNext,
                        31.,
                        can_next,
                        false,
                        Some(active_color),
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.playback
                            .update(cx, |playback, cx| playback.skip_next(cx));
                    })),
                )
                .child(
                    now_playing_icon_button(
                        "now-playing-repeat",
                        LucideIcons::Repeat,
                        23.,
                        true,
                        repeat != RepeatMode::Off,
                        Some(active_color),
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.playback
                            .update(cx, |playback, cx| playback.cycle_repeat(cx));
                    }))
                    .when(repeat == RepeatMode::One, |button| {
                        button.child(
                            div()
                                .absolute()
                                .bottom(px(5.))
                                .right(px(7.))
                                .text_size(px(9.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(active_color)
                                .child("1"),
                        )
                    }),
                ),
        )
        .into_any_element()
}

pub fn now_playing_icon_button(
    id: &'static str,
    icon: impl Into<SharedString>,
    icon_size: f32,
    enabled: bool,
    active: bool,
    active_color: Option<Rgba>,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .size(px(50.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(rgba(0xffffff18)))
        })
        .when(!enabled, |button| button.opacity(0.38))
        .when(!active, |button| button.opacity(0.58))
        .when(active, |button| button.bg(rgba(0xffffff18)))
        .child(svg().path(icon).size(px(icon_size)).text_color(if active {
            active_color.unwrap_or(rgba(ACCENT))
        } else if enabled {
            rgba(0xffffffff)
        } else {
            rgba(0xffffff70)
        }))
}
