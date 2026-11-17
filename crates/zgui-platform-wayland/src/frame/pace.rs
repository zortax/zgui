//! When a redraw is delivered, and when it waits.

use std::time::Instant;

use crate::frame::visibility::Visibility;
use zgui_platform::Watchdog;

/// What this surface owes the compositor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owed {
    /// Nothing. The surface has committed nothing the compositor has yet to answer.
    Nothing,
    /// A frame callback, asked for at this moment with a commit that carried a buffer.
    Callback(Instant),
    /// A frame callback, asked for at this moment with a commit that carried no buffer. The
    /// first redraw that input asks for after it does not wait.
    Bare(Instant),
    /// A [`Owed::Bare`] callback whose redraw for input was taken. Further redraws wait for it.
    Spent(Instant),
}

impl Owed {
    /// When the callback owed was asked for, if one is owed.
    const fn since(self) -> Option<Instant> {
        match self {
            Self::Nothing => None,
            Self::Callback(since) | Self::Bare(since) | Self::Spent(since) => Some(since),
        }
    }
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
/// # A window that cannot keep up composes ahead
///
/// Waiting for the callback costs nothing to a frame that fits the interval, and it lets that frame
/// draw from the input that arrived during the wait. A frame that does not fit misses the refresh
/// it was started for, so every frame lands one refresh late and the period becomes a whole number
/// of intervals. Such a window composes its next frame while the callback is owed, and the frame
/// is *held*: it is presented when the callback arrives, so there is still one commit per callback,
/// and nothing more is composed until it is presented.
///
/// # Input after a frame that drew nothing does not wait
///
/// A redraw that ran and drew nothing commits no buffer, and the callback it asks for only keeps
/// the chain alive. The compositor has no new buffer from this surface for the coming refresh, so
/// a frame drawn now is still one buffer per refresh. A pointer motion and the press after it
/// often arrive in two reads: the motion's redraw draws nothing, and a press that waited for its
/// callback would show its frame one refresh late. So the first redraw that input asks for after
/// such a commit is delivered at once, and presented at once. Later redraws wait for the callback,
/// so a stream of input over a still surface draws once per refresh, as before.
///
/// # A press composes ahead
///
/// A press over a surface that animates finds a callback owed for a buffer. A press that waits
/// for that callback opens what it opens one refresh late, and a second surface the press shows
/// waits behind the first. So the first redraw a press asks for is composed at once and held: it
/// is presented when the callback arrives, so there is still one commit per callback. Motion does
/// not compose ahead, so an animation keeps its pace under a moving pointer.
///
/// # A new extent does not wait
///
/// A frame for an extent the compositor has just configured is drawn and presented at once, even
/// while a callback is owed. During a drag, a configure arrives on every refresh, and the frame
/// that waits for the callback commits just after the refresh, which puts every later configure
/// one refresh behind. The frame it replaces shows an older extent, so nothing is lost when only
/// the newer one is shown. The runtime answers at most one configure per refresh interval, so this
/// adds at most one commit per refresh.
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
    /// Whether a composed frame waits for the callback before it is presented.
    held: bool,
    /// Whether a frame may be composed while a callback is owed.
    ahead: bool,
    /// Whether input arrived since the last redraw was delivered.
    input: bool,
    /// Whether a press arrived since the last redraw was delivered.
    press: bool,
    /// Whether the last redraw delivered answered input.
    for_input: bool,
    /// Whether the next redraw answers a new extent, so it neither waits for nor is held behind an
    /// owed callback.
    resizing: bool,
}

impl Pacer {
    /// A surface that has committed nothing and been asked for nothing.
    pub fn new() -> Self {
        Self {
            owed: Owed::Nothing,
            wanted: false,
            watchdog: Watchdog::default(),
            abandoned: 0,
            held: false,
            ahead: false,
            input: false,
            press: false,
            for_input: false,
            resizing: false,
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

    /// Records that input arrived for this surface, see "Input after a frame that drew nothing
    /// does not wait".
    pub const fn input(&mut self) {
        self.input = true;
    }

    /// Records that a press arrived for this surface, see "A press composes ahead".
    pub const fn press(&mut self) {
        self.input = true;
        self.press = true;
    }

    /// Whether the last redraw delivered answered input.
    ///
    /// Such a redraw that draws nothing still commits and asks for a callback, so a stream of
    /// input over a still surface is paced by the display.
    pub const fn drew_for_input(&self) -> bool {
        self.for_input
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
        self.expire(visibility, now);
        // A surface the compositor has not configured may not be drawn into *at all*. That is not
        // the same as being hidden, and the two must not share a gate: attaching a buffer to a
        // surface that has never been configured is a protocol error, and the compositor answers it
        // by closing the connection. The request is kept, and the first configure delivers it.
        //
        // A held frame is the one frame allowed ahead of the compositor, so nothing more is
        // composed until it is presented.
        let owed = match self.owed {
            Owed::Nothing => false,
            Owed::Bare(_) => !self.input,
            Owed::Callback(_) => !self.press,
            Owed::Spent(_) => true,
        };
        let waiting = owed && !self.ahead && !self.resizing;
        if !self.wanted || !visibility.configured || self.held || waiting {
            return false;
        }
        if let Owed::Bare(since) = self.owed
            && self.input
        {
            self.owed = Owed::Spent(since);
        }
        self.wanted = false;
        self.for_input = self.input;
        self.input = false;
        self.press = false;
        true
    }

    /// Whether a frame that has just been composed may be presented now.
    ///
    /// Not while a callback for a buffer is owed: two buffers with no callback between them is a
    /// client racing ahead of the display, and only the second of them is ever shown. The frame
    /// is held instead, and [`Pacer::take_present`] hands it over when the callback arrives or is
    /// given up on. A callback asked for with no buffer does not hold a frame.
    pub fn present_now(&mut self, now: Instant) -> bool {
        let owed = !self.resizing
            && matches!(self.owed, Owed::Callback(since) if !self.watchdog.expired(since, now));
        self.held = owed;
        !owed
    }

    /// Whether the held frame should be presented now, taking it if so.
    ///
    /// Asked once per turn per surface, before [`Pacer::take`]: the held frame goes first, and
    /// the frame after it is composed while the compositor answers for it.
    pub fn take_present(&mut self, visibility: &mut Visibility, now: Instant) -> bool {
        self.expire(visibility, now);
        if !self.held || self.owes_callback() {
            return false;
        }
        self.held = false;
        true
    }

    /// Drops the held frame, which a new extent or scale made obsolete.
    pub const fn cancel_held(&mut self) {
        self.held = false;
    }

    /// Whether a frame callback is owed.
    pub const fn owes_callback(&self) -> bool {
        self.owed.since().is_some()
    }

    /// Whether a composed frame is waiting to be presented.
    pub const fn held(&self) -> bool {
        self.held
    }

    /// Lets a frame be composed while a callback is owed, or stops that.
    ///
    /// A window whose frames fit the interval waits for the callback, and draws from the input
    /// that arrived meanwhile. A window that cannot keep up composes its next frame during the
    /// wait instead, because waiting puts every frame onto the refresh after the one it could
    /// have made.
    pub const fn compose_ahead(&mut self, ahead: bool) {
        self.ahead = ahead;
    }

    /// Says whether the next redraw answers a new extent, see "A new extent does not wait".
    pub const fn answer_resize(&mut self, resizing: bool) {
        self.resizing = resizing;
    }

    /// Gives up on a callback that has been owed for too long.
    fn expire(&mut self, visibility: &mut Visibility, now: Instant) {
        if let Some(since) = self.owed.since()
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
    }

    /// Records that a commit went out and a callback was asked for with it.
    ///
    /// Every delivered redraw that *ran* ends here, whether it presented a frame or not. A turn
    /// that ends without a commit ends the callback chain, and the surface never draws again. A
    /// declined redraw never committed, so it never reaches this — the pacer keeps owing nothing,
    /// and the next request is taken the moment it is made.
    pub const fn committed(&mut self, now: Instant) {
        self.owed = Owed::Callback(now);
    }

    /// Records that a commit with no buffer went out and a callback was asked for with it.
    pub const fn committed_bare(&mut self, now: Instant) {
        self.owed = Owed::Bare(now);
    }

    /// Records that the commit a redraw was closed out for did not go out.
    ///
    /// No callback was asked for, so none is owed.
    pub const fn commit_withheld(&mut self) {
        self.owed = Owed::Nothing;
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
        self.owed
            .since()
            .is_some_and(|since| self.watchdog.expired(since, now))
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
    /// Nothing while nothing is owed, and nothing while neither a redraw nor a held frame is
    /// waiting. A surface that has
    /// committed its last frame and been asked for nothing further has no reason to be woken at
    /// all — including a hidden one, which comes back the moment the compositor next says anything
    /// about it: a configure, a focus, an output it entered. Waking anyway to ask would be a wake
    /// every fifth of a second, for every hidden window, for as long as the program runs — which
    /// is the cost the whole frame loop is built to avoid.
    pub fn deadline(&self, visibility: Visibility, now: Instant) -> Option<Instant> {
        let _ = (visibility, now);
        match self.owed.since() {
            Some(since) if self.wanted || self.held => Some(self.watchdog.expiry(since)),
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
    fn a_frame_for_a_new_extent_neither_waits_nor_is_held() {
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.committed(now);
        pacer.request();
        assert!(!pacer.take(&mut shown(), now), "an ordinary frame waits");
        pacer.answer_resize(true);
        assert!(pacer.take(&mut shown(), now));
        assert!(pacer.present_now(now), "and it is presented at once");
        assert!(!pacer.held());
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
    fn input_after_a_frame_that_drew_nothing_draws_and_presents_at_once() {
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.committed_bare(now);
        pacer.request();
        assert!(
            !pacer.take(&mut shown(), now),
            "a redraw nobody pressed for waits"
        );
        pacer.input();
        assert!(pacer.take(&mut shown(), now), "the press draws at once");
        assert!(pacer.present_now(now), "and its frame goes out at once");
        assert!(!pacer.held());
        pacer.committed(now);
        pacer.input();
        pacer.request();
        assert!(
            !pacer.take(&mut shown(), now),
            "the frame after a buffer waits for the callback"
        );
    }

    #[test]
    fn a_withheld_commit_owes_no_callback() {
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.committed_bare(now);
        pacer.commit_withheld();
        assert!(!pacer.owes_callback());
        pacer.request();
        assert!(
            pacer.take(&mut shown(), now),
            "the next redraw does not wait for a callback nobody asked for"
        );
    }

    #[test]
    fn input_over_a_still_surface_draws_once_per_callback() {
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.committed_bare(now);
        pacer.input();
        pacer.request();
        assert!(pacer.take(&mut shown(), now));
        // That frame drew nothing either, and committed nothing.
        pacer.input();
        pacer.request();
        assert!(
            !pacer.take(&mut shown(), now),
            "a second redraw for input waits for the callback"
        );
        assert!(pacer.owes_callback());
        pacer.callback();
        assert!(pacer.take(&mut shown(), now));
    }

    #[test]
    fn a_press_over_an_animating_surface_composes_ahead_and_is_held() {
        let mut pacer = Pacer::new();
        let now = Instant::now();
        let mut visibility = shown();
        pacer.committed(now);
        pacer.input();
        pacer.request();
        assert!(
            !pacer.take(&mut visibility, now),
            "motion waits for the callback"
        );
        pacer.press();
        assert!(
            pacer.take(&mut visibility, now),
            "the press composes at once"
        );
        assert!(
            !pacer.present_now(now),
            "and its frame waits for the callback"
        );
        pacer.press();
        pacer.request();
        assert!(
            !pacer.take(&mut visibility, now),
            "nothing is composed over the held frame"
        );
        pacer.callback();
        assert!(pacer.take_present(&mut visibility, now));
        pacer.committed(now);
        pacer.request();
        assert!(
            pacer.take(&mut visibility, now),
            "the press that waited behind the held frame composes ahead"
        );
        assert!(!pacer.present_now(now));
        pacer.callback();
        assert!(pacer.take_present(&mut visibility, now));
        pacer.committed(now);
        pacer.request();
        assert!(
            !pacer.take(&mut visibility, now),
            "the next frame waits for its callback"
        );
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

    #[test]
    fn a_frame_composed_while_a_callback_is_owed_is_held_until_it_arrives() {
        let mut pacer = Pacer::new();
        pacer.compose_ahead(true);
        let now = Instant::now();
        let mut visibility = shown();
        pacer.committed(now);
        pacer.request();
        assert!(
            pacer.take(&mut visibility, now),
            "the window composes ahead"
        );
        assert!(!pacer.present_now(now), "two commits for one callback");
        pacer.request();
        assert!(
            !pacer.take(&mut visibility, now),
            "a second frame was composed over the held one"
        );
        assert!(!pacer.take_present(&mut visibility, now));

        pacer.callback();
        assert!(pacer.take_present(&mut visibility, now));
        assert!(!pacer.take_present(&mut visibility, now), "presented twice");
        assert!(
            pacer.take(&mut visibility, now),
            "the next frame is composed"
        );
    }

    #[test]
    fn a_window_that_keeps_up_waits_for_the_callback_before_composing() {
        let mut pacer = Pacer::new();
        let now = Instant::now();
        pacer.committed(now);
        pacer.request();
        assert!(!pacer.take(&mut shown(), now));
    }

    #[test]
    fn a_held_frame_is_presented_when_its_callback_is_given_up_on() {
        let mut pacer = Pacer::new();
        pacer.watch(Watchdog::for_interval(Some(Duration::from_millis(100))));
        pacer.compose_ahead(true);
        let now = Instant::now();
        let mut visibility = shown();
        pacer.committed(now);
        assert!(!pacer.present_now(now));
        assert_eq!(
            pacer.deadline(visibility, now),
            Some(now + Duration::from_millis(400)),
            "the loop has to wake for the held frame"
        );
        assert!(!pacer.take_present(&mut visibility, now + Duration::from_millis(399)));
        assert!(pacer.take_present(&mut visibility, now + Duration::from_millis(400)));
        assert_eq!(visibility.abandoned, 1);
    }

    #[test]
    fn a_new_extent_drops_the_held_frame_and_lets_the_next_one_be_composed() {
        let mut pacer = Pacer::new();
        pacer.compose_ahead(true);
        let now = Instant::now();
        let mut visibility = shown();
        pacer.committed(now);
        assert!(!pacer.present_now(now));
        pacer.cancel_held();
        pacer.request();
        assert!(pacer.take(&mut visibility, now));
        pacer.callback();
        assert!(
            !pacer.take_present(&mut visibility, now),
            "the obsolete frame was presented"
        );
    }
}
