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
use gpui_effects::{ColorFlowOptions, TextBlur, TimedText, TimedTextUnit, color_flow};

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
    controller: Entity<PlaybackController>,
    interaction: Rc<Cell<Option<Instant>>>,
}

pub fn render(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    controller: Entity<PlaybackController>,
    width: f32,
) -> AnyElement {
    render_panel(playback, state, controller, false, width)
}

pub fn render_fullscreen(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    controller: Entity<PlaybackController>,
) -> AnyElement {
    render_panel(playback, state, controller, true, 0.)
}

fn render_panel(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    controller: Entity<PlaybackController>,
    fullscreen: bool,
    width: f32,
) -> AnyElement {
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
    let mut rows = Vec::with_capacity(lyrics.lines.len() + usize::from(gap.is_some()) + 2);
    rows.push(
        div()
            .w_full()
            .h(px(if fullscreen { 300. } else { 160. }))
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
        controller,
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
    update_scroll_position(scroll_state, focus, focus_child);
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
        let edge_opacity = lyric_edge_opacity(&scroll_state.handle, rows.len(), row_offset);
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
            let edge_opacity = lyric_edge_opacity(&scroll_state.handle, rows.len(), 0.);
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
            .h(px(if fullscreen { 300. } else { 370. }))
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
    let rows = div()
        .id(if fullscreen {
            "fullscreen-lyrics-scroll"
        } else {
            "inline-lyrics-scroll"
        })
        .w_full()
        .when(fullscreen, |lyrics| lyrics.h(relative(0.98)))
        .when(!fullscreen, |lyrics| lyrics.h_full().min_h_0())
        .px(px(if fullscreen { 30. } else { 22. }))
        .flex()
        .flex_col()
        .gap(px(if fullscreen { 10. } else { 8. }))
        .overflow_y_scroll()
        .track_scroll(&scroll_handle)
        .on_scroll_wheel(move |_, _, _| {
            scroll_interaction.set(Some(Instant::now()));
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

fn update_scroll_position(state: &mut LyricsScrollState, focus: usize, focus_child: usize) {
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

    let Some(target) = centered_scroll_target(&state.handle, focus_child) else {
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

fn centered_scroll_target(handle: &ScrollHandle, item: usize) -> Option<f32> {
    let viewport = handle.bounds();
    if viewport.size.height <= px(0.) {
        return None;
    }
    let row = handle.bounds_for_item(item)?;
    let target = viewport.center().y - row.center().y;
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
    let step = if large { 0.8 } else { 0.65 };
    let maximum = if large { 3.2 } else { 2.5 };
    (distance as f32 * step).min(maximum)
}

fn lyric_visual_target(index: usize, context: &LyricRowContext) -> LyricVisual {
    let distance = index.abs_diff(context.focus);
    let is_active = context.mode == LyricsMode::Synced && context.active == Some(index);
    let blur_radius =
        (!context.manually_scrolling && context.mode == LyricsMode::Synced && distance > 0)
            .then(|| lyric_blur_radius(distance, context.large));
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
    let emphasis = visual.emphasis.value.clamp(0., 1.);
    let activation = visual.activation.value.clamp(0., 1.);
    let text_alpha = (visual.alpha.value.clamp(0., 1.) * 255.).round() as u32;
    let text_color = rgba(0xffffff00 | text_alpha);
    let translation_color = rgba(0xffffff00 | text_alpha.saturating_sub(0x10));
    let blur_radius = visual.blur.value.max(0.);
    let font_weight = FontWeight::MEDIUM;
    let normal_text_size = if context.large { 24. } else { 16. };
    let active_text_size = if context.large { 28. } else { 19. };
    let text_size = normal_text_size
        + (active_text_size - normal_text_size) * visual.emphasis.value.clamp(0., 1.03);
    // Reserve each synced row's geometry, so changing visual size/weight cannot
    // move the scroll target. Both layers share a left edge and line box.
    // Keep the same glyph renderer before, during and after word activation.
    // The unreached mask has a stable base color; only the reveal adds white.
    let timed_units = timed_text_units(line);
    let highlighted = if let Some(units) = timed_units {
        TimedText::new(line.text.clone(), units)
            .position(context.position)
            .active_fill(rgb(0xffffff))
            .inactive_opacity(1.)
            .progressive_lift(px(if context.large { 3. } else { 1. } * activation))
            .w_full()
            .line_height(relative(1.3))
            .reveal_wave_width(px(8.))
            .font_weight(FontWeight::BOLD)
            .text_color(rgba(
                0xffffff00 | (visual.resting_alpha.value.clamp(0., 1.) * 255.).round() as u32,
            ))
            .into_any_element()
    } else {
        TextBlur::new(line.text.clone())
            .radius(px(blur_radius))
            .w_full()
            .line_height(relative(1.3))
            .font_weight(FontWeight::BOLD)
            .text_color(text_color)
            .into_any_element()
    };
    let primary = if context.mode == LyricsMode::Plain {
        div()
            .font_weight(FontWeight::BOLD)
            .child(line.text.clone())
            .into_any_element()
    } else {
        div()
            .relative()
            .w_full()
            .h(px(active_text_size * 1.3))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .w_full()
                    .opacity(1. - emphasis)
                    .child(
                        TextBlur::new(line.text.clone())
                            .radius(px(blur_radius))
                            .w_full()
                            .line_height(relative(1.3))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(text_color),
                    ),
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
    let text = div().w_full().flex().flex_col().child(primary).when_some(
        line.translation.clone(),
        |text, translation| {
            let translation = if context.mode == LyricsMode::Synced {
                TextBlur::new(translation)
                    .radius(px(blur_radius))
                    .mt(px(if context.large { 5. } else { 3. }))
                    .text_size(px(if context.large { 13.5 } else { 10.5 }))
                    .line_height(relative(1.25))
                    .font_weight(font_weight)
                    .text_color(translation_color)
                    .into_any_element()
            } else {
                div()
                    .mt(px(if context.large { 5. } else { 3. }))
                    .text_size(px(if context.large { 13.5 } else { 10.5 }))
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
            row.h(px(match (context.large, line.translation.is_some()) {
                (true, true) => 72.,
                (true, false) => 64.,
                (false, true) => 56.,
                (false, false) => 44.,
            }))
            .flex_none()
        })
        .px(px(if context.large { 26. } else { 18. }))
        .py(px(if context.large { 3. } else { 2. }))
        .flex()
        .items_center()
        .rounded(px(18.))
        .text_size(px(text_size))
        .line_height(relative(1.3))
        .text_color(text_color)
        .font_weight(font_weight)
        .opacity(edge_opacity)
        .child(text)
        .when_some(line.start, |row, start| {
            let controller = context.controller.clone();
            let interaction = context.interaction.clone();
            row.cursor_pointer()
                .hover(|row| row.bg(rgba(0xffffff0a)))
                .on_click(move |_, _, cx| {
                    interaction.set(None);
                    controller.update(cx, |playback, cx| playback.seek_to(start, cx));
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
