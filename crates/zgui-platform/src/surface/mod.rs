//! A thing that can be drawn into and interacted with.

mod attributes;
mod chrome;
mod drag;
mod event;
mod gpu;
mod id;
mod text_input;

pub use crate::surface::attributes::SurfaceAttributes;
pub use crate::surface::chrome::{CursorStyle, DecorationSource, FullscreenMode, ResizeEdge};
pub use crate::surface::drag::DragEvent;
pub use crate::surface::event::SurfaceEvent;
pub use crate::surface::gpu::GpuSurface;
pub use crate::surface::id::SurfaceId;
pub use crate::surface::text_input::{TextInput, TextInputPurpose};

use accesskit::TreeUpdate;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};

use crate::error::Unsupported;

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

    /// Sets the window title.
    fn set_title(&self, title: &str);

    /// Shows or hides the surface.
    ///
    /// A surface is created hidden and is shown by this, once its first frame is ready. Showing an
    /// unpainted surface is what produces a flash of empty window at launch.
    fn set_visible(&self, visible: bool);

    /// Asks the platform to draw a frame and title bar, or not to.
    fn set_decorated(&self, decorated: bool);

    /// Allows or forbids the user resizing the surface.
    fn set_resizable(&self, resizable: bool);

    /// Maximises or restores the surface.
    fn set_maximized(&self, maximized: bool);

    /// Minimises or restores the surface.
    fn set_minimized(&self, minimized: bool);

    /// Puts the surface full screen, or takes it out again.
    fn set_fullscreen(&self, mode: Option<FullscreenMode>);

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
