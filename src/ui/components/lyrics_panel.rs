use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use gpui::{
    Animation, AnimationExt, AnyElement, Entity, FontWeight, Image, ImageFormat, ScrollHandle,
    SharedString, div, linear_color_stop, linear_gradient, point, prelude::*, px, relative, rgb,
    rgba,
};
use gpui_effects::{TimedText, TimedTextUnit, album_glow};

use crate::{
    domain::PlaybackState,
    domain::lyrics::{LyricGap, LyricLine, LyricsMode},
    state::PlaybackController,
    ui::theme::layout,
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
    }
}

#[derive(Clone, Copy)]
struct ScrollAnimation {
    from: f32,
    to: f32,
    started_at: Instant,
    duration: Duration,
}

struct LyricRowContext {
    focus: usize,
    active: Option<usize>,
    mode: LyricsMode,
    position: Duration,
    large: bool,
    controller: Entity<PlaybackController>,
    interaction: Rc<Cell<Option<Instant>>>,
}

pub fn render(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    controller: Entity<PlaybackController>,
) -> AnyElement {
    render_panel(playback, state, controller, false)
}

pub fn render_fullscreen(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    controller: Entity<PlaybackController>,
) -> AnyElement {
    render_panel(playback, state, controller, true)
}

fn render_panel(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    controller: Entity<PlaybackController>,
    fullscreen: bool,
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
    let has_word_timing = lyrics.lines.iter().any(|line| line.words.len() > 1);
    let gap = lyrics.gap_at(position, LYRIC_GAP_MINIMUM);
    if fullscreen {
        state.fullscreen.reset_for_document(document_id);
    } else {
        state.inline.reset_for_document(document_id);
    }
    let mut rows = Vec::with_capacity(lyrics.lines.len() + usize::from(gap.is_some()) + 2);
    rows.push(
        div()
            .w_full()
            .h(px(if fullscreen { 300. } else { 160. }))
            .flex_none()
            .into_any_element(),
    );
    let mut focus_child = 1;
    let interaction = if fullscreen {
        state.fullscreen.last_user_scroll.clone()
    } else {
        state.inline.last_user_scroll.clone()
    };
    let row_context = LyricRowContext {
        focus,
        active,
        mode: lyrics.mode,
        position,
        large: fullscreen,
        controller,
        interaction,
    };
    for index in 0..lyrics.lines.len() {
        if index == focus {
            focus_child = rows.len();
        }
        rows.push(lyric_row(&lyrics.lines[index], index, &row_context));
        if let Some(gap) = gap
            && gap.previous == index
        {
            rows.push(lyric_gap_row(
                gap,
                position,
                fullscreen,
                playback.is_playing(),
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
    let scroll_state = if fullscreen {
        &mut state.fullscreen
    } else {
        &mut state.inline
    };
    update_scroll_position(scroll_state, focus, focus_child);
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
        .when(fullscreen, |lyrics| lyrics.h(px(620.)))
        .when(!fullscreen, |lyrics| lyrics.h_full().max_h(px(760.)))
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
                .w(px(layout::LYRICS_WIDTH))
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
                            album_glow(gray_flow_image())
                                .uniform(0, [0.34, 0.12, 0.58, 0.38])
                                .uniform(1, [0.72, 0.46, 0.16, 0.41])
                                .uniform(2, [0.06, 0.0, 0.0, 0.0])
                                .bg(rgb(0x2c2d32))
                                .effect_opacity(0.38)
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
                    .text_color(rgba(0xb7b8c28c))
                    .child("LYRICS")
                    .child(match lyrics.mode {
                        LyricsMode::Synced if has_word_timing => "WORD SYNC",
                        LyricsMode::Synced => "LINE SYNC",
                        LyricsMode::Plain => "EMBEDDED",
                        LyricsMode::Empty => "",
                    }),
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
                            .text_color(rgba(0xb7b8c278))
                            .child("NO LYRICS"),
                    )
                })
                .when(lyrics.mode != LyricsMode::Empty, |panel| {
                    panel
                        .when(!fullscreen, |panel| panel.pt(px(64.)).pb(px(160.)))
                        .items_center()
                        .justify_center()
                        .child(rows)
                }),
        )
        .when(!fullscreen, |panel| {
            panel
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(105.))
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(rgba(0x2c2d32e6), 0.),
                            linear_color_stop(rgba(0x2c2d3200), 1.),
                        )),
                )
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(92.))
                        .bg(linear_gradient(
                            0.,
                            linear_color_stop(rgba(0x2c2d32e6), 0.),
                            linear_color_stop(rgba(0x2c2d3200), 1.),
                        )),
                )
        })
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
            state.animation = Some(ScrollAnimation {
                from,
                to: target,
                started_at: now,
                duration: if returning_from_user_scroll {
                    Duration::from_millis(780)
                } else {
                    Duration::from_millis(620)
                },
            });
        }
    }
    state.last_target = Some(target);

    if let Some(animation) = state.animation {
        let progress = now
            .saturating_duration_since(animation.started_at)
            .as_secs_f32()
            / animation.duration.as_secs_f32();
        let progress = progress.clamp(0., 1.);
        let value = animation.from + (animation.to - animation.from) * smootherstep(progress);
        state
            .handle
            .set_offset(point(state.handle.offset().x, px(value)));
        if progress >= 1. {
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

pub(crate) const LYRIC_GAP_MINIMUM: Duration = Duration::from_secs(10);

fn lyric_gap_row(gap: LyricGap, position: Duration, large: bool, animate: bool) -> AnyElement {
    let elapsed = position.saturating_sub(gap.start).as_secs_f32();
    let remaining = gap.end.saturating_sub(position).as_secs_f32();
    let visibility = smootherstep((elapsed / 0.28).min(remaining / 0.42).clamp(0., 1.));
    let row_height = if large { 42. } else { 32. };
    let dots = lyric_gap_dots(gap, animate, if large { 8. } else { 6. });

    div()
        .w_full()
        .h(px(row_height * visibility))
        .flex_none()
        .overflow_hidden()
        .opacity(visibility)
        .px(px(if large { 26. } else { 18. }))
        .flex()
        .items_center()
        .child(dots)
        .into_any_element()
}

pub(crate) fn lyric_gap_dots(gap: LyricGap, animate: bool, gap_px: f32) -> AnyElement {
    let dots = div().h(px(14.)).flex().items_center().gap(px(gap_px));
    if animate {
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
                .opacity(0.26 + pulse * 0.74),
        )
        .into_any_element()
}

fn smootherstep(value: f32) -> f32 {
    let value = value.clamp(0., 1.);
    value * value * value * (value * (value * 6. - 15.) + 10.)
}

fn lyric_row(line: &LyricLine, index: usize, context: &LyricRowContext) -> AnyElement {
    let distance = index.abs_diff(context.focus);
    let is_active = context.mode == LyricsMode::Synced && context.active == Some(index);
    let text_alpha: u32 = if context.mode == LyricsMode::Plain {
        0xc0
    } else if context.active.is_none() {
        match distance {
            0 => 0xc0,
            1 => 0x9c,
            2 => 0x78,
            3 => 0x5c,
            _ => 0x44,
        }
    } else {
        match distance {
            0 => 0xff,
            1 => 0xaa,
            2 => 0x82,
            3 => 0x65,
            _ => 0x48,
        }
    };
    let text_size = if context.large { 24. } else { 16. };
    let timed_units = is_active.then(|| timed_text_units(line)).flatten();
    let primary = if let Some(units) = timed_units {
        TimedText::new(line.text.clone(), units)
            .position(context.position)
            .active_fill(rgb(0xfff8fb))
            .inactive_opacity(0.58)
            .progressive_lift(px(if context.large { 4. } else { 3. }))
            .w_full()
            .line_height(relative(1.3))
            .font_weight(FontWeight::BOLD)
            .text_color(rgba(0xffffff00 | text_alpha))
            .into_any_element()
    } else {
        div().child(line.text.clone()).into_any_element()
    };
    let text = div().w_full().flex().flex_col().child(primary).when_some(
        line.translation.clone(),
        |text, translation| {
            text.child(
                div()
                    .mt(px(if context.large { 5. } else { 3. }))
                    .text_size(px(if context.large { 13.5 } else { 10.5 }))
                    .line_height(relative(1.25))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(if is_active {
                        0xffffffb8
                    } else {
                        0xffffff00 | text_alpha.saturating_sub(0x24)
                    }))
                    .child(translation),
            )
        },
    );
    let row = div()
        .id(("lyric-row", index))
        .relative()
        .w_full()
        .min_h(px(if context.large { 64. } else { 44. }))
        .px(px(if context.large { 26. } else { 18. }))
        .py(px(if context.large { 3. } else { 2. }))
        .flex()
        .items_center()
        .rounded(px(18.))
        .text_size(px(text_size))
        .line_height(relative(1.3))
        .text_color(rgba(0xffffff00 | text_alpha))
        .font_weight(FontWeight::BOLD)
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

    row.into_any_element()
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
