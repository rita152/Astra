use std::time::{Duration, Instant};

/// ChatGPT's shared disclosure transition: 300 ms, cubic-bezier(.19, 1, .22, 1).
const DURATION: Duration = Duration::from_millis(300);

#[derive(Default)]
pub(super) struct DisclosureTransition {
    pub(super) value: f32,
    from: f32,
    target: f32,
    started: Option<Instant>,
}

impl DisclosureTransition {
    pub(super) fn sample(
        &mut self,
        expanded: bool,
        now: Instant,
        reduce_motion: bool,
    ) -> (f32, bool) {
        let target = if expanded { 1. } else { 0. };
        if reduce_motion {
            self.value = target;
            self.target = target;
            self.started = None;
            return (target, false);
        }
        if let Some(started) = self.started {
            let progress = (now.saturating_duration_since(started).as_secs_f32()
                / DURATION.as_secs_f32())
            .min(1.);
            self.value = self.from + (self.target - self.from) * ease(progress);
            if progress >= 1. {
                self.value = self.target;
                self.started = None;
            }
        }
        if self.target != target {
            self.from = self.value;
            self.target = target;
            self.started = Some(now);
        }
        (self.value.clamp(0., 1.), self.started.is_some())
    }
}

fn ease(x: f32) -> f32 {
    if x <= 0. || x >= 1. {
        return x.clamp(0., 1.);
    }
    let (mut lower, mut upper) = (0., 1.);
    for _ in 0..18 {
        let t = (lower + upper) * 0.5;
        let inverse = 1. - t;
        let current = 3. * inverse * inverse * t * 0.19 + 3. * inverse * t * t * 0.22 + t * t * t;
        if current < x {
            lower = t;
        } else {
            upper = t;
        }
    }
    let t = (lower + upper) * 0.5;
    1. - (1. - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disclosure_retargets_without_jumping_and_settles_at_both_endpoints() {
        let start = Instant::now();
        let mut transition = DisclosureTransition::default();
        assert_eq!(transition.sample(true, start, false), (0., true));
        let (half, active) = transition.sample(true, start + Duration::from_millis(100), false);
        assert!(active && half > 0. && half < 1.);
        assert_eq!(
            transition.sample(false, start + Duration::from_millis(100), false),
            (half, true)
        );
        assert_eq!(
            transition.sample(false, start + Duration::from_millis(400), false),
            (0., false)
        );
        assert_eq!(
            transition.sample(true, start + Duration::from_millis(400), true),
            (1., false)
        );
    }
}
