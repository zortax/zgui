//! The persistent target a frame is composed into.

use zgui_geom::{Device, Point, Rect, Size};

use crate::gpu::device::Gpu;

/// How coarsely the composed target's extent is rounded.
///
/// An interactive resize delivers one new size per frame, and reallocating an exact-size target
/// per event is a multi-megabyte allocate-and-free on the frame path during the one interaction
/// where the budget is tightest. Rounding up to a class means growth during a drag costs a handful
/// of allocations rather than one per frame, and shrinking costs none at all.
pub const SIZE_CLASS: i32 = 256;

/// The extent an axis of `length` pixels is allocated at.
pub fn size_class(length: i32) -> i32 {
    let length = length.max(1) as u32;
    (length.div_ceil(SIZE_CLASS as u32) * SIZE_CLASS as u32) as i32
}

/// The persistent colour target every frame composes into, at the surface's *size class*.
///
/// A frame is never composed straight into an acquired surface texture. Every acquisition yields a
/// brand-new resource marked wholly uninitialised, so loading from one costs a full clear before
/// the frame's own commands run — which would make "redraw only what changed" produce a black
/// frame everywhere it did not draw. Composing into a target that outlives the frame is what makes
/// partial redrawing legal at all, and it is why the copy to the surface is unconditional and
/// covers the whole surface.
///
/// The format is the surface's with any `*Srgb` suffix removed, so that no blend into this texture
/// runs in linear light.
#[derive(Debug)]
pub struct SceneTexture {
    /// The texture.
    texture: wgpu::Texture,
    /// A view of the whole of it.
    view: wgpu::TextureView,
    /// Its format.
    format: wgpu::TextureFormat,
    /// The sub-rectangle the surface currently occupies. The slack around it is never read.
    used: Rect<i32, Device>,
}

impl SceneTexture {
    /// The usage a composed target needs: drawn into, read by the copy, and copied out by a test.
    const USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
        .union(wgpu::TextureUsages::TEXTURE_BINDING)
        .union(wgpu::TextureUsages::COPY_SRC);

    /// Allocates a target able to hold `size`, in `format`.
    ///
    /// # Panics
    ///
    /// Panics in a debug build if `format` encodes, which would move every blend into linear light
    /// one step before the surface could.
    pub fn new(gpu: &Gpu, size: Size<i32, Device>, format: wgpu::TextureFormat) -> Self {
        debug_assert!(
            !format.is_srgb(),
            "the composed target must not encode: {format:?}"
        );
        let allocated: Size<i32, Device> =
            Size::new(size_class(size.width), size_class(size.height));
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("zgui.scene"),
            size: wgpu::Extent3d {
                width: allocated.width as u32,
                height: allocated.height as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: Self::USAGE,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            format,
            used: Rect::new(Point::new(0, 0), size.non_negative()),
        }
    }

    /// Points the target at a surface of `size`, reallocating only when the size class changes.
    ///
    /// Returns whether the texture was reallocated, which is what tells the caller that nothing in
    /// it survives and the next frame has to redraw all of it.
    pub fn resize(&mut self, gpu: &Gpu, size: Size<i32, Device>) -> bool {
        let wanted: Size<i32, Device> = Size::new(size_class(size.width), size_class(size.height));
        if wanted == self.allocated() {
            self.used = Rect::new(Point::new(0, 0), size.non_negative());
            return false;
        }
        *self = Self::new(gpu, size, self.format);
        true
    }

    /// The extent actually allocated, which is at least the surface's.
    pub fn allocated(&self) -> Size<i32, Device> {
        Size::new(self.texture.width() as i32, self.texture.height() as i32)
    }

    /// The sub-rectangle the surface occupies.
    pub fn used(&self) -> Rect<i32, Device> {
        self.used
    }

    /// The texture, for a copy out of it.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// A view of the whole texture.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// The format it holds.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// How many bytes of device memory it occupies.
    ///
    /// The figure is the texel's own size times the *allocated* extent, which is the size class
    /// rather than the surface: the slack is allocated and paid for whether or not it is drawn
    /// into. It is deliberately not the attachment cost a device budgets a render pass against —
    /// that figure is eight bytes for a four-byte texel, and reporting it here would put every
    /// memory budget out by a factor of two against an atlas measured in real bytes.
    pub fn bytes(&self) -> u64 {
        let allocated = self.allocated();
        let per_texel = self.format.block_copy_size(None).unwrap_or(4) as u64;
        per_texel * allocated.width.max(0) as u64 * allocated.height.max(0) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::{SIZE_CLASS, size_class};

    #[test]
    fn a_size_rounds_up_to_its_class_and_never_to_nothing() {
        assert_eq!(size_class(1), SIZE_CLASS);
        assert_eq!(size_class(0), SIZE_CLASS);
        assert_eq!(size_class(-4), SIZE_CLASS);
        assert_eq!(size_class(SIZE_CLASS), SIZE_CLASS);
        assert_eq!(size_class(SIZE_CLASS + 1), 2 * SIZE_CLASS);
        assert_eq!(size_class(1920), 2048);
        assert_eq!(size_class(1080), 1280);
    }

    #[test]
    fn a_drag_across_one_class_costs_one_reallocation_and_not_one_per_frame() {
        // The whole point of the class: consecutive resize events inside a band agree.
        let band: Vec<i32> = (1793..=2048).map(size_class).collect();
        assert!(band.iter().all(|allocated| *allocated == 2048));
        assert_ne!(size_class(1792), size_class(1793));
    }
}
