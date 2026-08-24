//! When a redraw is delivered, and when it waits.

use std::time::Instant;

use crate::frame::visibility::Visibility;
use zgui_platform::Watchdog;

/// What this surface owes the compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owed {
    /// Nothing. The surface has committed nothing the compositor has yet to answer.
    Nothing,
    /// A frame callback, asked for at this moment and not yet arrived.
    Callback(Instant),
}

/// Whether a surface may be drawn now, and what the loop should wait for if not.
///
/// # The rule
///
/// A Wayland client is told when to draw. It asks for a `wl_surface.frame` callback before the
/// commit that carries a frame, and the compositor answers when it is ready for the next one. Two
/// commits with no callback in between is a client racing ahead of the display for no benefit,
/// because only the last of them is ever shown.
///
/// So a surface with a callback owed waits. A surface with nothing owed does **not**: it draws in
/// the same turn the redraw was asked for. That second half is the whole difference from a
/// portable backend that turns the callback into a lock on redraw delivery — a window that has
/// been quiet answers the first click immediately rather than after a round trip through the
/// compositor.
///
/// # Not configured is a reason to refuse a frame; hidden is not
///
/// The two look alike and are opposites. A surface the compositor has never configured may not be
/// drawn into at all — attaching a buffer to one is a protocol error, and a compositor answers it
/// by closing the connection — so a redraw asked for before the first configure is kept and
/// delivered by that configure.
///
/// # Being hidden is not a reason to refuse a frame
///
/// A surface the compositor has stopped drawing is *reported* hidden and is not gated here. The
/// contract is explicit that a hidden surface must still be given the frames a timer asks for —
/// work waiting on a timer behind a minimised window otherwise never resumes — and the decision
/// about what a hidden window is worth belongs to the application, which already makes it.
///
/// What the platform does instead is keep the conversation alive. A hidden surface's redraw ends
/// in a commit like any other, that commit asks for a callback like any other, and the answer or
/// the silence that follows is what decides whether it is still hidden.
///
/// # The compositor that never answers
///
/// A callback that is asked for and never arrives is [`Watchdog`]'s: after long enough the
/// obligation is dropped and the next redraw runs. The probe costs one frame and cannot cost a
/// stall, because presentation on this backend never waits.
///
/// A run of those is also the only occlusion signal that works everywhere — see
/// [`Visibility`] — which is why giving up on one is recorded there rather than only counted here.
#[derive(Clone, Copy, Debug)]
pub struct Pacer {
    /// What the compositor has yet to answer.
    owed: Owed,
    /// Whether a redraw has been asked for and not yet delivered.
    wanted: bool,
    /// How long a callback may be owed.
    watchdog: Watchdog,
    /// How many callbacks were given up on rather than waited for.
    abandoned: u64,
}

impl Pacer {
    /// A surface that has committed nothing and been asked for nothing.
    pub fn new() -> Self {
        Self {
            owed: Owed::Nothing,
            wanted: false,
            watchdog: Watchdog::default(),
            abandoned: 0,
        }
    }

    /// Sets how long a callback may be owed, from the interval the output refreshes at.
    pub const fn watch(&mut self, watchdog: Watchdog) {
        self.watchdog = watchdog;
    }

    /// Records that something asked for a frame.
    pub const fn request(&mut self) {
        self.wanted = true;
    }

    /// Whether a redraw is waiting to be delivered.
    pub const fn wanted(&self) -> bool {
        self.wanted
    }

    /// Whether a redraw should be delivered now, taking it if so.
    ///
    /// Asked once per turn per surface, after everything the compositor said has been applied.
    /// `visibility` is read *and written*: a callback given up on is the evidence the compositor
    /// has stopped drawing this surface, and there is nowhere else that evidence appears.
    pub fn take(&mut self, visibility: &mut Visibility, now: Instant) -> bool {
        if let Owed::Callback(since) = self.owed
            && self.watchdog.expired(since, now)
        {
            // The compositor was asked for a callback and has not answered in long enough that it
            // is not going to. Waiting further is the freeze; drawing again is one wasted frame.
            self.owed = Owed::Nothing;
            self.abandoned += 1;
            visibility.unanswered();
            tracing::debug!(
                grace_ms = self.watchdog.grace().as_millis() as u64,
                abandoned = self.abandoned,
                "no frame callback arrived in time; drawing without one"
            );
        }
        // A surface the compositor has not configured may not be drawn into *at all*. That is not
        // the same as being hidden, and the two must not share a gate: attaching a buffer to a
        // surface that has never been configured is a protocol error, and the compositor answers it
        // by closing the connection. The request is kept, and the first configure delivers it.
        if !self.wanted || !visibility.configured || matches!(self.owed, Owed::Callback(_)) {
            return false;
        }
        self.wanted = false;
        true
    }

    /// Records that a commit went out and a callback was asked for with it.
    ///
    /// Every delivered redraw ends here, whether it presented a frame or not. A turn that ends
    /// without a commit ends the callback chain, and the surface never draws again.
    pub const fn committed(&mut self, now: Instant) {
        self.owed = Owed::Callback(now);
    }

    /// Records the compositor answering with a frame callback.
    pub const fn callback(&mut self) {
        self.owed = Owed::Nothing;
    }

    /// Whether a callback is owed and has been owed for too long.
    ///
    /// Read by the loop when it has to decide about a surface it is not going to draw — a hidden
    /// one — because the run of unanswered frames is what eventually decides it is visible again.
    pub fn is_overdue(&self, now: Instant) -> bool {
        matches!(self.owed, Owed::Callback(since) if self.watchdog.expired(since, now))
    }

    /// Records the surface becoming hidden or visible.
    ///
    /// Becoming visible asks for the frame that shows what changed while it was not being drawn.
    /// Whatever was owed is cleared either way: a callback asked for before the surface stopped
    /// being drawn will never arrive, and waiting for it would leave a visible surface stopped.
    pub const fn hidden(&mut self, hidden: bool) {
        self.owed = Owed::Nothing;
        if !hidden {
            self.wanted = true;
        }
    }

    /// When the loop must wake to give up on a callback that has not arrived.
    ///
    /// Nothing while nothing is owed, and nothing while no redraw is waiting. A surface that has
    /// committed its last frame and been asked for nothing further has no reason to be woken at
    /// all — including a hidden one, which comes back the moment the compositor next says anything
    /// about it: a configure, a focus, an output it entered. Waking anyway to ask would be a wake
    /// every fifth of a second, for every hidden window, for as long as the program runs — which
    /// is the cost the whole frame loop is built to avoid.
    pub fn deadline(&self, visibility: Visibility, now: Instant) -> Option<Instant> {
        let _ = (visibility, now);
        match self.owed {
            Owed::Callback(since) if self.wanted => Some(self.watchdog.expiry(since)),
            _ => None,
        }
    }

    /// How many callbacks were given up on rather than waited for.
    pub const fn abandoned(&self) -> u64 {
        self.abandoned
    }
}

impl Default for Pacer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Pacer;
    use crate::frame::visibility::Visibility;
    use std::time::{Duration, Instant};
    use zgui_platform::Watchdog;

    fn shown() -> Visibility {
        Visibility {
            configured: true,
            ..Visibility::default()
        }
    }

    fn hidden() -> Visibility {
        Visibility {
            configured: true,
            suspended: true,
            ..Visibility::default()
        }
    }

    #[test]
    fn a_quiet_surface_draws_in_the_turn_it_was_asked() {
        // This is the whole latency difference from a backend that withholds the redraw until the
        // compositor answers: there is nothing to answer, so there is nothing to wait for.
        let mut pacer = Pacer::new();
        let now = Instant::now();
        assert!(!pacer.take(&mut shown(), now));
        pacer.request();
        assert!(pacer.take(&mut shown(), now));
        assert!(!pacer.take(&mut shown(), now), "the request was consumed");
    }

    #[test]
    fn a_surface_that_has_committed_waits_for_the_compositor() {
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.request();
        assert!(pacer.take(&mut shown(), now));
        pacer.committed(now);

        pacer.request();
        assert!(!pacer.take(&mut shown(), now + Duration::from_millis(1)));
        pacer.callback();
        assert!(pacer.take(&mut shown(), now + Duration::from_millis(2)));
    }

    #[test]
    fn a_request_made_while_waiting_is_kept_and_answered_on_the_callback() {
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.committed(now);
        pacer.request();
        pacer.request();
        pacer.request();
        pacer.callback();
        assert!(pacer.take(&mut shown(), now));
        assert!(
            !pacer.take(&mut shown(), now),
            "three requests produced one frame"
        );
    }

    #[test]
    fn a_surface_the_compositor_has_not_configured_is_never_drawn_into() {
        // Attaching a buffer to one is a protocol error, and the compositor answers it by closing
        // the connection — which is a window that vanishes rather than a frame that looks wrong.
        // Found by a compositor that configures later than another, which is the whole reason a
        // property like this is asserted rather than reasoned about.
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.request();
        assert!(!pacer.take(&mut Visibility::default(), now));
        assert!(
            pacer.wanted(),
            "the request is kept for the first configure"
        );
        assert!(pacer.take(&mut shown(), now));
    }

    #[test]
    fn a_hidden_surface_is_still_given_the_frames_something_asked_for() {
        // The contract is explicit: work waiting on a timer behind a minimised window has to
        // resume, and what a hidden window is worth is the application's decision rather than the
        // platform's. What the platform must not do is stop the conversation.
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.request();
        assert!(pacer.take(&mut hidden(), now));
    }

    #[test]
    fn a_surface_nobody_is_asking_about_is_never_woken_for() {
        // Including a hidden one. It comes back when the compositor next says anything about it —
        // a configure, a focus, an output it entered — and waking anyway to ask would be a wake
        // every fifth of a second, for every hidden window, for as long as the program runs.
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.committed(now);
        assert_eq!(pacer.deadline(shown(), now), None);
        assert_eq!(pacer.deadline(hidden(), now), None);
    }

    #[test]
    fn becoming_visible_again_asks_for_the_frame_that_restarts_the_chain() {
        // The callback asked for before the surface was hidden will never arrive. Keeping the
        // obligation would leave a visible surface waiting on an answer that is not coming.
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.committed(now);
        pacer.hidden(true);
        pacer.hidden(false);
        assert!(pacer.take(&mut shown(), now));
    }

    #[test]
    fn a_callback_that_never_arrives_is_given_up_on_rather_than_waited_for() {
        let mut pacer = Pacer::new();
        pacer.watch(Watchdog::for_interval(Some(Duration::from_millis(100))));
        let now = Instant::now();
        pacer.committed(now);
        pacer.request();

        assert!(!pacer.take(&mut shown(), now + Duration::from_millis(399)));
        assert_eq!(pacer.abandoned(), 0);
        assert!(pacer.take(&mut shown(), now + Duration::from_millis(400)));
        assert_eq!(pacer.abandoned(), 1);
    }

    #[test]
    fn the_loop_is_told_when_to_wake_to_give_up() {
        let mut pacer = Pacer::new();
        pacer.watch(Watchdog::for_interval(Some(Duration::from_millis(100))));
        let now = Instant::now();

        // Nothing owed and nothing wanted: no reason to wake at all.
        assert_eq!(pacer.deadline(shown(), now), None);
        pacer.committed(now);
        assert_eq!(
            pacer.deadline(shown(), now),
            None,
            "still nothing is asking"
        );
        pacer.request();
        assert_eq!(
            pacer.deadline(shown(), now),
            Some(now + Duration::from_millis(400)),
            "a callback is owed and a frame is waiting on it"
        );
    }

    #[test]
    fn giving_up_on_a_callback_is_recorded_where_visibility_is_decided() {
        // A run of them is the only occlusion signal that needs no protocol version and no
        // cooperation, so the evidence has to reach the place that decides.
        let mut pacer = Pacer::new();
        pacer.watch(Watchdog::for_interval(Some(Duration::from_millis(100))));
        let now = Instant::now();
        let mut visibility = shown();
        pacer.committed(now);
        pacer.request();
        pacer.take(&mut visibility, now + Duration::from_millis(400));
        assert_eq!(visibility.abandoned, 1);
    }
}
