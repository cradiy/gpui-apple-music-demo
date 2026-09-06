/// Analytic damped spring: retargeting preserves position and velocity, and
/// sampling is stable even after a dropped frame or a hidden panel resumes.
#[derive(Clone, Copy)]
pub(super) struct Spring {
    pub value: f32,
    pub velocity: f32,
}

impl Spring {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            velocity: 0.,
        }
    }

    pub fn advance(&mut self, target: f32, seconds: f32) {
        if self.value == target && self.velocity == 0. {
            return;
        }
        const FREQUENCY: f32 = 16.;
        const DAMPING: f32 = 0.8;
        let decay = FREQUENCY * DAMPING;
        let frequency = FREQUENCY * (1. - DAMPING * DAMPING).sqrt();
        let displacement = self.value - target;
        let b = (self.velocity + decay * displacement) / frequency;
        let (sin, cos) = (frequency * seconds).sin_cos();
        let envelope = (-decay * seconds).exp();
        let position = displacement * cos + b * sin;
        self.value = target + envelope * position;
        self.velocity = envelope * (-decay * position + frequency * (b * cos - displacement * sin));
        if (self.value - target).abs() < 0.0001 && self.velocity.abs() < 0.001 {
            *self = Self::new(target);
        }
    }
}

/// Each row receives a new destination a little later, but keeps moving with
/// its existing velocity while waiting. Delays never affect lyric timing.
pub(super) struct DelayedSpring {
    pub spring: Spring,
    target: f32,
    pending: Option<(f32, f32)>,
}

impl DelayedSpring {
    pub fn new(value: f32) -> Self {
        Self {
            spring: Spring::new(value),
            target: value,
            pending: None,
        }
    }

    pub fn retarget(&mut self, target: f32, delay: Option<f32>) {
        if let Some(delay) = delay {
            self.pending = Some((target, delay));
        } else if let Some((pending_target, _)) = &mut self.pending {
            // Layout corrections must not keep restarting the delay.
            *pending_target = target;
        } else {
            self.target = target;
        }
    }

    pub fn advance(&mut self, seconds: f32) {
        let mut remaining = seconds;
        if let Some((target, delay)) = self.pending {
            let waiting = remaining.min(delay);
            self.spring.advance(self.target, waiting);
            remaining -= waiting;
            if delay > waiting {
                self.pending = Some((target, delay - waiting));
            } else {
                self.target = target;
                self.pending = None;
            }
        }
        self.spring.advance(self.target, remaining);
    }
}

pub(super) fn row_delay(index: usize, focus: usize) -> f32 {
    index.saturating_sub(focus.saturating_sub(2)).min(5) as f32 * 0.03
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_rows_start_later_and_settle_together() {
        let mut upper = DelayedSpring::new(0.);
        let mut lower = DelayedSpring::new(0.);
        upper.retarget(-74., Some(row_delay(8, 10)));
        lower.retarget(-74., Some(row_delay(12, 10)));
        upper.advance(0.06);
        lower.advance(0.06);
        assert!(upper.spring.value < -1.);
        assert_eq!(lower.spring.value, 0.);
        lower.advance(0.1);
        assert!(lower.spring.value < -1.);
        upper.advance(2.);
        lower.advance(2.);
        assert!((upper.spring.value + 74.).abs() < 0.001);
        assert!((lower.spring.value + 74.).abs() < 0.001);
        assert!((row_delay(1000, 10) - 0.15).abs() < 0.0001);
    }

    #[test]
    fn retarget_while_waiting_keeps_momentum() {
        let mut row = DelayedSpring::new(0.);
        row.retarget(-74., None);
        row.advance(0.1);
        let before = row.spring;
        row.retarget(-148., Some(0.12));
        row.advance(0.);
        assert!((row.spring.value - before.value).abs() < 0.001);
        assert!((row.spring.velocity - before.velocity).abs() < 0.001);
        row.advance(0.03);
        assert!(row.spring.value < before.value);
        // Updating a layout target doesn't restart its remaining delay.
        row.retarget(-150., None);
        row.advance(0.1);
        assert!(row.pending.is_none());
        row.advance(2.);
        assert!((row.spring.value + 150.).abs() < 0.001);
    }

    #[test]
    fn delayed_motion_is_independent_of_frame_rate() {
        let mut regular = DelayedSpring::new(0.);
        regular.retarget(-74., Some(0.09));
        for _ in 0..30 {
            regular.advance(1. / 60.);
        }
        let mut dropped = DelayedSpring::new(0.);
        dropped.retarget(-74., Some(0.09));
        dropped.advance(0.5);
        assert!((regular.spring.value - dropped.spring.value).abs() < 0.001);
        assert!((regular.spring.velocity - dropped.spring.velocity).abs() < 0.001);
    }

    #[test]
    fn gentle_overshoot_then_settle() {
        let mut spring = Spring::new(0.);
        let mut peak = 0_f32;
        for _ in 0..120 {
            spring.advance(1., 1. / 120.);
            peak = peak.max(spring.value);
        }
        assert!(peak > 1. && peak < 1.03);
        assert!((spring.value - 1.).abs() < 0.001);
        assert!(spring.velocity.abs() < 0.001);
    }

    #[test]
    fn retarget_preserves_velocity_and_settles() {
        let mut spring = Spring::new(0.);
        spring.advance(100., 0.1);
        let before = spring;
        spring.advance(-50., 0.);
        assert!((spring.value - before.value).abs() < 0.0001);
        assert!((spring.velocity - before.velocity).abs() < 0.001);
        spring.advance(-50., 2.);
        assert!((spring.value + 50.).abs() < 0.001);
    }

    #[test]
    fn independent_of_frame_rate() {
        let mut regular = Spring::new(0.);
        for _ in 0..30 {
            regular.advance(100., 1. / 60.);
        }
        let mut dropped = Spring::new(0.);
        dropped.advance(100., 0.5);
        assert!((regular.value - dropped.value).abs() < 0.001);
        assert!((regular.velocity - dropped.velocity).abs() < 0.001);
    }
}
