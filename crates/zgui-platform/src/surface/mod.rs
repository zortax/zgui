//! A thing that can be drawn into and interacted with.

mod attributes;
mod chrome;
mod drag;
mod event;
mod gpu;
mod icon;
mod id;
mod role;
mod text_input;
mod timing;
mod watchdog;

pub use crate::surface::attributes::SurfaceAttributes;
pub use crate::surface::chrome::{
    CursorStyle, DecorationSource, Decorations, FullscreenMode, ResizeEdge, WindowLevel,
};
pub use crate::surface::drag::DragEvent;
pub use crate::surface::event::SurfaceEvent;
pub use crate::surface::gpu::GpuSurface;
pub use crate::surface::icon::{BadIcon, WindowIcon};
pub use crate::surface::id::SurfaceId;
pub use crate::surface::role::{
    Anchor, Constrain, KeyboardInteractivity, Layer, LayerPlacement, PopupPlacement, SurfaceRole,
};
pub use crate::surface::text_input::{TextInput, TextInputPurpose};
pub use crate::surface::timing::{PresentPacing, PresentationTiming};
pub use crate::surface::watchdog::Watchdog;

use accesskit::TreeUpdate;
use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Size};

use crate::error::Unsupported;
use crate::theme::ColorScheme;

/// A thing that can be drawn into and interacted with.
///
/// It is called a surface rather than a window because not every implementation has a window: a
/// test backend draws into memory, and a backend targeting a document draws into a region of one.
/// Everything above this trait is written in terms of "the thing being drawn into", and that is
/// what makes those two possible at all.
///
/// A surface is shared and thread-safe, because the one operation that must work from anywhere is
/// [`Surface::request_redraw`]: background work finishing on another thread is one of the four
/// things that can ask for a frame, and requiring it to hop threads first would put a scheduling
/// delay in front of every one of them.
///
/// # Sizes come in two spaces and they are not interchangeable
///
/// [`Surface::size`] is in physical pixels, because that is what a swap chain is allocated in and
/// a rounding error there is a visibly stretched frame. The sizes a *caller* asks for are in CSS
/// pixels, because that is the space a layout is written in. [`Surface::scale_factor`] is the
/// bridge, and it is the surface's own — never the monitor's, which can differ.
pub trait Surface: Send + Sync + 'static {
    /// Which surface this is.
    fn id(&self) -> SurfaceId;

    /// How large the drawable area is, in physical pixels.
    fn size(&self) -> Size<DevicePx, Device>;

    /// How many physical pixels there are to a CSS pixel on this surface.
    fn scale_factor(&self) -> f64;

    /// How many frames per second this surface is presented at, in thousandths.
    ///
    /// Absent when the platform does not say, in which case a deadline should be computed against
    /// the documented fallback rather than against an assumed rate.
    fn refresh_rate_millihertz(&self) -> Option<u32> {
        None
    }

    /// Who waits for the display on this surface.
    ///
    /// The default hands the wait to the graphics API, which is right wherever the platform has no
    /// timing of its own to pace against. A backend that answers [`PresentPacing::Platform`] is
    /// promising two things: that it paces frames itself, and that presentation on this surface
    /// must therefore be configured never to block the thread that asks for it.
    fn present_pacing(&self) -> PresentPacing {
        PresentPacing::Display
    }

    /// When this surface's frames reach the screen, as far as the platform knows.
    ///
    /// Absent means the platform reports no timing at all, and a caller falls back to
    /// [`Surface::refresh_rate_millihertz`]. It is read once a frame, so it answers with a snapshot
    /// rather than a borrow of whatever the backend keeps.
    fn presentation_timing(&self) -> Option<PresentationTiming> {
        None
    }

    /// Asks for a different content size, and reports the size actually taken.
    ///
    /// A platform may refuse, in which case the answer is the size that was kept.
    fn request_size(&self, size: Size<CssPx, Css>) -> Option<Size<DevicePx, Device>>;

    /// Sets the smallest content size the user may drag to.
    fn set_min_size(&self, size: Option<Size<CssPx, Css>>);

    /// Sets the largest content size the user may drag to.
    fn set_max_size(&self, size: Option<Size<CssPx, Css>>);

    /// Asks for this surface to be drawn.
    ///
    /// Safe from any thread, and coalescing: a hundred requests before the next frame produce one
    /// frame. That coalescing is what lets everything that might need a redraw simply ask,
    /// without any of them having to know about the others.
    fn request_redraw(&self);

    /// Tells the platform a frame is about to be presented.
    ///
    /// Called between finishing the drawing and presenting it. On a compositor that uses it this
    /// is what keeps frame pacing honest; where it means nothing it costs nothing.
    fn pre_present_notify(&self);

    /// Tells the platform the redraw just delivered was refused without running.
    ///
    /// Called inside the delivery of a redraw, before the platform closes it out. The contract:
    /// treat the turn as if no redraw ran — commit nothing, owe the compositor nothing — because
    /// the application keeps the obligation and asks for the frame itself when its own deadline
    /// arrives. A refused redraw that still cost a compositor round trip would put that deadline
    /// behind the answer to an empty commit.
    ///
    /// Distinct from a redraw that ran and presented nothing: that one keeps whatever bookkeeping
    /// the platform's pacing needs, because only the platform knows what a skipped frame owes.
    fn redraw_declined(&self) {}

    /// Sets the window title.
    fn set_title(&self, title: &str);

    /// Shows or hides the surface.
    ///
    /// A surface is created hidden and is shown by this, once its first frame is ready. Showing an
    /// unpainted surface is what produces a flash of empty window at launch.
    fn set_visible(&self, visible: bool);

    /// Asks the platform for a frame.
    ///
    /// macOS settles whether a frame carries a title bar when the surface is created, so
    /// [`Decorations::NoTitleBar`] reaches a live surface there as [`Decorations::Full`].
    fn set_decorations(&self, decorations: Decorations);

    /// Allows or forbids the user resizing the surface.
    fn set_resizable(&self, resizable: bool);

    /// Maximises or restores the surface.
    fn set_maximized(&self, maximized: bool);

    /// Minimises or restores the surface.
    fn set_minimized(&self, minimized: bool);

    /// Puts the surface full screen, or takes it out again.
    fn set_fullscreen(&self, mode: Option<FullscreenMode>);

    /// Asks for the surface to be placed at `position`, measured from the desktop's origin.
    ///
    /// The defaults from here down are the cross-platform contract, not laziness: a desktop that
    /// will not let a window place itself, stack itself or carry its own icon answers by doing
    /// nothing, so an application asks once and runs everywhere instead of branching per platform.
    fn set_position(&self, position: Point<CssPx, Css>) {
        let _ = position;
    }

    /// Where the surface is, measured from the desktop's origin.
    ///
    /// Absent where the desktop does not say, which is every Wayland compositor: a window there is
    /// not told where it has been placed.
    fn position(&self) -> Option<Point<CssPx, Css>> {
        None
    }

    /// Whether the surface is maximised.
    fn is_maximized(&self) -> bool {
        false
    }

    /// Whether the surface is minimised, where the platform says.
    fn is_minimized(&self) -> Option<bool> {
        None
    }

    /// How the surface fills the screen, if it does.
    fn fullscreen(&self) -> Option<FullscreenMode> {
        None
    }

    /// Asks for the surface to sit at `level` in the stacking order.
    fn set_window_level(&self, level: WindowLevel) {
        let _ = level;
    }

    /// Sets the picture the desktop shows for this surface, where it takes one from the window.
    fn set_icon(&self, icon: Option<&WindowIcon>) {
        let _ = icon;
    }

    /// Overrides this surface's light or dark preference, or returns it to the desktop's.
    fn set_theme(&self, theme: Option<ColorScheme>) {
        let _ = theme;
    }

    /// Asks for keyboard focus.
    ///
    /// A desktop is free to refuse: stealing focus from what the user is typing into is the
    /// behaviour focus-stealing prevention exists to stop.
    fn focus(&self) {}

    /// Asks the desktop to draw attention to this surface, or stops asking.
    ///
    /// What that looks like is the desktop's business — a bouncing icon, a flashing task-bar
    /// entry, an urgency hint.
    fn request_attention(&self, urgent: bool) {
        let _ = urgent;
    }

    /// Begins a platform-driven move of the surface.
    ///
    /// This exists for an application drawing its own title bar: on a desktop that forbids a
    /// window from placing itself, dragging a self-drawn title bar is only possible by asking the
    /// platform to take over the drag.
    fn begin_move_drag(&self) -> Result<(), Unsupported> {
        Err(Unsupported)
    }

    /// Begins a platform-driven resize of the surface from `edge`.
    fn begin_resize_drag(&self, edge: ResizeEdge) -> Result<(), Unsupported> {
        let _ = edge;
        Err(Unsupported)
    }

    /// Sets what the pointer looks like over this surface.
    fn set_cursor(&self, cursor: CursorStyle);

    /// Makes the surface transparent to the pointer, so presses reach whatever is behind it.
    fn set_pointer_passthrough(&self, passthrough: bool) -> Result<(), Unsupported> {
        let _ = passthrough;
        Err(Unsupported)
    }

    /// Tells the platform where text is being typed and what kind, or that none is.
    ///
    /// One call rather than three, because the parts are only meaningful together: an input method
    /// told a caret moved but not that the field is still active places its candidate window over
    /// the text being composed.
    fn set_text_input(&self, state: Option<TextInput>);

    /// Abandons any half-composed accent so the next key stands alone.
    fn reset_dead_keys(&self) {}

    /// Publishes an accessibility tree update, building it only if anything is listening.
    ///
    /// The closure is what makes that possible: building a tree costs a walk of the document, and
    /// on a machine with no assistive technology running that walk would be pure waste on every
    /// frame. Nothing is built unless something is listening.
    ///
    /// It is a mutable closure reference rather than a value so that the trait stays usable behind
    /// a pointer.
    fn push_a11y_update(&self, build: &mut dyn FnMut() -> TreeUpdate);

    /// This surface seen as something a graphics API can draw into, when it is one.
    ///
    /// A backend with no native handles keeps the default and is invisible to the renderer.
    fn gpu(&self) -> Option<&dyn GpuSurface> {
        None
    }

    /// The same, as a shared handle rather than a borrow.
    ///
    /// A graphics API is given the handles once and keeps them for as long as it draws — it does
    /// not re-borrow them per frame — so a borrow is the wrong shape for the one caller there is.
    /// A backend that answers [`Surface::gpu`] answers this with `Some(self)`; one with no native
    /// handles keeps the default and both answers are `None` together.
    ///
    /// It cannot be derived from [`Surface::gpu`]: nothing can turn a shared handle to one trait
    /// into a shared handle to another, so each backend states it once.
    fn gpu_shared(self: std::sync::Arc<Self>) -> Option<std::sync::Arc<dyn GpuSurface>> {
        None
    }
}
