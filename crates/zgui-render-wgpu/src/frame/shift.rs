//! Moving a region of the composed target instead of drawing it again.
//!
//! The composed target outlives the frame — that is what makes partial redrawing legal at all — so
//! the pixels a scroll moves are already on the device, one whole-pixel translation from where they
//! now belong. This copies them there, and the caller draws only the strip the copy left undefined.
//!
//! # Why it goes through a scratch texture
//!
//! A copy whose source and destination subresources overlap is forbidden, and a scroll's do overlap
//! by construction: a list moved up by thirty pixels reads rows 50.. and writes rows 20.., which is
//! most of the port twice. So the region is copied out to a scratch of its own and back, which is
//! two copies of the surviving area rather than one. That is bandwidth, and bandwidth is what this
//! is trading for: the emit walk it removes is CPU, and 77 % of a scroll frame
//! (`docs/perf/scroll-frame.md`).
//!
//! The scratch is allocated once, at the composed target's own size class, and only for a window
//! that has actually shifted something.

use zgui_geom::{Device, Rect};
use zgui_render::ScrollShift;

use crate::gpu::device::Gpu;
use crate::target::scene_texture::{SceneTexture, size_class};

/// The scratch a shift is staged through.
#[derive(Debug)]
pub struct ShiftScratch {
    /// The texture.
    texture: wgpu::Texture,
    /// What it was allocated to hold.
    allocated: (u32, u32),
    /// The format it was allocated in, which has to match what is copied through it.
    format: wgpu::TextureFormat,
}

impl ShiftScratch {
    /// The usage staging a copy needs, and nothing else: it is never drawn into and never sampled.
    const USAGE: wgpu::TextureUsages =
        wgpu::TextureUsages::COPY_SRC.union(wgpu::TextureUsages::COPY_DST);

    /// A scratch able to stage a copy of `size` pixels in `format`.
    fn new(gpu: &Gpu, size: (u32, u32), format: wgpu::TextureFormat) -> Self {
        let allocated = (
            size_class(size.0 as i32).max(1) as u32,
            size_class(size.1 as i32).max(1) as u32,
        );
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("zgui.shift.scratch"),
            size: wgpu::Extent3d {
                width: allocated.0,
                height: allocated.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: Self::USAGE,
            view_formats: &[],
        });
        Self {
            texture,
            allocated,
            format,
        }
    }

    /// How many bytes it is holding.
    pub fn bytes(&self) -> u64 {
        let block = self.format.target_pixel_byte_cost().unwrap_or(4) as u64;
        u64::from(self.allocated.0) * u64::from(self.allocated.1) * block
    }
}

/// Copies the still-valid part of `shift`'s region to where it now belongs.
///
/// Returns whether anything was copied. False means the movement carried the whole region off
/// itself, so there is nothing to keep and the caller's damage already covers all of it.
///
/// `scratch` is created on first use and grown when a larger region is shifted; it is passed in
/// rather than owned here so that the renderer can report and release its memory with everything
/// else it holds.
pub fn apply(
    gpu: &Gpu,
    encoder: &mut wgpu::CommandEncoder,
    composed: &SceneTexture,
    scratch: &mut Option<ShiftScratch>,
    shift: ScrollShift,
) -> bool {
    let (Some(from), Some(to)) = (shift.source(), shift.destination()) else {
        return false;
    };
    debug_assert_eq!(
        from.size, to.size,
        "a shift reads exactly what it writes, or it is not a translation",
    );
    let extent = wgpu::Extent3d {
        width: from.size.width.max(0) as u32,
        height: from.size.height.max(0) as u32,
        depth_or_array_layers: 1,
    };
    if extent.width == 0 || extent.height == 0 {
        return false;
    }

    let format = composed.format();
    let wanted = (extent.width, extent.height);
    let fits = scratch.as_ref().is_some_and(|held| {
        held.format == format && held.allocated.0 >= wanted.0 && held.allocated.1 >= wanted.1
    });
    if !fits {
        *scratch = Some(ShiftScratch::new(gpu, wanted, format));
    }
    let staged = scratch.as_ref().expect("just allocated");

    encoder.copy_texture_to_texture(
        texel_copy(composed.texture(), from),
        wgpu::TexelCopyTextureInfo {
            texture: &staged.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        extent,
    );
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &staged.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        texel_copy(composed.texture(), to),
        extent,
    );
    true
}

/// One end of a copy, at a rectangle's origin.
fn texel_copy(texture: &wgpu::Texture, at: Rect<i32, Device>) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d {
            x: at.origin.x.max(0) as u32,
            y: at.origin.y.max(0) as u32,
            z: 0,
        },
        aspect: wgpu::TextureAspect::All,
    }
}
