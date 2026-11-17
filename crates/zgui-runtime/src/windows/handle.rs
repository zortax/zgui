//! One window, as the application holds it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Weak};

use zgui_geom::{Css, CssPx, Point, Rect, Size};
use zgui_platform::{
    CloseReason, ColorScheme, CursorStyle, Decorations, FullscreenMode, LayerPatch,
    PlatformCapabilities, ResizeEdge, Surface, WindowIcon, WindowLevel,
};
use zgui_reactive::prelude::*;
use zgui_reactive::{ArcRwSignal, Signal};
use zgui_vocab::Modifiers;

use crate::commands::{WindowCommands, WindowToken};

/// Which window this is.
///
/// Stable for the window's whole life, including across a suspend that takes its surface away and
/// gives it a different one back.
pub type WindowId = WindowToken;

/// One window of the application.
///
/// Cheap to clone and safe to keep: **every operation on a window that has closed is a silent
/// no-op**, and so is every operation a desktop cannot carry out. That is the whole cross-platform
/// contract — an application asks for what it wants once, and runs everywhere without a branch per
/// platform. What a desktop can actually do is readable through [`WindowHandle::capabilities`] for
/// an application that wants to show or hide an affordance rather than let it do nothing.
///
/// Bound to the thread the windows run on, like every other view-layer handle. Work on another
/// thread reaches a window the way it reaches anything else: by posting to the UI thread.
#[derive(Clone)]
pub struct WindowHandle {
    /// What every clone of this handle shares.
    shared: Rc<WindowShared>,
}

/// The state behind every clone of one window's handle.
///
/// The signals are reference-counted and have no reactive owner. Thus they stay alive as long as
/// the handle. The code that opens a window can unmount while the window stays open.
pub(crate) struct WindowShared {
    /// Which window this is.
    pub(crate) id: WindowId,
    /// The surface it draws into, while it has one.
    ///
    /// Weak, because a handle must not keep a window alive: an application holding the handle of a
    /// window the user closed is holding a name, not the window. Absent before it first opens and
    /// while a suspended platform is holding every surface.
    pub(crate) surface: RefCell<Option<Weak<dyn Surface>>>,
    /// Whether the window has closed for good.
    pub(crate) closed: Cell<bool>,
    /// Whether a platform-driven move or resize was begun since the last frame.
    ///
    /// Read by the frame, which then tells the router the press it was tracking is over: the
    /// compositor has taken the pointer and no release will ever arrive.
    pub(crate) drag_started: Cell<bool>,
    /// Where a close or an open is asked for.
    pub(crate) commands: WindowCommands,
    /// What this desktop can do, once the platform has said.
    pub(crate) capabilities: Rc<RefCell<PlatformCapabilities>>,
    /// The content size, in CSS pixels.
    pub(crate) size: ArcRwSignal<Size<CssPx, Css>>,
    /// How many physical pixels there are to a CSS pixel.
    pub(crate) scale: ArcRwSignal<f32>,
    /// Whether the window holds the keyboard.
    pub(crate) focused: ArcRwSignal<bool>,
    /// What hears about the window gaining and losing the keyboard.
    pub(crate) focus_listeners: Rc<RefCell<crate::windows::focus::FocusListeners>>,
    /// Whether the desktop says nothing of the window is visible.
    pub(crate) occluded: ArcRwSignal<bool>,
    /// Whether a frame of the window has reached the screen.
    pub(crate) presented: ArcRwSignal<bool>,
    /// Whether the window is maximised.
    pub(crate) maximized: ArcRwSignal<bool>,
    /// How the window fills the screen, if it does.
    pub(crate) fullscreen: ArcRwSignal<Option<FullscreenMode>>,
    /// Why the window closed, once it has.
    pub(crate) close_reason: ArcRwSignal<Option<CloseReason>>,
    /// Which lock keys are on, as of the last change the window was told about.
    pub(crate) lock_keys: ArcRwSignal<Modifiers>,
    /// The last input region the application gave, if it gave one.
    ///
    /// A surface takes it when it attaches, so a region set while the window builds holds.
    pub(crate) input_region: RefCell<Option<Region>>,
    /// The last opaque region the application gave, if it gave one.
    pub(crate) opaque_region: RefCell<Option<Region>>,
}

/// A region in CSS pixels, or `None` for the whole window.
type Region = Option<Vec<Rect<CssPx, Css>>>;

impl WindowHandle {
    /// A handle for a window that has been asked for and does not exist yet.
    pub(crate) fn pending(
        id: WindowId,
        commands: WindowCommands,
        capabilities: Rc<RefCell<PlatformCapabilities>>,
    ) -> Self {
        Self {
            shared: Rc::new(WindowShared {
                id,
                surface: RefCell::new(None),
                closed: Cell::new(false),
                drag_started: Cell::new(false),
                commands,
                capabilities,
                size: ArcRwSignal::new(Size::new(CssPx(0.0), CssPx(0.0))),
                scale: ArcRwSignal::new(1.0),
                // A window is opened focused until the desktop says otherwise, which is the same
                // assumption the window itself starts from. A shell window is the exception, and
                // the runtime says so when it opens one.
                focused: ArcRwSignal::new(true),
                focus_listeners: Rc::default(),
                occluded: ArcRwSignal::new(false),
                presented: ArcRwSignal::new(false),
                maximized: ArcRwSignal::new(false),
                fullscreen: ArcRwSignal::new(None),
                close_reason: ArcRwSignal::new(None),
                lock_keys: ArcRwSignal::new(Modifiers::NONE),
                input_region: RefCell::new(None),
                opaque_region: RefCell::new(None),
            }),
        }
    }

    /// Gives the handle the surface its window has just been opened on.
    ///
    /// What it reports is seeded from the surface rather than from what was asked for: a desktop is
    /// free to open a window at a different size, maximised, or on a display of another density.
    pub(crate) fn attach(&self, surface: &Arc<dyn Surface>) {
        *self.shared.surface.borrow_mut() = Some(Arc::downgrade(surface));
        let scale = surface.scale_factor() as f32;
        let size = surface.size();
        self.shared.scale.set(scale);
        self.shared.size.set(Size::new(
            CssPx(size.width.0 / scale),
            CssPx(size.height.0 / scale),
        ));
        self.shared.maximized.set(surface.is_maximized());
        self.shared.fullscreen.set(surface.fullscreen());
        if let Some(region) = self.shared.input_region.borrow().as_ref() {
            let _ = surface.set_input_region(region.as_deref());
        }
        if let Some(region) = self.shared.opaque_region.borrow().as_ref() {
            surface.set_opaque_region(region.as_deref());
        }
    }

    /// Takes the surface away, leaving the window named but not open.
    ///
    /// What a suspend does. The signals keep their last values: an application reading the size of
    /// a suspended window is better served by the size it had than by zero.
    pub(crate) fn detach(&self) {
        *self.shared.surface.borrow_mut() = None;
    }

    /// Records that the window has closed for good.
    pub(crate) fn note_closed(&self) {
        self.shared.closed.set(true);
        self.detach();
    }

    /// Records why the window closed.
    pub(crate) fn set_close_reason(&self, reason: CloseReason) {
        self.shared.close_reason.set(Some(reason));
    }

    /// Whether a platform-driven drag was begun since this was last asked, clearing the record.
    pub(crate) fn take_drag_started(&self) -> bool {
        self.shared.drag_started.replace(false)
    }

    /// Records the size and density the window actually turned out to have.
    ///
    /// Each signal is written only when its value moved. A drag delivers a configure per step, and
    /// a step that changed the width alone must wake the readers of the width alone — a scale
    /// signal that fires with an unchanged value re-runs every effect reading it, once per step.
    pub(crate) fn set_geometry(
        &self,
        size: zgui_geom::Size<zgui_geom::DevicePx, zgui_geom::Device>,
        scale: f32,
    ) {
        if self.shared.scale.get_untracked() != scale {
            self.shared.scale.set(scale);
        }
        let size = Size::new(CssPx(size.width.0 / scale), CssPx(size.height.0 / scale));
        if self.shared.size.get_untracked() != size {
            self.shared.size.set(size);
        }
    }

    /// Records whether the window holds the keyboard.
    pub(crate) fn set_focused(&self, focused: bool) {
        if self.shared.focused.get_untracked() != focused {
            self.shared.focused.set(focused);
        }
    }

    /// Tells the callbacks registered with [`WindowHandle::on_focus_change`] about an edge.
    pub(crate) fn focus_changed(&self, focused: bool) {
        crate::windows::focus::tell(&self.shared.focus_listeners, focused);
    }

    /// Records which lock keys are on.
    pub(crate) fn set_lock_keys(&self, locks: Modifiers) {
        let locks = locks.locks();
        if self.shared.lock_keys.get_untracked() != locks {
            self.shared.lock_keys.set(locks);
        }
    }

    /// Records whether the desktop says nothing of the window is visible.
    pub(crate) fn set_occluded(&self, occluded: bool) {
        self.shared.occluded.set(occluded);
    }

    /// Records that a frame of the window reached the screen.
    pub(crate) fn set_presented(&self) {
        if !self.shared.presented.get_untracked() {
            self.shared.presented.set(true);
        }
    }

    /// Asks the surface again whether it is maximised or full screen.
    ///
    /// Read on resize, and when the platform reports a state change. A desktop can change either
    /// state and keep the size of the window.
    pub(crate) fn refresh_window_state(&self) {
        if let Some(surface) = self.surface() {
            let maximized = surface.is_maximized();
            if self.shared.maximized.get_untracked() != maximized {
                self.shared.maximized.set(maximized);
            }
            let fullscreen = surface.fullscreen();
            if self.shared.fullscreen.get_untracked() != fullscreen {
                self.shared.fullscreen.set(fullscreen);
            }
        }
    }

    /// Which window this is.
    pub fn id(&self) -> WindowId {
        self.shared.id
    }

    /// What this desktop can do.
    ///
    /// For an application that would rather hide an affordance than offer one that does nothing —
    /// a "move window here" menu item on a desktop that does not let a window place itself. Every
    /// operation is safe to call whatever this says. Before the platform's surfaces become
    /// available, this answers [`PlatformCapabilities::default`].
    pub fn capabilities(&self) -> PlatformCapabilities {
        self.shared.capabilities.borrow().clone()
    }

    /// Whether the window is open.
    pub fn is_open(&self) -> bool {
        self.surface().is_some()
    }

    /// The content size, in CSS pixels.
    pub fn size(&self) -> Signal<Size<CssPx, Css>> {
        self.shared.size.clone().into()
    }

    /// How many physical pixels there are to a CSS pixel on this window's display.
    pub fn scale(&self) -> Signal<f32> {
        self.shared.scale.clone().into()
    }

    /// Whether the window holds the keyboard.
    ///
    /// An ordinary window starts as focused, because the desktop gives a new window the keyboard.
    /// A shell window starts as not focused: one that takes no keyboard is never given it, and
    /// one that takes it on demand is given it when the desktop says so.
    pub fn focused(&self) -> Signal<bool> {
        self.shared.focused.clone().into()
    }

    /// Calls `callback` each time the window gains the keyboard or loses it, with `true` for a
    /// gain.
    ///
    /// Each call is one edge: a gain and a loss inside one frame are two calls, where the
    /// [`WindowHandle::focused`] signal can settle on the value it started from. A pop-over
    /// closes itself from the loss. The callback runs inside the frame that handles the edge, as
    /// a listener does, and a signal it reads is not subscribed to.
    ///
    /// The guard removes the callback when it is dropped. Keep it for as long as the edges
    /// matter.
    pub fn on_focus_change(&self, callback: impl FnMut(bool) + 'static) -> crate::FocusGuard {
        crate::windows::focus::listen(&self.shared.focus_listeners, callback)
    }

    /// Which lock keys are on: [`Modifiers::CAPS_LOCK`] and [`Modifiers::NUM_LOCK`].
    ///
    /// The desktop tells the window while it holds the keyboard, and when it gains the keyboard.
    /// The set holds no other modifier. It stays empty on a desktop that does not report the
    /// locks.
    pub fn lock_keys(&self) -> Signal<Modifiers> {
        self.shared.lock_keys.clone().into()
    }

    /// Whether caps lock is on, as [`WindowHandle::lock_keys`] reports it.
    ///
    /// A password field reads it to warn that the letters typed are upper case.
    pub fn caps_lock(&self) -> Signal<bool> {
        let locks = self.shared.lock_keys.clone();
        Signal::derive(move || locks.get().caps_lock())
    }

    /// Whether the desktop says nothing of the window is visible.
    ///
    /// What an application watches to stop animating something nobody can see.
    pub fn occluded(&self) -> Signal<bool> {
        self.shared.occluded.clone().into()
    }

    /// Whether a frame of the window has reached the screen.
    ///
    /// It turns true once and stays true. Where the platform reports presentation, the desktop says
    /// when the first frame was shown. Elsewhere it turns true when the first frame is handed to the
    /// desktop. A lock screen reads it to know that every output shows the lock.
    pub fn presented(&self) -> Signal<bool> {
        self.shared.presented.clone().into()
    }

    /// Whether the window is maximised.
    pub fn maximized(&self) -> Signal<bool> {
        self.shared.maximized.clone().into()
    }

    /// How the window fills the screen, if it does.
    pub fn fullscreen(&self) -> Signal<Option<FullscreenMode>> {
        self.shared.fullscreen.clone().into()
    }

    /// Why the window closed, or nothing while it has not.
    ///
    /// A shell window on an output that went away closes with
    /// [`CloseReason::OutputGone`]. Read it from outside the window: the window's own scope ends
    /// when it closes.
    pub fn close_reason(&self) -> Signal<Option<CloseReason>> {
        self.shared.close_reason.clone().into()
    }

    /// The surface, while there is one to act on.
    fn surface(&self) -> Option<Arc<dyn Surface>> {
        if self.shared.closed.get() {
            return None;
        }
        self.shared
            .surface
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
    }

    /// Runs `act` on the surface, or does nothing at all.
    fn act(&self, act: impl FnOnce(&Arc<dyn Surface>)) {
        if let Some(surface) = self.surface() {
            act(&surface);
        }
    }

    /// Names the window.
    pub fn set_title(&self, title: impl AsRef<str>) {
        self.act(|surface| surface.set_title(title.as_ref()));
    }

    /// Asks for a different content size, in CSS pixels.
    ///
    /// A request: a desktop may answer with a different size, or refuse. Watch
    /// [`WindowHandle::size`] for what was actually taken.
    pub fn request_size(&self, width: f32, height: f32) {
        self.act(|surface| {
            surface.request_size(Size::new(CssPx(width), CssPx(height)));
        });
    }

    /// Sets the smallest size the user may drag to, in CSS pixels.
    pub fn set_min_size(&self, size: Option<(f32, f32)>) {
        self.act(|surface| {
            surface.set_min_size(size.map(|(w, h)| Size::new(CssPx(w), CssPx(h))));
        });
    }

    /// Sets the largest size the user may drag to, in CSS pixels.
    pub fn set_max_size(&self, size: Option<(f32, f32)>) {
        self.act(|surface| {
            surface.set_max_size(size.map(|(w, h)| Size::new(CssPx(w), CssPx(h))));
        });
    }

    /// Asks for the window to be placed at a point on the desktop.
    ///
    /// Does nothing where a desktop places windows itself, which is every Wayland compositor.
    pub fn set_position(&self, x: f32, y: f32) {
        self.act(|surface| surface.set_position(Point::new(CssPx(x), CssPx(y))));
    }

    /// Where the window is, when the desktop says.
    ///
    /// `None` on a desktop that does not tell a window where it has been put.
    pub fn position(&self) -> Option<Point<CssPx, Css>> {
        self.surface().and_then(|surface| surface.position())
    }

    /// Allows or forbids the user resizing the window.
    pub fn set_resizable(&self, resizable: bool) {
        self.act(|surface| surface.set_resizable(resizable));
    }

    /// Asks the desktop for a frame.
    ///
    /// macOS settles whether a frame carries a title bar when the window is created, so
    /// [`Decorations::NoTitleBar`] reaches a window that is already open as
    /// [`Decorations::Full`]. Ask for it in [`WindowOptions`](crate::windows::WindowOptions).
    pub fn set_decorations(&self, decorations: Decorations) {
        self.act(|surface| surface.set_decorations(decorations));
    }

    /// Maximises or restores the window.
    pub fn set_maximized(&self, maximized: bool) {
        self.act(|surface| surface.set_maximized(maximized));
    }

    /// Maximises the window, or restores it if it is already maximised.
    ///
    /// What a title bar's double-press does, and what its maximise button does.
    pub fn toggle_maximized(&self) {
        self.act(|surface| surface.set_maximized(!surface.is_maximized()));
    }

    /// Does what a double press on a title bar does on this desktop.
    ///
    /// That maximises the window or restores it, unless the desktop has a preference: macOS zooms,
    /// minimizes or does nothing, as the user set it in System Settings.
    pub fn title_bar_double_click(&self) {
        self.act(|surface| surface.title_bar_double_click());
    }

    /// Minimises the window.
    pub fn minimize(&self) {
        self.act(|surface| surface.set_minimized(true));
    }

    /// Puts the window full screen, or takes it out again.
    ///
    /// A desktop that cannot give a window a screen exclusively gives it a borderless one instead,
    /// which is what was asked for minus the part that could not be done.
    pub fn set_fullscreen(&self, mode: Option<FullscreenMode>) {
        self.act(|surface| surface.set_fullscreen(mode));
    }

    /// Asks for the window to sit at a level in the desktop's stacking order.
    ///
    /// Does nothing where a desktop does not let an application place itself in the stack.
    pub fn set_level(&self, level: WindowLevel) {
        self.act(|surface| surface.set_window_level(level));
    }

    /// Shows or hides the window.
    ///
    /// Hiding keeps the window, its graphics surface and its whole document, so showing it again
    /// costs one frame.
    pub fn set_visible(&self, visible: bool) {
        self.act(|surface| surface.set_visible(visible));
    }

    /// Changes where a shell window sits and what it claims.
    ///
    /// The change reaches the screen with the next frame, together with the content drawn for
    /// it, so a bar that grows and the room it reserves change at once. What `patch` leaves
    /// absent stays as it is. Does nothing for a window that is not a shell window.
    ///
    /// A shell window that is hidden and shown again keeps its placement, changes included.
    pub fn set_layer_placement(&self, patch: LayerPatch) {
        self.act(|surface| {
            let _ = surface.set_layer_placement(&patch);
        });
    }

    /// Sets the part of the window that takes pointer and touch input, in CSS pixels.
    ///
    /// Input on the rest of the window reaches whatever is behind it. `None` makes the whole
    /// window take input again, and an empty list makes none of it take input. The change reaches
    /// the screen with the next frame. A region set before the window has a surface reaches the
    /// surface when the window opens, and every later surface gets it on a resume. Does nothing
    /// where the desktop cannot do it; see [`PlatformCapabilities::input_regions`].
    ///
    /// [`use_input_region`](crate::windows::use_input_region) keeps the region equal to the boxes
    /// of some elements, which is what a pop-over or a stack of notifications wants.
    pub fn set_input_region(&self, region: Option<Vec<Rect<CssPx, Css>>>) {
        if self.shared.closed.get() {
            return;
        }
        self.act(|surface| {
            let _ = surface.set_input_region(region.as_deref());
        });
        *self.shared.input_region.borrow_mut() = Some(region);
    }

    /// Tells the desktop which part of the window is opaque, in CSS pixels.
    ///
    /// A hint that lets a compositor skip drawing what is behind the opaque part. `None` says
    /// nothing is opaque. The change reaches the screen with the next frame. A region set before
    /// the window has a surface reaches the surface when the window opens.
    pub fn set_opaque_region(&self, region: Option<Vec<Rect<CssPx, Css>>>) {
        if self.shared.closed.get() {
            return;
        }
        self.act(|surface| surface.set_opaque_region(region.as_deref()));
        *self.shared.opaque_region.borrow_mut() = Some(region);
    }

    /// Sets the picture the desktop shows for this window.
    ///
    /// Does nothing where the desktop takes the icon from elsewhere — from the desktop entry on
    /// Wayland, from the bundle on macOS.
    pub fn set_icon(&self, icon: Option<WindowIcon>) {
        self.act(|surface| surface.set_icon(icon.as_ref()));
    }

    /// Overrides this window's light or dark preference, or returns it to the desktop's.
    pub fn set_theme(&self, theme: Option<ColorScheme>) {
        self.act(|surface| surface.set_theme(theme));
    }

    /// Sets what the pointer looks like over this window.
    pub fn set_cursor(&self, cursor: CursorStyle) {
        self.act(|surface| surface.set_cursor(cursor));
    }

    /// Asks for the keyboard.
    ///
    /// A desktop is free to refuse: taking focus from what somebody is typing into is what
    /// focus-stealing prevention exists to stop.
    pub fn focus(&self) {
        self.act(|surface| surface.focus());
    }

    /// The token of the latest press the user made, for a pop-up that takes a grab.
    ///
    /// Read it in the handler of the press that opens a menu, and pass it to
    /// [`PopupOptions::with_grab`](crate::windows::PopupOptions::with_grab). `None` before the
    /// first press, while the window is not open, and on a desktop that needs no token.
    pub fn input_serial(&self) -> Option<zgui_platform::InputSerial> {
        self.surface().and_then(|surface| surface.input_serial())
    }

    /// When the window's frames reach the screen, as far as the desktop says.
    ///
    /// A caller that plans work around the window's frames reads the next refresh from it with
    /// [`PresentationTiming::refresh_after`](zgui_platform::PresentationTiming::refresh_after).
    /// `None` while the window is not open and on a desktop that reports no timing.
    pub fn presentation_timing(&self) -> Option<zgui_platform::PresentationTiming> {
        self.surface()
            .and_then(|surface| surface.presentation_timing())
    }

    /// Asks the desktop to draw attention to this window, or stops asking.
    pub fn request_attention(&self, urgent: bool) {
        self.act(|surface| surface.request_attention(urgent));
    }

    /// Begins a desktop-driven move of the window.
    ///
    /// What a window drawing its own title bar calls from a pointer press on it: on a desktop that
    /// forbids a window from placing itself, this is the only way a self-drawn title bar can move
    /// one. See [`WindowHandle::move_drag_handler`], which is the form a view uses.
    pub fn begin_move_drag(&self) {
        self.act(|surface| {
            if surface.begin_move_drag().is_ok() {
                // The compositor has the pointer now and no release will arrive, so the frame has
                // to be told to end the press the router is still tracking.
                self.shared.drag_started.set(true);
            }
        });
    }

    /// Begins a desktop-driven resize of the window from one edge or corner.
    pub fn begin_resize_drag(&self, edge: ResizeEdge) {
        self.act(|surface| {
            if surface.begin_resize_drag(edge).is_ok() {
                self.shared.drag_started.set(true);
            }
        });
    }

    /// Closes the window.
    ///
    /// The window's own close callbacks are not consulted: those answer the *user* asking to close,
    /// and an application closing its own window has already decided.
    pub fn close(&self) {
        self.shared.commands.close(self.shared.id);
    }
}

impl core::fmt::Debug for WindowHandle {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WindowHandle")
            .field("id", &self.shared.id)
            .field("open", &self.is_open())
            .finish_non_exhaustive()
    }
}

impl PartialEq for WindowHandle {
    /// Two handles are equal when they name the same window.
    fn eq(&self, other: &Self) -> bool {
        self.shared.id == other.shared.id
    }
}

impl Eq for WindowHandle {}
