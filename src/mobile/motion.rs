use super::*;

pub(super) struct ValueMotion {
    from: f32,
    velocity: f32,
    to: f32,
    started: Instant,
}

impl ValueMotion {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            velocity: 0.,
            to: value,
            started: Instant::now(),
        }
    }

    pub fn sample(&mut self, target: f32, window: &Window) -> f32 {
        let duration = 0.28;
        let t = (self.started.elapsed().as_secs_f32() / duration).min(1.);
        let t2 = t * t;
        let t3 = t2 * t;
        let value = (2. * t3 - 3. * t2 + 1.) * self.from
            + (t3 - 2. * t2 + t) * self.velocity * duration
            + (-2. * t3 + 3. * t2) * self.to;
        if target != self.to {
            self.velocity = ((6. * t2 - 6. * t) * (self.from - self.to)
                + (3. * t2 - 4. * t + 1.) * self.velocity * duration)
                / duration;
            self.from = value;
            self.to = target;
            self.started = Instant::now();
        }
        if (t < 1. && (self.from != self.to || self.velocity != 0.)) || target != value {
            window.request_animation_frame();
        }
        value.clamp(0., 1.)
    }
}

pub(super) struct ControlsMotion {
    from: f32,
    velocity: f32,
    hidden: bool,
    started: Instant,
}

impl ControlsMotion {
    pub fn new() -> Self {
        Self {
            from: 0.,
            velocity: 0.,
            hidden: false,
            started: Instant::now(),
        }
    }

    fn evaluate(&self, now: Instant) -> (f32, f32, bool) {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f32();
        if self.hidden {
            let duration = 0.18;
            let t = (elapsed / duration).min(1.);
            if t >= 1. {
                return (1., 0., false);
            }
            let distance = 1. - self.from;
            let value =
                self.from + distance * t * t + self.velocity * duration * t * (1. - t).powi(2);
            let velocity =
                2. * distance * t / duration + self.velocity * (1. - 4. * t + 3. * t * t);
            (value, velocity, true)
        } else {
            let windup = if self.from >= 0.99 && self.velocity.abs() < 0.005 {
                0.04
            } else {
                0.
            };
            if elapsed < windup {
                let t = elapsed / windup;
                return (
                    self.from + 0.02 * t * t * (3. - 2. * t),
                    0.12 * t * (1. - t) / windup,
                    true,
                );
            }
            let elapsed = elapsed - windup;
            let rate = 24.;
            let a = self.from + if windup > 0. { 0.02 } else { 0. };
            let b = self.velocity + rate * a;
            let envelope = (-rate * elapsed).exp();
            let value = (a + b * elapsed) * envelope;
            let velocity = (b - rate * (a + b * elapsed)) * envelope;
            if value < 0. || (value.abs() < 0.0005 && velocity.abs() < 0.005) {
                (0., 0., false)
            } else {
                (value, velocity, true)
            }
        }
    }

    fn sample_at(&mut self, hidden: bool, now: Instant) -> (f32, bool) {
        let (value, velocity, moving) = self.evaluate(now);
        if self.hidden != hidden {
            self.from = value;
            self.velocity = velocity;
            self.hidden = hidden;
            self.started = now;
            return (value, true);
        }
        (value, moving)
    }

    pub fn sample(&mut self, hidden: bool, window: &Window) -> f32 {
        let (value, moving) = self.sample_at(hidden, Instant::now());
        if moving {
            window.request_animation_frame();
        }
        value
    }
}

pub(super) struct PageMotion {
    pub from: Page,
    pub to: Page,
    started: Instant,
}

impl PageMotion {
    pub fn new(from: Page, to: Page) -> Self {
        Self {
            from,
            to,
            started: Instant::now(),
        }
    }

    pub fn progress(&self) -> (f32, bool) {
        let duration = if self.from.is_tab() && self.to.is_tab() {
            0.18
        } else {
            0.32
        };
        let t = (self.started.elapsed().as_secs_f32() / duration).min(1.);
        (1. - (1. - t).powi(3), t < 1.)
    }
}

#[derive(Default)]
pub(super) struct PlayerDismiss {
    offset: f32,
    touch: Option<PlayerTouch>,
    settle: Option<PlayerSettle>,
}

struct PlayerTouch {
    id: TouchId,
    start: Point<Pixels>,
    last_y: f32,
    sampled: Instant,
    velocity: f32,
    claimed: bool,
}

struct PlayerSettle {
    from: f32,
    to: f32,
    velocity: f32,
    started: Instant,
    duration: f32,
    closing: bool,
}

impl PlayerDismiss {
    pub fn opening(height: f32) -> Self {
        let mut state = Self {
            offset: height,
            ..Self::default()
        };
        state.settle(height, 0., false, Instant::now());
        state
    }

    pub fn progress(&self, height: f32) -> f32 {
        (self.offset / height.max(1.)).clamp(0., 1.)
    }

    pub fn active(&self) -> bool {
        self.offset > 0. || self.settle.is_some() || self.touch.as_ref().is_some_and(|t| t.claimed)
    }

    pub fn sample(&mut self, now: Instant) -> (f32, bool, bool) {
        let Some(motion) = &self.settle else {
            return (self.offset, false, false);
        };
        let t =
            (now.saturating_duration_since(motion.started).as_secs_f32() / motion.duration).min(1.);
        let t2 = t * t;
        let t3 = t2 * t;
        self.offset = (2. * t3 - 3. * t2 + 1.) * motion.from
            + (t3 - 2. * t2 + t) * motion.velocity * motion.duration
            + (-2. * t3 + 3. * t2) * motion.to;
        let closed = t >= 1. && motion.closing;
        if t >= 1. {
            self.settle = None;
        }
        (self.offset, t < 1., closed)
    }

    fn settle(&mut self, height: f32, velocity: f32, closing: bool, now: Instant) {
        let target = if closing { height } else { 0. };
        let distance = target - self.offset;
        let duration = (distance.abs() / height.max(1.) * 0.34).clamp(0.18, 0.34);
        // Keep the release tangent without allowing a settling curve to overshoot either endpoint.
        let velocity = if velocity.signum() == distance.signum() {
            velocity.signum() * velocity.abs().min(3. * distance.abs() / duration)
        } else {
            0.
        };
        self.touch = None;
        self.settle = Some(PlayerSettle {
            from: self.offset,
            to: target,
            velocity,
            started: now,
            duration,
            closing,
        });
    }

    fn move_to(&mut self, touch: &mut PlayerTouch, y: f32, height: f32, now: Instant) {
        let delta = y - touch.last_y;
        if delta.abs() > 0.1 {
            let dt = now
                .saturating_duration_since(touch.sampled)
                .as_secs_f32()
                .max(0.001);
            let velocity = (delta / dt).clamp(-6500., 6500.);
            touch.velocity = if touch.velocity.signum() == velocity.signum() {
                touch.velocity * 0.25 + velocity * 0.75
            } else {
                velocity
            };
            touch.sampled = now;
            self.offset = (self.offset + delta).clamp(0., height);
        }
        touch.last_y = y;
    }

    fn release(&mut self, touch: PlayerTouch, height: f32, cancelled: bool, now: Instant) {
        let velocity = if now.saturating_duration_since(touch.sampled) < Duration::from_millis(100)
        {
            touch.velocity
        } else {
            0.
        };
        let closing = !cancelled
            && velocity > -120.
            && (self.offset > height * 0.22 || (velocity > 800. && self.offset > 16.));
        self.settle(height, velocity, closing, now);
    }
}

#[derive(Default)]
pub(super) struct ControlsRevealGesture {
    contact: Option<TouchId>,
    last_scroll: Option<Instant>,
}

impl ControlsRevealGesture {
    fn scroll(&mut self, delta: f32, now: Instant) {
        if self.contact.is_some() {
            if delta > 0. {
                self.last_scroll = Some(now);
            } else if delta < 0. {
                self.last_scroll = None;
            }
        }
    }

    fn touch(&mut self, event: &TouchEvent, inside: bool, now: Instant) -> bool {
        match event.phase {
            TouchPhase::Started if inside => {
                self.contact = Some(event.id);
                self.last_scroll = None;
            }
            TouchPhase::Ended if self.contact == Some(event.id) => {
                let reveal = self.last_scroll.is_some_and(|last| {
                    now.saturating_duration_since(last) <= Duration::from_millis(350)
                });
                *self = Self::default();
                return reveal;
            }
            TouchPhase::Cancelled if self.contact == Some(event.id) => *self = Self::default(),
            _ => {}
        }
        false
    }
}

#[derive(Default)]
pub(super) struct LyricsControlsScroll {
    previous_line: Option<usize>,
    advanced_lines: usize,
}

impl LyricsControlsScroll {
    fn advance(&mut self, line: usize, phase: TouchPhase) -> bool {
        match phase {
            TouchPhase::Started => self.previous_line = Some(line),
            TouchPhase::Ended => self.previous_line = None,
            TouchPhase::Cancelled => *self = Self::default(),
            TouchPhase::Moved => {
                if let Some(previous) = self.previous_line.replace(line) {
                    self.advanced_lines += line.saturating_sub(previous);
                }
            }
        }
        self.advanced_lines >= 3
    }
}

impl MobileApp {
    pub(super) fn touch_player_controls(
        &mut self,
        event: &TouchEvent,
        inside: bool,
        cx: &mut Context<Self>,
    ) {
        if self.controls_reveal.touch(event, inside, Instant::now()) {
            self.reveal_player_controls(cx);
        }
    }

    pub(super) fn record_player_activity(&mut self) {
        if !self.controls_hidden {
            self.controls_idle_since = Instant::now();
        }
    }

    pub(super) fn reveal_player_controls(&mut self, cx: &mut Context<Self>) {
        self.controls_idle_since = Instant::now();
        self.controls_scroll = LyricsControlsScroll::default();
        if std::mem::replace(&mut self.controls_hidden, false) {
            cx.notify();
        }
    }

    pub(super) fn scroll_player_controls(
        &mut self,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        if event.touch_phase == TouchPhase::Cancelled {
            self.controls_scroll = LyricsControlsScroll::default();
            self.controls_reveal = ControlsRevealGesture::default();
            return;
        }
        let delta = event.delta.pixel_delta(px(1.)).y.as_f32();
        if event.touch_phase == TouchPhase::Moved {
            self.controls_reveal.scroll(delta, Instant::now());
        }
        if delta > 0. {
            self.controls_scroll = LyricsControlsScroll::default();
        } else if !self.controls_hidden
            && let Some(line) = self.lyric_state.mobile_scroll_line()
            && self.controls_scroll.advance(line, event.touch_phase)
        {
            self.controls_hidden = true;
            self.controls_idle_task = None;
            self.controls_scroll = LyricsControlsScroll::default();
            cx.notify();
        }
    }

    pub(super) fn update_controls_idle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let eligible = self.page == Page::NowPlaying
            && self.show_lyrics
            && !self.lyrics.lines.is_empty()
            && window.is_window_active()
            && !modal::is_open(cx)
            && !self.seek_dragging
            && !self.player_dismiss.active();
        if !eligible {
            self.controls_idle_task = None;
            self.reveal_player_controls(cx);
            return;
        }
        if self.controls_hidden || self.controls_idle_task.is_some() {
            return;
        }
        self.controls_idle_task = Some(cx.spawn_in(window, async move |this, cx| {
            let timeout = Duration::from_secs(5);
            let mut remaining = timeout;
            loop {
                cx.background_executor().timer(remaining).await;
                let Ok(next) = this.update_in(cx, |this, _, cx| {
                    let elapsed = this.controls_idle_since.elapsed();
                    if elapsed < timeout {
                        Some(timeout - elapsed)
                    } else {
                        this.controls_hidden = true;
                        this.controls_idle_task = None;
                        cx.notify();
                        None
                    }
                }) else {
                    break;
                };
                let Some(next) = next else { break };
                remaining = next;
            }
        }));
    }

    pub(super) fn dismiss_player(&mut self, window: &Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.player_dismiss.sample(now);
        self.player_dismiss
            .settle(window.viewport_size().height.as_f32(), 0., true, now);
        self.page_motion = None;
        cx.notify();
    }

    pub(super) fn player_touch(
        &mut self,
        event: &TouchEvent,
        phase: DispatchPhase,
        inside: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page != Page::NowPlaying || self.page_motion.is_some() || modal::is_open(cx) {
            return;
        }
        let owned = self
            .player_dismiss
            .touch
            .as_ref()
            .is_some_and(|t| t.id == event.id && t.claimed);
        if phase.capture() != owned {
            return;
        }
        let now = Instant::now();
        let height = window.viewport_size().height.as_f32();
        if event.phase == TouchPhase::Started {
            if self.player_dismiss.touch.is_some() {
                self.player_dismiss.settle(height, 0., false, now);
                cx.notify();
            } else if inside && !window.default_prevented() {
                self.player_dismiss.sample(now);
                let claimed = self.player_dismiss.settle.take().is_some();
                self.player_dismiss.touch = Some(PlayerTouch {
                    id: event.id,
                    start: event.position,
                    last_y: event.position.y.as_f32(),
                    sampled: now,
                    velocity: 0.,
                    claimed,
                });
                if claimed {
                    window.prevent_default();
                    cx.notify();
                }
            }
            return;
        }
        let Some(mut touch) = self.player_dismiss.touch.take() else {
            return;
        };
        if touch.id != event.id {
            self.player_dismiss.touch = Some(touch);
            return;
        }
        match event.phase {
            TouchPhase::Moved => {
                let delta = event.position - touch.start;
                if !owned
                    && (window.default_prevented()
                        || delta.y < px(-8.)
                        || (delta.x.abs() > px(8.) && delta.x.abs() >= delta.y.abs()))
                {
                    return;
                }
                if !owned && delta.y <= px(8.) {
                    self.player_dismiss.touch = Some(touch);
                    return;
                }
                touch.claimed = true;
                self.player_dismiss
                    .move_to(&mut touch, event.position.y.as_f32(), height, now);
                self.player_dismiss.touch = Some(touch);
                window.prevent_default();
                cx.notify();
            }
            TouchPhase::Ended | TouchPhase::Cancelled if owned => {
                let cancelled = event.phase == TouchPhase::Cancelled;
                if !cancelled {
                    self.player_dismiss
                        .move_to(&mut touch, event.position.y.as_f32(), height, now);
                }
                self.player_dismiss.release(touch, height, cancelled, now);
                window.prevent_default();
                cx.notify();
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {}
            _ => self.player_dismiss.touch = Some(touch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControlsMotion, ControlsRevealGesture, LyricsControlsScroll, PlayerDismiss, PlayerTouch,
    };
    use gpui::{TouchEvent, TouchId, TouchPhase, point, px};
    use std::time::{Duration, Instant};

    #[test]
    fn controls_reveal_on_fresh_release_not_during_drag_or_fling() {
        let start = Instant::now();
        let touch = |phase| TouchEvent {
            id: TouchId(1),
            phase,
            ..Default::default()
        };
        let mut gesture = ControlsRevealGesture::default();
        assert!(!gesture.touch(&touch(TouchPhase::Started), true, start));
        gesture.scroll(20., start + Duration::from_millis(30));
        assert!(!gesture.touch(
            &touch(TouchPhase::Moved),
            true,
            start + Duration::from_millis(40)
        ));
        assert!(gesture.touch(
            &touch(TouchPhase::Ended),
            true,
            start + Duration::from_millis(60)
        ));
        gesture.scroll(20., start + Duration::from_millis(80));
        assert!(!gesture.touch(
            &touch(TouchPhase::Ended),
            true,
            start + Duration::from_millis(100)
        ));
    }

    #[test]
    fn controls_stay_hidden_after_holding_or_reversing_the_drag() {
        let start = Instant::now();
        let touch = |phase| TouchEvent {
            id: TouchId(1),
            phase,
            ..Default::default()
        };
        let mut gesture = ControlsRevealGesture::default();
        gesture.touch(&touch(TouchPhase::Started), true, start);
        gesture.scroll(20., start + Duration::from_millis(20));
        assert!(!gesture.touch(
            &touch(TouchPhase::Ended),
            true,
            start + Duration::from_secs(3)
        ));
        gesture.touch(&touch(TouchPhase::Started), true, start);
        gesture.scroll(20., start + Duration::from_millis(20));
        gesture.scroll(-10., start + Duration::from_millis(40));
        assert!(!gesture.touch(
            &touch(TouchPhase::Ended),
            true,
            start + Duration::from_millis(60)
        ));
    }

    #[test]
    fn controls_slide_out_quickly_and_wind_up_without_rebounding() {
        let start = Instant::now();
        let mut motion = ControlsMotion::new();
        motion.started = start;
        motion.sample_at(true, start);
        let hidden_at = start + Duration::from_millis(180);
        assert_eq!(motion.sample_at(true, hidden_at), (1., false));

        motion.sample_at(false, hidden_at);
        let (windup, moving) = motion.sample_at(false, hidden_at + Duration::from_millis(20));
        assert!(moving && windup > 1. && windup < 1.03);
        let mut previous = 1.03;
        for time in (40..=800).step_by(10) {
            let (value, _) = motion.sample_at(false, hidden_at + Duration::from_millis(time));
            assert!(value >= 0. && value <= previous);
            previous = value;
        }
        assert_eq!(
            motion.sample_at(false, hidden_at + Duration::from_secs(1)),
            (0., false)
        );
    }

    #[test]
    fn controls_reversal_preserves_position_and_velocity() {
        let start = Instant::now();
        let mut motion = ControlsMotion::new();
        motion.started = start;
        motion.sample_at(true, start);
        for (time, hidden) in [(90, false), (150, true)] {
            let now = start + Duration::from_millis(time);
            let (before, velocity, _) = motion.evaluate(now);
            let (after, moving) = motion.sample_at(hidden, now);
            assert!(moving && (after - before).abs() < 0.00001);
            assert!((motion.evaluate(now).1 - velocity).abs() < 0.00001);
        }
        assert_eq!(
            motion.sample_at(true, start + Duration::from_millis(330)),
            (1., false)
        );
    }

    #[test]
    fn lyrics_controls_count_manual_lines_across_gestures() {
        let mut scroll = LyricsControlsScroll::default();
        assert!(!scroll.advance(10, TouchPhase::Started));
        assert!(!scroll.advance(10, TouchPhase::Moved));
        assert!(!scroll.advance(11, TouchPhase::Moved));
        assert!(!scroll.advance(11, TouchPhase::Ended));

        assert!(!scroll.advance(20, TouchPhase::Started));
        assert!(!scroll.advance(21, TouchPhase::Moved));
        assert!(scroll.advance(22, TouchPhase::Moved));

        assert!(!scroll.advance(22, TouchPhase::Cancelled));
        assert!(!scroll.advance(22, TouchPhase::Started));
        assert!(!scroll.advance(23, TouchPhase::Moved));
    }

    #[test]
    fn player_dismiss_reversal_and_interrupted_settle_keep_position() {
        let start = Instant::now();
        let height = 900.;
        let mut state = PlayerDismiss::default();
        let mut touch = PlayerTouch {
            id: TouchId(1),
            start: point(px(100.), px(100.)),
            last_y: 100.,
            sampled: start,
            velocity: 0.,
            claimed: true,
        };
        state.move_to(&mut touch, 460., height, start + Duration::from_millis(600));
        state.move_to(&mut touch, 400., height, start + Duration::from_millis(630));
        state.release(touch, height, false, start + Duration::from_millis(630));
        assert!(!state.settle.as_ref().unwrap().closing);
        let (y, moving, _) = state.sample(start + Duration::from_millis(680));
        assert!(moving && y > 0. && y < 300.);

        // Catch a running return animation and move in the opposite direction.
        state.settle.take();
        let mut touch = PlayerTouch {
            id: TouchId(2),
            start: point(px(100.), px(400.)),
            last_y: 400.,
            sampled: start + Duration::from_millis(680),
            velocity: 0.,
            claimed: true,
        };
        state.move_to(&mut touch, 440., height, start + Duration::from_millis(700));
        assert!((state.offset - y - 40.).abs() < 0.01);
        state.release(touch, height, false, start + Duration::from_millis(700));
        let (end, moving, closed) = state.sample(start + Duration::from_secs(2));
        assert!(closed && !moving && (end - height).abs() < 0.01);

        // Platform cancellation must return, even beyond the dismissal threshold.
        state.offset = height * 0.6;
        let touch = PlayerTouch {
            id: TouchId(3),
            start: point(px(0.), px(0.)),
            last_y: 0.,
            sampled: start,
            velocity: 2000.,
            claimed: true,
        };
        state.release(touch, height, true, start);
        let (end, moving, closed) = state.sample(start + Duration::from_secs(2));
        assert!(!closed && !moving && end.abs() < 0.01);
    }
}
