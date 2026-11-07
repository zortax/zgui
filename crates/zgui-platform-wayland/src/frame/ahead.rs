//! Whether a window composes its next frame before the compositor is ready for it.

use std::time::{Duration, Instant};

/// What the recent worst redraw cost is multiplied by on each redraw.
///
/// The same decay as the runtime's present servo: a window that slowed down is believed at once,
/// and one that sped up is believed over about thirty-five frames.
const DECAY: f32 = 0.98;

/// How many commits in a row may miss the refresh after them before the window composes ahead.
///
/// One is an accident: an idle window's first commit can land just after the compositor's deadline.
/// Two in a row while frames keep coming is a window that waits for the callback and then cannot
/// finish in what is left of the interval.
const MISSED_ENOUGH: u32 = 2;

/// How long a window that missed refreshes keeps composing ahead.
///
/// A miss is seen only while the window waits for the callback, so a window that composes ahead
/// never shows one. It tries waiting again after this long, which costs at most
/// [`MISSED_ENOUGH`] late frames each time when the frames still do not fit.
const MISSED_HOLD: Duration = Duration::from_secs(1);

/// The margin a frame keeps before the compositor's deadline: a sixth of the interval, within fixed
/// bounds. The same figure as the runtime's present servo.
fn margin(interval: Duration) -> Duration {
    (interval / 6).clamp(Duration::from_micros(500), Duration::from_millis(2))
}

/// What `ZGUI_WAYLAND_COMPOSE_AHEAD` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Compose ahead when the window cannot keep up. The default.
    Auto,
    /// Always compose ahead, for comparison runs.
    Always,
    /// Never compose ahead, for comparison runs.
    Never,
}

impl Mode {
    /// The mode the environment asks for, read once.
    pub fn from_environment() -> Self {
        static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            match std::env::var("ZGUI_WAYLAND_COMPOSE_AHEAD")
                .as_deref()
                .map(str::trim)
            {
                Ok("always") => Self::Always,
                Ok("never") => Self::Never,
                _ => Self::Auto,
            }
        })
    }
}

/// Whether a surface's frames fit the interval it is presented at.
///
/// Two observations, because neither sees every case. The **redraw cost** is the processor time
/// of one delivered redraw. It predicts a miss before one happens, but it does not include the
/// graphics device's time. The **commit-to-callback latency** is how long the compositor took to
/// answer a commit. A commit that missed the refresh after it is answered more than one interval
/// later, whatever made it late.
#[derive(Clone, Copy, Debug)]
pub struct Ahead {
    /// What the environment asked for.
    mode: Mode,
    /// What a redraw recently cost at its worst, decaying towards what one costs now.
    worst: Duration,
    /// How many answered commits in a row missed the refresh after them.
    missed: u32,
    /// Until when the window composes ahead because of missed refreshes.
    open_until: Option<Instant>,
}

impl Ahead {
    /// A surface with nothing observed, in `mode`.
    pub const fn new(mode: Mode) -> Self {
        Self {
            mode,
            worst: Duration::ZERO,
            missed: 0,
            open_until: None,
        }
    }

    /// Records the processor time of a redraw that ran.
    pub fn redrawn(&mut self, cost: Duration) {
        self.worst = cost.max(self.worst.mul_f32(DECAY));
    }

    /// Records the compositor answering a commit `latency` after it was made.
    pub fn answered(&mut self, latency: Duration, interval: Option<Duration>, now: Instant) {
        let Some(interval) = interval else {
            return;
        };
        if latency > interval + interval / 8 {
            self.missed += 1;
        } else {
            self.missed = 0;
        }
        if self.missed >= MISSED_ENOUGH {
            self.open_until = Some(now + MISSED_HOLD);
        }
    }

    /// Whether the window composes ahead of an owed callback now.
    ///
    /// An unknown interval keeps the window waiting, which is how every frame was paced before
    /// anything was measured.
    pub fn wanted(&self, interval: Option<Duration>, now: Instant) -> bool {
        match self.mode {
            Mode::Always => true,
            Mode::Never => false,
            Mode::Auto => {
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
    use super::{Ahead, MISSED_HOLD, Mode};
    use std::time::{Duration, Instant};

    const INTERVAL: Duration = Duration::from_micros(8_333);

    #[test]
    fn a_window_whose_frames_fit_waits_for_the_callback() {
        let mut ahead = Ahead::new(Mode::Auto);
        let now = Instant::now();
        for _ in 0..10 {
            ahead.redrawn(Duration::from_millis(4));
            ahead.answered(INTERVAL - Duration::from_millis(4), Some(INTERVAL), now);
        }
        assert!(!ahead.wanted(Some(INTERVAL), now));
    }

    #[test]
    fn a_frame_that_costs_the_interval_composes_ahead_at_once() {
        let mut ahead = Ahead::new(Mode::Auto);
        ahead.redrawn(Duration::from_millis(8));
        assert!(ahead.wanted(Some(INTERVAL), Instant::now()));
    }

    #[test]
    fn a_cheap_frame_after_a_dear_one_is_believed_slowly() {
        let mut ahead = Ahead::new(Mode::Auto);
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
    fn commits_that_miss_the_refresh_compose_ahead_for_a_while() {
        // A frame whose processor time fits, held up by something the processor does not see.
        let mut ahead = Ahead::new(Mode::Auto);
        let now = Instant::now();
        ahead.redrawn(Duration::from_millis(3));
        let late = 2 * INTERVAL - Duration::from_millis(3);
        ahead.answered(late, Some(INTERVAL), now);
        assert!(
            !ahead.wanted(Some(INTERVAL), now),
            "one miss is an accident"
        );
        ahead.answered(late, Some(INTERVAL), now);
        assert!(ahead.wanted(Some(INTERVAL), now));
        assert!(!ahead.wanted(Some(INTERVAL), now + MISSED_HOLD));
    }

    #[test]
    fn an_unknown_interval_keeps_the_window_waiting() {
        let mut ahead = Ahead::new(Mode::Auto);
        ahead.redrawn(Duration::from_millis(50));
        assert!(!ahead.wanted(None, Instant::now()));
    }

    #[test]
    fn the_environment_overrides_what_was_measured() {
        let now = Instant::now();
        assert!(Ahead::new(Mode::Always).wanted(None, now));
        let mut never = Ahead::new(Mode::Never);
        never.redrawn(Duration::from_millis(50));
        assert!(!never.wanted(Some(INTERVAL), now));
    }
}
