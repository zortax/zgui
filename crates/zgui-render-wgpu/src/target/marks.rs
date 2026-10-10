//! The pages union marks write their coverage into.

use zgui_geom::{Device, Size};
use zgui_render::{Decay, Extent};

use crate::gpu::device::Gpu;
use crate::pipeline::kind::PipelineKind;

/// A two-dimensional array texture of coverage, one layer per page of the frame's mark plan.
///
/// Each page is the size of the surface, so a bin never has to be split. Nothing is allocated
/// while no frame plans a bin, and the pages shrink again once fewer are needed for a while.
#[derive(Debug, Default)]
pub struct MarksScratch {
    /// The array texture, or nothing while no frame has needed one.
    texture: Option<wgpu::Texture>,
    /// One view per layer, which is what a coverage pass draws into.
    layers: Vec<wgpu::TextureView>,
    /// The whole array, which is what a composite reads.
    array: Option<wgpu::TextureView>,
    /// The texel extent of every layer.
    extent: Size<i32, Device>,
    /// What is held, and how long it has been more than any frame needed.
    decay: Decay,
    /// Changes whenever the texture does, for bind-group cache keys.
    generation: u64,
}

impl MarksScratch {
    /// Makes sure there is room for `pages` pages of `extent`, or lets go of what is held once
    /// no page has been needed for a while.
    pub fn ensure(&mut self, gpu: &Gpu, extent: Size<i32, Device>, pages: u32) {
        let want = if pages == 0 {
            Extent::NONE
        } else {
            Extent::new(
                extent.width.max(1) as u32,
                extent.height.max(1) as u32,
                pages,
            )
            .classed()
        };
        let Some(held) = self.decay.wants(want) else {
            return;
        };
        if held.layers == 0 {
            self.texture = None;
            self.layers.clear();
            self.array = None;
            self.extent = Size::new(0, 0);
            self.generation = self.generation.wrapping_add(1);
            return;
        }
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("zgui.marks.bins"),
            size: wgpu::Extent3d {
                width: held.width,
                height: held.height,
                depth_or_array_layers: held.layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: PipelineKind::COVERAGE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.layers = (0..held.layers)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("zgui.marks.bins.page"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        self.array = Some(texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("zgui.marks.bins.array"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        }));
        self.texture = Some(texture);
        self.extent = Size::new(held.width as i32, held.height as i32);
        self.generation = self.generation.wrapping_add(1);
    }

    /// The allocation epoch of the pages, for bind-group cache keys.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The view a coverage pass draws page `page` through.
    pub fn page(&self, page: u32) -> Option<&wgpu::TextureView> {
        self.layers.get(page as usize)
    }

    /// The view a composite reads every page through.
    pub fn array(&self) -> Option<&wgpu::TextureView> {
        self.array.as_ref()
    }

    /// The texel extent of every page.
    pub fn extent(&self) -> Size<i32, Device> {
        self.extent
    }

    /// How many bytes the pages hold.
    pub fn bytes(&self) -> u64 {
        self.texture.as_ref().map_or(0, |texture| {
            u64::from(texture.width())
                * u64::from(texture.height())
                * u64::from(texture.depth_or_array_layers())
                * u64::from(texture.format().block_copy_size(None).unwrap_or(1))
        })
    }

    /// Lets go of the pages, and reports how many bytes that freed.
    pub fn release(&mut self) -> u64 {
        let freed = self.bytes();
        let generation = self.generation.wrapping_add(1);
        *self = Self::default();
        self.generation = generation;
        freed
    }
}
