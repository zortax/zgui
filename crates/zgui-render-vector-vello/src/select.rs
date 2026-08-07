//! Which rasteriser a device gets.

use std::sync::Arc;

use zgui_geom::{Device, Size};
use zgui_render_vector_coverage::CoverageRaster;
use zgui_render_wgpu::Gpu;
use zgui_render_wgpu::frame::vector::VectorSource;

use crate::raster::VelloRaster;

/// Which of the two rasterisers a device is getting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// The compute-shader path renderer, which is what everything is measured against.
    Compute,
    /// The simpler one, for a device that cannot run the other.
    ///
    /// It is a visible downgrade rather than a transparent one — no blend or compose set, and
    /// multisampled coverage rather than analytic — and it exists so that a device without compute
    /// shaders draws icons rather than nothing.
    Coverage,
}

/// What `gpu` can run.
///
/// Read from what the *device* turned out to support rather than from what the adapter promised,
/// because the two can disagree and only one of them is the thing that will be asked to run a
/// shader.
pub fn chosen(gpu: &Gpu) -> Choice {
    if gpu.capabilities().vector_compute {
        Choice::Compute
    } else {
        Choice::Coverage
    }
}

/// The rasteriser for `gpu`, sized for a surface of `size` device pixels.
///
/// The probe and the fallback are one function on purpose. A capability check whose alternative
/// branch is written later is a device that renders nothing until it is, and the devices this
/// concerns are precisely the ones nobody develops on.
///
/// One line is logged when the fallback is taken, naming what was missing.
pub fn for_device(gpu: &Arc<Gpu>, size: Size<i32, Device>) -> Box<dyn VectorSource> {
    let width = size.width.max(1) as u32;
    let height = size.height.max(1) as u32;
    if chosen(gpu) == Choice::Compute {
        match VelloRaster::new(gpu, width, height) {
            Ok(raster) => return Box::new(raster),
            Err(failure) => tracing::warn!(
                %failure,
                adapter = %gpu.describe(),
                "the path renderer would not build on this device; falling back"
            ),
        }
    } else {
        tracing::info!(
            adapter = %gpu.describe(),
            "this device runs no compute shaders over writable storage textures, so vector \
             content is rasterised by the simpler path: no blend or compose set, and multisampled \
             coverage rather than analytic"
        );
    }
    Box::new(CoverageRaster::new(gpu, width, height))
}

/// Gives `renderer` a lazy rasteriser factory for its device.
///
/// This is the whole of what a window has to do to draw vector content. A renderer without it plans
/// a display list's vector passes, counts them, and composites a scratch nothing ever wrote — which
/// is not an error, not a warning and not a wrong colour, but an empty rectangle where a drawing
/// should be. So it is one call rather than a sequence, and every path that opens a renderer makes
/// it.
///
/// ```no_run
/// use zgui_geom::{Scale, Size};
/// use zgui_render::RenderTarget;
/// use zgui_render_wgpu::Builder;
///
/// let target = RenderTarget::new(Size::new(256, 256), Scale::new(1.0));
/// let mut renderer = Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)?;
/// zgui_render_vector_vello::attach(&mut renderer, target.size);
/// assert!(renderer.has_vector_raster());
/// # Ok::<(), zgui_render::GpuUnavailable>(())
/// ```
pub fn attach(renderer: &mut zgui_render_wgpu::WgpuRenderer, size: Size<i32, Device>) {
    let _ = size;
    let backend = match chosen(renderer.gpu()) {
        Choice::Compute => zgui_render::VectorBackend::Vello,
        Choice::Coverage => zgui_render::VectorBackend::Coverage,
    };
    renderer.set_vector_factory_for(for_device, backend);
}
