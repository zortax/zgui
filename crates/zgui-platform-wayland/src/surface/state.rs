//! Everything about one surface that changes, behind one lock.

use std::time::{Duration, Instant};

use smithay_client_toolkit::reexports::client::protocol::wl_output::WlOutput;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};
use zgui_platform::{PresentationTiming, SurfaceEvent};

use crate::frame::{Ahead, Pacer, Timing, Visibility, Watchdog};
use crate::surface::role::toplevel;
use crate::surface::role::xdg::configure::Pending;

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
    /// The redraw composed a frame that waits for the owed callback; nothing is committed yet.
    Held,
    /// The redraw ran ahead of an owed callback and presented nothing; that callback keeps the
    /// chain alive.
    Waiting,
    /// The redraw ran and presented nothing, and nothing asks for another one or for a change of
    /// the surface. Nothing is committed and nothing is owed.
    Settled,
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
    /// The normal extent from before the window became maximised or full screen.
    ///
    /// A zero extent means the window opened maximised or full screen. A restore then uses the
    /// extent the application asked for.
    restore_size: Option<Size<CssPx, Css>>,
    /// How many physical pixels there are to a logical one.
    pub(crate) scale: f64,
    /// Whether a redraw is owed, and what the compositor owes back.
    pub(crate) pacer: Pacer,
    /// Whether the surface's frames fit the interval, which decides whether it composes ahead.
    pub(crate) ahead: Ahead,
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
    /// Whether the application has been told that a frame reached the screen.
    pub(crate) told_presented: bool,
    /// The number of the last frame callback asked for.
    ///
    /// Only that callback releases the pacer. An older one still counts as the compositor speaking
    /// about the surface, but it answers for a commit that a hide, an unmap or a later commit
    /// already made obsolete.
    pub(crate) callback_serial: u64,
    /// When the last current frame callback arrived.
    pub(crate) released: Option<Instant>,
    /// The callback arrival that the owed commit is chained to.
    ///
    /// A commit is chained when its work began right after a callback. The callback that answers
    /// it then measures whether the frame made the refresh after that callback.
    pub(crate) chained: Option<Instant>,
    /// Whether the application hid the surface.
    ///
    /// A hide of a surface that shows a buffer unmaps it, and the show after it starts a new
    /// configure sequence.
    pub(crate) unmapped: bool,
    /// Whether the shell configured a surface that the application hid before the first buffer of
    /// its mapping.
    ///
    /// The shell keeps the configure sequence of a surface that shows no buffer. Showing such a
    /// surface starts no new sequence: the configure it holds counts, and the next frame maps the
    /// surface.
    pub(crate) configured_unshown: bool,
    /// Whether the current mapping shows a buffer, so that hiding the surface unmaps it.
    pub(crate) shown: bool,
    /// Which configures belong to the current mapping.
    pub(crate) sequence: crate::surface::sequence::Sequence,
    /// Whether the redraw now being delivered was refused without running.
    pub(crate) declined: bool,
    /// Whether a redraw or a held present is being delivered to the application.
    pub(crate) drawing: bool,
    /// Whether the application asked to hide the surface while it was drawing.
    ///
    /// The frame being drawn commits a buffer after the request. Hidden at once, that buffer
    /// would map the surface again with no configure, which the shell refuses. The hide is
    /// carried out when the delivery ends instead.
    pub(crate) hide_owed: bool,
    /// Whether the compositor configured an extent or a scale that no presented frame shows yet.
    ///
    /// A frame for a new extent is never composed ahead, because the next configure would make a
    /// held frame obsolete before it is presented. It is drawn and presented at once instead, see
    /// the pacer.
    pub(crate) resize_owed: bool,
    /// A resize the application asked for and the runtime has not heard of yet.
    ///
    /// A requested extent takes effect at once, so the configure that answers it changes nothing
    /// and reports nothing. The driver reports this event before the next redraw instead.
    pub(crate) unreported: Option<SurfaceEvent>,
    /// Whether a lock surface acknowledged an extent that no committed buffer shows yet.
    ///
    /// The commit after a lock surface acknowledges a configure has to carry a buffer of exactly
    /// that extent, and any other commit is a protocol error. So while this is set the surface
    /// commits only a frame at the configured extent.
    pub(crate) exact_owed: bool,
    /// Whether the redraw now being delivered answers a new extent or scale.
    ///
    /// Such a redraw rebuilds the swap chain, and a drag sends one per refresh, so its cost and its
    /// misses say nothing about whether the frames after the drag fit the interval.
    pub(crate) answering_resize: bool,
    /// Whether any buffer has ever been committed.
    ///
    /// What separates a configure that restates the extent from the one that maps the surface: the
    /// first configure has to be answered with a frame or the surface is never shown, and every
    /// later restatement is a frame identical to the one already on the screen.
    pub(crate) mapped: bool,
    /// The extent and scale whose buffer metadata the compositor was last given.
    ///
    /// A buffer scale, a viewport destination and a window geometry are sent only with a buffer
    /// that they describe. Sent at configure time, they ride whatever commit comes next — an empty
    /// one, or a buffer drawn at the extent from before the configure.
    pub(crate) declared: Option<(Size<i32, Device>, f64)>,
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
    pub(crate) pending_configure: Option<Pending>,
    /// The outputs the compositor says the surface is on.
    pub(crate) outputs: Vec<WlOutput>,
    /// How the colour stripes of those outputs are arranged.
    pub(crate) subpixel: zgui_platform::SubpixelLayout,
    /// The output a shell layer was opened on, when the application named one.
    pub(crate) bound: Option<WlOutput>,
    /// Whether an output the surface was bound to or on has gone.
    ///
    /// What tells a shell layer closed because its output went away from one the desktop closed
    /// for a reason of its own.
    pub(crate) output_lost: bool,
    /// Where the shell layer sits and what it claims, for a surface that is one.
    ///
    /// Kept whole because the shell forgets all of it when the surface is unmapped, and the remap
    /// has to send all of it again.
    pub(crate) layer: Option<zgui_platform::LayerPlacement>,
    /// Whether the placement or the extent of the shell layer changed since it was last sent.
    ///
    /// The change goes out with the next commit that carries a frame, so the placement and the
    /// content drawn for it are shown together.
    pub(crate) layer_owed: bool,
    /// The part of the surface that takes input, as asked for and not yet sent.
    pub(crate) input_owed: crate::surface::region::Owed,
    /// The part of the surface that is opaque, as asked for and not yet sent.
    pub(crate) opaque_owed: crate::surface::region::Owed,
}

impl Shared {
    /// A surface that has been created and not yet configured.
    pub(crate) fn new() -> Self {
        Self {
            size: Size::new(DevicePx(0.0), DevicePx(0.0)),
            logical: Size::new(CssPx(0.0), CssPx(0.0)),
            restore_size: None,
            scale: 1.0,
            pacer: Pacer::new(),
            ahead: Ahead::new(zgui_platform::ComposeAhead::Auto),
            timing: Timing::default(),
            visibility: Visibility::default(),
            told_hidden: true,
            presented: false,
            told_presented: false,
            callback_serial: 0,
            released: None,
            chained: None,
            unmapped: false,
            configured_unshown: false,
            shown: false,
            sequence: crate::surface::sequence::Sequence::default(),
            declined: false,
            drawing: false,
            hide_owed: false,
            resize_owed: false,
            unreported: None,
            exact_owed: false,
            answering_resize: false,
            mapped: false,
            declared: None,
            ladder: crate::surface::Scale::default(),
            maximized: false,
            fullscreen: false,
            resizing: false,
            told_resizing: false,
            bounds: (None, None),
            pending_configure: None,
            outputs: Vec::new(),
            subpixel: zgui_platform::SubpixelLayout::Unknown,
            bound: None,
            output_lost: false,
            layer: None,
            layer_owed: false,
            input_owed: None,
            opaque_owed: None,
        }
    }

    /// Applies the maximised and full-screen states of a configure.
    ///
    /// Returns the extent to draw at, and whether either state changed. An axis the compositor
    /// leaves unnamed takes the saved normal extent when the window leaves maximised or full
    /// screen. A second expanded state keeps the extent saved by the first.
    pub(crate) fn apply_shell_states(
        &mut self,
        pending: &Pending,
        wanted: Size<CssPx, Css>,
    ) -> (Size<CssPx, Css>, bool) {
        let flipped = self.maximized != pending.maximized || self.fullscreen != pending.fullscreen;
        let was_expanded = self.maximized || self.fullscreen;
        let expanded = pending.maximized || pending.fullscreen;
        if !was_expanded && expanded {
            self.restore_size = Some(self.logical);
        }
        let fallback = if was_expanded && !expanded {
            self.restore_size.take().unwrap_or(self.logical)
        } else {
            self.logical
        };
        self.maximized = pending.maximized;
        self.fullscreen = pending.fullscreen;
        (toplevel::extent(pending.size, wanted, fallback), flipped)
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
        self.unreported = None;
        self.pacer.cancel_held();
        self.resize_owed = true;
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
    ///
    /// A redraw that ran and presented nothing is **settled** when `again` says no other redraw
    /// was asked for, the surface owes the compositor no change of its own, and the redraw did not
    /// answer input. There is no chain to keep alive then, so nothing is committed and nothing is
    /// owed. The next request is answered the moment it is made. A redraw that answered input
    /// keeps the chain alive, so input that arrives faster than the display refreshes is paced by
    /// the callback, see the pacer.
    pub(crate) fn end_of_redraw(
        &mut self,
        began: Instant,
        now: Instant,
        again: bool,
    ) -> EndOfRedraw {
        let presented = std::mem::replace(&mut self.presented, false);
        let declined = std::mem::replace(&mut self.declined, false);
        if presented {
            self.pacer.committed(now);
            // A frame for a new extent is not counted as a miss: see `answering_resize`.
            self.chained = if self.answering_resize {
                None
            } else {
                self.chain(began)
            };
            self.resize_owed = false;
            EndOfRedraw::Presented
        } else if declined {
            EndOfRedraw::Declined
        } else if self.pacer.held() {
            EndOfRedraw::Held
        } else if self.pacer.owes_callback() {
            EndOfRedraw::Waiting
        } else if !again
            && !self.pacer.wanted()
            && !self.pacer.drew_for_input()
            && !self.owes_surface_change()
        {
            self.chained = None;
            EndOfRedraw::Settled
        } else {
            self.pacer.committed_bare(now);
            self.chained = None;
            EndOfRedraw::KeepChainAlive
        }
    }

    /// Closes out a delivered [`SurfaceEvent::PresentReady`].
    ///
    /// A held frame that reached the surface owes a callback, like any presented frame. One that
    /// did not — an acquisition that failed, or a frame a configure made obsolete — owes nothing,
    /// and the application asks for the frame that replaces it.
    pub(crate) fn end_of_present(&mut self, began: Instant, now: Instant) {
        if std::mem::replace(&mut self.presented, false) {
            self.pacer.committed(now);
            self.chained = self.chain(began);
            // A new extent cancels a held frame, so a held frame that is presented shows the
            // current extent.
            self.resize_owed = false;
        }
    }

    /// Whether the next redraw may be composed while a frame callback is owed.
    pub(crate) fn compose_ahead_now(&self, now: Instant) -> bool {
        !self.resize_owed && self.ahead.wanted(self.timing.interval(), now)
    }

    /// Records that a redraw is about to be delivered.
    pub(crate) fn redraw_taken(&mut self) {
        self.answering_resize = self.resize_owed;
        self.drawing = true;
    }

    /// Whether a change of the surface waits for a commit: a layer placement, a region, or a
    /// buffer at an exact extent.
    const fn owes_surface_change(&self) -> bool {
        self.layer_owed
            || self.input_owed.is_some()
            || self.opaque_owed.is_some()
            || self.exact_owed
    }

    /// Closes out a delivered redraw: [`Shared::end_of_redraw`], and the redraw's cost for
    /// [`Ahead`] unless the redraw was declined or answered a new extent.
    pub(crate) fn redrawn(&mut self, began: Instant, now: Instant, again: bool) -> EndOfRedraw {
        let end = self.end_of_redraw(began, now, again);
        let answering = std::mem::take(&mut self.answering_resize);
        if end != EndOfRedraw::Declined && !answering {
            self.ahead.redrawn(now.saturating_duration_since(began));
        }
        end
    }

    /// The callback arrival a commit whose work began at `began` is chained to, if it is.
    ///
    /// Work that began within a quarter of the interval after the callback tries for the refresh
    /// after it. Work that began later can miss that refresh only because it began late.
    fn chain(&self, began: Instant) -> Option<Instant> {
        let slack = self.timing.interval()? / 4;
        self.released
            .filter(|&released| began.saturating_duration_since(released) <= slack)
    }

    /// Whether a configure that moved nothing still buys a redraw.
    ///
    /// The first configure has to be answered with a frame, or a surface configured at the extent
    /// it was created with never commits a buffer and is never mapped. A maximise or full-screen
    /// flip that kept the extent is answered too, conservatively. The runtime hears of the flip
    /// through a `SurfaceStateChanged` wake.
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
        self.cancel_callbacks();
        Some(SurfaceEvent::Occluded(hidden))
    }

    /// The first-presentation event, once a committed buffer reached the screen.
    ///
    /// `shown` says the compositor showed a frame of this surface. The event goes out once, and
    /// only after a buffer was committed.
    pub(crate) fn presentation_edge(&mut self, shown: bool) -> Option<SurfaceEvent> {
        if !shown || !self.mapped || self.told_presented {
            return None;
        }
        self.told_presented = true;
        Some(SurfaceEvent::Presented)
    }

    /// Records that a lock surface acknowledged a configure that its next commit has to match.
    pub(crate) const fn owe_exact_extent(&mut self) {
        self.exact_owed = true;
    }

    /// Records that a frame of `size` is about to be committed.
    ///
    /// A frame at the configured extent pays what [`Shared::owe_exact_extent`] recorded.
    pub(crate) fn committing_frame(&mut self, size: Size<i32, Device>) {
        if size == self.device_extent() {
            self.exact_owed = false;
        }
    }

    /// Whether a commit that carries no new buffer is allowed now.
    ///
    /// Such a commit keeps the buffer on the screen. It is refused while the surface is hidden or
    /// about to be, because it would map the surface again. It is refused while a lock surface owes
    /// a buffer at a new extent, because the buffer on the screen has the old one.
    pub(crate) const fn may_commit_bare(&self) -> bool {
        !self.unmapped && !self.hide_owed && !self.exact_owed
    }

    /// The configured extent, in whole device pixels.
    fn device_extent(&self) -> Size<i32, Device> {
        Size::new(
            self.size.width.0.round() as i32,
            self.size.height.0.round() as i32,
        )
    }

    /// The logical extent a buffer of `size` drawn at `scale` maps to, when its metadata changed.
    ///
    /// The configured extent is the answer for a buffer at the configured size. A buffer drawn
    /// before the latest configure maps to its own extent, divided back out of its scale.
    pub(crate) fn frame_metadata(
        &mut self,
        size: Size<i32, Device>,
        scale: f64,
    ) -> Option<Size<CssPx, Css>> {
        if self.declared == Some((size, scale)) {
            return None;
        }
        self.declared = Some((size, scale));
        if self.device_extent() == size && (self.scale - scale).abs() < 1e-5 {
            return Some(self.logical);
        }
        Some(Size::new(
            CssPx((f64::from(size.width) / scale) as f32),
            CssPx((f64::from(size.height) / scale) as f32),
        ))
    }

    /// Numbers a new frame callback, which makes every older one obsolete.
    pub(crate) fn begin_callback(&mut self) -> u64 {
        self.callback_serial += 1;
        self.callback_serial
    }

    /// Makes every callback asked for so far obsolete.
    pub(crate) fn cancel_callbacks(&mut self) {
        self.callback_serial += 1;
        self.released = None;
        self.chained = None;
    }

    /// Records the compositor answering frame callback `serial`.
    pub(crate) fn frame_callback(&mut self, serial: u64, now: Instant) {
        if serial == self.callback_serial {
            if let Some(released) = self.chained.take() {
                let period = now.saturating_duration_since(released);
                self.ahead.answered(period, self.timing.interval(), now);
            }
            self.released = Some(now);
            self.pacer.callback();
        }
        // The compositor spoke about this surface, which ends whatever run of unanswered frames
        // had accumulated — and with it the only occlusion signal that works on every version of
        // the shell.
        self.visibility.answered();
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
    use crate::frame::Ahead;
    use crate::surface::role::xdg::configure::Pending;
    use std::num::NonZeroU32;
    use std::time::{Duration, Instant};
    use zgui_geom::{CssPx, DevicePx, Size};
    use zgui_platform::SurfaceEvent;

    fn logical(width: f32, height: f32) -> Size<CssPx, zgui_geom::Css> {
        Size::new(CssPx(width), CssPx(height))
    }

    #[test]
    fn an_obsolete_callback_releases_nothing_and_still_counts_as_an_answer() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        let old = shared.begin_callback();
        shared.cancel_callbacks();
        let current = shared.begin_callback();
        shared.pacer.committed(now);
        shared.visibility.unanswered();

        shared.frame_callback(old, now);
        shared.pacer.request();
        let mut visibility = shared.visibility;
        assert!(
            !shared.pacer.take(&mut visibility, now),
            "the old callback released the pacer"
        );
        assert_eq!(
            shared.visibility.abandoned, 0,
            "the compositor spoke about the surface"
        );

        shared.frame_callback(current, now);
        assert!(shared.pacer.take(&mut visibility, now));
    }

    /// A configured surface on a 120 Hz output whose redraws cost too little to compose ahead.
    fn on_a_known_output() -> (Shared, Duration) {
        let interval = Duration::from_micros(8_333);
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        shared.declares(Some(interval));
        (shared, interval)
    }

    /// Runs one redraw that began `late` after the last callback and committed 7.5 ms later, and
    /// answers it `period` after that callback. Returns when the answer arrived.
    fn frame(shared: &mut Shared, released: Instant, late: Duration, period: Duration) -> Instant {
        let began = released + late;
        let serial = shared.begin_callback();
        shared.presented = true;
        shared.end_of_redraw(began, began + Duration::from_micros(7_500), true);
        let answered = released + period;
        shared.frame_callback(serial, answered);
        answered
    }

    #[test]
    fn frames_started_on_the_callback_that_miss_the_refresh_compose_ahead() {
        let (mut shared, interval) = on_a_known_output();
        let first = shared.begin_callback();
        let mut at = Instant::now();
        shared.frame_callback(first, at);
        // Each frame ends late in the interval. Its commit is answered only a little more than one
        // interval after the commit, but two intervals after the callback that started it.
        for _ in 0..2 {
            at = frame(&mut shared, at, Duration::ZERO, 2 * interval);
        }
        assert!(shared.ahead.wanted(shared.timing.interval(), at));
    }

    #[test]
    fn frames_started_late_are_not_taken_as_misses() {
        // A redraw asked for by input in the middle of an interval misses the refresh because it
        // began late, and composing ahead would not help it.
        let (mut shared, interval) = on_a_known_output();
        let first = shared.begin_callback();
        let mut at = Instant::now();
        shared.frame_callback(first, at);
        for _ in 0..4 {
            at = frame(&mut shared, at, interval / 2, 2 * interval);
        }
        assert!(!shared.ahead.wanted(shared.timing.interval(), at));
    }

    #[test]
    fn a_frame_for_a_new_extent_waits_for_the_callback() {
        // The next configure of a drag would cancel a frame composed ahead, so the frame for a new
        // extent is composed when the compositor is ready for it.
        let (mut shared, _) = on_a_known_output();
        shared.ahead = Ahead::new(zgui_platform::ComposeAhead::Always);
        let now = Instant::now();
        assert!(shared.compose_ahead_now(now));
        shared.resized(logical(800.0, 600.0), 1.0);
        assert!(!shared.compose_ahead_now(now));
        shared.redraw_taken();
        shared.presented = true;
        shared.redrawn(now, now + Duration::from_millis(5), true);
        assert!(
            shared.compose_ahead_now(now),
            "the frame that showed the extent ends the wait"
        );
    }

    fn expanded(full: bool) -> Pending {
        Pending {
            maximized: !full,
            fullscreen: full,
            ..Pending::default()
        }
    }

    #[test]
    fn restoring_from_maximized_and_fullscreen_recovers_normal_size() {
        for full in [false, true] {
            let mut shared = Shared::new();
            let wanted = logical(800.0, 600.0);
            let normal = logical(940.0, 700.0);
            shared.resized(normal, 1.5);
            shared.apply_shell_states(&expanded(full), wanted);
            shared.resized(logical(1920.0, 1080.0), 1.5);
            let (extent, _) = shared.apply_shell_states(&Pending::default(), wanted);
            assert_eq!(extent, normal);
        }
    }

    #[test]
    fn a_second_expanded_state_keeps_the_first_saved_extent() {
        let mut shared = Shared::new();
        let wanted = logical(800.0, 600.0);
        let normal = logical(940.0, 700.0);
        shared.resized(normal, 1.0);
        shared.apply_shell_states(&expanded(false), wanted);
        shared.resized(logical(1920.0, 1080.0), 1.0);
        shared.apply_shell_states(&expanded(true), wanted);
        shared.resized(logical(1920.0, 1200.0), 1.0);
        shared.apply_shell_states(&expanded(false), wanted);
        shared.resized(logical(1920.0, 1080.0), 1.0);
        let (extent, _) = shared.apply_shell_states(&Pending::default(), wanted);
        assert_eq!(extent, normal);
    }

    #[test]
    fn a_window_opened_expanded_restores_to_the_extent_it_asked_for() {
        for full in [false, true] {
            let mut shared = Shared::new();
            let wanted = logical(800.0, 600.0);
            let named = Pending {
                size: (NonZeroU32::new(1920), NonZeroU32::new(1080)),
                ..expanded(full)
            };
            let (extent, _) = shared.apply_shell_states(&named, wanted);
            shared.resized(extent, 1.0);
            let (extent, _) = shared.apply_shell_states(&Pending::default(), wanted);
            assert_eq!(extent, wanted);
        }
    }

    #[test]
    fn a_restore_naming_one_axis_takes_the_saved_size_on_the_other() {
        let mut shared = Shared::new();
        let wanted = logical(800.0, 600.0);
        shared.resized(wanted, 1.0);
        shared.apply_shell_states(&expanded(false), wanted);
        shared.resized(logical(1920.0, 1080.0), 1.0);
        let restore = Pending {
            size: (NonZeroU32::new(1000), None),
            ..Pending::default()
        };
        let (extent, _) = shared.apply_shell_states(&restore, wanted);
        assert_eq!(extent, logical(1000.0, 600.0));
    }

    #[test]
    fn only_a_changed_state_reports_a_flip() {
        let mut shared = Shared::new();
        let wanted = logical(800.0, 600.0);
        shared.resized(wanted, 1.0);
        assert!(!shared.apply_shell_states(&Pending::default(), wanted).1);
        assert!(shared.apply_shell_states(&expanded(false), wanted).1);
        assert!(!shared.apply_shell_states(&expanded(false), wanted).1);
        assert!(shared.apply_shell_states(&expanded(true), wanted).1);
        assert!(shared.apply_shell_states(&Pending::default(), wanted).1);
    }

    #[test]
    fn frames_for_new_extents_neither_cost_nor_miss() {
        // A drag rebuilds the swap chain on every frame and may miss refreshes. None of that says
        // whether the frames after the drag fit, so none of it arms composing ahead.
        let (mut shared, interval) = on_a_known_output();
        let first = shared.begin_callback();
        let mut at = Instant::now();
        shared.frame_callback(first, at);
        for step in 0..8 {
            shared.resized(logical(800.0 + step as f32, 600.0), 1.0);
            shared.redraw_taken();
            let serial = shared.begin_callback();
            shared.presented = true;
            shared.redrawn(at, at + 2 * interval, true);
            at += 2 * interval;
            shared.frame_callback(serial, at);
        }
        assert!(!shared.ahead.wanted(shared.timing.interval(), at));
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
    fn buffer_metadata_is_sent_once_for_each_extent_and_scale() {
        let mut shared = Shared::new();
        shared.resized(logical(800.0, 600.0), 1.5);
        let buffer = Size::new(1200, 900);
        assert_eq!(
            shared.frame_metadata(buffer, 1.5),
            Some(logical(800.0, 600.0))
        );
        assert_eq!(shared.frame_metadata(buffer, 1.5), None);
    }

    #[test]
    fn a_buffer_from_before_a_configure_is_described_by_its_own_extent() {
        let mut shared = Shared::new();
        shared.resized(logical(800.0, 600.0), 1.25);
        shared.resized(logical(1000.0, 750.0), 1.25);
        assert_eq!(
            shared.frame_metadata(Size::new(1000, 750), 1.25),
            Some(logical(800.0, 600.0)),
            "the newer extent was declared for the older buffer"
        );
        assert_eq!(
            shared.frame_metadata(Size::new(1250, 938), 1.25),
            Some(logical(1000.0, 750.0))
        );
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
    fn the_first_frame_on_the_screen_is_reported_once_after_a_buffer() {
        let mut shared = Shared::new();
        assert!(
            shared.presentation_edge(true).is_none(),
            "an answer before any buffer shows nothing"
        );
        shared.mapped = true;
        assert!(shared.presentation_edge(false).is_none());
        assert!(matches!(
            shared.presentation_edge(true),
            Some(SurfaceEvent::Presented)
        ));
        assert!(
            shared.presentation_edge(true).is_none(),
            "the event goes out once"
        );
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
        assert_eq!(shared.end_of_redraw(now, now, true), EndOfRedraw::Declined);
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
        assert_eq!(
            shared.end_of_redraw(now, now, true),
            EndOfRedraw::KeepChainAlive
        );
        // The commit went out, so the compositor is owed an answer before the next frame.
        shared.pacer.request();
        let mut visibility = shared.visibility;
        assert!(!shared.pacer.take(&mut visibility, now));
        // That commit carried no buffer, so a press draws and presents without the answer.
        shared.pacer.input();
        assert!(shared.pacer.take(&mut visibility, now));
        assert!(shared.pacer.present_now(now));
    }

    #[test]
    fn a_redraw_that_presented_nothing_and_asks_for_nothing_commits_nothing() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        assert_eq!(shared.end_of_redraw(now, now, false), EndOfRedraw::Settled);
        assert!(!shared.pacer.owes_callback(), "nothing was committed");
        shared.pacer.request();
        let mut visibility = shared.visibility;
        assert!(
            shared.pacer.take(&mut visibility, now),
            "the next request is answered at once"
        );

        // A change of the placement waits for a commit.
        shared.layer_owed = true;
        assert_eq!(
            shared.end_of_redraw(now, now, false),
            EndOfRedraw::KeepChainAlive
        );
    }

    #[test]
    fn input_over_a_still_surface_is_paced_by_the_callback() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        let mut visibility = shared.visibility;
        let mut redraw_for_input = |shared: &mut Shared| {
            shared.pacer.input();
            shared.pacer.request();
            shared
                .pacer
                .take(&mut visibility, now)
                .then(|| shared.end_of_redraw(now, now, false))
        };
        // The first redraw draws nothing, and its commit asks for a callback.
        assert_eq!(
            redraw_for_input(&mut shared),
            Some(EndOfRedraw::KeepChainAlive)
        );
        // One more redraw runs ahead of that callback, and the next ones wait for it.
        assert_eq!(redraw_for_input(&mut shared), Some(EndOfRedraw::Waiting));
        assert_eq!(redraw_for_input(&mut shared), None);
        shared.pacer.callback();
        assert_eq!(
            redraw_for_input(&mut shared),
            Some(EndOfRedraw::KeepChainAlive)
        );
    }

    #[test]
    fn a_lock_surface_commits_only_a_frame_at_its_new_extent() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let _ = shared.resized(logical(1280.0, 720.0), 1.0);
        shared.committing_frame(Size::new(1280, 720));
        assert!(shared.may_commit_bare());

        // The output changed its mode; the toolkit acknowledged the configure on arrival.
        let _ = shared.resized(logical(1920.0, 1080.0), 1.0);
        shared.owe_exact_extent();
        assert!(
            !shared.may_commit_bare(),
            "the buffer on the screen is 1280x720"
        );
        // A frame drawn before the configure pays nothing.
        shared.committing_frame(Size::new(1280, 720));
        assert!(!shared.may_commit_bare());
        shared.committing_frame(Size::new(1920, 1080));
        assert!(shared.may_commit_bare());
    }

    #[test]
    fn a_redraw_ahead_of_an_owed_callback_commits_nothing() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        shared.pacer.compose_ahead(true);
        shared.pacer.committed(now);
        assert_eq!(shared.end_of_redraw(now, now, true), EndOfRedraw::Waiting);
        assert!(!shared.pacer.present_now(now));
        assert_eq!(shared.end_of_redraw(now, now, true), EndOfRedraw::Held);
    }

    #[test]
    fn a_held_frame_that_reached_the_surface_owes_a_callback() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        shared.end_of_present(now, now);
        assert!(!shared.pacer.owes_callback(), "nothing was presented");
        shared.presented = true;
        shared.end_of_present(now, now);
        assert!(shared.pacer.owes_callback());
    }

    #[test]
    fn a_new_extent_drops_the_held_frame() {
        let mut shared = Shared::new();
        shared.resized(logical(800.0, 600.0), 1.0);
        shared.pacer.committed(Instant::now());
        assert!(!shared.pacer.present_now(Instant::now()));
        shared.resized(logical(900.0, 600.0), 1.0);
        assert!(!shared.pacer.held());
    }

    #[test]
    fn a_redraw_that_presented_rides_its_own_commit() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        shared.presented = true;
        assert_eq!(shared.end_of_redraw(now, now, true), EndOfRedraw::Presented);
    }

    #[test]
    fn a_decline_is_consumed_by_the_redraw_it_belongs_to() {
        let mut shared = Shared::new();
        shared.visibility.configured = true;
        let now = Instant::now();
        shared.declined = true;
        assert_eq!(shared.end_of_redraw(now, now, true), EndOfRedraw::Declined);
        // The next redraw runs normally; yesterday's refusal says nothing about it.
        assert_eq!(
            shared.end_of_redraw(now, now, true),
            EndOfRedraw::KeepChainAlive
        );
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
