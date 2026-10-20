use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext, Bounds, Context, CursorStyle, Div, Entity,
    FontWeight, Image, MouseButton, ObjectFit, Render, ScrollHandle, SharedString, Stateful,
    Window, WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowDecorations,
    WindowKind, WindowOptions, bounds, div, img, point, prelude::*, px, relative, rgb, rgba, size,
    svg,
};
use gpui_effects::{TimedText, color_flow};
use uic::{
    assets::LucideIcons,
    components::slider::{
        Slider, SliderAppearance, SliderState, SliderThumbVisibility, SliderTooltipPlacement,
    },
};

use crate::{
    app::music_shell::{EffectsViewer, MusicShell},
    domain::lyrics::{LyricLine, LyricsMode},
    resources::RomusIcons,
    services::config::LoadedClientConfig,
    state::{PlaybackController, RepeatMode},
    ui::{
        components::{lyrics_panel, now_playing},
        strings,
        theme::layout,
        timeline_tooltip,
    },
    utils::format_time,
};

const LYRIC_TEXT_SIZE: f32 = 13.;
const LYRIC_LIFT: f32 = 3.;
const LYRIC_SLOT_HEIGHT: f32 = LYRIC_TEXT_SIZE * 1.5 + LYRIC_LIFT;

pub struct MiniPlayer {
    music: Entity<MusicShell>,
    playback: Entity<PlaybackController>,
    timeline_slider: Entity<SliderState>,
    main: AnyWindowHandle,
    lyrics_clock: lyrics_panel::LyricsPresentationClock,
    marquee: MiniLyricMarquee,

    pub effects_viewer: Option<EffectsViewer>,
}

struct MiniLyricMarquee {
    handle: ScrollHandle,
    identity: Option<(usize, i64)>,
    started_at: Instant,
}

impl Default for MiniLyricMarquee {
    fn default() -> Self {
        Self {
            handle: ScrollHandle::new(),
            identity: None,
            started_at: Instant::now(),
        }
    }
}

impl MiniPlayer {
    fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.music.update(cx, |music, _| {
            music.mini_window = None;
        });
        let _ = self.main.update(cx, |_, window, _| {
            if let Err(error) = window.set_mapped(true) {
                log::error!("unable to show the main window: {error:#}");
            }
            window.activate_window();
        });
        window.remove_window();
    }
}

impl Render for MiniPlayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = &cx.global::<LoadedClientConfig>().config;
        let playback = self.playback.read(cx);
        let track_id = playback.queue.current().as_ref().map(|t| t.id.clone());
        let playing = playback.snapshot.is_playing();
        let can_previous = playback.can_skip_previous();
        let can_next = playback.can_skip_next();
        let snapshot = playback.snapshot.clone();
        let _ = playback;

        let lyrics = &snapshot.track.lyrics;
        let document_id = Arc::as_ptr(&lyrics.lines) as *const LyricLine as usize;
        let position = self.lyrics_clock.position(document_id, &snapshot);
        if playing && window.is_window_active() {
            window.request_animation_frame();
        }

        let artwork = now_playing::playback_artwork(&snapshot);

        let (effect_artwork, background_time) = {
            let needs_refresh = self.effects_viewer.as_ref().is_none_or(|effect| {
                effect.track_id != track_id || !Arc::ptr_eq(&effect.album_cover, &artwork)
            });
            if needs_refresh {
                self.effects_viewer = Some(EffectsViewer {
                    track_id,
                    album_cover: artwork.clone(),
                    clock: Default::default(),
                });
            }
            let effect = self
                .effects_viewer
                .as_mut()
                .expect("effects viewer initialized");
            (
                effect.album_cover.clone(),
                effect
                    .clock
                    .sample(Instant::now(), playing && window.is_window_active()),
            )
        };

        let title = if snapshot.track.title.is_empty() {
            SharedString::from(strings::not_playing())
        } else {
            snapshot.track.title.clone()
        };
        let lyric = current_lyric_element(
            &snapshot,
            position,
            document_id,
            playing,
            &mut self.marquee,
            window,
        );
        let timeline = snapshot.timeline;
        let slider_position = timeline.duration().map_or_else(
            || timeline.position(),
            |_| Duration::from_secs_f64(self.timeline_slider.read(cx).value()),
        );

        let repeat = playback.queue.repeat;
        let elapsed = format_time(slider_position);
        let remaining = timeline
            .duration()
            .map(|duration| format_time(duration.saturating_sub(slider_position)))
            .unwrap_or_else(|| "--".to_owned());

        div()
            .when_some(config.fonts.font_family.clone(), |this, val| {
                this.font_family(val)
            })
            .size_full()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .left_0()
                    .overflow_hidden()
                    .child(color_flow(effect_artwork).time(background_time).size_full()),
            )
            .text_color(rgba(0xffffffff))
            .cursor(CursorStyle::OpenHand)
            .on_mouse_down(MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
            .child(
                div()
                    .size_full()
                    .rounded(px(20.))
                    .border_1()
                    .overflow_hidden()
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(self.render_cover(artwork, cx))
                    .child(self.render_meta(title, lyric, elapsed, remaining))
                    .child(self.render_transport(playing, can_previous, can_next, cx, repeat)),
            )
    }
}

impl MiniPlayer {
    fn render_cover(&self, artwork: Arc<Image>, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("mini-player-cover")
            .relative()
            .size(px(58.))
            .flex_none()
            .overflow_hidden()
            .rounded(px(12.))
            .cursor(CursorStyle::PointingHand)
            .group("mini-cover")
            .shadow(vec![
                gpui::BoxShadow::new(px(0.), px(6.), rgba(0x00000066).into()).blur_radius(px(14.)),
            ])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, window, cx| this.restore(window, cx)))
            .child(
                img(artwork)
                    .size_full()
                    .rounded(px(12.))
                    .object_fit(ObjectFit::Cover),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .opacity(0.)
                    .group_hover("mini-cover", |overlay| {
                        overlay.opacity(1.).bg(rgba(0x090a0fc2))
                    })
                    .child(
                        svg()
                            .path(LucideIcons::Maximize2)
                            .size(px(18.))
                            .text_color(rgba(0xffffffff)),
                    ),
            )
    }

    fn render_meta(
        &self,
        title: SharedString,
        lyric: AnyElement,
        elapsed: String,
        remaining: String,
    ) -> impl IntoElement {
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .pt(px(8.))
            .pb(px(4.))
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(2.))
            .child(
                div()
                    .w_full()
                    .truncate()
                    .text_size(px(11.5))
                    .font_weight(FontWeight::BOLD)
                    .window_control_area(WindowControlArea::Drag)
                    .child(title),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(LYRIC_SLOT_HEIGHT))
                    .pt(px(LYRIC_LIFT))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .window_control_area(WindowControlArea::Drag)
                    .text_size(px(LYRIC_TEXT_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgba(0xffffffd1))
                    .child(lyric),
            )
            .child(
                div()
                    .w_full()
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(time_label(elapsed, false))
                    .child(
                        Slider::new(&self.timeline_slider)
                            .label("Playback position")
                            .cursor(CursorStyle::PointingHand)
                            .appearance(
                                SliderAppearance::default()
                                    .active_track(rgba(0xffffffff).into())
                                    .h(px(1.))
                                    .rounded_full()
                                    .bg(rgba(0xffffff40)),
                            )
                            .thumb_visibility(SliderThumbVisibility::Never)
                            .hover_track_height(px(4.))
                            .value_tooltip(SliderTooltipPlacement::Top, |value, _, _| {
                                timeline_tooltip(value)
                            })
                            .h(px(16.))
                            .mx(px(4.))
                            .flex_1(),
                    )
                    .child(time_label(remaining, true)),
            )
    }

    fn render_transport(
        &self,
        playing: bool,
        can_previous: bool,
        can_next: bool,
        cx: &mut Context<Self>,
        repeat: RepeatMode,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .flex_none()
            .pr(px(4.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(transport_button(
                "mini-skip-back",
                RomusIcons::MusicPrevious,
                can_previous,
                cx.listener(|this, _, _, cx| {
                    this.playback
                        .update(cx, |playback, cx| playback.skip_previous(cx));
                }),
            ))
            .child(
                div()
                    .id("mini-play-pause")
                    .size(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor(CursorStyle::PointingHand)
                    .bg(rgba(0xffffff28))
                    .hover(|style| style.bg(rgba(0xffffff3a)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.playback
                            .update(cx, |playback, cx| playback.toggle_playback(cx));
                    }))
                    .child(
                        svg()
                            .path(if playing {
                                RomusIcons::MusicPause
                            } else {
                                RomusIcons::MusicPlay
                            })
                            .size(px(18.))
                            .text_color(rgba(0xffffffff)),
                    ),
            )
            .child(transport_button(
                "mini-skip-forward",
                RomusIcons::MusicNext,
                can_next,
                cx.listener(|this, _, _, cx| {
                    this.playback
                        .update(cx, |playback, cx| playback.skip_next(cx));
                }),
            ))
            .child(
                mini_player_icon_button(
                    "now-playing-repeat",
                    LucideIcons::Repeat,
                    17.,
                    true,
                    repeat != RepeatMode::Off,
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
                            .right(px(3.))
                            .text_size(px(9.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xffffffff))
                            .child("1"),
                    )
                }),
            )
    }
}

pub fn mini_player_icon_button(
    id: &'static str,
    icon: impl Into<SharedString>,
    icon_size: f32,
    enabled: bool,
    active: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .size(px(34.))
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
        .child(svg().path(icon).size(px(icon_size)).text_color(if enabled {
            rgba(0xffffffff)
        } else {
            rgba(0xffffff70)
        }))
}

fn transport_button(
    id: &'static str,
    icon: RomusIcons,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(34.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(enabled, |button| {
            button
                .cursor(CursorStyle::PointingHand)
                .hover(|style| style.bg(rgba(0xffffff18)))
                .on_click(on_click)
        })
        .when(!enabled, |button| button.opacity(0.38))
        .child(svg().path(icon).size(px(17.)).text_color(if enabled {
            rgba(0xffffffff)
        } else {
            rgba(0xffffff70)
        }))
}

fn current_lyric_element(
    playback: &crate::domain::PlaybackState,
    position: Duration,
    document_id: usize,
    playing: bool,
    marquee: &mut MiniLyricMarquee,
    window: &mut Window,
) -> AnyElement {
    let lyrics = &playback.track.lyrics;
    match lyrics.mode {
        LyricsMode::Synced if !lyrics.lines.is_empty() => {
            if let Some(gap) = lyrics.gap_at(position, lyrics_panel::LYRIC_GAP_MINIMUM) {
                return lyrics_panel::lyric_gap_dots(gap, position, playing, 6.);
            }
            let active = lyrics.active_index(position);
            let focus = active.unwrap_or_else(|| {
                lyrics
                    .lines
                    .partition_point(|line| line.start.is_some_and(|start| start <= position))
                    .min(lyrics.lines.len().saturating_sub(1))
            });
            let line = &lyrics.lines[focus];
            let content = if active == Some(focus) {
                karaoke_or_text(line, position).into_any_element()
            } else {
                nowrap_lyric(line.text.clone(), false).into_any_element()
            };
            marquee.wrap((document_id, focus as i64), content, window)
        }
        LyricsMode::Plain if !lyrics.lines.is_empty() => marquee.wrap(
            (document_id, 0),
            nowrap_lyric(lyrics.lines[0].text.clone(), false).into_any_element(),
            window,
        ),
        _ => marquee.wrap(
            (document_id, -1),
            nowrap_lyric(playback.track.artist.clone(), false).into_any_element(),
            window,
        ),
    }
}

fn karaoke_or_text(line: &LyricLine, position: Duration) -> AnyElement {
    if let Some(units) = lyrics_panel::timed_text_units(line) {
        TimedText::new(line.text.clone(), units)
            .position(position)
            .active_fill(rgb(0xffffff))
            .inactive_opacity(0.58)
            .flex_none()
            .whitespace_nowrap()
            .reveal_wave_width(px(8.))
            .text_size(px(LYRIC_TEXT_SIZE))
            .line_height(relative(1.3))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgba(0xffffffff))
            .into_any_element()
    } else {
        nowrap_lyric(line.text.clone(), true).into_any_element()
    }
}

fn nowrap_lyric(text: SharedString, highlighted: bool) -> Div {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(px(LYRIC_TEXT_SIZE))
        .line_height(relative(1.3))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if highlighted {
            rgba(0xffffffff)
        } else {
            rgba(0xffffffd1)
        })
        .child(text)
}

impl MiniLyricMarquee {
    fn wrap(
        &mut self,
        identity: (usize, i64),
        content: AnyElement,
        window: &mut Window,
    ) -> AnyElement {
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.started_at = Instant::now();
            self.handle.set_offset(point(px(0.), px(0.)));
        }

        let max = self.handle.max_offset().x.as_f32().max(0.);
        if max > 1. {
            window.request_animation_frame();
            self.handle
                .set_offset(point(px(-marquee_offset(self.started_at, max)), px(0.)));
        } else {
            self.handle.set_offset(point(px(0.), px(0.)));
        }

        div()
            .id("mini-lyric-marquee")
            .w_full()
            .h_full()
            .min_w_0()
            .overflow_x_scroll()
            .scrollbar_width(px(0.))
            .track_scroll(&self.handle)
            .flex()
            .items_center()
            .child(content)
            .into_any_element()
    }
}

fn marquee_offset(started_at: Instant, max: f32) -> f32 {
    let pause = 1.25;
    let travel = (max / 32.).max(1.8);
    let t = started_at.elapsed().as_secs_f32() % ((pause + travel) * 2.);
    if t < pause {
        0.
    } else if t < pause + travel {
        ((t - pause) / travel) * max
    } else if t < pause + travel + pause {
        max
    } else {
        max * (1. - ((t - pause - travel - pause) / travel).clamp(0., 1.))
    }
}

fn time_label(text: String, right_align: bool) -> Div {
    div()
        .max_w(px(32.))
        .flex_none()
        .when(right_align, |label| label.text_right())
        .text_size(px(9.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(0xffffffe5))
        .child(text)
}

fn mini_bounds(window: &Window, cx: &App) -> Bounds<gpui::Pixels> {
    let size = size(
        px(layout::MINI_PLAYER_WIDTH),
        px(layout::MINI_PLAYER_HEIGHT),
    );
    let Some(display) = window.display(cx).or_else(|| cx.primary_display()) else {
        return Bounds::centered(None, size, cx);
    };
    let screen = display.bounds();
    bounds(
        point(
            screen.origin.x + (screen.size.width - size.width) / 2.,
            screen.origin.y + screen.size.height - size.height - px(56.),
        ),
        size,
    )
}

fn mini_window_kind() -> WindowKind {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        WindowKind::PopUp
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        WindowKind::Normal
    }
}

pub fn open(
    playback: Entity<PlaybackController>,
    timeline_slider: Entity<SliderState>,
    window: &mut Window,
    cx: &mut Context<MusicShell>,
) -> anyhow::Result<AnyWindowHandle> {
    let music = cx.entity();
    let main = window.window_handle();
    let bounds = mini_bounds(window, cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: true,
            kind: mini_window_kind(),
            is_movable: true,
            app_owns_titlebar_drag: true,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: Some("romus".to_owned()),
            window_min_size: Some(size(
                px(layout::MINI_PLAYER_WIDTH),
                px(layout::MINI_PLAYER_HEIGHT),
            )),
            window_decorations: Some(WindowDecorations::Client),
            ..Default::default()
        },
        move |window, cx| {
            window.set_window_title("Romus");
            crate::window::remove_frame(window);
            cx.new(|cx| {
                cx.observe(&playback, |_, _, cx| cx.notify()).detach();
                cx.observe(&timeline_slider, |_, _, cx| cx.notify())
                    .detach();
                let music_for_close = music.downgrade();
                window.on_window_should_close(cx, move |_, cx| {
                    let music = music_for_close.clone();
                    cx.defer(move |cx| {
                        if let Some(music) = music.upgrade() {
                            music.update(cx, |music, _| music.mini_window = None);
                        }
                        let _ = main.update(cx, |_, window, _| {
                            if let Err(error) = window.set_mapped(true) {
                                log::error!("unable to show the main window: {error:#}");
                            }
                            window.activate_window();
                        });
                    });
                    true
                });
                MiniPlayer {
                    music,
                    playback,
                    timeline_slider,
                    effects_viewer: None,
                    main,
                    lyrics_clock: lyrics_panel::LyricsPresentationClock::default(),
                    marquee: MiniLyricMarquee::default(),
                }
            })
        },
    )?;
    Ok(handle.into())
}
