use std::{collections::BTreeMap, time::Duration};

use gpui::{
    Animation, AnimationExt, AnyElement, FontWeight, SharedString, div, ease_out_quint,
    linear_color_stop, linear_gradient, multi_linear_gradient, point, prelude::*, px, relative,
    rgb, rgba,
};
use gpui_effects::{TimedText, TimedTextEmphasis, TimedTextUnit};

use crate::{
    lyrics::{LyricGap, LyricLine, LyricsMode},
    state::PlaybackState,
    theme::layout,
};

#[derive(Default)]
pub struct LyricsPanelState {
    inline: LyricsScrollState,
    fullscreen: LyricsScrollState,
}

#[derive(Default)]
struct LyricsScrollState {
    previous_focus: Option<usize>,
    direction: f32,
    animation_generation: u64,
}

impl LyricsScrollState {
    fn update(&mut self, focus: usize) -> (f32, Option<u64>) {
        if let Some(previous) = self.previous_focus.replace(focus) {
            if previous != focus {
                self.direction = if focus > previous { 1. } else { -1. };
                self.animation_generation = self.animation_generation.wrapping_add(1);
            }
        } else {
            return (0., None);
        }
        (
            self.direction,
            (self.animation_generation > 0).then_some(self.animation_generation),
        )
    }
}

pub fn render(playback: &PlaybackState, state: &mut LyricsPanelState) -> AnyElement {
    render_panel(playback, state, false)
}

pub fn render_fullscreen(playback: &PlaybackState, state: &mut LyricsPanelState) -> AnyElement {
    render_panel(playback, state, true)
}

fn render_panel(
    playback: &PlaybackState,
    state: &mut LyricsPanelState,
    fullscreen: bool,
) -> AnyElement {
    let lyrics = &playback.track.lyrics;
    let position = playback.timeline.position();
    let active = lyrics.active_index(position);
    let focus = active.unwrap_or_else(|| {
        lyrics
            .lines
            .partition_point(|line| line.start.is_some_and(|start| start <= position))
            .min(lyrics.lines.len().saturating_sub(1))
    });
    let has_word_timing = lyrics.lines.iter().any(|line| line.words.len() > 1);
    let gap = lyrics.gap_at(position, Duration::from_secs(10));
    let (start, end) = visible_range(
        lyrics.lines.len(),
        focus,
        lyrics.mode,
        if fullscreen { 9 } else { 11 },
    );
    let mut rows = Vec::with_capacity(end.saturating_sub(start) + usize::from(gap.is_some()));
    for index in start..end {
        rows.push(lyric_row(
            &lyrics.lines[index],
            index,
            focus,
            active,
            lyrics.mode,
            position,
            fullscreen,
        ));
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
    let scroll_state = if fullscreen {
        &mut state.fullscreen
    } else {
        &mut state.inline
    };
    let (scroll_direction, scroll_animation) = scroll_state.update(focus);
    let scroll_distance = if fullscreen { 74. } else { 52. };
    let rows = div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(if fullscreen { 10. } else { 8. }))
        .children(rows);
    let rows = if let Some(generation) = scroll_animation {
        rows.with_animation(
            SharedString::from(format!(
                "lyrics-scroll-{}-{generation}",
                if fullscreen { "fullscreen" } else { "inline" }
            )),
            Animation::new(scroll_animation_duration()),
            move |rows, delta| {
                let spring = scroll_spring_progress(delta);
                rows.mt(px((1. - spring) * scroll_distance * scroll_direction))
                    .opacity(0.92 + smootherstep(delta) * 0.08)
            },
        )
        .into_any_element()
    } else {
        rows.into_any_element()
    };

    div()
        .relative()
        .h_full()
        .when(cfg!(target_os = "linux"), |this| {
            this.font_family(crate::FONT_FAMILY)
        })
        .flex_none()
        .flex()
        .flex_col()
        .when(fullscreen, |panel| panel.w_full())
        .when(!fullscreen, |panel| {
            panel
                .w(px(layout::LYRICS_WIDTH))
                .bg(rgb(0x12141c))
                .border_l_1()
                .border_color(rgba(0xffffff10))
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
                    .text_size(px(9.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(0xb7b8c25c))
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
                .px(px(if fullscreen { 30. } else { 22. }))
                .flex()
                .flex_col()
                .justify_center()
                .when(lyrics.mode == LyricsMode::Empty, |panel| {
                    panel.child(
                        div()
                            .px(px(24.))
                            .text_center()
                            .text_size(px(10.))
                            .text_color(rgba(0xb7b8c278))
                            .child("No embedded lyrics found."),
                    )
                })
                .when(lyrics.mode != LyricsMode::Empty, |panel| panel.child(rows)),
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
                            linear_color_stop(rgba(0x12141cff), 0.),
                            linear_color_stop(rgba(0x12141c00), 1.),
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
                            linear_color_stop(rgba(0x12141cff), 0.),
                            linear_color_stop(rgba(0x12141c00), 1.),
                        )),
                )
        })
        .into_any_element()
}

fn visible_range(
    length: usize,
    active: usize,
    mode: LyricsMode,
    max_rows: usize,
) -> (usize, usize) {
    if length <= max_rows {
        return (0, length);
    }
    if mode == LyricsMode::Plain {
        return (0, max_rows);
    }
    let start = active.saturating_sub(max_rows / 2).min(length - max_rows);
    (start, start + max_rows)
}

fn lyric_gap_row(gap: LyricGap, position: Duration, large: bool, animate: bool) -> AnyElement {
    let elapsed = position.saturating_sub(gap.start).as_secs_f32();
    let remaining = gap.end.saturating_sub(position).as_secs_f32();
    let visibility = smootherstep((elapsed / 0.28).min(remaining / 0.42).clamp(0., 1.));
    let row_height = if large { 42. } else { 32. };
    let dots = div()
        .h(px(14.))
        .flex()
        .items_center()
        .gap(px(if large { 8. } else { 6. }));
    let dots = if animate {
        dots.with_animation(
            SharedString::from(format!("lyrics-gap-dots-{}-{}", gap.previous, gap.next)),
            Animation::new(Duration::from_millis(1_080)).repeat(),
            |dots, phase| dots.children((0..3).map(|index| lyric_gap_dot(phase, index))),
        )
        .into_any_element()
    } else {
        dots.children((0..3).map(|index| lyric_gap_dot(0.18, index)))
            .into_any_element()
    };

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

fn scroll_animation_duration() -> Duration {
    Duration::from_millis(820)
}

fn scroll_spring_progress(delta: f32) -> f32 {
    let delta = delta.clamp(0., 1.);
    if delta >= 1. {
        return 1.;
    }
    1. - (-8. * delta).exp() * (11. * delta).cos()
}

fn lyric_row(
    line: &LyricLine,
    index: usize,
    focus: usize,
    active: Option<usize>,
    mode: LyricsMode,
    position: Duration,
    large: bool,
) -> AnyElement {
    let distance = index.abs_diff(focus);
    let is_active = mode == LyricsMode::Synced && active == Some(index);
    let text_alpha: u32 = if mode == LyricsMode::Plain {
        0xc0
    } else if active.is_none() {
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
    let resting_text_size = if large { 24. } else { 16. };
    let active_text_size = if large { 26. } else { 17. };
    let text_size = if is_active {
        active_text_size
    } else {
        resting_text_size
    };
    let timed_units = timed_text_units(line);
    let primary = if let Some(units) = timed_units {
        let elastic_groups = elastic_timed_groups(&units);
        let active_fill = if is_active {
            multi_linear_gradient(
                90.,
                [
                    linear_color_stop(rgb(0xffffff), 0.),
                    linear_color_stop(rgb(0xffe8f2), 0.48),
                    linear_color_stop(rgb(0xffffff), 1.),
                ],
            )
        } else {
            rgba(0xffffff00 | text_alpha).into()
        };
        TimedText::new(line.text.clone(), units)
            .position(position)
            .active_fill(active_fill)
            .inactive_opacity(if is_active { 0.68 } else { 1. })
            .emphasis(TimedTextEmphasis {
                scale: if large { 1.065 } else { 1.045 },
                translation: point(px(0.), px(if large { -3.2 } else { -2.2 })),
                surrounding_spread: px(if large { 3.2 } else { 2.2 }),
                enter_fraction: 0.2,
                exit_fraction: 0.28,
            })
            .elastic_groups(elastic_groups)
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
                    .mt(px(if large { 5. } else { 3. }))
                    .text_size(px(if large { 13.5 } else { 10.5 }))
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
        .relative()
        .w_full()
        .min_h(px(if large { 64. } else { 44. }))
        .px(px(if large { 26. } else { 18. }))
        .py(px(if large { 3. } else { 2. }))
        .flex()
        .items_center()
        .rounded(px(18.))
        .text_size(px(text_size))
        .line_height(relative(1.3))
        .text_color(rgba(0xffffff00 | text_alpha))
        .font_weight(FontWeight::BOLD)
        .child(text);

    if is_active {
        row.with_animation(
            SharedString::from(format!("active-lyric-{index}")),
            Animation::new(Duration::from_millis(440)).with_easing(ease_out_quint()),
            move |row, delta| {
                row.text_size(px(
                    resting_text_size + (active_text_size - resting_text_size) * delta
                ))
            },
        )
        .into_any_element()
    } else {
        row.into_any_element()
    }
}

fn timed_text_units(line: &LyricLine) -> Option<Vec<TimedTextUnit>> {
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

fn elastic_timed_groups(units: &[TimedTextUnit]) -> Vec<usize> {
    let mut ranges = BTreeMap::<usize, (Duration, Duration)>::new();
    for unit in units {
        ranges
            .entry(unit.group)
            .and_modify(|(start, end)| {
                *start = (*start).min(unit.start);
                *end = (*end).max(unit.end);
            })
            .or_insert((unit.start, unit.end));
    }
    ranges
        .into_iter()
        .filter_map(|(group, (start, end))| {
            (end.saturating_sub(start) >= Duration::from_millis(900)).then_some(group)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use gpui::SharedString;

    use super::{LyricsScrollState, elastic_timed_groups, timed_text_units};
    use crate::lyrics::{LyricLine, LyricWord};

    #[test]
    fn maps_utf8_word_ranges_to_timed_text_units() {
        let line = LyricLine {
            start: Some(Duration::from_secs(1)),
            end: Some(Duration::from_secs(3)),
            text: SharedString::from("你好吗"),
            translation: None,
            words: Arc::from([
                LyricWord::new(
                    0..3,
                    Duration::from_millis(1000),
                    Duration::from_millis(1400),
                ),
                LyricWord::new(
                    3..6,
                    Duration::from_millis(1400),
                    Duration::from_millis(1900),
                )
                .group(1),
                LyricWord::new(
                    6..9,
                    Duration::from_millis(1900),
                    Duration::from_millis(2500),
                )
                .group(1),
            ]),
        };

        let units = timed_text_units(&line).expect("valid timed units");
        assert_eq!(units.len(), 3);
        assert_eq!(units[0].range, 0..3);
        assert_eq!(units[1].group, units[2].group);
    }

    #[test]
    fn rejects_invalid_utf8_ranges_before_rendering() {
        let line = LyricLine {
            start: Some(Duration::ZERO),
            end: Some(Duration::from_secs(1)),
            text: SharedString::from("你"),
            translation: None,
            words: Arc::from([LyricWord::new(1..3, Duration::ZERO, Duration::from_secs(1))]),
        };

        assert!(timed_text_units(&line).is_none());
    }

    #[test]
    fn only_long_timed_groups_receive_emphasis() {
        let units = vec![
            gpui_effects::TimedTextUnit::new(0..3, Duration::ZERO, Duration::from_millis(180))
                .group(0),
            gpui_effects::TimedTextUnit::new(
                3..6,
                Duration::from_millis(200),
                Duration::from_millis(900),
            )
            .group(1),
            gpui_effects::TimedTextUnit::new(
                6..9,
                Duration::from_millis(900),
                Duration::from_millis(1_800),
            )
            .group(1),
        ];

        assert_eq!(elastic_timed_groups(&units), vec![1]);
    }

    #[test]
    fn scroll_direction_tracks_forward_and_backward_window_changes() {
        let mut state = LyricsScrollState::default();
        assert_eq!(state.update(0), (0., None));

        let (direction, forward_animation) = state.update(1);
        assert_eq!(direction, 1.);
        let forward_animation = forward_animation.expect("forward animation");
        assert_eq!(state.update(1), (1., Some(forward_animation)));

        let (direction, backward_animation) = state.update(0);
        assert_eq!(direction, -1.);
        assert_ne!(backward_animation, Some(forward_animation));
    }

    #[test]
    fn scroll_spring_overshoots_then_settles() {
        assert_eq!(super::scroll_spring_progress(0.), 0.);
        assert!(super::scroll_spring_progress(0.25) > 1.);
        assert_eq!(super::scroll_spring_progress(1.), 1.);
    }
}
