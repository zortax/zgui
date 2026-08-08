//! One window, as the application holds it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Weak};

use zgui_geom::{Css, CssPx, Point, Size};
use zgui_platform::{
    ColorScheme, CursorStyle, Decorations, FullscreenMode, PlatformCapabilities, ResizeEdge,
    Surface, WindowIcon, WindowLevel,
};
use zgui_reactive::prelude::*;
use zgui_reactive::{RwSignal, Signal};

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
    pub(crate) size: RwSignal<Size<CssPx, Css>>,
    /// How many physical pixels there are to a CSS pixel.
    pub(crate) scale: RwSignal<f32>,
    /// Whether the window holds the keyboard.
    pub(crate) focused: RwSignal<bool>,
    /// Whether the desktop says nothing of the window is visible.
    pub(crate) occluded: RwSignal<bool>,
    /// Whether the window is maximised.
    pub(crate) maximized: RwSignal<bool>,
    /// How the window fills the screen, if it does.
    pub(crate) fullscreen: RwSignal<Option<FullscreenMode>>,
}

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
                size: RwSignal::new(Size::new(CssPx(0.0), CssPx(0.0))),
                scale: RwSignal::new(1.0),
                // A window is opened focused until the desktop says otherwise, which is the same
                // assumption the window itself starts from.
                focused: RwSignal::new(true),
                occluded: RwSignal::new(false),
                maximized: RwSignal::new(false),
                fullscreen: RwSignal::new(None),
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

    /// Whether a platform-driven drag was begun since this was last asked, clearing the record.
    pub(crate) fn take_drag_started(&self) -> bool {
        self.shared.drag_started.replace(false)
    }

    /// Records the size and density the window actually turned out to have.
    pub(crate) fn set_geometry(
        &self,
        size: zgui_geom::Size<zgui_geom::DevicePx, zgui_geom::Device>,
        scale: f32,
    ) {
        self.shared.scale.set(scale);
        self.shared.size.set(Size::new(
            CssPx(size.width.0 / scale),
            CssPx(size.height.0 / scale),
        ));
    }

    /// Records whether the window holds the keyboard.
    pub(crate) fn set_focused(&self, focused: bool) {
        self.shared.focused.set(focused);
    }

    /// Records whether the desktop says nothing of the window is visible.
    pub(crate) fn set_occluded(&self, occluded: bool) {
        self.shared.occluded.set(occluded);
    }

    /// Asks the surface again whether it is maximised or full screen.
    ///
    /// Neither is reported as an event by any desktop, and both always resize the window — so a
    /// resize is where they are noticed.
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
    /// operation is safe to call whatever this says.
    pub fn capabilities(&self) -> PlatformCapabilities {
        self.shared.capabilities.borrow().clone()
    }

    /// Whether the window is open.
    pub fn is_open(&self) -> bool {
        self.surface().is_some()
    }

    /// The content size, in CSS pixels.
    pub fn size(&self) -> Signal<Size<CssPx, Css>> {
        self.shared.size.into()
    }

    /// How many physical pixels there are to a CSS pixel on this window's display.
    pub fn scale(&self) -> Signal<f32> {
        self.shared.scale.into()
    }

    /// Whether the window holds the keyboard.
    pub fn focused(&self) -> Signal<bool> {
        self.shared.focused.into()
    }

    /// Whether the desktop says nothing of the window is visible.
    ///
    /// What an application watches to stop animating something nobody can see.
    pub fn occluded(&self) -> Signal<bool> {
        self.shared.occluded.into()
    }

    /// Whether the window is maximised.
    pub fn maximized(&self) -> Signal<bool> {
        self.shared.maximized.into()
    }

    /// How the window fills the screen, if it does.
    pub fn fullscreen(&self) -> Signal<Option<FullscreenMode>> {
        self.shared.fullscreen.into()
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
