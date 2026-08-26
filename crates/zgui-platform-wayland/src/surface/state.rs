//! Everything about one surface that changes, behind one lock.

use std::time::{Duration, Instant};

use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::{PresentationTiming, SurfaceEvent};

use crate::frame::{Pacer, Timing, Visibility, Watchdog};

/// The smallest and largest extents a window may be dragged to, in that order.
///
/// A pair rather than two fields, because the shell takes them as two requests that have to agree:
/// a window is made unresizable by setting both to what it is at, and stating only one afterwards
/// would leave the other pinned there.
pub(crate) type Bounds = (Option<Size<CssPx, Css>>, Option<Size<CssPx, Css>>);

/// How a delivered redraw ended, and what the surface owes the compositor for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndOfRedraw {
    /// A buffer was committed, and the frame callback rode that commit.
    Presented,
    /// The redraw ran and presented nothing; a bufferless commit must keep the chain alive.
    KeepChainAlive,
    /// The redraw was refused without running; nothing is committed and nothing is owed.
    Declined,
}

/// The mutable half of a surface.
///
/// It is one lock rather than several because every field is read or written in the same two
/// places — a protocol event arriving, and a frame being delivered — and a surface split across
/// several locks can be observed half-updated by the application between them: a size from after
/// the configure with the scale from before it, which is a frame drawn at the wrong extent.
///
/// The lock is never held across a call into the application. Everything the loop needs is copied
/// out first, because the application answers by asking the surface its size, and a surface that
/// held its own lock while asking would deadlock on the first frame.
#[derive(Debug)]
pub(crate) struct Shared {
    /// The drawable extent, in physical pixels.
    pub(crate) size: Size<DevicePx, Device>,
    /// The extent the compositor configured, in logical pixels.
    pub(crate) logical: Size<CssPx, Css>,
    /// How many physical pixels there are to a logical one.
    pub(crate) scale: f64,
    /// Whether a redraw is owed, and what the compositor owes back.
    pub(crate) pacer: Pacer,
    /// When frames reached the screen.
    pub(crate) timing: Timing,
    /// Whether the compositor is still drawing this surface.
    pub(crate) visibility: Visibility,
    /// Whether the application has been told the surface is hidden.
    ///
    /// A surface starts hidden, because it starts unconfigured: nothing may be drawn into it and
    /// nothing about it is worth an animation. The first edge is therefore the one that says the
    /// compositor has accepted it.
    pub(crate) told_hidden: bool,
    /// Whether a buffer was committed during the redraw now being delivered.
    pub(crate) presented: bool,
    /// Whether the redraw now being delivered was refused without running.
    pub(crate) declined: bool,
    /// Whether any buffer has ever been committed.
    ///
    /// What separates a configure that restates the extent from the one that maps the surface: the
    /// first configure has to be answered with a frame or the surface is never shown, and every
    /// later restatement is a frame identical to the one already on the screen.
    pub(crate) mapped: bool,
    /// A viewport destination that has not yet ridden a commit.
    pub(crate) pending_viewport: Option<Size<CssPx, Css>>,
    /// The scale the compositor wants, from whichever source it offered.
    pub(crate) ladder: crate::surface::Scale,
    /// Whether the compositor last configured the surface as maximised.
    pub(crate) maximized: bool,
    /// Whether the compositor last configured the surface as full screen.
    pub(crate) fullscreen: bool,
    /// The smallest and largest extents the user may drag to.
    ///
    /// Remembered because the shell takes them as two requests and this contract sets them one at
    /// a time: a window that stated only the new one has left the other at whatever it was, which
    /// includes the pair that made it unresizable.
    pub(crate) bounds: Bounds,
    /// A configure that has arrived and not yet been applied.
    pub(crate) pending_configure: Option<crate::surface::role::xdg::configure::Pending>,
}

impl Shared {
    /// A surface that has been created and not yet configured.
    pub(crate) fn new() -> Self {
        Self {
            size: Size::new(DevicePx(0.0), DevicePx(0.0)),
            logical: Size::new(CssPx(0.0), CssPx(0.0)),
            scale: 1.0,
            pacer: Pacer::new(),
            timing: Timing::default(),
            visibility: Visibility::default(),
            told_hidden: true,
            presented: false,
            declined: false,
            mapped: false,
            pending_viewport: None,
            ladder: crate::surface::Scale::default(),
            maximized: false,
            fullscreen: false,
            bounds: (None, None),
            pending_configure: None,
        }
    }

    /// Records the extent and scale the compositor settled on, and what changed with it.
    ///
    /// The buffer is the logical extent multiplied by the scale, so a change to either produces a
    /// new one. A scale change carries the resize it causes rather than being reported separately,
    /// because they are one event to anything that has to redraw.
    pub(crate) fn resized(
        &mut self,
        logical: Size<CssPx, Css>,
        scale: f64,
    ) -> Option<SurfaceEvent> {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let moved_scale = (scale - self.scale).abs() > f64::EPSILON;
        let size = Size::new(
            DevicePx((logical.width.0 as f64 * scale).round() as f32),
            DevicePx((logical.height.0 as f64 * scale).round() as f32),
        );
        let moved_size = size != self.size;
        if !moved_scale && !moved_size {
            return None;
        }
        self.logical = logical;
        self.scale = scale;
        self.size = size;
        self.pending_viewport = Some(logical);
        Some(if moved_scale {
            SurfaceEvent::ScaleFactorChanged {
                scale_factor: scale,
                size,
            }
        } else {
            SurfaceEvent::Resized(size)
        })
    }

    /// How the redraw just delivered ended, consuming what the delivery recorded.
    ///
    /// A redraw that **presented** owes the compositor nothing further here — the callback rode
    /// the commit `pre_present` made. One that **ran and presented nothing** must keep the chain
    /// alive with a bufferless commit, or a compositor that answers only commits never speaks
    /// about this surface again — and the silence that follows such a commit is also how a hidden
    /// surface is recognised. One that was **declined** never ran: nothing is committed and the
    /// pacer is left as it was, so the runtime's own deadline is answered the moment it asks.
    pub(crate) fn end_of_redraw(&mut self, now: Instant) -> EndOfRedraw {
        let presented = std::mem::replace(&mut self.presented, false);
        let declined = std::mem::replace(&mut self.declined, false);
        if presented {
            self.pacer.committed(now);
            EndOfRedraw::Presented
        } else if declined {
            EndOfRedraw::Declined
        } else {
            self.pacer.committed(now);
            EndOfRedraw::KeepChainAlive
        }
    }

    /// Whether a configure that moved nothing still buys a redraw.
    ///
    /// The first configure has to be answered with a frame, or a surface configured at the extent
    /// it was created with never commits a buffer and is never mapped. A maximise or full-screen
    /// flip that kept the extent is answered too, conservatively: the runtime reads those levels
    /// off the surface during a frame.
    pub(crate) fn answers_restatement(&self, state_flip: bool) -> bool {
        !self.mapped || state_flip
    }

    /// How many frames may go unanswered before the compositor is taken to be showing none of them.
    ///
    /// Four rather than one, because a single unanswered frame is ordinary: a compositor may drop
    /// the feedback for a frame it superseded, and one that is briefly busy answers late. Four in a
    /// row is not — a compositor composites at the rate of its output, and four of its own frames
    /// with nothing said about any of them is a surface it is not compositing.
    pub(crate) const UNANSWERED_ENOUGH: u32 = 4;

    /// Records a frame reaching the screen, and re-arms the watchdog against the new interval.
    pub(crate) fn presented(&mut self, at: Instant, refresh: Duration) {
        self.timing.presented(at, refresh);
        self.pacer
            .watch(Watchdog::for_interval(self.timing.interval()));
    }

    /// Records a committed frame whose presentation has been asked about.
    ///
    /// This is where a run of unanswered frames becomes a statement about visibility, and it is the
    /// signal that works where the others do not. The state that says a window has stopped being
    /// repainted needs version six of the shell *and* a compositor that sends it, and there are
    /// current compositors at version seven that never do. Leaving every output needs a compositor
    /// that reports the leave, and several do not for a window merely moved out of sight. A
    /// compositor that is not compositing a surface, on the other hand, cannot answer for its
    /// frames — there is nothing to answer.
    pub(crate) fn feedback_asked(&mut self) {
        self.timing.asked();
        if self.timing.awaiting() > Self::UNANSWERED_ENOUGH {
            self.visibility.unanswered();
        }
    }

    /// The visibility edge the application has not been told about, if there is one.
    ///
    /// Reported as an edge rather than a level because the runtime treats it as one: becoming
    /// visible forces a full redraw, and re-stating a visibility that has not changed would do it
    /// on every configure.
    pub(crate) fn visibility_edge(&mut self) -> Option<SurfaceEvent> {
        let hidden = self.visibility.is_hidden();
        if hidden == self.told_hidden {
            return None;
        }
        self.told_hidden = hidden;
        self.pacer.hidden(hidden);
        Some(SurfaceEvent::Occluded(hidden))
    }

    /// This surface's timing, as the contract reports it.
    pub(crate) fn snapshot(&self) -> PresentationTiming {
        self.timing.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::{EndOfRedraw, Shared};
    use std::time::{Duration, Instant};
    use zgui_geom::{CssPx, DevicePx, Size};
    use zgui_platform::SurfaceEvent;

    fn logical(width: f32, height: f32) -> Size<CssPx, zgui_geom::Css> {
        Size::new(CssPx(width), CssPx(height))
    }

    #[test]
    fn the_buffer_is_the_logical_extent_at_the_surfaces_own_scale() {
        let mut shared = Shared::new();
        shared.resized(logical(1080.0, 720.0), 1.25);
        assert_eq!(
            shared.size,
            Size::new(DevicePx(1350.0), DevicePx(900.0)),
            "a fractional scale produces a whole number of pixels"
        );
    }

    #[test]
    fn a_configure_that_changes_nothing_asks_for_nothing() {
        // A compositor restates the extent it already sent, more than once per drag. Reporting
        // each would run the whole pipeline for a frame identical to the one on the screen.
        let mut shared = Shared::new();
        assert!(shared.resized(logical(800.0, 600.0), 1.0).is_some());
        assert!(shared.resized(logical(800.0, 600.0), 1.0).is_none());
    }

    #[test]
    fn a_scale_change_carries_the_extent_it_causes() {
        let mut shared = Shared::new();
        shared.resized(logical(800.0, 600.0), 1.0);
        let event = shared
            .resized(logical(800.0, 600.0), 2.0)
            .expect("the scale moved");
        match event {
            SurfaceEvent::ScaleFactorChanged { scale_factor, size } => {
                assert_eq!(scale_factor, 2.0);
                assert_eq!(size, Size::new(DevicePx(1600.0), DevicePx(1200.0)));
            }
            other => panic!("a scale change was reported as {other:?}"),
        }
    }

    #[test]
    fn a_scale_of_zero_is_refused_rather_than_dividing_every_later_conversion_by_it() {
        let mut shared = Shared::new();
        shared.resized(logical(800.0, 600.0), 0.0);
        assert_eq!(shared.scale, 1.0);
    }

    #[test]
    fn the_viewport_destination_is_owed_until_a_commit_carries_it() {
        let mut shared = Shared::new();
        shared.resized(logical(800.0, 600.0), 1.5);
        assert_eq!(shared.pending_viewport, Some(logical(800.0, 600.0)));
    }

    #[test]
    fn a_surface_starts_hidden_and_is_shown_by_its_first_configure() {
        let mut shared = Shared::new();
        assert!(shared.told_hidden);
        assert!(shared.visibility_edge().is_none(), "nothing changed yet");
        shared.visibility.configured = true;
        assert!(matches!(
            shared.visibility_edge(),
            Some(SurfaceEvent::Occluded(false))
        ));
    }

    #[test]
    fn visibility_is_reported_on_its_edges_and_never_restated() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        assert!(matches!(
            shared.visibility_edge(),
            Some(SurfaceEvent::Occluded(false))
        ));
        assert!(shared.visibility_edge().is_none());

        shared.visibility.suspended = true;
        assert!(matches!(
            shared.visibility_edge(),
            Some(SurfaceEvent::Occluded(true))
        ));
        assert!(shared.visibility_edge().is_none());
    }

    #[test]
    fn a_run_of_frames_the_compositor_says_nothing_about_hides_the_surface() {
        // The one signal that needs no protocol version and no cooperation. Measured against a
        // compositor at shell version seven that never sends the suspended state and never reports
        // a leave for a window moved to a workspace nobody is looking at.
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        for _ in 0..Shared::UNANSWERED_ENOUGH + 1 {
            shared.feedback_asked();
        }
        assert!(!shared.visibility.is_hidden(), "a short run is ordinary");
        for _ in 0..3 {
            shared.feedback_asked();
        }
        assert!(shared.visibility.is_hidden());
    }

    #[test]
    fn one_frame_the_compositor_was_slow_about_is_not_an_occlusion() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        shared.feedback_asked();
        shared.presented(Instant::now(), Duration::from_millis(16));
        shared.visibility.answered();
        assert!(!shared.visibility.is_hidden());
    }

    #[test]
    fn a_presented_frame_re_arms_the_watchdog_against_the_interval_it_reported() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        shared.presented(now, Duration::from_millis(100));
        shared.pacer.committed(now);
        shared.pacer.request();
        assert_eq!(
            shared.pacer.deadline(shared.visibility, now),
            Some(now + Duration::from_millis(400))
        );
    }

    #[test]
    fn a_declined_redraw_owes_nothing_and_the_next_request_is_answered_at_once() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        shared.declined = true;
        assert_eq!(shared.end_of_redraw(now), EndOfRedraw::Declined);
        assert_eq!(
            shared.pacer.deadline(shared.visibility, now),
            None,
            "a declined redraw left a callback owed"
        );
        // The runtime's deadline renews the request, and nothing stands in front of it.
        shared.pacer.request();
        let mut visibility = shared.visibility;
        assert!(shared.pacer.take(&mut visibility, now));
    }

    #[test]
    fn a_redraw_that_ran_and_presented_nothing_still_owes_the_chain_a_commit() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        assert_eq!(shared.end_of_redraw(now), EndOfRedraw::KeepChainAlive);
        // The commit went out, so the compositor is owed an answer before the next frame.
        shared.pacer.request();
        let mut visibility = shared.visibility;
        assert!(!shared.pacer.take(&mut visibility, now));
    }

    #[test]
    fn a_redraw_that_presented_rides_its_own_commit() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        shared.presented = true;
        assert_eq!(shared.end_of_redraw(now), EndOfRedraw::Presented);
    }

    #[test]
    fn a_decline_is_consumed_by_the_redraw_it_belongs_to() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        shared.declined = true;
        assert_eq!(shared.end_of_redraw(now), EndOfRedraw::Declined);
        // The next redraw runs normally; yesterday's refusal says nothing about it.
        assert_eq!(shared.end_of_redraw(now), EndOfRedraw::KeepChainAlive);
    }

    #[test]
    fn the_first_configure_is_answered_even_when_it_moves_nothing() {
        let shared = Shared::new();
        assert!(shared.answers_restatement(false));
    }

    #[test]
    fn a_restated_extent_after_the_first_buffer_buys_no_frame() {
        let mut shared = Shared::new();
        shared.mapped = true;
        assert!(!shared.answers_restatement(false));
    }

    #[test]
    fn a_state_flip_that_kept_the_extent_is_still_answered() {
        let mut shared = Shared::new();
        shared.mapped = true;
        assert!(shared.answers_restatement(true));
    }
}
