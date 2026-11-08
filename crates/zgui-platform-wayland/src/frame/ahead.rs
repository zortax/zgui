//! Whether a window composes its next frame before the compositor is ready for it.

use std::time::{Duration, Instant};

use zgui_platform::ComposeAhead;

/// What the recent worst redraw cost is multiplied by on each redraw.
///
/// The same decay as the runtime's present servo: a window that slowed down is believed at once,
/// and one that sped up is believed over about thirty-five frames.
const DECAY: f32 = 0.98;

/// How many of the last [`WINDOW`] chained frames must miss their refresh before the window
/// composes ahead.
///
/// One is an accident: an idle window's first commit can land just after the compositor's deadline.
/// Two in a few frames is a window that waits for the callback and then cannot finish in what is
/// left of the interval. The misses need not be consecutive, because a frame whose cost is near
/// the limit misses about every second refresh.
const MISSED_ENOUGH: u32 = 2;

/// How many recent chained frames [`MISSED_ENOUGH`] counts over.
const WINDOW: u32 = 4;

/// How long a window that missed refreshes first keeps composing ahead.
///
/// A miss is seen only while the window waits for the callback, so a window that composes ahead
/// never shows one. It tries waiting again after the hold, which costs a few late frames when the
/// frames still do not fit.
const MISSED_HOLD: Duration = Duration::from_secs(1);

/// The longest hold. Each try that fails at once doubles the hold, up to this.
const MAX_HOLD: Duration = Duration::from_secs(8);

/// How soon after a hold ends a new run of misses counts as a failed try.
const PROBE: Duration = Duration::from_millis(250);

/// The margin a frame keeps before the compositor's deadline: a sixth of the interval, within fixed
/// bounds. The same figure as the runtime's present servo.
fn margin(interval: Duration) -> Duration {
    (interval / 6).clamp(Duration::from_micros(500), Duration::from_millis(2))
}

/// The mode a surface that asked for `asked` runs in.
///
/// `ZGUI_WAYLAND_COMPOSE_AHEAD` (`auto`, `always` or `never`) overrides what the application asked
/// for, for comparison runs. The environment is read once.
pub fn mode(asked: ComposeAhead) -> ComposeAhead {
    static OVERRIDE: std::sync::OnceLock<Option<ComposeAhead>> = std::sync::OnceLock::new();
    OVERRIDE
        .get_or_init(|| {
            match std::env::var("ZGUI_WAYLAND_COMPOSE_AHEAD")
                .as_deref()
                .map(str::trim)
            {
                Ok("auto") => Some(ComposeAhead::Auto),
                Ok("always") => Some(ComposeAhead::Always),
                Ok("never") => Some(ComposeAhead::Never),
                _ => None,
            }
        })
        .unwrap_or(asked)
}

/// Whether a surface's frames fit the interval it is presented at.
///
/// Two observations, because neither sees every case. The **redraw cost** is the processor time
/// of one delivered redraw. It predicts a miss before one happens, but it does not include the
/// graphics device's time. The **callback period** is the time from the callback that started a
/// frame to the callback that answers it. A frame that fits is answered one interval later, and one
/// that missed its refresh two intervals later, whatever made it late.
#[derive(Clone, Copy, Debug)]
pub struct Ahead {
    /// What the application or the environment asked for.
    mode: ComposeAhead,
    /// What a redraw recently cost at its worst, decaying towards what one costs now.
    worst: Duration,
    /// Whether each of the recent chained frames missed its refresh, newest in the lowest bit.
    recent: u8,
    /// Until when the window composes ahead because of missed refreshes.
    open_until: Option<Instant>,
    /// How long the next run of misses keeps the window composing ahead.
    hold: Duration,
}

impl Ahead {
    /// A surface with nothing observed, in `mode`.
    pub const fn new(mode: ComposeAhead) -> Self {
        Self {
            mode,
            worst: Duration::ZERO,
            recent: 0,
            open_until: None,
            hold: MISSED_HOLD,
        }
    }

    /// Records the processor time of a redraw that ran.
    pub fn redrawn(&mut self, cost: Duration) {
        self.worst = cost.max(self.worst.mul_f32(DECAY));
    }

    /// Records a chained frame answered `period` after the callback that started it.
    pub fn answered(&mut self, period: Duration, interval: Option<Duration>, now: Instant) {
        let Some(interval) = interval else {
            return;
        };
        let missed = period > interval + interval / 2;
        self.recent = ((self.recent << 1) | u8::from(missed)) & ((1 << WINDOW) - 1);
        if self.recent.count_ones() < MISSED_ENOUGH {
            return;
        }
        self.recent = 0;
        match self.open_until {
            // Composing ahead and still late: the frames do not fit even then.
            Some(until) if until > now => self.open_until = Some(until.max(now + self.hold)),
            // The window tried waiting again and missed at once.
            Some(until) if now <= until + PROBE => {
                self.hold = (self.hold * 2).min(MAX_HOLD);
                self.open_until = Some(now + self.hold);
            }
            _ => {
                self.hold = MISSED_HOLD;
                self.open_until = Some(now + self.hold);
            }
        }
    }

    /// Whether the window composes ahead of an owed callback now.
    ///
    /// An unknown interval keeps the window waiting, which is how every frame was paced before
    /// anything was measured.
    pub fn wanted(&self, interval: Option<Duration>, now: Instant) -> bool {
        match self.mode {
            ComposeAhead::Always => true,
            ComposeAhead::Never => false,
            _ => {
                let Some(interval) = interval else {
                    return false;
                };
                self.worst + margin(interval) >= interval
                    || self.open_until.is_some_and(|until| until > now)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Ahead, MAX_HOLD, MISSED_HOLD};
    use std::time::{Duration, Instant};
    use zgui_platform::ComposeAhead;

    const INTERVAL: Duration = Duration::from_micros(8_333);

    /// A chained frame that made its refresh.
    const FIT: Duration = INTERVAL;

    /// A chained frame that missed its refresh.
    const MISS: Duration = Duration::from_micros(16_666);

    /// Feeds `periods` to `ahead`, one per refresh from `now`, and returns when the last one came.
    fn answer(ahead: &mut Ahead, now: Instant, periods: &[Duration]) -> Instant {
        let mut at = now;
        for &period in periods {
            at += period;
            ahead.answered(period, Some(INTERVAL), at);
        }
        at
    }

    #[test]
    fn a_window_whose_frames_fit_waits_for_the_callback() {
        let mut ahead = Ahead::new(ComposeAhead::Auto);
        let now = Instant::now();
        for _ in 0..10 {
            ahead.redrawn(Duration::from_millis(4));
        }
        let at = answer(&mut ahead, now, &[FIT; 10]);
        assert!(!ahead.wanted(Some(INTERVAL), at));
    }

    #[test]
    fn a_frame_that_costs_the_interval_composes_ahead_at_once() {
        let mut ahead = Ahead::new(ComposeAhead::Auto);
        ahead.redrawn(Duration::from_millis(8));
        assert!(ahead.wanted(Some(INTERVAL), Instant::now()));
    }

    #[test]
    fn a_cheap_frame_after_a_dear_one_is_believed_slowly() {
        let mut ahead = Ahead::new(ComposeAhead::Auto);
        let now = Instant::now();
        ahead.redrawn(Duration::from_millis(9));
        for _ in 0..5 {
            ahead.redrawn(Duration::from_millis(2));
        }
        assert!(ahead.wanted(Some(INTERVAL), now));
        for _ in 0..200 {
            ahead.redrawn(Duration::from_millis(2));
        }
        assert!(!ahead.wanted(Some(INTERVAL), now));
    }

    #[test]
    fn frames_that_miss_every_second_refresh_compose_ahead() {
        // A frame whose processor time fits, held up by something the processor does not see.
        let mut ahead = Ahead::new(ComposeAhead::Auto);
        let now = Instant::now();
        ahead.redrawn(Duration::from_millis(3));
        let at = answer(&mut ahead, now, &[MISS]);
        assert!(!ahead.wanted(Some(INTERVAL), at), "one miss is an accident");
        let at = answer(&mut ahead, at, &[FIT, MISS]);
        assert!(ahead.wanted(Some(INTERVAL), at));
        assert!(!ahead.wanted(Some(INTERVAL), at + MISSED_HOLD));
    }

    #[test]
    fn misses_far_apart_are_accidents() {
        let mut ahead = Ahead::new(ComposeAhead::Auto);
        let now = Instant::now();
        let at = answer(
            &mut ahead,
            now,
            &[MISS, FIT, FIT, FIT, MISS, FIT, FIT, FIT, MISS],
        );
        assert!(!ahead.wanted(Some(INTERVAL), at));
    }

    #[test]
    fn a_try_at_waiting_that_fails_at_once_doubles_the_hold() {
        let mut ahead = Ahead::new(ComposeAhead::Auto);
        let now = Instant::now();
        let opened = answer(&mut ahead, now, &[MISS, MISS]);
        let mut hold = MISSED_HOLD;
        assert!(!ahead.wanted(Some(INTERVAL), opened + hold));
        let mut at = opened;
        for _ in 0..5 {
            at = answer(&mut ahead, at + hold, &[MISS, MISS]);
            hold = (hold * 2).min(MAX_HOLD);
            assert!(ahead.wanted(Some(INTERVAL), at + hold - INTERVAL));
            assert!(!ahead.wanted(Some(INTERVAL), at + hold));
        }
        assert_eq!(hold, MAX_HOLD);

        // A try that succeeds for a while starts the next run of misses from the first hold.
        let fits = answer(&mut ahead, at + hold, &[FIT; 100]);
        let at = answer(&mut ahead, fits, &[MISS, MISS]);
        assert!(ahead.wanted(Some(INTERVAL), at + MISSED_HOLD - INTERVAL));
        assert!(!ahead.wanted(Some(INTERVAL), at + MISSED_HOLD));
    }

    #[test]
    fn misses_while_composing_ahead_extend_the_hold_without_doubling_it() {
        let mut ahead = Ahead::new(ComposeAhead::Auto);
        let now = Instant::now();
        let opened = answer(&mut ahead, now, &[MISS, MISS]);
        let at = answer(&mut ahead, opened, &[MISS, MISS]);
        assert!(ahead.wanted(Some(INTERVAL), at + MISSED_HOLD - INTERVAL));
        assert!(!ahead.wanted(Some(INTERVAL), at + MISSED_HOLD));
    }

    #[test]
    fn an_unknown_interval_keeps_the_window_waiting() {
        let mut ahead = Ahead::new(ComposeAhead::Auto);
        ahead.redrawn(Duration::from_millis(50));
        assert!(!ahead.wanted(None, Instant::now()));
    }

    #[test]
    fn a_fixed_mode_overrides_what_was_measured() {
        let now = Instant::now();
        assert!(Ahead::new(ComposeAhead::Always).wanted(None, now));
        let mut never = Ahead::new(ComposeAhead::Never);
        never.redrawn(Duration::from_millis(50));
        assert!(!never.wanted(Some(INTERVAL), now));
    }
}
