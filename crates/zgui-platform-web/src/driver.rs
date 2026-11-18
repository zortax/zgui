//! The turn: events in, wakes, the idle policy, the next callback.
//!
//! A native backend blocks in a loop and each pass of the loop is a turn. A page has no loop, so
//! each of its callbacks runs one turn here and returns:
//!
//! 1. the events the callback brought, and every event they caused;
//! 2. the wakes that arrived since the last turn;
//! 3. the idle policy, which decides the next callback.
//!
//! Three callbacks bring the application back. An animation frame draws each surface that asked
//! for a frame, and only then: an idle application has no frame callback at all. A timer answers
//! a deadline. A message on a channel answers a wake, in a task of its own, so that the page can
//! paint and read input between two turns.
//!
//! An event that arrives while a turn runs (the page dispatches some events synchronously, a
//! focus change among them) joins the queue of that turn.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Event, HtmlCanvasElement, MessageChannel};
use web_time::Instant;
use zgui_platform::{
    AppHandler, Clock, ColorScheme, CursorStyle, IdlePolicy, Park, Parked, PlatformCapabilities,
    PlatformError, PresentationTiming, Surface, SurfaceAttributes, SurfaceEvent, SurfaceId,
    TextInput, Unsupported, Waker,
};
use zgui_vocab::{
    ImeEvent, Key, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerId,
    PointerKind, WheelEvent,
};

use crate::clipboard::PageClipboard;
use crate::clock::PageClock;
use crate::cx::PageCx;
use crate::input::{keyboard, pointer, wheel};
use crate::listen::Listener;
use crate::surface::{CanvasSurface, cursor_name};
use crate::text_input::{self, Edit};
use crate::waker::PageWaker;

thread_local! {
    /// The application of this page. A page runs one.
    static DRIVER: RefCell<Option<Rc<Driver>>> = const { RefCell::new(None) };
}

/// Runs `visit` on the page's driver, where one is installed.
pub(crate) fn with<R>(visit: impl FnOnce(&Driver) -> R) -> Option<R> {
    let driver = DRIVER.with(|driver| driver.borrow().clone())?;
    Some(visit(&driver))
}

/// Asks for a turn in a task of its own.
pub(crate) fn schedule_turn() {
    with(Driver::post_turn);
}

/// Installs `handler` on `canvas` and returns. The page drives the application from then on.
///
/// `owns_page` says that the canvas fills the page, so the page's title follows the window's.
/// `on_exit` runs after the application received its shutdown callback, which happens when it
/// asks to exit or when [`stop`] is called.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when this page already runs an application or has no
/// window.
pub fn run(
    handler: Box<dyn AppHandler>,
    canvas: HtmlCanvasElement,
    owns_page: bool,
    on_exit: impl FnOnce() + 'static,
) -> Result<(), PlatformError> {
    if DRIVER.with(|driver| driver.borrow().is_some()) {
        return Err(PlatformError::Backend(
            "this page already runs an application".to_owned(),
        ));
    }
    let driver = Driver::new(handler, canvas, owns_page, Box::new(on_exit))?;
    DRIVER.with(|installed| *installed.borrow_mut() = Some(Rc::clone(&driver)));
    driver.incoming.borrow_mut().push_back(Incoming::Start);
    driver.pump();
    Ok(())
}

/// Asks the application of this page to exit.
pub fn stop() {
    with(Driver::request_exit);
}

/// The canvas behind a surface of this page.
pub fn canvas_of(id: SurfaceId) -> Option<HtmlCanvasElement> {
    with(|driver| driver.entry(id).map(|entry| entry.canvas.clone())).flatten()
}

/// The callback of a size observer.
type SizeCallback = Closure<dyn FnMut(js_sys::Array)>;

/// The callback of an animation frame.
type FrameCallback = Closure<dyn FnMut(f64)>;

/// The callback of a message port.
type EventCallback = Closure<dyn FnMut(Event)>;

/// One thing a turn has to hand the application.
enum Incoming {
    /// Surfaces may be created.
    Start,
    /// Something happened to a surface.
    Surface(SurfaceId, SurfaceEvent),
    /// A surface gets the frame it asked for, if it still wants one.
    Redraw(SurfaceId),
    /// A deadline was reached.
    Deadline,
    /// A surface was destroyed and is forgotten after the application heard of it.
    Forget(SurfaceId),
}

/// One canvas and what listens to it.
struct Entry {
    /// What the application holds.
    surface: Arc<CanvasSurface>,
    /// The element.
    canvas: HtmlCanvasElement,
    /// The modifiers the application was last told about.
    modifiers: Cell<Modifiers>,
    /// The listeners on the canvas, removed when the entry goes.
    listeners: RefCell<Vec<Listener>>,
    /// The hidden text area, made when a field is first typed into.
    area: RefCell<Option<web_sys::HtmlTextAreaElement>>,
    /// The listeners on the text area.
    area_listeners: RefCell<Vec<Listener>>,
    /// Whether a field of the application is being typed into.
    typing: Cell<bool>,
    /// Whether an input method is composing in the text area.
    composing: Cell<bool>,
    /// The size observer and its callback.
    observer: RefCell<Option<(web_sys::ResizeObserver, SizeCallback)>>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.area_listeners.borrow_mut().clear();
        if let Some(area) = self.area.borrow_mut().take() {
            area.remove();
        }
        if let Some((observer, _)) = self.observer.borrow_mut().take() {
            observer.disconnect();
        }
    }
}

/// How the animation frames of this page are spaced.
#[derive(Clone, Copy, Default)]
struct Cadence {
    /// When the last frame began.
    last: Option<Instant>,
    /// The page's own stamp of that frame, in milliseconds.
    last_ms: f64,
    /// The smoothed interval between two frames, in milliseconds.
    interval_ms: f64,
    /// How many intervals were measured.
    samples: u32,
}

/// The state of the page's loop.
pub(crate) struct Driver {
    /// A way to reach this state from a callback.
    this: Weak<Driver>,
    /// The application. Taken out for the length of a turn.
    handler: RefCell<Option<Box<dyn AppHandler>>>,
    /// Whether a turn is running.
    in_turn: Cell<bool>,
    /// What the running or the next turn has to deliver.
    incoming: RefCell<VecDeque<Incoming>>,
    /// The deadline arithmetic every backend shares.
    park: RefCell<Park>,
    /// The clock.
    clock: Arc<PageClock>,
    /// The waker.
    waker: Arc<PageWaker>,
    /// The clipboard.
    clipboard: PageClipboard,
    /// What this backend can do.
    capabilities: PlatformCapabilities,
    /// The page's color scheme.
    scheme: Cell<ColorScheme>,
    /// Whether the application asked to exit.
    exiting: Cell<bool>,
    /// Whether the canvas fills the page.
    owns_page: bool,
    /// Whether the command key is this machine's shortcut modifier.
    apple: bool,
    /// The canvas the first surface takes.
    canvas: RefCell<Option<HtmlCanvasElement>>,
    /// The surfaces.
    entries: RefCell<Vec<Rc<Entry>>>,
    /// The next surface identifier.
    next_surface: Cell<u64>,
    /// The animation frame callback.
    frame_callback: RefCell<Option<FrameCallback>>,
    /// The animation frame that was asked for and has not run.
    frame_pending: Cell<Option<i32>>,
    /// The spacing of the frames.
    cadence: Cell<Cadence>,
    /// The timer callback.
    timer_callback: RefCell<Option<Closure<dyn FnMut()>>>,
    /// The armed timer and the deadline it answers.
    timer: Cell<Option<(i32, Instant)>>,
    /// The channel a wake is posted on, and its callback.
    channel: RefCell<Option<(MessageChannel, EventCallback)>>,
    /// Whether a wake message is on its way.
    turn_posted: Cell<bool>,
    /// The listeners on the page itself.
    page_listeners: RefCell<Vec<Listener>>,
    /// Runs after the application shut down.
    on_exit: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl Driver {
    /// The state of a page that runs `handler` on `canvas`.
    fn new(
        handler: Box<dyn AppHandler>,
        canvas: HtmlCanvasElement,
        owns_page: bool,
        on_exit: Box<dyn FnOnce()>,
    ) -> Result<Rc<Self>, PlatformError> {
        let window = web_sys::window()
            .ok_or_else(|| PlatformError::Backend("this context has no window".to_owned()))?;
        let waker = Arc::new(PageWaker::default());
        let mut capabilities = PlatformCapabilities::none();
        capabilities.system_color_scheme = true;
        capabilities.ime = true;
        capabilities.ime_purpose_hints = true;
        capabilities.decorations = zgui_platform::DecorationSource::Platform;

        let dark = window
            .match_media("(prefers-color-scheme: dark)")
            .ok()
            .flatten();
        let scheme = scheme_of(dark.as_ref().is_some_and(web_sys::MediaQueryList::matches));

        let driver = Rc::new_cyclic(|this: &Weak<Self>| Self {
            this: this.clone(),
            handler: RefCell::new(Some(handler)),
            in_turn: Cell::new(false),
            incoming: RefCell::new(VecDeque::new()),
            park: RefCell::new(Park::new()),
            clock: Arc::new(PageClock::new()),
            clipboard: PageClipboard::new(Arc::clone(&waker) as Arc<dyn Waker>),
            waker,
            capabilities,
            scheme: Cell::new(scheme),
            exiting: Cell::new(false),
            owns_page,
            apple: is_apple(&window),
            canvas: RefCell::new(Some(canvas)),
            entries: RefCell::new(Vec::new()),
            next_surface: Cell::new(1),
            frame_callback: RefCell::new(None),
            frame_pending: Cell::new(None),
            cadence: Cell::new(Cadence::default()),
            timer_callback: RefCell::new(None),
            timer: Cell::new(None),
            channel: RefCell::new(None),
            turn_posted: Cell::new(false),
            page_listeners: RefCell::new(Vec::new()),
            on_exit: RefCell::new(Some(on_exit)),
        });
        driver.install_callbacks(&window, dark.as_ref());
        Ok(driver)
    }

    /// Creates the callbacks the page calls, each holding this state weakly.
    fn install_callbacks(&self, window: &web_sys::Window, dark: Option<&web_sys::MediaQueryList>) {
        let this = self.this.clone();
        *self.frame_callback.borrow_mut() = Some(Closure::new(move |stamp: f64| {
            if let Some(driver) = this.upgrade() {
                driver.on_frame(stamp);
            }
        }));

        let this = self.this.clone();
        *self.timer_callback.borrow_mut() = Some(Closure::new(move || {
            if let Some(driver) = this.upgrade() {
                driver.on_timer();
            }
        }));

        if let Ok(channel) = MessageChannel::new() {
            let this = self.this.clone();
            let callback = Closure::<dyn FnMut(Event)>::new(move |_| {
                if let Some(driver) = this.upgrade() {
                    driver.turn_posted.set(false);
                    driver.pump();
                }
            });
            channel
                .port1()
                .set_onmessage(Some(callback.as_ref().unchecked_ref()));
            *self.channel.borrow_mut() = Some((channel, callback));
        }

        let mut listeners = Vec::new();
        if let Some(dark) = dark {
            let this = self.this.clone();
            let query = dark.clone();
            listeners.push(Listener::new(dark, "change", move |_| {
                if let Some(driver) = this.upgrade() {
                    let scheme = scheme_of(query.matches());
                    driver.scheme.set(scheme);
                    driver.deliver_all(|| SurfaceEvent::ColorSchemeChanged(scheme));
                }
            }));
        }
        if let Some(document) = window.document() {
            let this = self.this.clone();
            let page = document.clone();
            listeners.push(Listener::new(&document, "visibilitychange", move |_| {
                if let Some(driver) = this.upgrade() {
                    let hidden = page.hidden();
                    driver.deliver_all(|| SurfaceEvent::Occluded(hidden));
                }
            }));
        }
        // A browser without device-pixel observation reports a zoom change only here.
        let this = self.this.clone();
        listeners.push(Listener::new(window, "resize", move |_| {
            if let Some(driver) = this.upgrade() {
                let entries: Vec<Rc<Entry>> = driver.entries.borrow().clone();
                for entry in entries {
                    driver.measure(&entry, None);
                }
            }
        }));
        *self.page_listeners.borrow_mut() = listeners;
    }

    // --- the turn -------------------------------------------------------------------------

    /// Queues `incoming` and runs a turn, unless one is running, which then delivers it.
    fn deliver(&self, incoming: Incoming) {
        self.incoming.borrow_mut().push_back(incoming);
        self.pump();
    }

    /// Queues one event for every surface and runs a turn.
    fn deliver_all(&self, mut event: impl FnMut() -> SurfaceEvent) {
        let ids: Vec<SurfaceId> = self
            .entries
            .borrow()
            .iter()
            .map(|entry| entry.surface.id())
            .collect();
        {
            let mut incoming = self.incoming.borrow_mut();
            for id in ids {
                incoming.push_back(Incoming::Surface(id, event()));
            }
        }
        self.pump();
    }

    /// Runs one turn.
    fn pump(&self) {
        if self.in_turn.replace(true) {
            return;
        }
        let Some(mut handler) = self.handler.borrow_mut().take() else {
            self.in_turn.set(false);
            return;
        };
        let cx = PageCx { driver: self };

        self.drain(handler.as_mut(), &cx);
        for reason in self.waker.take() {
            handler.wake(&cx, reason);
            self.drain(handler.as_mut(), &cx);
        }

        if self.exiting.get() {
            handler.shutting_down(&cx);
            drop(handler);
            self.in_turn.set(false);
            self.teardown();
            return;
        }

        let policy: IdlePolicy = handler.idle(&cx);
        let now = self.clock.now();
        let install = self.park.borrow_mut().install(policy, now);
        let answered = install.overdue().is_some();
        let parked = install.park(|_| handler.deadline_reached(&cx));
        match parked {
            Parked::Until(deadline) => self.arm(deadline, now),
            Parked::Never => {
                self.disarm();
                self.post_turn();
            }
            _ => self.disarm(),
        }
        // An overdue deadline was answered inside this turn, and what it asked for is the next
        // turn's to deliver.
        if answered || !self.incoming.borrow().is_empty() {
            self.post_turn();
        }

        *self.handler.borrow_mut() = Some(handler);
        self.in_turn.set(false);
    }

    /// Delivers everything that is queued, including what the deliveries queue.
    fn drain(&self, handler: &mut dyn AppHandler, cx: &PageCx<'_>) {
        loop {
            let Some(next) = self.incoming.borrow_mut().pop_front() else {
                return;
            };
            match next {
                Incoming::Start => handler.surfaces_available(cx),
                Incoming::Surface(id, event) => {
                    let resized = matches!(
                        event,
                        SurfaceEvent::Resized(_) | SurfaceEvent::ScaleFactorChanged { .. }
                    );
                    handler.surface_event(cx, id, event);
                    // A size change is observed after the frame callbacks and before the page
                    // paints. The new size is drawn in the same callback, so the page never
                    // shows the old frame stretched to the new size.
                    if resized
                        && self
                            .entry(id)
                            .is_some_and(|entry| entry.surface.wants_redraw())
                    {
                        self.incoming.borrow_mut().push_front(Incoming::Redraw(id));
                    }
                }
                Incoming::Redraw(id) => {
                    if self
                        .entry(id)
                        .is_some_and(|entry| entry.surface.take_redraw())
                    {
                        handler.surface_event(cx, id, SurfaceEvent::RedrawRequested);
                    }
                }
                Incoming::Deadline => handler.deadline_reached(cx),
                Incoming::Forget(id) => {
                    handler.surface_event(cx, id, SurfaceEvent::Destroyed);
                    self.entries
                        .borrow_mut()
                        .retain(|entry| entry.surface.id() != id);
                }
            }
        }
    }

    /// Asks for a turn in a task of its own.
    pub(crate) fn post_turn(&self) {
        if self.turn_posted.replace(true) {
            return;
        }
        let posted = self
            .channel
            .borrow()
            .as_ref()
            .is_some_and(|(channel, _)| channel.port2().post_message(&JsValue::NULL).is_ok());
        if !posted {
            self.turn_posted.set(false);
            tracing::warn!(target: "zgui::platform", "the page refused a wake message");
        }
    }

    /// Asks for an animation frame, unless one is on its way.
    pub(crate) fn schedule_frame(&self) {
        if self.frame_pending.get().is_some() {
            return;
        }
        let callback = self.frame_callback.borrow();
        let (Some(window), Some(callback)) = (web_sys::window(), callback.as_ref()) else {
            return;
        };
        if let Ok(handle) = window.request_animation_frame(callback.as_ref().unchecked_ref()) {
            self.frame_pending.set(Some(handle));
        }
    }

    /// An animation frame began.
    fn on_frame(&self, stamp: f64) {
        self.frame_pending.set(None);
        self.measure_cadence(stamp);
        let wanting: Vec<SurfaceId> = self
            .entries
            .borrow()
            .iter()
            .filter(|entry| entry.surface.wants_redraw())
            .map(|entry| entry.surface.id())
            .collect();
        {
            let mut incoming = self.incoming.borrow_mut();
            for id in wanting {
                incoming.push_back(Incoming::Redraw(id));
            }
        }
        self.pump();
    }

    /// Records the spacing between this frame and the last.
    fn measure_cadence(&self, stamp: f64) {
        let mut cadence = self.cadence.get();
        if cadence.last.is_some() {
            let step = stamp - cadence.last_ms;
            // A step far from the measured interval is a frame the page skipped, or the first
            // frame after an idle stretch. Neither says anything about the display.
            let plausible = (2.0..100.0).contains(&step)
                && (cadence.samples == 0 || step < cadence.interval_ms * 1.5);
            if plausible {
                cadence.interval_ms = if cadence.samples == 0 {
                    step
                } else {
                    cadence.interval_ms * 0.9 + step * 0.1
                };
                cadence.samples = cadence.samples.saturating_add(1);
            }
        }
        cadence.last = Some(self.clock.now());
        cadence.last_ms = stamp;
        self.cadence.set(cadence);
        if cadence.samples == 12 || (cadence.samples > 12 && cadence.samples.is_multiple_of(120)) {
            let rate = refresh_millihertz(cadence.interval_ms);
            for entry in self.entries.borrow().iter() {
                entry.surface.set_refresh(rate);
            }
        }
    }

    /// When frames reach the screen, as far as the frame callbacks say.
    pub(crate) fn presentation_timing(&self) -> Option<PresentationTiming> {
        let cadence = self.cadence.get();
        let interval = (cadence.samples >= 12).then(|| {
            Duration::from_secs_f64(1000.0 / f64::from(refresh_millihertz(cadence.interval_ms)))
        });
        Some(PresentationTiming {
            last_presented: cadence.last,
            interval,
            next_refresh: None,
        })
    }

    /// Arms the timer for `deadline`, unless it is armed for it already.
    fn arm(&self, deadline: Instant, now: Instant) {
        if let Some((_, armed)) = self.timer.get()
            && armed == deadline
        {
            return;
        }
        self.disarm();
        let callback = self.timer_callback.borrow();
        let (Some(window), Some(callback)) = (web_sys::window(), callback.as_ref()) else {
            return;
        };
        let wait = deadline.saturating_duration_since(now).as_secs_f64() * 1000.0;
        if let Ok(handle) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            wait.ceil() as i32,
        ) {
            self.timer.set(Some((handle, deadline)));
        }
    }

    /// Clears the timer.
    fn disarm(&self) {
        if let Some((handle, _)) = self.timer.take()
            && let Some(window) = web_sys::window()
        {
            window.clear_timeout_with_handle(handle);
        }
    }

    /// The timer fired.
    fn on_timer(&self) {
        self.timer.set(None);
        self.park.borrow_mut().resumed();
        self.deliver(Incoming::Deadline);
    }

    /// Drops everything the page holds for this application.
    fn teardown(&self) {
        self.disarm();
        if let Some(handle) = self.frame_pending.take()
            && let Some(window) = web_sys::window()
        {
            let _ = window.cancel_animation_frame(handle);
        }
        if let Some((channel, _)) = self.channel.borrow_mut().take() {
            channel.port1().set_onmessage(None);
        }
        self.page_listeners.borrow_mut().clear();
        self.entries.borrow_mut().clear();
        self.incoming.borrow_mut().clear();
        DRIVER.with(|driver| driver.borrow_mut().take());
        if let Some(on_exit) = self.on_exit.borrow_mut().take() {
            on_exit();
        }
    }

    // --- what the context answers -----------------------------------------------------------

    /// Makes the page's canvas a surface.
    pub(crate) fn create_surface(
        &self,
        attributes: &SurfaceAttributes,
    ) -> Result<Arc<dyn Surface>, PlatformError> {
        // One canvas, one surface. A second window has no canvas to open on.
        let Some(canvas) = self.canvas.borrow_mut().take() else {
            return Err(Unsupported.into());
        };
        let id = SurfaceId::new(self.next_surface.get());
        self.next_surface.set(id.raw() + 1);
        let scale = device_pixel_ratio();
        let surface = Arc::new(CanvasSurface::new(id, laid_out_size(&canvas, scale), scale));
        let entry = Rc::new(Entry {
            surface: Arc::clone(&surface),
            canvas,
            modifiers: Cell::new(Modifiers::NONE),
            listeners: RefCell::new(Vec::new()),
            area: RefCell::new(None),
            area_listeners: RefCell::new(Vec::new()),
            typing: Cell::new(false),
            composing: Cell::new(false),
            observer: RefCell::new(None),
        });
        self.listen(&entry);
        self.observe(&entry);
        self.entries.borrow_mut().push(entry);
        self.set_title(attributes.title.as_str());
        Ok(surface)
    }

    /// Forgets a surface after the application heard that it is gone.
    pub(crate) fn destroy_surface(&self, id: SurfaceId) {
        self.incoming.borrow_mut().push_back(Incoming::Forget(id));
        self.post_turn();
    }

    /// The surface `id` names.
    pub(crate) fn surface(&self, id: SurfaceId) -> Option<Arc<dyn Surface>> {
        self.entry(id)
            .map(|entry| Arc::clone(&entry.surface) as Arc<dyn Surface>)
    }

    /// Every surface.
    pub(crate) fn surfaces(&self) -> Vec<Arc<dyn Surface>> {
        self.entries
            .borrow()
            .iter()
            .map(|entry| Arc::clone(&entry.surface) as Arc<dyn Surface>)
            .collect()
    }

    /// The page's color scheme.
    pub(crate) fn scheme(&self) -> ColorScheme {
        self.scheme.get()
    }

    /// The clipboard.
    pub(crate) fn clipboard(&self) -> &PageClipboard {
        &self.clipboard
    }

    /// What this backend can do.
    pub(crate) fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    /// The clock.
    pub(crate) fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.clock) as Arc<dyn Clock>
    }

    /// The waker.
    pub(crate) fn waker(&self) -> Arc<dyn Waker> {
        Arc::clone(&self.waker) as Arc<dyn Waker>
    }

    /// Ends the application at the end of the next turn.
    pub(crate) fn request_exit(&self) {
        self.exiting.set(true);
        self.post_turn();
    }

    /// Whether the application asked to exit.
    pub(crate) fn is_exiting(&self) -> bool {
        self.exiting.get()
    }

    // --- what a surface asks of the page ---------------------------------------------------

    /// The entry of surface `id`.
    fn entry(&self, id: SurfaceId) -> Option<Rc<Entry>> {
        self.entries
            .borrow()
            .iter()
            .find(|entry| entry.surface.id() == id)
            .cloned()
    }

    /// Sets the page's title, where the application owns the page.
    pub(crate) fn set_title(&self, title: &str) {
        if self.owns_page
            && !title.is_empty()
            && let Some(document) = web_sys::window().and_then(|window| window.document())
        {
            document.set_title(title);
        }
    }

    /// Shows or hides a canvas.
    pub(crate) fn set_visible(&self, id: SurfaceId, visible: bool) {
        if let Some(entry) = self.entry(id) {
            let value = if visible { "visible" } else { "hidden" };
            let _ = entry.canvas.style().set_property("visibility", value);
        }
    }

    /// Enters or leaves fullscreen. A browser grants it only inside a user gesture.
    pub(crate) fn set_fullscreen(&self, id: SurfaceId, fullscreen: bool) {
        let Some(entry) = self.entry(id) else { return };
        if fullscreen {
            let _ = entry.canvas.request_fullscreen();
        } else if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            document.exit_fullscreen();
        }
    }

    /// Whether a canvas is the page's fullscreen element.
    pub(crate) fn is_fullscreen(&self, id: SurfaceId) -> bool {
        let Some(entry) = self.entry(id) else {
            return false;
        };
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.fullscreen_element())
            .is_some_and(|element| element.is_same_node(Some(entry.canvas.as_ref())))
    }

    /// Gives a canvas the keyboard.
    pub(crate) fn focus(&self, id: SurfaceId) {
        if let Some(entry) = self.entry(id) {
            entry.take_focus();
        }
    }

    /// Sets the cursor over a canvas.
    pub(crate) fn set_cursor(&self, id: SurfaceId, cursor: CursorStyle) {
        if let Some(entry) = self.entry(id) {
            let _ = entry
                .canvas
                .style()
                .set_property("cursor", cursor_name(cursor));
        }
    }

    /// Says where text is being edited, or that none is.
    ///
    /// While a field is typed into, the hidden text area has the keyboard, so that an input
    /// method and the keyboard of a phone have an element to write into. It gives the keyboard
    /// back to the canvas when the field closes.
    pub(crate) fn set_text_input(&self, id: SurfaceId, state: Option<TextInput>) {
        let Some(entry) = self.entry(id) else { return };
        let Some(state) = state else {
            if entry.typing.replace(false) && entry.holds_focus() {
                text_input::focus(&entry.canvas);
            }
            return;
        };
        if entry.area.borrow().is_none() {
            *entry.area.borrow_mut() = text_input::create(&entry.canvas);
            self.listen_to_area(&entry);
        }
        let area = entry.area.borrow().clone();
        let Some(area) = area else { return };
        text_input::place(&area, &entry.canvas, state);
        if !entry.typing.replace(true) && entry.holds_focus() {
            text_input::focus(&area);
        }
    }

    // --- the canvas's own events ------------------------------------------------------------

    /// Watches the size of a canvas in device pixels.
    fn observe(&self, entry: &Rc<Entry>) {
        let this = self.this.clone();
        let observed = Rc::downgrade(entry);
        let callback = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
            let (Some(driver), Some(entry)) = (this.upgrade(), observed.upgrade()) else {
                return;
            };
            driver.measure(&entry, device_pixel_box(&entries.get(0)));
        });
        let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) else {
            return;
        };
        let options = web_sys::ResizeObserverOptions::new();
        options.set_box(web_sys::ResizeObserverBoxOptions::DevicePixelContentBox);
        // A browser that does not know the device-pixel box refuses the option. The content box
        // times the pixel ratio is the answer there.
        let precise = js_sys::Reflect::apply(
            &js_sys::Reflect::get(&observer, &"observe".into())
                .ok()
                .and_then(|observe| observe.dyn_into::<js_sys::Function>().ok())
                .unwrap_or_else(|| js_sys::Function::new_no_args("")),
            &observer,
            &js_sys::Array::of2(entry.canvas.as_ref(), options.as_ref()),
        );
        if precise.is_err() {
            observer.observe(&entry.canvas);
        }
        *entry.observer.borrow_mut() = Some((observer, callback));
    }

    /// Reads the size of a canvas and tells the application when it changed.
    fn measure(&self, entry: &Rc<Entry>, device: Option<(u32, u32)>) {
        let scale = device_pixel_ratio();
        let size = device.unwrap_or_else(|| laid_out_size(&entry.canvas, scale));
        let surface = &entry.surface;
        if size == surface.extent() && scale == surface.scale_factor() {
            return;
        }
        let rescaled = surface.resize(size, scale);
        let event = if rescaled {
            SurfaceEvent::ScaleFactorChanged {
                scale_factor: scale,
                size: surface.size(),
            }
        } else {
            SurfaceEvent::Resized(surface.size())
        };
        self.deliver(Incoming::Surface(surface.id(), event));
    }

    /// Listens to a canvas.
    fn listen(&self, entry: &Rc<Entry>) {
        let mut listeners = Vec::new();
        let mut on = |kind: &'static str, handle: fn(&Driver, &Rc<Entry>, Event)| {
            let this = self.this.clone();
            let listened = Rc::downgrade(entry);
            listeners.push(Listener::new(&entry.canvas, kind, move |event| {
                if let (Some(driver), Some(entry)) = (this.upgrade(), listened.upgrade()) {
                    handle(&driver, &entry, event);
                }
            }));
        };
        on("pointerdown", |driver, entry, event| {
            driver.on_pointer(entry, event, PointerAction::Pressed);
        });
        on("pointerup", |driver, entry, event| {
            driver.on_pointer(entry, event, PointerAction::Released);
        });
        on("pointermove", |driver, entry, event| {
            driver.on_pointer(entry, event, PointerAction::Moved);
        });
        on("pointerenter", |driver, entry, event| {
            driver.on_pointer(entry, event, PointerAction::Entered);
        });
        on("pointerleave", |driver, entry, event| {
            driver.on_pointer(entry, event, PointerAction::Left);
        });
        on("pointercancel", |driver, entry, event| {
            driver.on_pointer(entry, event, PointerAction::Cancelled);
        });
        on("wheel", Self::on_wheel);
        on("keydown", |driver, entry, event| {
            driver.on_key(entry, event, KeyState::Pressed);
        });
        on("keyup", |driver, entry, event| {
            driver.on_key(entry, event, KeyState::Released);
        });
        on("focus", |driver, entry, event| {
            driver.on_focus(entry, &event, true)
        });
        on("blur", |driver, entry, event| {
            driver.on_focus(entry, &event, false)
        });
        on("paste", Self::on_paste);
        // The application opens its own context menus.
        on("contextmenu", |_, _, event| event.prevent_default());
        *entry.listeners.borrow_mut() = listeners;
    }

    /// Listens to the hidden text area of a surface.
    fn listen_to_area(&self, entry: &Rc<Entry>) {
        let area = entry.area.borrow().clone();
        let Some(area) = area else { return };
        let mut listeners = Vec::new();
        let mut on = |kind: &'static str, handle: fn(&Driver, &Rc<Entry>, Event)| {
            let this = self.this.clone();
            let listened = Rc::downgrade(entry);
            listeners.push(Listener::new(&area, kind, move |event| {
                if let (Some(driver), Some(entry)) = (this.upgrade(), listened.upgrade()) {
                    handle(&driver, &entry, event);
                }
            }));
        };
        on("keydown", |driver, entry, event| {
            driver.on_key(entry, event, KeyState::Pressed);
        });
        on("keyup", |driver, entry, event| {
            driver.on_key(entry, event, KeyState::Released);
        });
        on("focus", |driver, entry, event| {
            driver.on_focus(entry, &event, true)
        });
        on("blur", |driver, entry, event| {
            driver.on_focus(entry, &event, false)
        });
        on("paste", Self::on_paste);
        on("compositionstart", |driver, entry, _| {
            entry.composing.set(true);
            driver.deliver_ime(entry, ImeEvent::Enabled);
        });
        on("compositionupdate", |driver, entry, event| {
            let text = composed(&event);
            let end = text.len();
            driver.deliver_ime(
                entry,
                ImeEvent::Preedit {
                    text: text.as_str().into(),
                    cursor: Some(end..end),
                },
            );
        });
        on("compositionend", |driver, entry, event| {
            entry.composing.set(false);
            entry.clear_area();
            let text = composed(&event);
            // A composition that ends with no text was abandoned. An empty preedit says so.
            let finished = if text.is_empty() {
                ImeEvent::Preedit {
                    text: "".into(),
                    cursor: None,
                }
            } else {
                ImeEvent::Commit(text.as_str().into())
            };
            driver.deliver_ime(entry, finished);
        });
        on("input", Self::on_input);
        *entry.area_listeners.borrow_mut() = listeners;
    }

    /// Hands the application an input-method event.
    fn deliver_ime(&self, entry: &Entry, event: ImeEvent) {
        self.deliver(Incoming::Surface(
            entry.surface.id(),
            SurfaceEvent::Ime(event),
        ));
    }

    /// The keyboard moved to or from an element of a surface.
    fn on_focus(&self, entry: &Rc<Entry>, event: &Event, focused: bool) {
        // The canvas and its text area pass the keyboard between them. The surface keeps it.
        let other = event
            .dyn_ref::<web_sys::FocusEvent>()
            .and_then(web_sys::FocusEvent::related_target);
        if other.is_some_and(|other| entry.owns(&other)) {
            return;
        }
        if focused {
            // The keyboard came to the canvas while a field is typed into.
            if entry.typing.get()
                && let Some(area) = entry.area.borrow().as_ref()
            {
                text_input::focus(area);
            }
        } else {
            // A key released while another element has the keyboard is never reported here.
            self.note_modifiers(entry, Modifiers::NONE);
        }
        self.deliver(Incoming::Surface(
            entry.surface.id(),
            SurfaceEvent::Focused(focused),
        ));
    }

    /// The page pasted into a surface.
    ///
    /// The paste event carries the text with no permission prompt. It arrives after the key that
    /// caused it and before the frame that answers the application's paste request, so the
    /// clipboard has the text when the frame reads it.
    fn on_paste(&self, _entry: &Rc<Entry>, event: Event) {
        event.prevent_default();
        let text = event
            .dyn_ref::<web_sys::ClipboardEvent>()
            .and_then(web_sys::ClipboardEvent::clipboard_data)
            .and_then(|data| data.get_data("text/plain").ok());
        if let Some(text) = text {
            self.clipboard.remember(text);
        }
    }

    /// The text area changed without a key the application was given.
    ///
    /// This is how the keyboard of a phone types, and how a dead key delivers its character.
    fn on_input(&self, entry: &Rc<Entry>, event: Event) {
        if entry.composing.get() {
            return;
        }
        entry.clear_area();
        let Some(event) = event.dyn_ref::<web_sys::InputEvent>() else {
            return;
        };
        let named = |key: NamedKey| {
            let event = KeyEvent::named(key, PhysicalKey::Unidentified(0));
            for state in [KeyState::Pressed, KeyState::Released] {
                self.incoming.borrow_mut().push_back(Incoming::Surface(
                    entry.surface.id(),
                    SurfaceEvent::Key {
                        state,
                        event: event.clone(),
                        modifiers: entry.modifiers.get(),
                        timestamp: self.clock.timestamp(),
                    },
                ));
            }
            self.pump();
        };
        match text_input::edit(&event.input_type(), event.data()) {
            Some(Edit::Insert(text)) => {
                self.deliver_ime(entry, ImeEvent::Commit(text.as_str().into()));
            }
            Some(Edit::Enter) => named(NamedKey::Enter),
            Some(Edit::Backspace) => named(NamedKey::Backspace),
            Some(Edit::Delete) => named(NamedKey::Delete),
            None => {}
        }
    }

    /// The modifiers of an event, as the application's shortcuts read them.
    ///
    /// On an Apple machine the command key is the shortcut modifier, and the framework's
    /// shortcut modifier on this target is the control key. The two change places there, so
    /// that the command key copies and pastes.
    fn shortcut_modifiers(&self, modifiers: Modifiers) -> Modifiers {
        if !self.apple {
            return modifiers;
        }
        modifiers
            .with(Modifiers::CONTROL, modifiers.meta())
            .with(Modifiers::META, modifiers.control())
    }

    /// Queues a modifier change, where the modifiers differ from the ones last told.
    fn note_modifiers(&self, entry: &Entry, modifiers: Modifiers) {
        if entry.modifiers.replace(modifiers) != modifiers {
            self.incoming.borrow_mut().push_back(Incoming::Surface(
                entry.surface.id(),
                SurfaceEvent::ModifiersChanged(modifiers),
            ));
        }
    }

    /// A pointer event on a canvas.
    fn on_pointer(&self, entry: &Rc<Entry>, event: Event, action: PointerAction) {
        let Ok(event) = event.dyn_into::<web_sys::PointerEvent>() else {
            return;
        };
        let carries_button = matches!(action, PointerAction::Pressed | PointerAction::Released);
        let translated = pointer::event(&event, carries_button);
        match action {
            PointerAction::Pressed => {
                // The canvas keeps the pointer until the release, wherever it goes, and takes
                // the keyboard. The default action would start a text selection in the page.
                event.prevent_default();
                let _ = entry.canvas.set_pointer_capture(event.pointer_id());
                entry.take_focus();
            }
            // A contact does not hover: it ends with its release.
            PointerAction::Entered | PointerAction::Left if !translated.kind.can_hover() => return,
            _ => {}
        }
        let modifiers = self.shortcut_modifiers(keyboard::of_mouse(&event));
        self.note_modifiers(entry, modifiers);
        self.deliver(Incoming::Surface(
            entry.surface.id(),
            SurfaceEvent::Pointer {
                action,
                event: translated,
                modifiers,
                timestamp: self.clock.stamp(event.time_stamp()),
            },
        ));
    }

    /// A wheel event on a canvas.
    fn on_wheel(&self, entry: &Rc<Entry>, event: Event) {
        let Ok(event) = event.dyn_into::<web_sys::WheelEvent>() else {
            return;
        };
        // With the control key, a wheel is the browser's zoom, and a pinch on a touch surface
        // arrives the same way. The page keeps both.
        if event.ctrl_key() {
            return;
        }
        event.prevent_default();
        // The mode is read before the deltas: one browser reports pixels to a page that reads
        // the deltas first.
        let mode = event.delta_mode();
        let legacy = |name: &str| {
            js_sys::Reflect::get(&event, &name.into())
                .ok()
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0)
        };
        let (delta, phase) = wheel::classify(wheel::Reported {
            delta: (event.delta_x(), event.delta_y()),
            mode,
            legacy: (legacy("wheelDeltaX"), legacy("wheelDeltaY")),
            page: f64::from(entry.canvas.client_height()),
        });
        let modifiers = self.shortcut_modifiers(keyboard::of_mouse(&event));
        self.note_modifiers(entry, modifiers);
        self.deliver(Incoming::Surface(
            entry.surface.id(),
            SurfaceEvent::Wheel {
                event: WheelEvent {
                    delta,
                    phase,
                    position: pointer::position(&event),
                    id: PointerId::MOUSE,
                    kind: PointerKind::Mouse,
                },
                modifiers,
                timestamp: self.clock.stamp(event.time_stamp()),
            },
        ));
    }

    /// A key event on a canvas.
    fn on_key(&self, entry: &Rc<Entry>, event: Event, state: KeyState) {
        let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>() else {
            return;
        };
        // A key that feeds a composition belongs to the input method. A browser marks the key
        // that starts one with the code 229.
        if event.is_composing() || event.key_code() == 229 {
            return;
        }
        let modifiers = self.shortcut_modifiers(keyboard::of_key(&event));
        let translated = keyboard::event(&event);
        // A dead key writes its character into the text area, which reports it as input.
        let composes = entry.typing.get() && matches!(translated.key, Key::Dead(_));
        if !composes && !keyboard::page_keeps(&translated, modifiers) {
            event.prevent_default();
        }
        self.note_modifiers(entry, modifiers);
        self.deliver(Incoming::Surface(
            entry.surface.id(),
            SurfaceEvent::Key {
                state,
                event: translated,
                modifiers,
                timestamp: self.clock.stamp(event.time_stamp()),
            },
        ));
    }
}

impl Entry {
    /// Whether `target` is the canvas or the text area of this surface.
    fn owns(&self, target: &web_sys::EventTarget) -> bool {
        let canvas: &web_sys::EventTarget = self.canvas.as_ref();
        target == canvas
            || self.area.borrow().as_ref().is_some_and(|area| {
                let area: &web_sys::EventTarget = area.as_ref();
                target == area
            })
    }

    /// Whether the canvas or the text area has the keyboard.
    fn holds_focus(&self) -> bool {
        self.canvas
            .owner_document()
            .and_then(|document| document.active_element())
            .is_some_and(|active| self.owns(active.as_ref()))
    }

    /// Gives the keyboard to the element that takes it now: the text area while a field is
    /// typed into, the canvas otherwise.
    fn take_focus(&self) {
        match self.area.borrow().as_ref() {
            Some(area) if self.typing.get() => text_input::focus(area),
            _ => text_input::focus(&self.canvas),
        }
    }

    /// Empties the text area, which only carries text on its way to the application.
    fn clear_area(&self) {
        if let Some(area) = self.area.borrow().as_ref() {
            area.set_value("");
        }
    }
}

/// The text a composition event carries.
fn composed(event: &Event) -> String {
    event
        .dyn_ref::<web_sys::CompositionEvent>()
        .and_then(web_sys::CompositionEvent::data)
        .unwrap_or_default()
}

/// Whether the browser runs on an Apple machine, where the command key is the shortcut modifier.
fn is_apple(window: &web_sys::Window) -> bool {
    window.navigator().platform().is_ok_and(|platform| {
        ["Mac", "iPhone", "iPad", "iPod"]
            .iter()
            .any(|name| platform.contains(name))
    })
}

/// The scheme a dark-mode query answers.
const fn scheme_of(dark: bool) -> ColorScheme {
    if dark {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    }
}

/// How many device pixels one CSS pixel covers.
fn device_pixel_ratio() -> f64 {
    web_sys::window()
        .map(|window| window.device_pixel_ratio())
        .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
        .unwrap_or(1.0)
}

/// The size of a canvas in device pixels, from its layout box. Never smaller than one pixel.
fn laid_out_size(canvas: &HtmlCanvasElement, scale: f64) -> (u32, u32) {
    let bounds = canvas.get_bounding_client_rect();
    let pixels = |css: f64| ((css * scale).round() as u32).max(1);
    (pixels(bounds.width()), pixels(bounds.height()))
}

/// The device-pixel content box of a size observation, where the browser reports one.
fn device_pixel_box(entry: &JsValue) -> Option<(u32, u32)> {
    let boxes = js_sys::Reflect::get(entry, &"devicePixelContentBoxSize".into()).ok()?;
    let first = js_sys::Reflect::get_u32(&boxes, 0).ok()?;
    let side = |name: &str| {
        js_sys::Reflect::get(&first, &name.into())
            .ok()
            .and_then(|value| value.as_f64())
            .map(|pixels| (pixels.round() as u32).max(1))
    };
    Some((side("inlineSize")?, side("blockSize")?))
}

/// The refresh rate a frame interval means, in millihertz.
///
/// A measured interval is within a few percent of the display's. A rate near a common display
/// rate is that rate.
fn refresh_millihertz(interval_ms: f64) -> u32 {
    const COMMON: [f64; 9] = [30.0, 60.0, 75.0, 90.0, 100.0, 120.0, 144.0, 165.0, 240.0];
    if interval_ms <= 0.0 {
        return 60_000;
    }
    let measured = 1000.0 / interval_ms;
    let rate = COMMON
        .into_iter()
        .find(|common| (measured - common).abs() / common < 0.03)
        .unwrap_or(measured);
    (rate * 1000.0).round() as u32
}
