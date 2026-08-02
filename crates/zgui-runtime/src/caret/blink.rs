//! The caret's on-and-off phase, measured on the loop's own clock.

use std::time::{Duration, Instant};

/// How long the caret stays in one phase.
///
/// Half a second: fast enough to read as an insertion point rather than a stray rule, slow enough
/// that the frame it costs is one frame twice a second rather than one per refresh.
pub const HALF_PERIOD: Duration = Duration::from_millis(500);

/// How many phases a caret blinks for after it last moved before it stops blinking.
///
/// A caret that blinks for ever in a window nobody is typing into is a window that never parks:
/// two frames a second, for as long as the application is open, with nothing else moving. It
/// therefore settles **on**, which is the phase a caret that is not blinking has to be in — settling
/// off is a field with no visible insertion point at all.
pub const PHASES: u32 = 20;

/// The caret's phase, and the moment it next changes.
///
/// Restarted from whatever moved the caret rather than running free, because a caret that blinked
/// on its own schedule would go dark under the user's own typing: every keystroke would land in a
/// field whose insertion point is invisible half the time.
#[derive(Clone, Copy, Debug)]
pub struct Blink {
    /// When the caret last moved, which is the phase's origin.
    since: Option<Instant>,
}

impl Default for Blink {
    fn default() -> Self {
        Self::new()
    }
}

impl Blink {
    /// A caret that has never been placed.
    pub const fn new() -> Self {
        Self { since: None }
    }

    /// Restarts the phase, which is what typing and clicking do.
    ///
    /// The caret is on for the whole of the first half period after this, so the character just
    /// typed is followed by a visible insertion point rather than by a gap.
    pub fn restart(&mut self, now: Instant) {
        self.since = Some(now);
    }

    /// Forgets the phase, which is what losing the caret does.
    pub fn stop(&mut self) {
        self.since = None;
    }

    /// Whether a caret is being blinked at all.
    pub const fn is_running(&self) -> bool {
        self.since.is_some()
    }

    /// Which phase the caret is in.
    ///
    /// On for the first half period, off for the second, and on for good once it has blinked its
    /// count out. A caret that was never placed is off, because there is no caret.
    pub fn is_visible(&self, now: Instant) -> bool {
        let Some(since) = self.since else {
            return false;
        };
        let elapsed = now.saturating_duration_since(since);
        let phase = (elapsed.as_nanos() / HALF_PERIOD.as_nanos()) as u64;
        if phase >= u64::from(PHASES) {
            return true;
        }
        phase.is_multiple_of(2)
    }

    /// Which phase the caret is in, as the number that goes into a paint record.
    ///
    /// The *phase index* rather than the boolean, so that a fingerprint built from it moves on
    /// every flip even where two flips fall inside one frame.
    pub fn phase(&self, now: Instant) -> u64 {
        let Some(since) = self.since else {
            return 0;
        };
        let elapsed = now.saturating_duration_since(since);
        ((elapsed.as_nanos() / HALF_PERIOD.as_nanos()) as u64).min(u64::from(PHASES))
    }

    /// When the phase next changes, if it is ever going to change again.
    ///
    /// Nothing once the blinking has settled, which is what lets the loop park indefinitely over a
    /// focused field nobody is typing into.
    pub fn next_flip(&self, now: Instant) -> Option<Instant> {
        let since = self.since?;
        let elapsed = now.saturating_duration_since(since);
        let phase = (elapsed.as_nanos() / HALF_PERIOD.as_nanos()) as u64;
        if phase >= u64::from(PHASES) {
            return None;
        }
        Some(since + HALF_PERIOD.mul_f64((phase + 1) as f64))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Blink, HALF_PERIOD, PHASES};

    #[test]
    fn a_caret_that_was_never_placed_is_off_and_owes_no_frame() {
        let blink = Blink::new();
        let now = Instant::now();
        assert!(!blink.is_visible(now));
        assert_eq!(blink.next_flip(now), None);
        assert!(!blink.is_running());
    }

    #[test]
    fn the_phase_starts_on_and_alternates_and_the_deadline_is_the_next_edge() {
        let origin = Instant::now();
        let mut blink = Blink::new();
        blink.restart(origin);

        assert!(
            blink.is_visible(origin),
            "the caret just moved, so it is shown"
        );
        assert!(
            blink.is_visible(origin + HALF_PERIOD - Duration::from_millis(1)),
            "still inside the first phase"
        );
        assert!(
            !blink.is_visible(origin + HALF_PERIOD),
            "the second phase is off, so a caret that is always on is not a blink"
        );
        assert!(blink.is_visible(origin + HALF_PERIOD * 2));

        assert_eq!(blink.next_flip(origin), Some(origin + HALF_PERIOD));
        assert_eq!(
            blink.next_flip(origin + HALF_PERIOD),
            Some(origin + HALF_PERIOD * 2),
            "an expired edge must never be handed back, or the loop spins on it"
        );
    }

    #[test]
    fn the_phase_number_moves_on_every_flip_so_a_fingerprint_built_from_it_does_too() {
        let origin = Instant::now();
        let mut blink = Blink::new();
        blink.restart(origin);
        let phases: Vec<u64> = (0..4)
            .map(|step| blink.phase(origin + HALF_PERIOD * step))
            .collect();
        assert_eq!(phases, vec![0, 1, 2, 3]);
    }

    #[test]
    fn blinking_settles_on_rather_than_off_and_stops_asking_for_frames() {
        let origin = Instant::now();
        let mut blink = Blink::new();
        blink.restart(origin);
        let settled = origin + HALF_PERIOD * PHASES;
        assert!(
            blink.is_visible(settled),
            "a settled caret has to be the visible one: an invisible resting state is no caret"
        );
        assert!(
            blink.is_visible(settled + HALF_PERIOD * 7),
            "and it stays visible rather than resuming"
        );
        assert_eq!(
            blink.next_flip(settled),
            None,
            "a settled caret owes no further frame, or the window never parks"
        );
    }

    #[test]
    fn typing_restarts_the_phase_so_a_keystroke_is_never_swallowed_by_a_dark_caret() {
        let origin = Instant::now();
        let mut blink = Blink::new();
        blink.restart(origin);
        let dark = origin + HALF_PERIOD;
        assert!(!blink.is_visible(dark));
        blink.restart(dark);
        assert!(blink.is_visible(dark));
    }

    #[test]
    fn losing_the_caret_stops_the_blink_dead() {
        let origin = Instant::now();
        let mut blink = Blink::new();
        blink.restart(origin);
        blink.stop();
        assert!(!blink.is_visible(origin));
        assert_eq!(blink.next_flip(origin), None);
    }
}
