//! The graphics-capable view of a surface.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use crate::surface::Surface;

/// A surface a graphics API can draw into directly.
///
/// This is a separate trait, and the separation is the point. The framework's core describes what
/// to draw and never touches a native handle; a backend with no native handle at all — one that
/// renders offscreen, one that targets a document rather than a screen — simply does not implement
/// this, and nothing above notices. Only the renderer asks for it, through
/// [`Surface::gpu`](crate::Surface::gpu), and only to hand the handles to the graphics API.
///
/// The two handle traits are supertraits rather than methods so that a graphics API which already
/// accepts anything carrying them accepts this without an adapter.
pub trait GpuSurface: Surface + HasWindowHandle + HasDisplayHandle {}

impl<T> GpuSurface for T where T: Surface + HasWindowHandle + HasDisplayHandle {}
