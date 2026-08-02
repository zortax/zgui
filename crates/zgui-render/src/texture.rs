//! Textures a renderer did not draw into itself.

use zgui_geom::{Device, Size};
use zgui_scene::ExternalTextureId;

/// A renderer's handle to something it owns.
///
/// Opaque, and a plain integer rather than a pointer, so that a handle can be stored in a display
/// list, sent between threads and compared without knowing what it refers to. What it *is* is the
/// renderer's own business, and two renderers' handles are not interchangeable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureHandle(pub u64);

/// A texture supplied from outside the renderer: a decoded video frame, a screen capture, a
/// consumer's own content.
///
/// The display list refers to one by [`ExternalTextureId`] and knows nothing else about it. This is
/// what a renderer keeps against that id, and it is the smallest thing a compositing draw needs: how
/// big it is, whether it is already premultiplied, and the renderer's own handle for the resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternalTexture {
    /// What the display list calls it.
    pub id: ExternalTextureId,
    /// The renderer's handle to the resource.
    pub handle: TextureHandle,
    /// Its extent in device pixels.
    pub size: Size<i32, Device>,
    /// Whether its colour channels are already scaled by its alpha.
    ///
    /// Everything composites premultiplied, so a texture that is not gets converted on the way in
    /// rather than blended as though it were — the difference being a bright fringe wherever the
    /// content is partly transparent.
    pub premultiplied: bool,
}
