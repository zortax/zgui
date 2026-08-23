//! A selection written by this application is read back through the compositor.
//!
//! Not through the value it was written from: the write claims the selection, the compositor tells
//! whoever asks, and the read that follows goes out over the data device and comes back through a
//! pipe. What is asserted is the whole of that round trip — because every part of it is a place a
//! copy can silently stop working, and none of them fails loudly.
//!
//! The value is read back by a **second connection**, which is what makes it a round trip at all.
//! Reading it from the connection that wrote it is answered out of what this application already
//! holds, and would pass with nothing on the wire.

#[path = "support/mod.rs"]
mod support;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use zgui_platform::{
    AppHandler, ClipboardData, ClipboardFormat, ClipboardKind, IdlePolicy, PlatformCx,
    SurfaceEvent, SurfaceId, WakeReason,
};

/// What this application put on the clipboard.
const COPIED: &str = "a selection somebody copied — with an em dash in it";

/// What the other connection read back, once it has.
static READ_BACK: Mutex<Option<String>> = Mutex::new(None);

/// Whether this application ever succeeded in claiming the selection.
static CLAIMED: AtomicBool = AtomicBool::new(false);

/// How long the other connection is given to ask for the selection and be answered.
const PATIENCE: Duration = Duration::from_secs(4);

/// Copies, then has another connection paste.
struct CopyThenPaste<A: AppHandler> {
    /// The application.
    inner: A,
    /// Whether the copy has been made yet.
    copied: bool,
    /// How the deadline thread ends this loop.
    waker: Option<std::sync::Arc<dyn zgui_platform::Waker>>,
}

impl<A: AppHandler> AppHandler for CopyThenPaste<A> {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.waker.get_or_insert_with(|| cx.waker());
        self.inner.surfaces_available(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        let drawn = matches!(event, SurfaceEvent::RedrawRequested);
        self.inner.surface_event(cx, surface, event);
        if !drawn || self.copied {
            return;
        }
        self.copied = true;

        // A compositor grants a selection only against a serial from a press, so one has to
        // happen. A session with input devices produces them by itself; a headless one does not,
        // and is asked for a pointer of its own instead.
        support::virtual_pointer::click_over((1920, 1080), (400, 300));
        if let Some(waker) = self.waker.clone() {
            paste_from_elsewhere(waker);
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        // The press has landed, so the selection can be claimed. Asked here rather than from the
        // pointer's own thread, because a clipboard is the loop's.
        if matches!(
            reason,
            WakeReason::ReactiveWork { .. } | WakeReason::AppWork
        ) && READ_BACK
            .lock()
            .expect("the record is not poisoned")
            .is_none()
        {
            // Retried on every wake rather than once, because before the first press there is
            // nothing to quote — which is the compositor's rule and not a defect. The press is on
            // its way while this is being asked.
            if cx
                .clipboard()
                .write(
                    ClipboardKind::Standard,
                    ClipboardData::Text(COPIED.into()),
                    zgui_platform::ClipboardWriteOptions::default(),
                )
                .is_ok()
            {
                CLAIMED.store(true, Ordering::Relaxed);
            }
        }
        if matches!(reason, WakeReason::DeviceLost) {
            cx.request_exit();
        }
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        self.inner.idle(cx)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.inner.deadline_reached(cx);
    }

    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        self.inner.shutting_down(cx);
    }
}

/// Reads the selection from a connection of its own, then ends the loop.
///
/// A second connection because that is what makes it a round trip: a read on the connection that
/// wrote the selection is answered out of what this application already holds, without a byte on
/// the wire.
fn paste_from_elsewhere(waker: std::sync::Arc<dyn zgui_platform::Waker>) {
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + PATIENCE;
        while std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(150));
            // The loop has to keep running while this asks, because this application is the one
            // that answers: the selection is served from the loop's own thread.
            zgui_platform::Waker::wake(waker.as_ref(), WakeReason::AppWork);
            if let Some(found) = support::paste::from_a_second_connection() {
                *READ_BACK.lock().expect("the record is not poisoned") = Some(found);
                break;
            }
        }
        zgui_platform::Waker::wake(waker.as_ref(), WakeReason::DeviceLost);
    });
}

fn main() {
    const PROPERTY: &str = "a selection this application wrote is read back through the compositor";

    support::tracing();
    support::watchdog(PROPERTY);

    let handler = CopyThenPaste {
        inner: match support::application("clipboard") {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        },
        copied: false,
        waker: None,
    };

    let Some(mut app) = support::loop_for(PROPERTY, handler) else {
        return;
    };
    app.run().expect("the loop ran");

    if support::NO_DEVICE.load(Ordering::Relaxed) {
        support::skipped(
            PROPERTY,
            "this machine has no graphics device to draw through",
        );
        return;
    }
    if !CLAIMED.load(Ordering::Relaxed) {
        // A selection is granted only against a serial from a press. A headless session with no
        // input devices produces none and will not attach a virtual one, so there is nothing to
        // quote and the round trip cannot start. Said out loud rather than passed silently.
        support::skipped(
            PROPERTY,
            "this compositor grants no selection, because nothing here can press anything",
        );
        return;
    }

    let found = READ_BACK
        .lock()
        .expect("the record is not poisoned")
        .clone();
    assert_eq!(
        found.as_deref(),
        Some(COPIED),
        "what came back over the data device is not what was put on it"
    );
    let _ = ClipboardFormat::Text;
    support::passed(PROPERTY, "written, claimed, offered, read back");
}
