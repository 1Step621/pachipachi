use std::time::{Duration, Instant};

pub const BOUNCE_HEIGHT: f32 = 6.0;
pub const NEUTRAL_DURATION: Duration = Duration::from_millis(500);
pub const VISIBLE_DURATION: Duration = Duration::from_secs(2);
const FADE_DURATION: Duration = Duration::from_millis(300);
const RISE: Duration = Duration::from_millis(35);
const DURATION: Duration = Duration::from_millis(140);

#[derive(Default)]
pub struct TypingState {
    pub odd: bool,
    bounce: Option<(Instant, f32)>,
}

impl TypingState {
    pub fn is_idle(&self, now: Instant) -> bool {
        self.bounce
            .is_none_or(|(started, _)| now.saturating_duration_since(started) >= NEUTRAL_DURATION)
    }

    pub fn press(&mut self, now: Instant) {
        self.odd = !self.odd;
        // Repeated presses start from the current height and never accumulate displacement.
        let height = self.height(now);
        self.bounce = Some((now, height));
    }

    pub fn height(&self, now: Instant) -> f32 {
        let Some((started, initial)) = self.bounce else {
            return 0.0;
        };
        let elapsed = now.saturating_duration_since(started);
        if elapsed >= DURATION {
            return 0.0;
        }
        if elapsed < RISE {
            let t = elapsed.as_secs_f32() / RISE.as_secs_f32();
            initial + (BOUNCE_HEIGHT - initial) * (1.0 - (1.0 - t).powi(2))
        } else {
            let t = (elapsed - RISE).as_secs_f32() / (DURATION - RISE).as_secs_f32();
            BOUNCE_HEIGHT * (1.0 - t).powi(2)
        }
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.bounce.is_some_and(|(started, _)| {
            let elapsed = now.saturating_duration_since(started);
            elapsed < DURATION
                || (VISIBLE_DURATION..VISIBLE_DURATION + FADE_DURATION).contains(&elapsed)
        })
    }

    pub fn opacity(&self, now: Instant) -> f32 {
        let Some((started, _)) = self.bounce else {
            return 0.0;
        };
        let elapsed = now.saturating_duration_since(started);
        if elapsed <= VISIBLE_DURATION {
            return 1.0;
        }
        let t = ((elapsed - VISIBLE_DURATION).as_secs_f32() / FADE_DURATION.as_secs_f32()).min(1.0);
        // Smoothly ease into and out of the fade.
        1.0 - t * t * (3.0 - 2.0 * t)
    }

    pub fn is_visible(&self, now: Instant) -> bool {
        self.opacity(now) > 0.0
    }
}
