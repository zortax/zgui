//! Everything about one surface that changes, behind one lock.

use std::time::{Duration, Instant};

use smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::{PresentationTiming, SurfaceEvent};

use crate::frame::{Pacer, Timing, Visibility, Watchdog};

/// The smallest and largest extents a window may be dragged to, in that order.
///
/// A pair rather than two fields, because the shell takes them as two requests that have to agree:
/// a window is made unresizable by setting both to what it is at, and stating only one afterwards
/// would leave the other pinned there.
pub(crate) type Bounds = (Option<Size<CssPx, Css>>, Option<Size<CssPx, Css>>);

/// UI redraw bookkeeping, independent of asynchronous buffer presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndOfRedraw {
    /// A buffer was committed, and the frame callback rode that commit.
    Presented,
    /// The redraw ran without a synchronous presentation; no extra commit is needed.
    NoPresentation,
    /// The redraw was refused without running; nothing is committed and nothing is owed.
    Declined,
}

/// Control state may be committed only outside an in-flight buffer transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlCommit {
    None,
    Empty,
    Unmap,
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
    /// Serial and outstanding state of the real compositor callback. Watchdog expiry never
    /// releases graphics readiness; stale callbacks from an abandoned mapping are ignored.
    pub(crate) gpu_callback_serial: u64,
    pub(crate) gpu_callback_pending: bool,
    pub(crate) presenting: bool,
    pub(crate) unmap_requested: bool,
    pub(crate) control_commit: bool,
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
    /// Whether the compositor last configured the surface as being interactively resized.
    pub(crate) resizing: bool,
    /// Whether the application has been told a drag is running.
    pub(crate) told_resizing: bool,
    /// The smallest and largest extents the user may drag to.
    ///
    /// Remembered because the shell takes them as two requests and this contract sets them one at
    /// a time: a window that stated only the new one has left the other at whatever it was, which
    /// includes the pair that made it unresizable.
    pub(crate) bounds: Bounds,
    /// A configure that has arrived and not yet been applied.
    pub(crate) pending_configure: Option<crate::surface::role::xdg::configure::Pending>,
    /// The outputs the compositor says the surface is on.
    pub(crate) outputs: Vec<WlOutput>,
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
            gpu_callback_serial: 0,
            gpu_callback_pending: false,
            presenting: false,
            unmap_requested: false,
            control_commit: false,
            declined: false,
            mapped: false,
            pending_viewport: None,
            ladder: crate::surface::Scale::default(),
            maximized: false,
            fullscreen: false,
            resizing: false,
            told_resizing: false,
            bounds: (None, None),
            pending_configure: None,
            outputs: Vec::new(),
        }
    }

    /// Only one real callback is awaited, regardless of redraw or watchdog count.
    pub(crate) fn begin_callback(&mut self, now: Instant) -> Option<u64> {
        if self.gpu_callback_pending {
            return None;
        }
        self.gpu_callback_serial += 1;
        self.gpu_callback_pending = true;
        self.pacer.committed(now);
        Some(self.gpu_callback_serial)
    }

    pub(crate) fn cancel_callback(&mut self) {
        self.gpu_callback_serial += 1;
        self.gpu_callback_pending = false;
        self.pacer.callback();
    }

    pub(crate) fn frame_callback(&mut self, serial: u64) {
        if serial == self.gpu_callback_serial {
            self.gpu_callback_pending = false;
            self.pacer.callback();
        }
        self.visibility.answered();
    }

    pub(crate) fn begin_present(&mut self, size: Size<i32, Device>, scale: f64) -> bool {
        if self.presenting
            || !scale.is_finite()
            || scale <= 0.
            || !self.visibility.configured
            || self.visibility.is_hidden()
            || self.unmap_requested
            || self.size.width.0.round() as i32 != size.width
            || self.size.height.0.round() as i32 != size.height
            || (self.scale - scale).abs() > 0.00001
        {
            return false;
        }
        self.presenting = true;
        self.mapped = true;
        self.pending_viewport.take();
        true
    }

    pub(crate) fn request_visibility(&mut self, visible: bool) -> ControlCommit {
        self.unmap_requested = !visible;
        if visible {
            if self.visibility.configured {
                ControlCommit::None
            } else {
                ControlCommit::Empty
            }
        } else if self.presenting {
            ControlCommit::None
        } else {
            self.visibility.configured = false;
            self.cancel_callback();
            ControlCommit::Unmap
        }
    }

    pub(crate) fn control_commit(&mut self) -> ControlCommit {
        if self.presenting {
            self.control_commit = true;
            ControlCommit::None
        } else {
            ControlCommit::Empty
        }
    }

    pub(crate) fn end_present(&mut self) -> ControlCommit {
        self.presenting = false;
        let control = std::mem::take(&mut self.control_commit);
        if self.unmap_requested {
            self.request_visibility(false)
        } else if control {
            ControlCommit::Empty
        } else {
            ControlCommit::None
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
    /// Presentation callbacks belong to buffer commits, never to undamaged UI redraws.
    pub(crate) fn end_of_redraw(&mut self, _now: Instant) -> EndOfRedraw {
        let presented = std::mem::replace(&mut self.presented, false);
        let declined = std::mem::replace(&mut self.declined, false);
        if presented {
            EndOfRedraw::Presented
        } else if declined {
            EndOfRedraw::Declined
        } else {
            EndOfRedraw::NoPresentation
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

    /// Records the refresh interval of the outputs the surface is on.
    ///
    /// The timing uses it while the presentation feedback states no interval. The watchdog is
    /// re-armed against the interval that results.
    pub(crate) fn declares(&mut self, interval: Option<Duration>) {
        self.timing.declares(interval);
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

    /// The drag edge to report, if the resizing state moved since it was last reported.
    ///
    /// The same shape as [`Shared::visibility_edge`]: the compositor restates the level with
    /// every configure, and only the flips are events.
    pub(crate) fn resizing_edge(&mut self) -> Option<SurfaceEvent> {
        if self.resizing == self.told_resizing {
            return None;
        }
        self.told_resizing = self.resizing;
        Some(SurfaceEvent::ResizingChanged(self.resizing))
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
    fn hide_waits_for_the_matching_buffer_then_cancels_its_callback() {
        let mut shared = Shared::new();
        shared.resized(logical(800., 600.), 1.);
        shared.visibility.configured = true;
        assert!(shared.begin_present(Size::new(800, 600), 1.));
        let callback = shared.begin_callback(Instant::now()).unwrap();
        assert_eq!(shared.request_visibility(false), super::ControlCommit::None);
        assert!(shared.visibility.configured);
        assert_eq!(shared.end_present(), super::ControlCommit::Unmap);
        assert!(!shared.visibility.configured);
        assert!(!shared.gpu_callback_pending);
        assert_ne!(shared.gpu_callback_serial, callback);
    }

    #[test]
    fn control_changes_coalesce_until_the_buffer_commit_finishes() {
        let mut shared = Shared::new();
        shared.resized(logical(800., 600.), 1.);
        shared.visibility.configured = true;
        assert!(shared.begin_present(Size::new(800, 600), 1.));
        for _ in 0..32 {
            assert_eq!(shared.control_commit(), super::ControlCommit::None);
        }
        assert_eq!(shared.end_present(), super::ControlCommit::Empty);
        assert_eq!(shared.end_present(), super::ControlCommit::None);
    }

    #[test]
    fn showing_again_cancels_a_deferred_unmap() {
        let mut shared = Shared::new();
        shared.resized(logical(800., 600.), 1.);
        shared.visibility.configured = true;
        assert!(shared.begin_present(Size::new(800, 600), 1.));
        assert_eq!(shared.request_visibility(false), super::ControlCommit::None);
        assert_eq!(shared.request_visibility(true), super::ControlCommit::None);
        assert_eq!(shared.end_present(), super::ControlCommit::None);
        assert!(shared.visibility.configured);
    }

    #[test]
    fn stale_callback_cannot_release_a_new_mapping() {
        let mut shared = Shared::new();
        let now = Instant::now();
        let old = shared.begin_callback(now).unwrap();
        assert!(shared.begin_callback(now).is_none());
        shared.cancel_callback();
        let current = shared.begin_callback(now).unwrap();
        shared.frame_callback(old);
        assert!(shared.gpu_callback_pending);
        shared.frame_callback(current);
        assert!(!shared.gpu_callback_pending);
    }

    #[test]
    fn stale_size_or_scale_cannot_commit_new_viewport_metadata() {
        let mut shared = Shared::new();
        shared.resized(logical(800., 600.), 1.25);
        shared.visibility.configured = true;
        let viewport = shared.pending_viewport;
        assert!(!shared.begin_present(Size::new(800, 600), 1.25));
        assert!(!shared.begin_present(Size::new(1000, 750), 1.0));
        assert_eq!(shared.pending_viewport, viewport);
        assert!(!shared.presenting);
        assert!(shared.begin_present(Size::new(1000, 750), 1.25));
        assert!(shared.pending_viewport.is_none());
        assert!(shared.presenting);
    }

    #[test]
    fn a_hidden_frame_is_rejected_without_starting_a_commit() {
        let mut shared = Shared::new();
        shared.resized(logical(800., 600.), 1.);
        shared.visibility.configured = true;
        shared.unmap_requested = true;
        assert!(!shared.begin_present(Size::new(800, 600), 1.));
        assert!(!shared.presenting);
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
    fn feedback_that_states_no_interval_leaves_the_rate_of_the_output() {
        // A headless output and a variable-refresh output answer every frame with a refresh of
        // zero. Without the output's own rate, the runtime paces such a surface at sixty hertz.
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        let hz_240 = Duration::from_nanos(4_166_667);
        shared.declares(Some(hz_240));
        shared.presented(now, Duration::ZERO);
        assert_eq!(shared.snapshot().interval, Some(hz_240));
        assert_eq!(shared.timing.refresh_rate_millihertz(), Some(240_000));

        // The watchdog is armed against the declared interval. A slow output shows it, because
        // the watchdog never waits less than its floor.
        shared.declares(Some(Duration::from_millis(100)));
        shared.pacer.committed(now);
        shared.pacer.request();
        assert_eq!(
            shared.pacer.deadline(shared.visibility, now),
            Some(now + Duration::from_millis(400))
        );

        // A stated interval wins over the declared one.
        let hz_75 = Duration::from_nanos(13_346_680);
        shared.presented(now, hz_75);
        assert_eq!(shared.snapshot().interval, Some(hz_75));
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
    fn a_redraw_without_presentation_does_not_gate_the_next_request() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        assert_eq!(shared.end_of_redraw(now), EndOfRedraw::NoPresentation);
        // GPU callback ownership is independent of no-damage UI frames.
        shared.pacer.request();
        let mut visibility = shared.visibility;
        assert!(shared.pacer.take(&mut visibility, now));
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
        assert_eq!(shared.end_of_redraw(now), EndOfRedraw::NoPresentation);
    }

    #[test]
    fn a_drag_edge_is_reported_once_per_flip() {
        let mut shared = Shared::new();
        assert!(shared.resizing_edge().is_none(), "no drag has begun");
        shared.resizing = true;
        assert!(matches!(
            shared.resizing_edge(),
            Some(SurfaceEvent::ResizingChanged(true))
        ));
        assert!(
            shared.resizing_edge().is_none(),
            "the level was restated, and a restatement is no edge"
        );
        shared.resizing = false;
        assert!(matches!(
            shared.resizing_edge(),
            Some(SurfaceEvent::ResizingChanged(false))
        ));
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
