//! When a surface's frames reach the screen, and who decides that they do.

use std::time::{Duration, Instant};

/// Who waits for the display.
///
/// A graphics API asked to present in step with the display waits inside its own acquisition, on
/// whichever thread called it. That is the right answer when nothing else knows when a frame is
/// due, and the wrong one when the platform does: on a compositor that reports frame timing, the
/// wait lands on the thread that also reads input, and a surface the compositor stops drawing
/// blocks it for as long as the driver's timeout allows.
///
/// A backend that paces frames itself says so with [`PresentPacing::Platform`], and the renderer
/// then configures presentation that never blocks. Anything the display would have enforced is
/// enforced by the backend instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PresentPacing {
    /// The graphics API waits for the display.
    #[default]
    Display,
    /// The platform paces frames, and presentation must not block.
    Platform,
}

/// What the platform knows about when this surface's frames reach the screen.
///
/// Every field is optional and each is answered separately, because a platform can know the
/// refresh interval without knowing the phase, and can know when the last frame was shown without
/// being willing to predict the next one. A missing answer means unknown, never zero.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PresentationTiming {
    /// When the most recent frame was shown.
    pub last_presented: Option<Instant>,
    /// How long this surface's output takes between refreshes.
    pub interval: Option<Duration>,
    /// When the platform expects the next refresh.
    pub next_refresh: Option<Instant>,
}

impl PresentationTiming {
    /// The first refresh strictly after `now`, predicted from whatever is known.
    ///
    /// The platform's own prediction wins while it is still ahead. Otherwise the phase is walked
    /// forward from the last presentation by whole intervals, which is what makes a prediction that
    /// went stale while the loop was parked usable again rather than discarded.
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use zgui_platform::PresentationTiming;
    ///
    /// let now = Instant::now();
    /// let mut timing = PresentationTiming::default();
    /// assert_eq!(timing.refresh_after(now), None);
    ///
    /// timing.last_presented = Some(now - Duration::from_millis(25));
    /// timing.interval = Some(Duration::from_millis(10));
    /// assert_eq!(timing.refresh_after(now), Some(now + Duration::from_millis(5)));
    /// ```
    pub fn refresh_after(&self, now: Instant) -> Option<Instant> {
        if let Some(next) = self.next_refresh
            && next > now
        {
            return Some(next);
        }
        let last = self.last_presented?;
        let interval = self.interval.filter(|interval| !interval.is_zero())?;
        let elapsed = now.saturating_duration_since(last);
        let whole = elapsed.div_duration_f64(interval).floor() as u32;
        Some(last + interval * (whole + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::{PresentPacing, PresentationTiming};
    use std::time::{Duration, Instant};

    #[test]
    fn a_platform_that_says_nothing_predicts_nothing() {
        assert_eq!(
            PresentationTiming::default().refresh_after(Instant::now()),
            None
        );
    }

    #[test]
    fn a_prediction_that_is_still_ahead_is_the_answer() {
        let now = Instant::now();
        let timing = PresentationTiming {
            last_presented: Some(now),
            interval: Some(Duration::from_millis(10)),
            next_refresh: Some(now + Duration::from_millis(3)),
        };
        assert_eq!(
            timing.refresh_after(now),
            Some(now + Duration::from_millis(3))
        );
    }

    #[test]
    fn a_prediction_that_has_passed_is_walked_forward_by_whole_intervals() {
        // The loop parked through several refreshes. Discarding the phase would leave the next
        // frame unscheduled; walking it forward keeps it on the display's grid.
        let now = Instant::now();
        let timing = PresentationTiming {
            last_presented: Some(now - Duration::from_millis(95)),
            interval: Some(Duration::from_millis(10)),
            next_refresh: Some(now - Duration::from_millis(85)),
        };
        assert_eq!(
            timing.refresh_after(now),
            Some(now + Duration::from_millis(5))
        );
    }

    #[test]
    fn an_interval_of_zero_is_no_interval_at_all() {
        let now = Instant::now();
        let timing = PresentationTiming {
            last_presented: Some(now),
            interval: Some(Duration::ZERO),
            next_refresh: None,
        };
        assert_eq!(timing.refresh_after(now), None);
    }

    #[test]
    fn a_backend_that_says_nothing_lets_the_display_do_the_waiting() {
        assert_eq!(PresentPacing::default(), PresentPacing::Display);
    }
}
