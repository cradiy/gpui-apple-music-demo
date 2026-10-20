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
        let duration = if matches!(
            (self.from, self.to),
            (Page::Home, Page::Library) | (Page::Library, Page::Home)
        ) {
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

impl MobileApp {
    pub(super) fn reveal_player_controls(&mut self, cx: &mut Context<Self>) {
        self.controls_idle_since = Instant::now();
        if std::mem::replace(&mut self.controls_hidden, false) {
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
    use super::{PlayerDismiss, PlayerTouch};
    use gpui::{TouchId, point, px};
    use std::time::{Duration, Instant};

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
