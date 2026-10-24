use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt, AnyElement, Entity, FontWeight, Image, ImageFormat, ScrollHandle,
    SharedString, div, point, prelude::*, px, relative, rgb, rgba,
};
use gpui_effects::{
    BloomOptions, ColorFlowOptions, TextBlur, TimedText, TimedTextUnit, color_flow, subtree_bloom,
};

mod motion;
use motion::{DelayedSpring, Spring, row_delay};

use crate::{
    domain::PlaybackState,
    domain::lyrics::{LyricGap, LyricLine, LyricsMode},
    state::PlaybackController,
};

#[derive(Default)]
pub struct LyricsPanelState {
    inline: LyricsScrollState,
    fullscreen: LyricsScrollState,
    clock: LyricsPresentationClock,
    glow_started_at: Option<Instant>,
    mobile_layout: Option<MobileLyricsLayout>,
}

impl LyricsPanelState {
    pub(crate) fn mobile_scroll_line(&self) -> Option<usize> {
        let layout = self.mobile_layout.as_ref()?;
        if layout.rows.is_empty() || layout.document_id != self.fullscreen.document_id {
            return None;
        }
        let row = self.fullscreen.handle.top_item().saturating_sub(1);
        let line = row.saturating_sub(usize::from(
            self.fullscreen.gap_after.is_some_and(|index| row > index),
        ));
        Some(line.min(layout.rows.len() - 1))
    }

    pub(crate) fn resume_mobile_following(&mut self) {
        self.fullscreen.last_user_scroll.set(None);
        self.fullscreen.stagger_enabled = false;
    }

    pub(crate) fn is_animating(&self, playback: &PlaybackState, fullscreen: bool) -> bool {
        let state = if fullscreen {
            &self.fullscreen
        } else {
            &self.inline
        };
        let position = Duration::from_secs_f64(self.clock.presented_seconds.max(0.));
        state.animation.is_some()
            || (playback.is_playing()
                && playback
                    .track
                    .lyrics
                    .active_index(position)
                    .is_some_and(|index| {
                        playback.track.lyrics.lines[index]
                            .words
                            .iter()
                            .any(|word| position >= word.start && position < word.end)
                    }))
            || state.row_motion.iter().any(|row| !row.is_settled())
            || state.row_visuals.iter().any(|visual| {
                [
                    visual.emphasis,
                    visual.activation,
                    visual.alpha,
                    visual.resting_alpha,
                    visual.blur,
                ]
                .iter()
                .any(|spring| spring.velocity.abs() >= 0.001)
            })
    }
}

pub type LyricsSeek = Rc<dyn Fn(Duration, &mut gpui::App)>;

struct MobileLyricsLayout {
    document_id: usize,
    width: f32,
    show_translation: bool,
    rows: Rc<Vec<MobileLyricRow>>,
}

struct MobileLyricRow {
    primary_height: f32,
    height: f32,
    primary: Vec<LyricLine>,
    translations: Vec<SharedString>,
}

#[derive(Default)]
pub(crate) struct LyricsPresentationClock {
    document_id: usize,
    observed_position: Duration,
    presented_seconds: f64,
    last_frame: Option<Instant>,
    initialized: bool,
}

impl LyricsPresentationClock {
    pub(crate) fn position(&mut self, document_id: usize, playback: &PlaybackState) -> Duration {
        let now = Instant::now();
        let snapshot = playback.timeline.position();
        let snapshot_seconds = snapshot.as_secs_f64();
        let changed_document = self.document_id != document_id;
        let playing = playback.is_playing();

        if changed_document || !self.initialized {
            self.document_id = document_id;
            self.observed_position = snapshot;
            self.presented_seconds = snapshot_seconds;
            self.last_frame = Some(now);
            self.initialized = true;
            return snapshot;
        }

        if playing {
            if let Some(previous_frame) = self.last_frame {
                self.presented_seconds +=
                    now.saturating_duration_since(previous_frame).as_secs_f64();
            }
            if snapshot != self.observed_position {
                let correction = snapshot_seconds - self.presented_seconds;
                if correction.abs() > 0.35 {
                    self.presented_seconds = snapshot_seconds;
                } else {
                    self.presented_seconds += correction * 0.2;
                }
                self.observed_position = snapshot;
            }
        } else {
            self.presented_seconds = snapshot_seconds;
            self.observed_position = snapshot;
        }
        self.last_frame = Some(now);

        if let Some(duration) = playback.timeline.duration() {
            self.presented_seconds = self.presented_seconds.min(duration.as_secs_f64());
        }
        Duration::from_secs_f64(self.presented_seconds.max(0.))
    }
}

struct LyricsScrollState {
    document_id: usize,
    gap_after: Option<usize>,
    previous_focus: Option<usize>,
    handle: ScrollHandle,
    last_user_scroll: Rc<Cell<Option<Instant>>>,
    animation: Option<ScrollAnimation>,
    last_target: Option<f32>,
    initialized: bool,
    row_visuals: Vec<LyricVisual>,
    last_visual_frame: Option<Instant>,
    row_motion: Vec<DelayedSpring>,
    stagger_enabled: bool,
}

impl Default for LyricsScrollState {
    fn default() -> Self {
        Self {
            document_id: 0,
            gap_after: None,
            previous_focus: None,
            handle: ScrollHandle::new(),
            last_user_scroll: Rc::new(Cell::new(None)),
            animation: None,
            last_target: None,
            initialized: false,
            row_visuals: Vec::new(),
            last_visual_frame: None,
            row_motion: Vec::new(),
            stagger_enabled: false,
        }
    }
}

impl LyricsScrollState {
    fn reset_for_document(&mut self, document_id: usize) {
        if self.document_id == document_id {
            return;
        }
        self.document_id = document_id;
        self.gap_after = None;
        self.previous_focus = None;
        self.handle = ScrollHandle::new();
        self.last_user_scroll.set(None);
        self.animation = None;
        self.last_target = None;
        self.initialized = false;
        self.row_visuals.clear();
        self.last_visual_frame = None;
        self.row_motion.clear();
        self.stagger_enabled = false;
    }
}

#[derive(Clone, Copy)]
struct ScrollAnimation {
    spring: Spring,
    sampled_at: Instant,
}

#[derive(Clone, Copy)]
struct LyricVisual {
    emphasis: Spring,
    activation: Spring,
    alpha: Spring,
    resting_alpha: Spring,
    blur: Spring,
}

struct LyricRowContext {
    focus: usize,
    active: Option<usize>,
    mode: LyricsMode,
    position: Duration,
    large: bool,
    manually_scrolling: bool,
    seek: LyricsSeek,
    mobile_rows: Option<Rc<Vec<MobileLyricRow>>>,
    scale: f32,
    interaction: Rc<Cell<Option<Instant>>>,
}

pub fn render(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    controller: Entity<PlaybackController>,
    width: f32,
) -> AnyElement {
    let seek: LyricsSeek = Rc::new(move |start, cx| {
        controller.update(cx, |player, cx| player.seek_to(start, cx));
    });
    render_panel(playback, state, seek, false, width, None)
}

pub fn render_fullscreen(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    controller: Entity<PlaybackController>,
) -> AnyElement {
    let seek: LyricsSeek = Rc::new(move |start, cx| {
        controller.update(cx, |player, cx| player.seek_to(start, cx));
    });
    render_panel(playback, state, seek, true, 0., None)
}

pub fn render_mobile(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    seek: LyricsSeek,
    width: f32,
    show_translation: bool,
    window: &gpui::Window,
) -> AnyElement {
    let document_id = Arc::as_ptr(&playback.track.lyrics.lines) as *const LyricLine as usize;
    if state.mobile_layout.as_ref().is_none_or(|layout| {
        layout.document_id != document_id
            || layout.width != width
            || layout.show_translation != show_translation
    }) {
        let scale = width / 392.;
        // TextBlur and TimedText paint one shaped line. Split only their visual
        // fragments; the document, seek target and spring remain one lyric row.
        let wrap = |text: SharedString, size: f32, weight: FontWeight| {
            let mut font = gpui::font("Noto Sans CJK SC");
            font.weight = weight;
            let run = gpui::TextRun {
                len: text.len(),
                font,
                ..Default::default()
            };
            let mut ranges = Vec::new();
            if let Ok(lines) = window.text_system().shape_text(
                text.clone(),
                px(size * scale),
                &[run],
                Some(px(width - 64. * scale)),
                None,
            ) {
                let mut base = 0;
                for line in lines {
                    let mut start = 0;
                    for boundary in &line.wrap_boundaries {
                        let end = line.unwrapped_layout.runs[boundary.run_ix].glyphs
                            [boundary.glyph_ix]
                            .index;
                        if end > start {
                            ranges.push(base + start..base + end);
                            start = end;
                        }
                    }
                    ranges.push(base + start..base + line.text.len());
                    base += line.text.len() + 1;
                }
            }
            if ranges.is_empty() {
                ranges.push(0..text.len());
            }
            ranges
        };
        let rows = playback
            .track
            .lyrics
            .lines
            .iter()
            .map(|line| {
                let primary = wrap(line.text.clone(), 28., FontWeight::BOLD)
                    .into_iter()
                    .map(|range| slice_lyric_line(line, range))
                    .collect::<Vec<_>>();
                let translations = line
                    .translation
                    .as_ref()
                    .filter(|text| show_translation && !text.trim().is_empty())
                    .map_or_else(Vec::new, |text| {
                        wrap(text.clone(), 13.5, FontWeight::MEDIUM)
                            .into_iter()
                            .map(|range| SharedString::from(text[range].to_owned()))
                            .collect()
                    });
                let primary_height = primary.len() as f32 * 28. * scale * 1.3;
                let translation_height = if translations.is_empty() {
                    0.
                } else {
                    translations.len() as f32 * 13.5 * scale * 1.25 + 5. * scale
                };
                MobileLyricRow {
                    primary_height,
                    height: (primary_height + translation_height + 16. * scale).max(64. * scale),
                    primary,
                    translations,
                }
            })
            .collect();
        state.mobile_layout = Some(MobileLyricsLayout {
            document_id,
            width,
            show_translation,
            rows: Rc::new(rows),
        });
        window.request_animation_frame();
    }
    let rows = state.mobile_layout.as_ref().unwrap().rows.clone();
    render_panel(playback, state, seek, true, width, Some(rows))
}

fn render_panel(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    seek: LyricsSeek,
    fullscreen: bool,
    width: f32,
    mobile_rows: Option<Rc<Vec<MobileLyricRow>>>,
) -> AnyElement {
    let mobile = mobile_rows.is_some();
    let scale = if mobile { width / 392. } else { 1. };
    let lyrics = &playback.track.lyrics;
    let document_id = Arc::as_ptr(&lyrics.lines) as *const LyricLine as usize;
    let position = state.clock.position(document_id, playback);
    let active = lyrics.active_index(position);
    let focus = active.unwrap_or_else(|| {
        lyrics
            .lines
            .partition_point(|line| line.start.is_some_and(|start| start <= position))
            .min(lyrics.lines.len().saturating_sub(1))
    });
    let gap = lyrics.gap_at(position, LYRIC_GAP_MINIMUM);
    if fullscreen {
        state.fullscreen.reset_for_document(document_id);
    } else {
        state.inline.reset_for_document(document_id);
    }
    let scroll_state = if fullscreen {
        &mut state.fullscreen
    } else {
        &mut state.inline
    };
    scroll_state.gap_after = gap.map(|gap| gap.previous);
    let mut rows = Vec::with_capacity(lyrics.lines.len() + usize::from(gap.is_some()) + 2);
    rows.push(
        div()
            .w_full()
            .h(px(if mobile {
                100. * scale
            } else if fullscreen {
                300.
            } else {
                160.
            }))
            .flex_none()
            .into_any_element(),
    );
    let focus_child = 1 + focus + usize::from(gap.is_some_and(|gap| gap.previous < focus));
    let interaction = scroll_state.last_user_scroll.clone();
    let manually_scrolling = interaction.get().is_some_and(|last| {
        Instant::now().saturating_duration_since(last) < MANUAL_SCROLL_BLUR_PAUSE
    });
    let row_context = LyricRowContext {
        focus,
        active,
        mode: lyrics.mode,
        position,
        large: fullscreen,
        manually_scrolling,
        seek,
        mobile_rows,
        scale,
        interaction,
    };
    let now = Instant::now();
    let delta = scroll_state
        .last_visual_frame
        .replace(now)
        .map_or(0., |last| now.saturating_duration_since(last).as_secs_f32());
    let previous_focus = scroll_state.previous_focus;
    let focus_changed = previous_focus.is_some_and(|previous| previous != focus);
    let user_scrolling = scroll_state.last_user_scroll.get().is_some();
    let previous_offset = scroll_state.handle.offset().y.as_f32();
    let previous_child = mobile.then(|| {
        if focus == 0 {
            0
        } else {
            let previous = focus - 1;
            1 + previous + usize::from(gap.is_some_and(|gap| gap.previous < previous))
        }
    });
    update_scroll_position(scroll_state, focus, focus_child, previous_child);
    let scroll_offset = scroll_state.handle.offset().y.as_f32();
    if focus_changed {
        scroll_state.stagger_enabled = previous_focus.is_some_and(|previous| previous + 1 == focus);
    }
    if user_scrolling || lyrics.mode != LyricsMode::Synced || delta > 0.25 {
        // Direct manipulation, seeking across lines and resuming a hidden panel
        // use the common scroll position, without leaving trailing rows behind.
        scroll_state.stagger_enabled = false;
    }
    for index in 0..lyrics.lines.len() {
        if index >= scroll_state.row_motion.len() {
            scroll_state
                .row_motion
                .push(DelayedSpring::new(previous_offset));
        }
        let motion = &mut scroll_state.row_motion[index];
        let row_offset = if scroll_state.stagger_enabled {
            motion.advance(delta);
            motion.retarget(
                scroll_state.last_target.unwrap_or(scroll_offset),
                focus_changed.then(|| row_delay(index, focus)),
            );
            motion.spring.value - scroll_offset
        } else {
            *motion = DelayedSpring::new(scroll_offset);
            0.
        };
        let edge_opacity = if mobile {
            1.
        } else {
            lyric_edge_opacity(&scroll_state.handle, rows.len(), row_offset)
        };
        let target = lyric_visual_target(index, &row_context);
        if index >= scroll_state.row_visuals.len() {
            scroll_state.row_visuals.push(target);
        }
        let visual = &mut scroll_state.row_visuals[index];
        visual.emphasis.advance(target.emphasis.value, delta);
        visual.activation.advance(target.activation.value, delta);
        visual.alpha.advance(target.alpha.value, delta);
        visual
            .resting_alpha
            .advance(target.resting_alpha.value, delta);
        visual.blur.advance(target.blur.value, delta);
        rows.push(lyric_row(
            &lyrics.lines[index],
            index,
            edge_opacity,
            &row_context,
            *visual,
            row_offset,
        ));
        if let Some(gap) = gap
            && gap.previous == index
        {
            let edge_opacity = if mobile {
                1.
            } else {
                lyric_edge_opacity(&scroll_state.handle, rows.len(), 0.)
            };
            rows.push(lyric_gap_row(
                gap,
                position,
                fullscreen,
                playback.is_playing(),
                edge_opacity,
            ));
        }
    }
    rows.push(
        div()
            .w_full()
            .h(if mobile {
                scroll_state.handle.bounds().size.height
            } else {
                px(if fullscreen { 300. } else { 370. })
            })
            .flex_none()
            .into_any_element(),
    );
    let glow_time = if fullscreen {
        0.
    } else {
        state
            .glow_started_at
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f32()
    };
    let scroll_handle = scroll_state.handle.clone();
    let scroll_interaction = scroll_state.last_user_scroll.clone();
    let touch_interaction = scroll_state.last_user_scroll.clone();
    let rows = div()
        .id(if fullscreen {
            "fullscreen-lyrics-scroll"
        } else {
            "inline-lyrics-scroll"
        })
        .w_full()
        .when(fullscreen && !mobile, |lyrics| lyrics.h(relative(0.98)))
        .when(mobile, |lyrics| lyrics.h_full().min_h_0())
        .when(!fullscreen, |lyrics| lyrics.h_full().min_h_0())
        .px(px(if mobile {
            0.
        } else if fullscreen {
            30.
        } else {
            22.
        }))
        .flex()
        .flex_col()
        .gap(px(if fullscreen { 10. } else { 8. }))
        .overflow_y_scroll()
        .track_scroll(&scroll_handle)
        .on_scroll_wheel(move |_, _, _| {
            scroll_interaction.set(Some(Instant::now()));
        })
        .when(mobile, |rows| {
            rows.on_mouse_down(gpui::MouseButton::Left, move |_, _, _| {
                touch_interaction.set(Some(Instant::now()))
            })
        })
        .children(rows)
        .into_any_element();

    let options = ColorFlowOptions::default()
        .brightness(0.6)
        .diffusion(0.22)
        .saturation(0.9)
        .motion(1.0)
        .dither(1.0);
    div()
        .relative()
        .h_full()
        .overflow_hidden()
        .flex_none()
        .flex()
        .flex_col()
        .when(fullscreen, |panel| panel.w_full())
        .when(!fullscreen, |panel| {
            panel
                .w(px(width))
                .bg(rgb(0x2c2d32))
                .border_l_1()
                .border_color(rgba(0xffffff10))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .left_0()
                        .child(
                            color_flow(gray_flow_image())
                                .options(options)
                                // .uniform(0, [0.34, 0.12, 0.58, 0.38])
                                // .uniform(1, [0.72, 0.46, 0.16, 0.41])
                                // .uniform(2, [0.06, 0.0, 0.0, 0.0])
                                // .bg(rgb(0x2c2d32))
                                // .effect_opacity(0.38)
                                .time(glow_time)
                                .size_full(),
                        ),
                )
        })
        .when(!fullscreen, |panel| {
            panel.child(
                div()
                    .absolute()
                    .top(px(28.))
                    .left(px(28.))
                    .right(px(28.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(0xffffffe0))
                    .child("LYRICS"),
            )
        })
        .child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .when(lyrics.mode == LyricsMode::Empty, |panel| {
                    panel.child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .px(px(24.))
                            .text_center()
                            .text_size(px(16.))
                            .text_color(rgba(0xffffffe0))
                            .child("NO LYRICS"),
                    )
                })
                .when(lyrics.mode != LyricsMode::Empty, |panel| {
                    panel
                        .when(!fullscreen, |panel| panel.pt(px(64.)).pb(px(12.)))
                        .items_center()
                        .justify_center()
                        .child(rows)
                }),
        )
        .into_any_element()
}

fn update_scroll_position(
    state: &mut LyricsScrollState,
    focus: usize,
    focus_child: usize,
    previous_child: Option<usize>,
) {
    let now = Instant::now();
    if state
        .last_user_scroll
        .get()
        .is_some_and(|last| now.saturating_duration_since(last) < Duration::from_secs(4))
    {
        state.animation = None;
        state.previous_focus = Some(focus);
        return;
    }

    let Some(target) = lyric_scroll_target(&state.handle, focus_child, previous_child) else {
        return;
    };
    let returning_from_user_scroll = state.last_user_scroll.take().is_some();
    let focus_changed = state
        .previous_focus
        .is_some_and(|previous| previous != focus);
    let target_changed = state
        .last_target
        .is_some_and(|previous| (previous - target).abs() > 0.5);
    if !state.initialized {
        state
            .handle
            .set_offset(point(state.handle.offset().x, px(target)));
        state.initialized = true;
    } else if returning_from_user_scroll || focus_changed || target_changed {
        let from = state.handle.offset().y.as_f32();
        if (from - target).abs() > 0.5 {
            // Retarget the existing spring without discarding its velocity.
            state.animation.get_or_insert(ScrollAnimation {
                spring: Spring::new(from),
                sampled_at: now,
            });
        }
    }
    state.last_target = Some(target);

    if let Some(animation) = state.animation.as_mut() {
        let delta = now
            .saturating_duration_since(animation.sampled_at)
            .as_secs_f32();
        animation.sampled_at = now;
        animation.spring.advance(target, delta);
        let max_offset = state.handle.max_offset().y.as_f32().max(0.);
        let value = animation.spring.value.clamp(-max_offset, 0.);
        if value != animation.spring.value {
            animation.spring = Spring::new(value);
        }
        let settled = (value - target).abs() < 0.1 && animation.spring.velocity.abs() < 0.5;
        state.handle.set_offset(point(
            state.handle.offset().x,
            px(if settled { target } else { value }),
        ));
        if settled {
            state.animation = None;
        }
    }
    state.previous_focus = Some(focus);
}

fn lyric_scroll_target(
    handle: &ScrollHandle,
    item: usize,
    previous_child: Option<usize>,
) -> Option<f32> {
    let viewport = handle.bounds();
    if viewport.size.height <= px(0.) {
        return None;
    }
    let target = if let Some(previous_child) = previous_child {
        viewport.top() - handle.bounds_for_item(previous_child)?.top()
    } else {
        viewport.center().y - handle.bounds_for_item(item)?.center().y
    };
    let max_offset = handle.max_offset().y.as_f32().max(0.);
    Some(target.as_f32().clamp(-max_offset, 0.))
}

fn lyric_edge_opacity(handle: &ScrollHandle, item: usize, row_offset: f32) -> f32 {
    let viewport = handle.bounds();
    if viewport.size.height <= px(0.) {
        return 1.;
    }
    let Some(row) = handle.bounds_for_item(item) else {
        return 1.;
    };
    let center = row.center().y + handle.offset().y + px(row_offset);
    let distance = (center - viewport.top())
        .as_f32()
        .min((viewport.bottom() - center).as_f32());
    smootherstep(distance / (viewport.size.height.as_f32() * 0.14))
}

pub(crate) const LYRIC_GAP_MINIMUM: Duration = Duration::from_secs(10);
const MANUAL_SCROLL_BLUR_PAUSE: Duration = Duration::from_millis(220);
const GAP_COUNTDOWN_SECONDS: f32 = 6.;
const GAP_DOT_INTERVAL: f32 = 2.;
const GAP_DOT_LIGHT_FADE: f32 = 0.18;
const GAP_DOTS_EXIT_FADE: f32 = 0.30;

fn lyric_gap_row(
    gap: LyricGap,
    position: Duration,
    large: bool,
    animate: bool,
    edge_opacity: f32,
) -> AnyElement {
    let elapsed = position.saturating_sub(gap.start).as_secs_f32();
    let remaining = gap.end.saturating_sub(position).as_secs_f32();
    let entrance = smootherstep(elapsed / 0.28);
    let visibility = entrance * smootherstep(remaining / GAP_DOTS_EXIT_FADE);
    let row_height = if large { 42. } else { 32. };
    let dots = lyric_gap_dots(gap, position, animate, if large { 8. } else { 6. });

    div()
        .w_full()
        .h(px(row_height * visibility))
        .flex_none()
        .overflow_hidden()
        .opacity(entrance * edge_opacity)
        .px(px(if large { 26. } else { 18. }))
        .flex()
        .items_center()
        .child(dots)
        .into_any_element()
}

pub(crate) fn lyric_gap_dots(
    gap: LyricGap,
    position: Duration,
    animate: bool,
    gap_px: f32,
) -> AnyElement {
    let remaining = gap.end.saturating_sub(position).as_secs_f32();
    let dots = div()
        .h(px(14.))
        .flex()
        .items_center()
        .gap(px(gap_px))
        .opacity(smootherstep(remaining / GAP_DOTS_EXIT_FADE));
    if remaining <= GAP_COUNTDOWN_SECONDS {
        let elapsed = GAP_COUNTDOWN_SECONDS - remaining;
        dots.children((0..3).map(|index| {
            // Fixed geometry during the countdown; seeking and pausing follow playback time.
            let lit =
                smootherstep((elapsed - index as f32 * GAP_DOT_INTERVAL) / GAP_DOT_LIGHT_FADE);
            lyric_gap_dot_visual(5.8, 0.26 + lit * 0.74)
        }))
        .into_any_element()
    } else if animate {
        dots.with_animation(
            SharedString::from(format!("lyrics-gap-dots-{}-{}", gap.previous, gap.next)),
            Animation::new(Duration::from_millis(1_080)).repeat(),
            |dots, phase| dots.children((0..3).map(|index| lyric_gap_dot(phase, index))),
        )
        .into_any_element()
    } else {
        dots.children((0..3).map(|index| lyric_gap_dot(0.18, index)))
            .into_any_element()
    }
}

fn lyric_gap_dot(phase: f32, index: usize) -> AnyElement {
    let angle = phase * std::f32::consts::TAU - index as f32 * 0.92;
    let pulse = angle.sin().max(0.).powi(2);
    let size = 4.5 + pulse * 1.8;
    lyric_gap_dot_visual(size, 0.26 + pulse * 0.74)
}

fn lyric_gap_dot_visual(size: f32, opacity: f32) -> AnyElement {
    div()
        .size(px(9.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(size))
                .rounded_full()
                .bg(rgb(0xffffff))
                .opacity(opacity),
        )
        .into_any_element()
}

fn smootherstep(value: f32) -> f32 {
    let value = value.clamp(0., 1.);
    value * value * value * (value * (value * 6. - 15.) + 10.)
}

fn lyric_blur_radius(distance: usize, large: bool) -> f32 {
    let step = if large { 1.4 } else { 0.9 };
    let maximum = if large { 5.6 } else { 3.6 };
    (distance as f32 * step).min(maximum)
}

fn lyric_visual_target(index: usize, context: &LyricRowContext) -> LyricVisual {
    let distance = index.abs_diff(context.focus);
    let is_active = context.mode == LyricsMode::Synced && context.active == Some(index);
    let blur_radius =
        (!context.manually_scrolling && context.mode == LyricsMode::Synced && !is_active)
            .then(|| lyric_blur_radius(distance.max(1), context.large) * context.scale);
    let resting_alpha: u32 = if context.mode == LyricsMode::Plain {
        0xf0
    } else {
        // Scrolling to the upcoming line during a gap must not highlight it.
        match distance {
            0 | 1 => 0x80,
            2 => 0x66,
            3 => 0x52,
            _ => 0x40,
        }
    };
    let text_alpha = if is_active { 0xff } else { resting_alpha };
    LyricVisual {
        // Focus controls typography; playback activation alone controls color
        // and word highlighting, including the wait before an upcoming line.
        emphasis: Spring::new(
            if context.mode == LyricsMode::Synced && index == context.focus {
                1.
            } else {
                0.
            },
        ),
        activation: Spring::new(if is_active { 1. } else { 0. }),
        alpha: Spring::new(text_alpha as f32 / 255.),
        resting_alpha: Spring::new(resting_alpha as f32 / 255.),
        blur: Spring::new(blur_radius.unwrap_or(0.)),
    }
}

fn lyric_row(
    line: &LyricLine,
    index: usize,
    edge_opacity: f32,
    context: &LyricRowContext,
    visual: LyricVisual,
    row_offset: f32,
) -> AnyElement {
    if edge_opacity <= 0. && context.mode == LyricsMode::Synced {
        if let Some(rows) = &context.mobile_rows {
            return div()
                .id(("lyric-row", index))
                .w_full()
                .h(px(rows[index].height))
                .min_h(px(64.))
                .flex_none()
                .into_any_element();
        }
    }
    let emphasis = visual.emphasis.value.clamp(0., 1.);
    let activation = visual.activation.value.clamp(0., 1.);
    let text_alpha = (visual.alpha.value.clamp(0., 1.) * 255.).round() as u32;
    let text_color = rgba(0xffffff00 | text_alpha);
    let translation_color = rgba(0xffffff00 | text_alpha.saturating_sub(0x10));
    let blur_radius = visual.blur.value.max(0.);
    let font_weight = FontWeight::MEDIUM;
    let normal_text_size = if context.large { 24. } else { 16. } * context.scale;
    let active_text_size = if context.large { 28. } else { 19. } * context.scale;
    let text_size = normal_text_size
        + (active_text_size - normal_text_size) * visual.emphasis.value.clamp(0., 1.03);
    let mobile_scale = context.mobile_rows.is_some() && context.mode == LyricsMode::Synced;
    let raster_text_size = if mobile_scale {
        if emphasis > 0.001 {
            active_text_size
        } else {
            normal_text_size
        }
    } else {
        text_size
    };
    // Reserve each synced row's geometry, so changing visual size/weight cannot
    // move the scroll target. Both layers share a left edge and line box.
    // Keep the same glyph renderer before, during and after word activation.
    // The unreached mask has a stable base color; only the reveal adds white.
    let parts = context
        .mobile_rows
        .as_ref()
        .map_or(std::slice::from_ref(line), |rows| {
            rows[index].primary.as_slice()
        });
    let highlighted = div()
        .w_full()
        .flex()
        .flex_col()
        .children(parts.iter().map(|part| {
            let units = if context.mobile_rows.is_some() && !part.words.is_empty() {
                Some(
                    part.words
                        .iter()
                        .map(|word| {
                            TimedTextUnit::new(word.range.clone(), word.start, word.end)
                                .group(word.group)
                        })
                        .collect(),
                )
            } else {
                timed_text_units(part)
            };
            if let Some(units) = units
                && context.active == Some(index)
            {
                TimedText::new(part.text.clone(), units)
                    .position(context.position)
                    .active_fill(rgb(0xffffff))
                    .inactive_opacity(1.)
                    .progressive_lift(px(if context.large { 3. } else { 1. } * activation))
                    .w_full()
                    .line_height(relative(1.3))
                    .reveal_wave_width(px(8.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(
                        0xffffff00
                            | (visual.resting_alpha.value.clamp(0., 1.) * 255.).round() as u32,
                    ))
                    .into_any_element()
            } else {
                TextBlur::new(part.text.clone())
                    .radius(px(blur_radius))
                    .w_full()
                    .line_height(relative(1.3))
                    .font_weight(FontWeight::BOLD)
                    .text_color(text_color)
                    .into_any_element()
            }
        }))
        .into_any_element();
    let highlighted = if activation > 0.001 {
        subtree_bloom(
            highlighted,
            BloomOptions {
                threshold: 0.75,
                soft_knee: 0.1,
                intensity: 0.28 * activation,
                radius: px(if context.large { 7. } else { 5. } * context.scale),
                downsample: 2,
            },
        )
        .into_any_element()
    } else {
        highlighted
    };
    let primary =
        if context.mode == LyricsMode::Plain {
            div()
                .font_weight(FontWeight::BOLD)
                .children(parts.iter().map(|part| div().child(part.text.clone())))
                .into_any_element()
        } else {
            div()
                .relative()
                .w_full()
                .h(px(context
                    .mobile_rows
                    .as_ref()
                    .map_or(active_text_size * 1.3, |rows| {
                        rows[index].primary_height
                    })))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w_full()
                        .opacity(1. - emphasis)
                        .child(div().w_full().flex().flex_col().children(parts.iter().map(
                            |part| {
                                TextBlur::new(part.text.clone())
                                    .radius(px(blur_radius))
                                    .w_full()
                                    .line_height(relative(1.3))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(text_color)
                            },
                        ))),
                )
                .when(emphasis > 0.001, |primary| {
                    primary.child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .w_full()
                            .opacity(emphasis)
                            .child(highlighted),
                    )
                })
                .into_any_element()
        };
    // Keep the glyph atlas at endpoint sizes. Only the short emphasis transition
    // needs a paint-time scale, rather than a new font size on every frame.
    let primary = if mobile_scale && emphasis > 0.001 && emphasis < 0.999 {
        let scale = text_size / active_text_size;
        gpui_effects::transform_group(
            div()
                .w_full()
                .text_size(px(active_text_size))
                .child(primary),
            gpui::TransformationMatrix::unit().scale(gpui::size(scale, scale)),
        )
        .into_any_element()
    } else {
        primary
    };
    let text = div().w_full().flex().flex_col().child(primary).when_some(
        line.translation.clone().filter(|_| {
            context
                .mobile_rows
                .as_ref()
                .is_none_or(|rows| !rows[index].translations.is_empty())
        }),
        |text, translation| {
            let translation = if context.mode == LyricsMode::Synced {
                let translations = context
                    .mobile_rows
                    .as_ref()
                    .map_or(std::slice::from_ref(&translation), |rows| {
                        rows[index].translations.as_slice()
                    });
                div()
                    .mt(px(if context.large { 5. } else { 3. } * context.scale))
                    .w_full()
                    .flex()
                    .flex_col()
                    .children(translations.iter().map(|text| {
                        TextBlur::new(text.clone())
                            .radius(px(blur_radius))
                            .text_size(px(if context.large { 13.5 } else { 10.5 } * context.scale))
                            .line_height(relative(1.25))
                            .font_weight(font_weight)
                            .text_color(translation_color)
                    }))
                    .into_any_element()
            } else {
                div()
                    .mt(px(if context.large { 5. } else { 3. }))
                    .text_size(px(if context.large { 13.5 } else { 10.5 } * context.scale))
                    .line_height(relative(1.25))
                    .font_weight(font_weight)
                    .text_color(translation_color)
                    .child(translation)
                    .into_any_element()
            };
            text.child(translation)
        },
    );
    let row = div()
        .id(("lyric-row", index))
        .relative()
        .w_full()
        .min_h(px(if context.large { 64. } else { 44. }))
        .when(context.mode == LyricsMode::Synced, |row| {
            row.h(px(context.mobile_rows.as_ref().map_or_else(
                || match (context.large, line.translation.is_some()) {
                    (true, true) => 72.,
                    (true, false) => 64.,
                    (false, true) => 56.,
                    (false, false) => 44.,
                },
                |rows| rows[index].height,
            )))
            .flex_none()
        })
        .px(px(if context.mobile_rows.is_some() {
            32. * context.scale
        } else if context.large {
            26.
        } else {
            18.
        }))
        .py(px(if context.large { 3. } else { 2. }))
        .flex()
        .items_center()
        .rounded(px(18.))
        .text_size(px(raster_text_size))
        .line_height(relative(1.3))
        .text_color(text_color)
        .font_weight(font_weight)
        .opacity(edge_opacity)
        .child(text)
        .when_some(line.start, |row, start| {
            let seek = context.seek.clone();
            let interaction = context.interaction.clone();
            row.cursor_pointer()
                .when(context.mobile_rows.is_none(), |row| {
                    row.hover(|row| row.bg(rgba(0xffffff0a)))
                })
                .on_click(move |_, _, cx| {
                    interaction.set(None);
                    seek(start, cx);
                })
        });

    // The scroll handle measures this stationary slot. Move the whole inner
    // row (including its hover/click target), never the geometry used to center it.
    div()
        .w_full()
        .flex_none()
        .child(row.top(px(row_offset)))
        .into_any_element()
}

// Preserve absolute timing and group identity when one timed word spans a wrap.
fn slice_lyric_line(line: &LyricLine, range: std::ops::Range<usize>) -> LyricLine {
    let words = line
        .words
        .iter()
        .filter_map(|word| {
            let start = word.range.start.max(range.start);
            let end = word.range.end.min(range.end);
            if start >= end {
                return None;
            }
            let total = line.text[word.range.clone()].chars().count() as f64;
            let offset = line.text[word.range.start..start].chars().count() as f64 / total;
            let finish = line.text[word.range.start..end].chars().count() as f64 / total;
            let duration = word.end.saturating_sub(word.start);
            Some(crate::domain::lyrics::LyricWord {
                range: start - range.start..end - range.start,
                start: word.start + duration.mul_f64(offset),
                end: word.start + duration.mul_f64(finish),
                group: word.group,
            })
        })
        .collect::<Vec<_>>();
    LyricLine {
        start: line.start,
        end: line.end,
        text: line.text[range].to_owned().into(),
        translation: None,
        words: words.into(),
    }
}

pub(crate) fn timed_text_units(line: &LyricLine) -> Option<Vec<TimedTextUnit>> {
    let text = line.text.as_ref();
    if line.words.len() < 2 {
        return None;
    }

    let mut units = Vec::with_capacity(line.words.len());
    for word in line.words.iter() {
        if word.start > word.end
            || word.range.start >= word.range.end
            || word.range.end > text.len()
            || !text.is_char_boundary(word.range.start)
            || !text.is_char_boundary(word.range.end)
        {
            return None;
        }
        units.push(TimedTextUnit::new(word.range.clone(), word.start, word.end).group(word.group));
    }
    Some(units)
}

const GRAY_FLOW_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">
<rect width="128" height="128" fill="#2c2d32"/>
<defs>
<radialGradient id="a" cx="22%" cy="24%" r="58%"><stop stop-color="#d2d3d8"/><stop offset="1" stop-color="#2c2d32" stop-opacity="0"/></radialGradient>
<radialGradient id="b" cx="78%" cy="38%" r="52%"><stop stop-color="#9a9ba3"/><stop offset="1" stop-color="#2c2d32" stop-opacity="0"/></radialGradient>
<radialGradient id="c" cx="38%" cy="78%" r="56%"><stop stop-color="#6e6f77"/><stop offset="1" stop-color="#2c2d32" stop-opacity="0"/></radialGradient>
<radialGradient id="d" cx="86%" cy="86%" r="46%"><stop stop-color="#c4c5cc"/><stop offset="1" stop-color="#2c2d32" stop-opacity="0"/></radialGradient>
</defs>
<rect width="128" height="128" fill="url(#a)"/>
<rect width="128" height="128" fill="url(#b)"/>
<rect width="128" height="128" fill="url(#c)"/>
<rect width="128" height="128" fill="url(#d)"/>
</svg>"##;

fn gray_flow_image() -> Arc<Image> {
    static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                GRAY_FLOW_SVG.as_bytes().to_vec(),
            ))
        })
        .clone()
}

#[cfg(test)]
mod mobile_tests {
    use super::*;
    use crate::domain::lyrics::LyricWord;

    #[test]
    fn wrapped_word_keeps_utf8_ranges_and_continuous_absolute_timing() {
        let line = LyricLine {
            start: Some(Duration::from_secs(8)),
            end: Some(Duration::from_secs(12)),
            text: "a你🙂b".into(),
            translation: None,
            words: vec![
                LyricWord::new(0..9, Duration::from_secs(8), Duration::from_secs(12)).group(7),
            ]
            .into(),
        };
        let first = slice_lyric_line(&line, 0..4);
        let second = slice_lyric_line(&line, 4..9);
        assert_eq!(first.text.as_ref(), "a你");
        assert_eq!(second.text.as_ref(), "🙂b");
        assert_eq!(first.words[0].range, 0..4);
        assert_eq!(second.words[0].range, 0..5);
        assert_eq!(first.words[0].end, Duration::from_secs(10));
        assert_eq!(second.words[0].start, first.words[0].end);
        assert_eq!(second.words[0].end, Duration::from_secs(12));
        assert_eq!(second.words[0].group, 7);
        assert_eq!(second.start, line.start);
    }
}
