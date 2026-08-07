//! Deferred window commands: how code inside a frame asks for a window.
//!
//! Opening a window needs a platform context, and one exists only inside a handler callback. Code
//! that wants a window — a menu item's listener, an effect, a timer — runs inside a *frame*, where
//! the runtime is already borrowed and no surface can be made. So the request is queued here and
//! carried out by the next turn of the loop that can, which the queue asks for by waking it.
//!
//! Single-threaded on purpose. Every legal caller is a frame, a handler, a timer or an effect on
//! the UI thread; the one part that crosses threads is the wake, and that is the platform's own
//! [`Waker`], which is already thread-safe.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use zgui_platform::{SurfaceAttributes, SurfaceId, WakeReason, Waker};

use crate::app::ViewFactory;
use crate::window::WindowContent;

/// Names one window for its whole life, from before it has a surface to after it has lost one.
///
/// Distinct from [`SurfaceId`] because a window outlives its surfaces: a platform that suspends
/// takes every surface away and gives new ones back, and a name that changed under an application
/// holding it would be no name at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowToken(u64);

impl WindowToken {
    /// The number behind the token, for a caller that has to print or key on one.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// What one window should be, kept for as long as the window is wanted.
///
/// Retained rather than consumed at open, because a platform that suspends drops every surface and
/// then asks for them again: a specification consumed by the first open would leave a resumed
/// application with no windows at all.
pub struct WindowSpec {
    /// What the surface should be.
    pub attributes: SurfaceAttributes,
    /// What the window should hold.
    pub options: WindowContent,
    /// What builds the view.
    ///
    /// `FnMut` rather than `FnOnce` precisely so that a resume can build it again. Window-local
    /// state does not survive a suspend by design; state provided above the windows does.
    pub view: ViewFactory,
    /// What is asked before the user's close is carried out.
    pub close: Rc<RefCell<CloseCallbacks>>,
    /// The handle the caller already holds this window by, when it has one.
    ///
    /// [`Windows::open`](crate::windows::Windows::open) answers with a handle before the window
    /// exists, so the handle it answered with is the one the window has to be filled into. A
    /// specification made without one is given a handle when it opens.
    pub(crate) handle: Option<crate::windows::WindowHandle>,
}

impl WindowSpec {
    /// A window that should be `attributes`, hold `options`, and build `view`.
    pub fn new(attributes: SurfaceAttributes, options: WindowContent, view: ViewFactory) -> Self {
        Self {
            attributes,
            options,
            view,
            close: Rc::new(RefCell::new(CloseCallbacks::default())),
            handle: None,
        }
    }

    /// The same, asking `close` before a close the user asked for is carried out.
    pub fn with_close_callbacks(mut self, close: Rc<RefCell<CloseCallbacks>>) -> Self {
        self.close = close;
        self
    }

    /// The same, filled into a handle the caller already holds.
    pub(crate) fn with_handle(mut self, handle: crate::windows::WindowHandle) -> Self {
        self.handle = Some(handle);
        self
    }
}

/// What to do about a close the user asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloseResponse {
    /// Close the window.
    #[default]
    Close,
    /// Keep it open.
    ///
    /// For the application that has something to save first. Whatever vetoes owes the user a way
    /// out: a window that vetoes unconditionally cannot be closed at all.
    Veto,
}

/// The callbacks one window consults before closing.
///
/// A list rather than one slot, because a dialog deep in a view has as much right to ask as the
/// component that opened the window. Any one of them vetoing keeps the window.
#[derive(Default)]
pub struct CloseCallbacks {
    /// The callbacks, with the identity each was registered under.
    entries: Vec<(u64, Box<dyn FnMut() -> CloseResponse>)>,
    /// The next identity, never reused.
    next: u64,
}

impl CloseCallbacks {
    /// Registers `callback`, answering with the identity to remove it by.
    pub fn insert(&mut self, callback: Box<dyn FnMut() -> CloseResponse>) -> u64 {
        self.next += 1;
        let id = self.next;
        self.entries.push((id, callback));
        id
    }

    /// Removes the callback registered under `id`, if it is still there.
    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|(entry, _)| *entry != id);
    }

    /// Asks every callback, and answers whether any of them refused.
    ///
    /// All of them run, even after one has refused: a callback is how a component learns the user
    /// tried to close, and skipping the rest would make that depend on registration order.
    pub fn ask(&mut self) -> CloseResponse {
        let mut answer = CloseResponse::Close;
        for (_, callback) in &mut self.entries {
            if callback() == CloseResponse::Veto {
                answer = CloseResponse::Veto;
            }
        }
        answer
    }

    /// How many callbacks are registered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One thing the application asked its own runtime to do.
pub(crate) enum WindowCommand {
    /// Open a window.
    Open {
        /// The name it will be known by.
        token: WindowToken,
        /// What it should be.
        spec: Box<WindowSpec>,
    },
    /// Close a window, without asking what it thinks about that.
    Close(WindowToken),
    /// Stop the application.
    Quit,
}

/// Where a window is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowStatus {
    /// Asked for, and not yet opened.
    Pending,
    /// Open, on the surface named.
    Open(SurfaceId),
    /// Closed, or never opened at all.
    Closed,
}

/// The queue an application pushes window work onto.
///
/// Cheap to clone and reachable from any window through the application's own reactive scope, which
/// is what makes `open` callable from inside any view.
#[derive(Clone, Default)]
pub struct WindowCommands {
    /// The shared state.
    inner: Rc<RefCell<Inner>>,
}

/// What a [`WindowCommands`] holds.
#[derive(Default)]
struct Inner {
    /// What has been asked for and not yet carried out.
    queue: VecDeque<WindowCommand>,
    /// The next token, never reused.
    next: u64,
    /// Where each named window is in its life.
    status: FxHashMap<WindowToken, WindowStatus>,
    /// How to wake the loop, once there is a loop to wake.
    platform: Option<Arc<dyn Waker>>,
}

impl WindowCommands {
    /// An empty queue, with no loop to wake yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks for a window, answering with the name it will be known by.
    ///
    /// The window does not exist yet. Whatever is holding the token may address it at once — every
    /// operation on a window that is not open is a silent no-op — and the surface appears on the
    /// next turn of the loop.
    pub fn open(&self, spec: WindowSpec) -> WindowToken {
        let token = self.mint();
        self.open_named(token, spec);
        token
    }

    /// The same, for a window that has already been named.
    ///
    /// What [`Windows::open`](crate::windows::Windows::open) uses: it answers with a handle before
    /// the window exists, so the name has to be minted before the request is queued.
    pub(crate) fn open_named(&self, token: WindowToken, spec: WindowSpec) {
        self.inner
            .borrow_mut()
            .queue
            .push_back(WindowCommand::Open {
                token,
                spec: Box::new(spec),
            });
        self.ping();
    }

    /// Asks for a window to be closed.
    ///
    /// The window's own close callbacks are not consulted: those answer the *user* asking to close,
    /// and an application closing its own window has already decided.
    pub fn close(&self, token: WindowToken) {
        self.inner
            .borrow_mut()
            .queue
            .push_back(WindowCommand::Close(token));
        self.ping();
    }

    /// Asks for the application to stop.
    pub fn quit(&self) {
        self.inner.borrow_mut().queue.push_back(WindowCommand::Quit);
        self.ping();
    }

    /// Where a named window is in its life.
    pub fn status(&self, token: WindowToken) -> WindowStatus {
        self.inner
            .borrow()
            .status
            .get(&token)
            .copied()
            .unwrap_or(WindowStatus::Closed)
    }

    /// Whether nothing is waiting to be carried out.
    pub(crate) fn is_empty(&self) -> bool {
        self.inner.borrow().queue.is_empty()
    }

    /// Takes the next command, if there is one.
    pub(crate) fn pop(&self) -> Option<WindowCommand> {
        self.inner.borrow_mut().queue.pop_front()
    }

    /// Installs the platform's waker, once there is a loop to wake.
    pub(crate) fn set_platform(&self, waker: Arc<dyn Waker>) {
        self.inner.borrow_mut().platform = Some(waker);
    }

    /// Records that a window is now open on `surface`.
    pub(crate) fn note_opened(&self, token: WindowToken, surface: SurfaceId) {
        self.inner
            .borrow_mut()
            .status
            .insert(token, WindowStatus::Open(surface));
    }

    /// Records that a window is no longer open.
    pub(crate) fn note_closed(&self, token: WindowToken) {
        self.inner
            .borrow_mut()
            .status
            .insert(token, WindowStatus::Closed);
    }

    /// Mints a token for a window the runtime opens itself, without queueing anything.
    ///
    /// What the window an application launched with is named by: it is opened directly rather than
    /// asked for, and still has to have a name for the same reasons every other window does.
    pub(crate) fn mint(&self) -> WindowToken {
        let mut inner = self.inner.borrow_mut();
        inner.next += 1;
        let token = WindowToken(inner.next);
        inner.status.insert(token, WindowStatus::Pending);
        token
    }

    /// Wakes the loop so that what was just queued is carried out.
    ///
    /// Unconditional. A push made inside a frame produces one wake that drains to nothing, which is
    /// far cheaper than the alternative: routing this through the frame gate would turn "open a
    /// window" into "the window we are in owes another frame", which is not the same statement and
    /// would lose the request entirely when that frame is the last one.
    fn ping(&self) {
        let platform = self.inner.borrow().platform.clone();
        if let Some(platform) = platform {
            platform.wake(WakeReason::AppWork);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CloseCallbacks, CloseResponse, WindowCommands, WindowStatus};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn a_window_asked_for_before_there_is_a_loop_is_still_asked_for() {
        // The application describes its windows before the platform exists. A push with nothing to
        // wake has to queue rather than be dropped, or those windows never open.
        let commands = WindowCommands::new();
        let token = commands.mint();
        assert_eq!(commands.status(token), WindowStatus::Pending);
        commands.close(token);
        assert!(!commands.is_empty(), "the command was dropped");
    }

    #[test]
    fn a_name_is_never_reused() {
        let commands = WindowCommands::new();
        let first = commands.mint();
        let second = commands.mint();
        assert_ne!(first, second);
    }

    #[test]
    fn a_window_nobody_named_is_closed_as_far_as_anyone_can_tell() {
        let commands = WindowCommands::new();
        let token = commands.mint();
        assert_eq!(commands.status(token), WindowStatus::Pending);
        commands.note_closed(token);
        assert_eq!(commands.status(token), WindowStatus::Closed);
    }

    #[test]
    fn every_callback_is_asked_even_after_one_has_refused() {
        // A callback is how a component learns the user tried to close. Stopping at the first veto
        // would make that depend on the order they were registered in.
        let mut callbacks = CloseCallbacks::default();
        let asked = Rc::new(Cell::new(0));
        let counted = Rc::clone(&asked);
        callbacks.insert(Box::new(move || {
            counted.set(counted.get() + 1);
            CloseResponse::Veto
        }));
        let counted = Rc::clone(&asked);
        callbacks.insert(Box::new(move || {
            counted.set(counted.get() + 1);
            CloseResponse::Close
        }));

        assert_eq!(callbacks.ask(), CloseResponse::Veto);
        assert_eq!(asked.get(), 2);
    }

    #[test]
    fn a_removed_callback_is_not_asked() {
        let mut callbacks = CloseCallbacks::default();
        let id = callbacks.insert(Box::new(|| CloseResponse::Veto));
        assert_eq!(callbacks.ask(), CloseResponse::Veto);
        callbacks.remove(id);
        assert!(callbacks.is_empty());
        assert_eq!(callbacks.ask(), CloseResponse::Close);
    }

    #[test]
    fn nothing_registered_means_the_window_closes() {
        let mut callbacks = CloseCallbacks::default();
        assert_eq!(callbacks.ask(), CloseResponse::Close);
    }
}
