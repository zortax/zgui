//! A browser canvas as something to present to.

/// A canvas of a page, for [`SharedGraphics`](crate::SharedGraphics) to open a device for and to
/// make a renderer on.
///
/// The element type comes from the `web_sys` that wgpu links, so a crate that holds a canvas
/// passes it here without naming wgpu.
#[derive(Clone, Debug)]
pub struct Canvas(pub wgpu::web_sys::HtmlCanvasElement);

impl From<Canvas> for wgpu::SurfaceTarget<'static> {
    fn from(canvas: Canvas) -> Self {
        Self::Canvas(canvas.0)
    }
}
