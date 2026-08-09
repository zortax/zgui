//! The desktop's clipboards, reached from application code.
//!
//! The object that can touch a clipboard is borrowed from the platform and is valid only while the
//! loop is inside a callback, which is nowhere a component runs. So this is a queue: a component
//! asks, the request is carried out on the next turn of the loop with the platform in hand, and a
//! read's answer comes back the same way.
//!
//! Reads never block. A selection belongs to whichever application last set it, and on a desktop
//! that answers over its own socket the owner may be slow, stopped, or gone — so a read is started
//! and answered later rather than waited for.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use zgui_platform::{
    ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardWriteOptions, PlatformCx, WakeReason, Waker,
};
use zgui_reactive::prelude::*;
use zgui_reactive::{LocalStorage, RwSignal, Signal};
use zgui_vocab::SharedString;

/// What a started read calls when its answer arrives.
type ReadCallback = Box<dyn FnOnce(Option<String>)>;

/// One thing asked of a clipboard.
enum ClipboardRequest {
    /// Put this on that clipboard.
    Write {
        /// Which clipboard.
        kind: ClipboardKind,
        /// What goes on it.
        data: ClipboardData,
        /// How it goes on.
        options: ClipboardWriteOptions,
    },
    /// Empty that clipboard.
    Clear {
        /// Which clipboard.
        kind: ClipboardKind,
    },
    /// Read that clipboard, and call this back with what was on it.
    Read {
        /// Which clipboard.
        kind: ClipboardKind,
        /// What to call with the answer.
        then: ReadCallback,
    },
}

/// The desktop's clipboards.
///
/// Cheap to clone, and reachable from anywhere inside a running application through
/// [`use_clipboard`]. Bound to the UI thread: work on another thread reaches this by posting back
/// with [`task::ui`](zgui_reactive::task::ui).
///
/// Every operation this desktop cannot carry out does nothing rather than failing, which is what
/// keeps a caller from needing a branch per platform. The clipboard a desktop has no equivalent for
/// — [`ClipboardKind::Primary`], away from Linux — is the common case:
/// [`PlatformCapabilities::clipboard_primary_selection`](zgui_platform::PlatformCapabilities) says
/// whether it is there, for a caller that would rather offer nothing than offer a command that does
/// nothing.
#[derive(Clone, Default)]
pub struct Clipboards {
    /// The shared state.
    inner: Rc<RefCell<Inner>>,
}

// By hand: the queue holds callbacks, which are not `Debug`, and what a reader wants is how much is
// outstanding rather than what it is.
impl core::fmt::Debug for Clipboards {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let inner = self.inner.borrow();
        formatter
            .debug_struct("Clipboards")
            .field("queued", &inner.queue.len())
            .field("pending_reads", &inner.pending.len())
            .finish()
    }
}

/// What a [`Clipboards`] holds.
#[derive(Default)]
struct Inner {
    /// What has been asked for and not yet carried out.
    queue: VecDeque<ClipboardRequest>,
    /// Reads that have been started and not yet answered.
    pending: FxHashMap<ClipboardSerial, ReadCallback>,
    /// How to wake the loop, once there is a loop to wake.
    platform: Option<Arc<dyn Waker>>,
}

impl Clipboards {
    /// An empty queue, with no loop to wake yet.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Puts `text` on `kind`.
    ///
    /// ```no_run
    /// use zgui_platform::ClipboardKind;
    /// use zgui_runtime::clipboard::use_clipboard;
    ///
    /// # fn example(selected: String) {
    /// // Copy-on-select, the way a terminal or an editor offers it on Linux.
    /// use_clipboard().set_text(ClipboardKind::Primary, selected);
    /// # }
    /// ```
    pub fn set_text(&self, kind: ClipboardKind, text: impl Into<SharedString>) {
        self.push(ClipboardRequest::Write {
            kind,
            data: ClipboardData::Text(text.into()),
            options: ClipboardWriteOptions::default(),
        });
    }

    /// Reads `kind`, and calls `then` with what was on it.
    ///
    /// The callback runs later, on the UI thread, once the desktop has answered. It is given `None`
    /// when the clipboard is empty, holds something that is not text, or belongs to a desktop that
    /// refuses this kind. An application that stops before the answer arrives drops the callback
    /// without calling it: no answer is better than a made-up one.
    ///
    /// ```no_run
    /// use zgui_platform::ClipboardKind;
    /// use zgui_runtime::clipboard::use_clipboard;
    ///
    /// # fn example() {
    /// use_clipboard().read_text(ClipboardKind::Standard, |text| {
    ///     if let Some(text) = text {
    ///         println!("pasted {text}");
    ///     }
    /// });
    /// # }
    /// ```
    pub fn read_text(&self, kind: ClipboardKind, then: impl FnOnce(Option<String>) + 'static) {
        self.push(ClipboardRequest::Read {
            kind,
            then: Box::new(then),
        });
    }

    /// Reads `kind` into a signal, which holds `None` until the answer arrives.
    ///
    /// The reactive shape of [`read_text`](Self::read_text), for a view that shows what was on the
    /// clipboard rather than acting on it. The signal keeps `None` when the read finds nothing, so
    /// a view cannot tell "not answered yet" from "nothing there" — a caller that must
    /// distinguish the two uses [`read_text`](Self::read_text) and writes its own state.
    pub fn read_text_signal(&self, kind: ClipboardKind) -> Signal<Option<String>, LocalStorage> {
        let answer = RwSignal::new_local(None);
        self.read_text(kind, move |text| {
            if let Some(text) = text {
                answer.set(Some(text));
            }
        });
        answer.into()
    }

    /// Empties `kind`.
    pub fn clear(&self, kind: ClipboardKind) {
        self.push(ClipboardRequest::Clear { kind });
    }

    /// Queues one request and wakes the loop.
    fn push(&self, request: ClipboardRequest) {
        self.inner.borrow_mut().queue.push_back(request);
        self.ping();
    }

    /// Whether nothing is waiting to be carried out.
    pub(crate) fn is_empty(&self) -> bool {
        self.inner.borrow().queue.is_empty()
    }

    /// Installs the platform's waker, once there is a loop to wake.
    pub(crate) fn set_platform(&self, waker: Arc<dyn Waker>) {
        self.inner.borrow_mut().platform = Some(waker);
    }

    /// Carries out everything asked for so far.
    ///
    /// One request is taken at a time and the borrow released before the platform is called, since
    /// a backend is free to answer a read at once — and the answer runs a callback, which may ask
    /// for more.
    pub(crate) fn drain(&self, cx: &dyn PlatformCx) {
        loop {
            // A statement of its own, and not the scrutinee of a `while let`: a temporary in a
            // `while let` lives until the end of the body, so the borrow would still be held when
            // the platform answers a read at once and this reaches for it again.
            let Some(request) = self.inner.borrow_mut().queue.pop_front() else {
                break;
            };
            match request {
                ClipboardRequest::Write {
                    kind,
                    data,
                    options,
                } => {
                    if let Err(error) = cx.clipboard().write(kind, data, options) {
                        tracing::debug!(?kind, ?error, "this desktop did not take the copy");
                    }
                }
                ClipboardRequest::Clear { kind } => {
                    if let Err(error) = cx.clipboard().clear(kind) {
                        tracing::debug!(?kind, ?error, "this desktop did not empty the clipboard");
                    }
                }
                ClipboardRequest::Read { kind, then } => {
                    let serial = cx.clipboard().read(kind, ClipboardFormat::Text);
                    // Recorded after the read rather than before, because the read is what names
                    // it. That is sound because a wake is delivered through the loop and a handler
                    // holds the runtime exclusively while it runs: the answer cannot be routed
                    // before this call returns.
                    self.inner.borrow_mut().pending.insert(serial, then);
                }
            }
        }
    }

    /// Answers the read `serial` named.
    pub(crate) fn resolve(
        &self,
        serial: ClipboardSerial,
        result: Result<ClipboardData, ClipboardError>,
    ) {
        let Some(then) = self.inner.borrow_mut().pending.remove(&serial) else {
            tracing::debug!(
                ?serial,
                "an answer arrived for a read nothing was waiting on"
            );
            return;
        };
        // Outside the borrow: the callback may start another read.
        then(
            result
                .ok()
                .and_then(|data| data.as_text().map(str::to_owned)),
        );
    }

    /// Wakes the loop, so that what was queued is carried out.
    fn ping(&self) {
        let platform = self.inner.borrow().platform.clone();
        if let Some(platform) = platform {
            platform.wake(WakeReason::AppWork);
        }
    }
}

/// The desktop's clipboards.
///
/// # Panics
///
/// Panics when called outside a running application.
pub fn use_clipboard() -> Clipboards {
    try_use_clipboard().expect("this code is not running inside an application")
}

/// The desktop's clipboards, if there is an application running.
pub fn try_use_clipboard() -> Option<Clipboards> {
    zgui_reactive::use_local_context::<Clipboards>()
}

#[cfg(test)]
mod tests {
    use super::Clipboards;
    use std::cell::RefCell;
    use std::rc::Rc;
    use zgui_platform::{ClipboardData, ClipboardError, ClipboardKind, ClipboardSerial};

    #[test]
    fn a_request_is_queued_until_it_is_drained() {
        let clipboards = Clipboards::new();
        assert!(clipboards.is_empty());
        clipboards.set_text(ClipboardKind::Standard, "copied");
        assert!(!clipboards.is_empty());
    }

    #[test]
    fn an_answer_reaches_the_callback_that_started_the_read() {
        let clipboards = Clipboards::new();
        let seen: Rc<RefCell<Option<Option<String>>>> = Rc::new(RefCell::new(None));

        // The read is started by `drain`; here the pending entry is made directly, because the
        // point is what `resolve` does with it.
        let answer = Rc::clone(&seen);
        clipboards.inner.borrow_mut().pending.insert(
            ClipboardSerial::new(1),
            Box::new(move |text| *answer.borrow_mut() = Some(text)),
        );
        clipboards.resolve(ClipboardSerial::new(1), Ok(ClipboardData::from("pasted")));

        assert_eq!(*seen.borrow(), Some(Some("pasted".to_owned())));
    }

    #[test]
    fn a_refused_read_answers_with_nothing_rather_than_failing() {
        let clipboards = Clipboards::new();
        let seen: Rc<RefCell<Option<Option<String>>>> = Rc::new(RefCell::new(None));

        let answer = Rc::clone(&seen);
        clipboards.inner.borrow_mut().pending.insert(
            ClipboardSerial::new(2),
            Box::new(move |text| *answer.borrow_mut() = Some(text)),
        );
        clipboards.resolve(
            ClipboardSerial::new(2),
            Err(ClipboardError::Empty(ClipboardKind::Standard)),
        );

        assert_eq!(*seen.borrow(), Some(None));
    }

    #[test]
    fn an_answer_nothing_is_waiting_on_calls_nothing() {
        let clipboards = Clipboards::new();
        // No pending entry, and no panic.
        clipboards.resolve(ClipboardSerial::new(9), Ok(ClipboardData::from("stray")));
    }

    #[test]
    fn a_read_the_application_stopped_before_answering_drops_its_callback_uncalled() {
        /// Flips a flag when it is dropped.
        struct Flag(Rc<RefCell<bool>>);
        impl Drop for Flag {
            fn drop(&mut self) {
                *self.0.borrow_mut() = true;
            }
        }

        let dropped = Rc::new(RefCell::new(false));
        let called = Rc::new(RefCell::new(false));
        {
            let clipboards = Clipboards::new();
            let flag = Flag(Rc::clone(&dropped));
            let called = Rc::clone(&called);
            clipboards.inner.borrow_mut().pending.insert(
                ClipboardSerial::new(3),
                Box::new(move |_| {
                    let _flag = &flag;
                    *called.borrow_mut() = true;
                }),
            );
        }

        assert!(*dropped.borrow(), "the callback was dropped");
        assert!(!*called.borrow(), "and no answer was made up for it");
    }
}
