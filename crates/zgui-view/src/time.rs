//! Doing something later.
//!
//! A tooltip that opens after a delay, a toast that dismisses itself, a carousel that advances, a
//! filter that debounces: four categories of component that cannot be written without a way to
//! schedule work at a future time, and there is no style-sheet escape for any of them, because
//! the content they animate is portalled away from the element whose state would drive it.
//!
//! Both functions here schedule against the engine's own clock, never the wall clock, so a test
//! that advances time by hand fires them exactly as a running window does.

use core::cell::Cell;
use core::time::Duration;
use std::rc::Rc;

use zgui_reactive::on_cleanup_local;
use zgui_vocab::Timestamp;

use crate::cx::current_host;
use crate::host::{FrameRequestId, HostHandle, Repeat, TimerId};

/// A scheduled callback that is cancelled when this is dropped.
struct Scheduled {
    /// The engine holding it.
    host: HostHandle,
    /// Which registration it is.
    id: TimerId,
    /// Whether it has already been cancelled.
    cancelled: Cell<bool>,
}

impl Scheduled {
    /// Cancels the callback. Doing so twice does nothing the second time.
    fn cancel(&self) {
        if !self.cancelled.replace(true) {
            self.host.cancel_timer(self.id);
        }
    }
}

impl Drop for Scheduled {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Keeps a one-shot callback pending.
///
/// Dropping it cancels the callback. It is also cancelled when the scope that scheduled it goes
/// away, because a callback that outlives its scope has nothing valid left to write to — a tooltip
/// unmounted four hundred milliseconds into a seven-hundred-millisecond delay must cancel, not
/// fire into a scope that has been disposed of.
#[must_use = "dropping the handle cancels the timer"]
pub struct TimeoutHandle(Rc<Scheduled>);

impl TimeoutHandle {
    /// Cancels the callback now.
    pub fn cancel(self) {
        self.0.cancel();
    }
}

impl Drop for TimeoutHandle {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Keeps a repeating callback running.
///
/// Dropping it stops the repetition, as does the scope that started it going away.
#[must_use = "dropping the handle cancels the timer"]
pub struct IntervalHandle(Rc<Scheduled>);

impl IntervalHandle {
    /// Stops the repetition now.
    pub fn cancel(self) {
        self.0.cancel();
    }
}

impl Drop for IntervalHandle {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// A pending frame callback that is cancelled when this is dropped.
struct RequestedFrame {
    /// The engine holding it.
    host: HostHandle,
    /// Which registration it is.
    id: FrameRequestId,
    /// Whether it has already been cancelled.
    cancelled: Cell<bool>,
}

impl RequestedFrame {
    /// Cancels the callback. Doing so twice does nothing the second time.
    fn cancel(&self) {
        if !self.cancelled.replace(true) {
            self.host.cancel_frame_callback(self.id);
        }
    }
}

impl Drop for RequestedFrame {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Keeps a frame callback pending.
///
/// Dropping it cancels the callback, as does the scope that requested it going away when the
/// request was made through [`request_frame`]. A callback that already ran is cancelled to no
/// effect, so an animation that re-requests from its own body simply overwrites the handle it
/// holds with the new one.
#[must_use = "dropping the handle cancels the frame callback"]
pub struct FrameHandle(Rc<RequestedFrame>);

impl FrameHandle {
    /// Cancels the callback now.
    pub fn cancel(self) {
        self.0.cancel();
    }
}

impl Drop for FrameHandle {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// The window's clock, captured so that it can be scheduled against later.
///
/// [`set_timeout`] and [`set_interval`] find the clock through the scope they are called in, which
/// is exactly right in a component's body and no use at all in a listener: a listener runs while an
/// event is being delivered, and the scope that built the component is not the scope it runs in. A
/// toast that restarts its own timer when the pointer leaves it, a field that debounces what is
/// typed into it, a menu that closes after a delay — every one of those schedules from a handler.
///
/// So the clock is taken once, in the body, and carried into the handler with everything else the
/// handler captures.
///
/// Nothing scheduled through this is cancelled by a scope going away: the handle is what cancels
/// it, and a component that schedules from a handler is a component that was already holding one.
///
/// ```
/// use core::cell::Cell;
/// use core::time::Duration;
/// use std::rc::Rc;
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::stub::StubHost;
/// use zgui_view::{HostHandle, Timers, provide_host};
///
/// install().unwrap();
/// let engine = Rc::new(StubHost::default());
/// let window = Mounted::new();
/// window.with(|| provide_host(HostHandle::from_rc(engine.clone())));
///
/// // Taken inside the scope…
/// let clock = window.with(|| Timers::current().expect("inside a window"));
/// let fired = Rc::new(Cell::new(false));
/// let flag = Rc::clone(&fired);
///
/// // …and used outside it, exactly as a listener does.
/// let handle = clock.set_timeout(Duration::from_millis(200), move || flag.set(true));
/// engine.advance(Duration::from_millis(200));
/// assert!(fired.get());
/// drop(handle);
/// window.unmount();
/// ```
#[derive(Clone)]
pub struct Timers(HostHandle);

impl Timers {
    /// The clock the enclosing window keeps, when there is one.
    #[must_use]
    pub fn current() -> Option<Self> {
        current_host().map(Self)
    }

    /// Wraps a handle on an engine directly, for a caller that already holds one.
    #[must_use]
    pub fn new(host: HostHandle) -> Self {
        Self(host)
    }

    /// Runs `callback` once, no earlier than `after` from now.
    #[must_use = "dropping the handle cancels the timer"]
    pub fn set_timeout(&self, after: Duration, callback: impl FnOnce() + 'static) -> TimeoutHandle {
        let once = Cell::new(Some(callback));
        TimeoutHandle(self.scheduled(after, Repeat::Once, move || {
            if let Some(callback) = once.take() {
                callback();
            }
        }))
    }

    /// Runs `callback` every `every`, until the handle is dropped.
    #[must_use = "dropping the handle cancels the timer"]
    pub fn set_interval(
        &self,
        every: Duration,
        callback: impl FnMut() + 'static,
    ) -> IntervalHandle {
        let callback = core::cell::RefCell::new(callback);
        IntervalHandle(self.scheduled(every, Repeat::Every, move || {
            if let Ok(mut callback) = callback.try_borrow_mut() {
                callback();
            }
        }))
    }

    /// Runs `callback` once, during the next frame, with the moment that frame is for.
    ///
    /// While the registration is pending the window counts as animating, so the frame is paced
    /// at the display's refresh interval; a callback that requests again from its own body runs
    /// once per frame, one refresh apart, and stops by not requesting again. A motion written
    /// this way measures its steps from successive timestamps rather than from the wall clock,
    /// so a slow frame produces one larger step instead of a slower motion.
    ///
    /// Nothing requested through this is cancelled by a scope going away: the handle is what
    /// cancels it, exactly as with [`Timers::set_timeout`].
    #[must_use = "dropping the handle cancels the frame callback"]
    pub fn request_frame(&self, callback: impl FnOnce(Timestamp) + 'static) -> FrameHandle {
        let once = Cell::new(Some(callback));
        let id = self.0.request_frame_callback(Rc::new(move |at| {
            if let Some(callback) = once.take() {
                callback(at);
            }
        }));
        FrameHandle(Rc::new(RequestedFrame {
            host: self.0.clone(),
            id,
            cancelled: Cell::new(false),
        }))
    }

    /// Registers one callback with the engine.
    fn scheduled(
        &self,
        after: Duration,
        repeat: Repeat,
        callback: impl Fn() + 'static,
    ) -> Rc<Scheduled> {
        let id = self.0.schedule(after, repeat, Rc::new(callback));
        Rc::new(Scheduled {
            host: self.0.clone(),
            id,
            cancelled: Cell::new(false),
        })
    }
}

/// Runs `callback` once, no earlier than `after` from now.
///
/// The callback runs at the start of a frame, before that frame's reactive work, so whatever it
/// writes settles in the same frame it fired in: one wake, one frame.
///
/// Called from a component's body. A listener that has to schedule something takes a [`Timers`] in
/// the body instead and carries it into the handler, because the scope this reads the clock from is
/// not the scope a listener runs in.
///
/// # Panics
///
/// In debug builds, when called outside a window's reactive scope, where there is no engine to
/// schedule against. In release it returns a handle that cancels nothing.
///
/// ```
/// use core::cell::Cell;
/// use core::time::Duration;
/// use std::rc::Rc;
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::stub::StubHost;
/// use zgui_view::{HostHandle, provide_host, set_timeout};
///
/// install().unwrap();
/// let engine = Rc::new(StubHost::default());
/// let window = Mounted::new();
/// window.with(|| provide_host(HostHandle::from_rc(engine.clone())));
///
/// let opened = Rc::new(Cell::new(false));
/// let flag = Rc::clone(&opened);
/// let handle = window.with(|| set_timeout(Duration::from_millis(700), move || flag.set(true)));
///
/// engine.advance(Duration::from_millis(699));
/// assert!(!opened.get());
/// engine.advance(Duration::from_millis(1));
/// assert!(opened.get());
///
/// drop(handle);
/// window.unmount();
/// ```
#[must_use = "dropping the handle cancels the timer"]
#[track_caller]
pub fn set_timeout(after: Duration, callback: impl FnOnce() + 'static) -> TimeoutHandle {
    let once = Cell::new(Some(callback));
    TimeoutHandle(schedule(after, Repeat::Once, move || {
        if let Some(callback) = once.take() {
            callback();
        }
    }))
}

/// Runs `callback` every `every`, until it is cancelled.
///
/// # Panics
///
/// In debug builds, when called outside a window's reactive scope.
///
/// ```
/// use core::cell::Cell;
/// use core::time::Duration;
/// use std::rc::Rc;
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::stub::StubHost;
/// use zgui_view::{HostHandle, provide_host, set_interval};
///
/// install().unwrap();
/// let engine = Rc::new(StubHost::default());
/// let window = Mounted::new();
/// window.with(|| provide_host(HostHandle::from_rc(engine.clone())));
///
/// let ticks = Rc::new(Cell::new(0));
/// let counter = Rc::clone(&ticks);
/// let handle = window.with(|| {
///     set_interval(Duration::from_millis(100), move || counter.set(counter.get() + 1))
/// });
///
/// engine.advance(Duration::from_millis(350));
/// assert_eq!(ticks.get(), 3);
///
/// handle.cancel();
/// engine.advance(Duration::from_millis(1000));
/// assert_eq!(ticks.get(), 3, "a cancelled interval stops");
/// window.unmount();
/// ```
#[must_use = "dropping the handle cancels the timer"]
#[track_caller]
pub fn set_interval(every: Duration, callback: impl FnMut() + 'static) -> IntervalHandle {
    let callback = std::cell::RefCell::new(callback);
    IntervalHandle(schedule(every, Repeat::Every, move || {
        (callback.borrow_mut())()
    }))
}

/// Runs `callback` once, during the next frame, with the moment that frame is for.
///
/// This is the way a view drives an animation of its own: the callback runs before the frame's
/// reactive work, a pending registration keeps the window's frames coming at the display's
/// refresh interval, and a callback that requests again from its own body runs once per frame
/// until it stops requesting. [`set_timeout`] with a zero delay runs at the start of the next
/// frame too, but buys no cadence — it is a one-off, where this is a heartbeat.
///
/// Called from a component's body. A listener that has to request a frame takes a [`Timers`] in
/// the body instead and carries it into the handler, because the scope this reads the clock from
/// is not the scope a listener runs in.
///
/// The registration is cancelled when the scope that made it goes away, as well as by dropping
/// the handle.
///
/// # Panics
///
/// In debug builds, when called outside a window's reactive scope, where there is no engine to
/// register with. In release it returns a handle that cancels nothing.
#[must_use = "dropping the handle cancels the frame callback"]
#[track_caller]
pub fn request_frame(callback: impl FnOnce(Timestamp) + 'static) -> FrameHandle {
    let Some(host) = current_host() else {
        debug_assert!(
            false,
            "a frame callback was requested outside a window's scope, where there is no engine \
             to register with"
        );
        return FrameHandle(Rc::new(RequestedFrame {
            host: HostHandle::new(crate::time::never::NeverSchedules),
            id: FrameRequestId::new(0),
            cancelled: Cell::new(true),
        }));
    };

    let requested = Rc::new(RequestedFrame {
        host: host.clone(),
        id: {
            let once = Cell::new(Some(callback));
            host.request_frame_callback(Rc::new(move |at| {
                if let Some(callback) = once.take() {
                    callback(at);
                }
            }))
        },
        cancelled: Cell::new(false),
    });

    on_cleanup_local({
        let requested = Rc::clone(&requested);
        move || requested.cancel()
    });
    FrameHandle(requested)
}

/// The body both scheduling functions share.
#[track_caller]
fn schedule(after: Duration, repeat: Repeat, callback: impl Fn() + 'static) -> Rc<Scheduled> {
    let Some(host) = current_host() else {
        debug_assert!(
            false,
            "a timer was scheduled outside a window's scope, where there is no clock to \
             schedule against"
        );
        return Rc::new(Scheduled {
            host: HostHandle::new(crate::time::never::NeverSchedules),
            id: TimerId::new(0),
            cancelled: Cell::new(true),
        });
    };

    let id = host.schedule(after, repeat, Rc::new(callback));
    let scheduled = Rc::new(Scheduled {
        host,
        id,
        cancelled: Cell::new(false),
    });

    on_cleanup_local({
        let scheduled = Rc::clone(&scheduled);
        move || scheduled.cancel()
    });
    scheduled
}

/// The engine a timer scheduled outside a window is attached to.
mod never {
    use core::ops::Range;
    use core::time::Duration;
    use std::rc::Rc;

    use zgui_geom::{Device, DevicePx, Rect};
    use zgui_reactive::{LocalStorage, Signal};
    use zgui_vocab::Timestamp;

    use crate::host::{
        FocusMove, FocusTrapId, FocusTrapOptions, FrameRequestId, Repeat, TimerId, ViewHost,
    };
    use crate::id::NodeId;
    use crate::scroll::{ScrollBehavior, ScrollPosition, ScrollTarget};

    /// An engine that does nothing, for the release-build path of a misplaced timer.
    pub(super) struct NeverSchedules;

    impl ViewHost for NeverSchedules {
        fn border_box(&self, _node: NodeId) -> Option<Rect<DevicePx, Device>> {
            None
        }

        fn window_box(&self, _node: NodeId) -> Option<Rect<DevicePx, Device>> {
            None
        }

        fn scale(&self) -> f32 {
            1.0
        }

        fn scroll_position(&self, _node: NodeId) -> ScrollPosition {
            ScrollPosition::default()
        }

        fn scroll_to(&self, _node: NodeId, _target: ScrollTarget, _behavior: ScrollBehavior) {}

        fn freeze_scrolling(&self, _frozen: bool) {}

        fn focus(&self, _node: NodeId) {}

        fn focused(&self) -> Signal<Option<NodeId>, LocalStorage> {
            Signal::derive_local(|| None)
        }

        fn contains(&self, _ancestor: NodeId, _other: NodeId) -> bool {
            false
        }

        fn focusables(&self, _root: NodeId) -> Vec<NodeId> {
            Vec::new()
        }

        fn focus_move(&self, _root: NodeId, _direction: FocusMove) -> Option<NodeId> {
            None
        }

        fn push_focus_trap(&self, _root: NodeId, _options: FocusTrapOptions) -> FocusTrapId {
            FocusTrapId::new(0)
        }

        fn pop_focus_trap(&self, _id: FocusTrapId) {}

        fn add_window_shortcut(&self, _node: NodeId) {}

        fn remove_window_shortcut(&self, _node: NodeId) {}

        fn selection(&self, _node: NodeId) -> Option<Range<usize>> {
            None
        }

        fn set_selection(&self, _node: NodeId, _range: Range<usize>) {}

        fn select_all(&self, _node: NodeId) {}

        fn set_value(&self, _node: NodeId, _text: &str) {}

        fn running_animations(&self, _node: NodeId) -> usize {
            0
        }

        fn schedule(&self, _after: Duration, _repeat: Repeat, _callback: Rc<dyn Fn()>) -> TimerId {
            TimerId::new(0)
        }

        fn cancel_timer(&self, _timer: TimerId) {}

        fn request_frame_callback(&self, _callback: Rc<dyn Fn(Timestamp)>) -> FrameRequestId {
            FrameRequestId::new(0)
        }

        fn cancel_frame_callback(&self, _request: FrameRequestId) {}

        fn precedes(&self, _first: NodeId, _second: NodeId) -> bool {
            false
        }

        fn install_stylesheet(&self, _name: &str, _css: &str) {}

        fn remove_stylesheet(&self, _name: &str) {}
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use core::time::Duration;
    use std::rc::Rc;

    use zgui_reactive::Mounted;
    use zgui_vocab::Timestamp;

    use super::{Timers, request_frame, set_interval, set_timeout};
    use crate::fixture::Fixture;

    #[test]
    fn a_clock_taken_in_a_scope_can_be_scheduled_against_from_outside_it() {
        // The case this exists for: a listener runs while an event is being delivered, which is
        // not the scope that built the component, so the free functions cannot find the clock
        // there. Every component that restarts a timer from a handler depends on this.
        let f = Fixture::new();
        let clock = f
            .window
            .with(|| Timers::current().expect("inside a window"));
        let fired = Rc::new(Cell::new(false));
        let flag = Rc::clone(&fired);

        // Deliberately outside `f.window.with(…)`, exactly as a listener is.
        let handle = clock.set_timeout(Duration::from_millis(200), move || flag.set(true));
        f.engine.advance(Duration::from_millis(199));
        assert!(!fired.get());
        f.engine.advance(Duration::from_millis(1));
        assert!(fired.get());
        drop(handle);
    }

    #[test]
    fn a_timer_is_cancelled_when_the_scope_that_scheduled_it_goes_away() {
        let f = Fixture::new();
        let fired = Rc::new(Cell::new(0));

        let counter = Rc::clone(&fired);
        let component = f.window.with(Mounted::new);
        let handle = f.window.with(|| {
            component.with(|| {
                set_timeout(Duration::from_millis(700), move || {
                    counter.set(counter.get() + 1);
                })
            })
        });
        assert_eq!(f.engine.live_timers(), 1);

        component.unmount();
        assert_eq!(f.engine.live_timers(), 0, "the cleanup cancelled it");

        f.engine.advance(Duration::from_secs(10));
        assert_eq!(fired.get(), 0);
        drop(handle);
        f.window.unmount();
    }

    #[test]
    fn dropping_the_handle_cancels_before_the_deadline() {
        let f = Fixture::new();
        let fired = Rc::new(Cell::new(0));
        let counter = Rc::clone(&fired);

        let handle = f.window.with(|| {
            set_timeout(Duration::from_millis(100), move || {
                counter.set(counter.get() + 1)
            })
        });
        drop(handle);

        f.engine.advance(Duration::from_millis(200));
        assert_eq!(fired.get(), 0);
        f.window.unmount();
    }

    #[test]
    fn a_one_shot_callback_fires_exactly_once() {
        let f = Fixture::new();
        let fired = Rc::new(Cell::new(0));
        let counter = Rc::clone(&fired);
        let handle = f.window.with(|| {
            set_timeout(Duration::from_millis(10), move || {
                counter.set(counter.get() + 1)
            })
        });

        f.engine.advance(Duration::from_millis(1000));
        assert_eq!(fired.get(), 1);
        assert_eq!(f.engine.live_timers(), 0);
        drop(handle);
        f.window.unmount();
    }

    #[test]
    fn a_frame_callback_runs_once_with_the_moment_the_frame_is_for() {
        let f = Fixture::new();
        let seen = Rc::new(Cell::new(None));
        let report = Rc::clone(&seen);
        let handle = f
            .window
            .with(|| request_frame(move |at| report.set(Some(at))));

        let at = Timestamp::from_origin(Duration::from_millis(16));
        f.engine.frame(at);
        assert_eq!(seen.get(), Some(at));

        seen.set(None);
        f.engine.frame(at + Duration::from_millis(16));
        assert_eq!(seen.get(), None, "a frame callback is one frame's");
        drop(handle);
        f.window.unmount();
    }

    #[test]
    fn a_callback_that_requests_again_waits_for_the_next_frame() {
        // The heartbeat shape every self-driven animation is written in: request again from the
        // body, run once per frame. A host that ran the new registration in the same frame would
        // spin the animation to completion inside one frame.
        let f = Fixture::new();
        let clock = f
            .window
            .with(|| Timers::current().expect("inside a window"));
        let runs = Rc::new(Cell::new(0));
        let slot: Rc<core::cell::RefCell<Option<super::FrameHandle>>> =
            Rc::new(core::cell::RefCell::new(None));

        fn arm(
            clock: &Timers,
            runs: &Rc<Cell<u32>>,
            slot: &Rc<core::cell::RefCell<Option<super::FrameHandle>>>,
        ) -> super::FrameHandle {
            let clock_again = clock.clone();
            let runs = Rc::clone(runs);
            let slot_again = Rc::clone(slot);
            clock.request_frame(move |_| {
                runs.set(runs.get() + 1);
                if runs.get() < 3 {
                    let next = arm(&clock_again, &runs, &slot_again);
                    *slot_again.borrow_mut() = Some(next);
                }
            })
        }
        *slot.borrow_mut() = Some(arm(&clock, &runs, &slot));

        for frame in 0..5 {
            f.engine
                .frame(Timestamp::from_origin(Duration::from_millis(16 * frame)));
        }
        assert_eq!(runs.get(), 3, "one run per frame, stopping by not asking");
        assert_eq!(f.engine.pending_frames(), 0);
        drop(slot);
        f.window.unmount();
    }

    #[test]
    fn dropping_the_handle_cancels_the_frame_callback() {
        let f = Fixture::new();
        let runs = Rc::new(Cell::new(0));
        let counter = Rc::clone(&runs);
        let handle = f
            .window
            .with(|| request_frame(move |_| counter.set(counter.get() + 1)));
        drop(handle);

        f.engine.frame(Timestamp::ORIGIN);
        assert_eq!(runs.get(), 0);
        assert_eq!(f.engine.pending_frames(), 0);
        f.window.unmount();
    }

    #[test]
    fn a_frame_callback_is_cancelled_when_the_scope_that_requested_it_goes_away() {
        let f = Fixture::new();
        let runs = Rc::new(Cell::new(0));

        let counter = Rc::clone(&runs);
        let component = f.window.with(Mounted::new);
        let handle = f
            .window
            .with(|| component.with(|| request_frame(move |_| counter.set(counter.get() + 1))));
        assert_eq!(f.engine.pending_frames(), 1);

        component.unmount();
        assert_eq!(f.engine.pending_frames(), 0, "the cleanup cancelled it");

        f.engine.frame(Timestamp::ORIGIN);
        assert_eq!(runs.get(), 0);
        drop(handle);
        f.window.unmount();
    }

    #[test]
    fn a_clock_taken_in_a_scope_can_request_frames_from_outside_it() {
        // A glide restarted from a wheel listener is the case: the listener runs while an event
        // is being delivered, which is not the scope that built the component.
        let f = Fixture::new();
        let clock = f
            .window
            .with(|| Timers::current().expect("inside a window"));
        let runs = Rc::new(Cell::new(0));
        let counter = Rc::clone(&runs);

        // Deliberately outside `f.window.with(…)`, exactly as a listener is.
        let handle = clock.request_frame(move |_| counter.set(counter.get() + 1));
        f.engine.frame(Timestamp::ORIGIN);
        assert_eq!(runs.get(), 1);
        drop(handle);
        f.window.unmount();
    }

    #[test]
    fn an_interval_fires_once_per_elapsed_period() {
        let f = Fixture::new();
        let ticks = Rc::new(Cell::new(0));
        let counter = Rc::clone(&ticks);
        let handle = f.window.with(|| {
            set_interval(Duration::from_millis(100), move || {
                counter.set(counter.get() + 1)
            })
        });

        f.engine.advance(Duration::from_millis(250));
        assert_eq!(ticks.get(), 2);
        f.engine.advance(Duration::from_millis(250));
        assert_eq!(ticks.get(), 5);

        handle.cancel();
        f.engine.advance(Duration::from_millis(1000));
        assert_eq!(ticks.get(), 5);
        f.window.unmount();
    }
}
