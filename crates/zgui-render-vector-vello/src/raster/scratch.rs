//! The texture a batch of paths lands in before an ordinary draw composites it.

use zgui_profile::{Counter, counter};
use zgui_render::vector::decay::{Decay, Extent};
use zgui_render_wgpu::Gpu;

/// The format the path renderer's final stage is wired to write.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A two-dimensional array texture, in the surface's own coordinates, with room for the passes of
/// one frame that cannot share a layer.
///
/// A layer holds device pixels where they belong rather than in a corner of its own, so two passes
/// that do not meet on the screen do not meet in the layer either and one layer holds both. What
/// decides the depth is therefore how much of a frame's vector work overlaps, not how much of it
/// there is — a frame of five hundred passes that nowhere overlap costs one layer.
///
/// `STORAGE_BINDING` is what the path renderer needs; `RENDER_ATTACHMENT` is what the pre-clear
/// needs, and the pre-clear is not optional. A rasterisation that overflows a fixed buffer can
/// return *success* having written nothing at all, and a layer that was not cleared first then
/// composites the previous frame's paths — wrong pixels, with nothing to notice them by. Cleared
/// first, the same failure is missing content instead.
#[derive(Debug)]
pub struct Scratch {
    /// The array texture, or nothing before the first pass is resourced.
    texture: Option<wgpu::Texture>,
    /// One view per layer, which is what the rasterisation is pointed at.
    views: Vec<wgpu::TextureView>,
    /// The extent every layer is allocated at.
    extent: (u32, u32),
    /// What is held, and how long it has been more than any frame needed.
    decay: Decay,
}

impl Scratch {
    /// How many layers a scratch starts with, and the fewest it ever shrinks to.
    pub const LAYERS: u32 = 4;

    /// The most layers a frame may have.
    ///
    /// **This is a ceiling rather than a wrap, and it is reached by overlap rather than by count.**
    /// Every pass of a frame is rasterised before any of them is composited — the rasteriser submits
    /// work of its own, which has to happen before the frame's encoder is opened — so a layer holds
    /// its pass's coverage until that pass's composite has read it. Passes that do not overlap on
    /// the surface share a layer freely; only a frame with this many passes stacked over one point
    /// runs out, and it reports that it could not do the work rather than putting one on top of
    /// another, which would cost the wrong content in the right place.
    pub const MAX_LAYERS: u32 = 64;

    /// An unallocated scratch.
    pub fn new() -> Self {
        Self {
            texture: None,
            views: Vec::new(),
            extent: (0, 0),
            decay: Decay::new(),
        }
    }

    /// How many layers there are.
    pub fn layers(&self) -> u32 {
        self.views.len() as u32
    }

    /// Makes sure there is room for `layers` layers of at least `width` by `height`.
    ///
    /// Growth reallocates rather than adding a second texture, because a composite names one
    /// resource and two would mean two bind-group layouts for one job. It is immediate; the way back
    /// down is not, because a scroll's demand swings frame to frame and a texture reallocated on the
    /// quiet frame between two busy ones costs more than it saves.
    pub fn ensure(&mut self, gpu: &Gpu, width: u32, height: u32, layers: u32) {
        let want = Extent::new(
            width.max(1),
            height.max(1),
            layers.clamp(Self::LAYERS, Self::MAX_LAYERS),
        );
        if let Some(extent) = self.decay.wants(want) {
            self.allocate(gpu, extent);
        }
    }

    /// Reallocates the texture and its views at `extent`.
    fn allocate(&mut self, gpu: &Gpu, extent: Extent) {
        let want = (extent.width.max(1), extent.height.max(1));
        let wanted_layers = extent.layers.clamp(Self::LAYERS, Self::MAX_LAYERS);
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("zgui.vector.scratch"),
            size: wgpu::Extent3d {
                width: want.0,
                height: want.1,
                depth_or_array_layers: wanted_layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.views = (0..wanted_layers)
            .map(|layer| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("zgui.vector.scratch.layer"),
                    format: Some(FORMAT),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    usage: None,
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                })
            })
            .collect();
        self.texture = Some(texture);
        self.extent = want;
        self.publish();
    }

    /// Records what the scratch is holding, so that the largest texture in the process is a number
    /// somebody can see.
    ///
    /// Both figures are gauges rather than totals: what is wanted is what the texture costs *now*,
    /// which is the quantity a memory report has to reconcile against and the one a growth check
    /// compares between two moments of the same run.
    fn publish(&self) {
        counter::set(Counter::ScratchLayers, u64::from(self.layers()));
        counter::set(Counter::ScratchBytes, self.bytes());
    }

    /// The view for one layer, or `None` for a layer that does not exist.
    pub fn view(&self, layer: u32) -> Option<&wgpu::TextureView> {
        self.views.get(layer as usize)
    }

    /// The extent every layer is allocated at.
    pub fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// The bytes of video memory the texture occupies.
    pub fn bytes(&self) -> u64 {
        match &self.texture {
            None => 0,
            Some(texture) => {
                u64::from(texture.width())
                    * u64::from(texture.height())
                    * u64::from(texture.depth_or_array_layers())
                    * 4
            }
        }
    }

    /// Drops reproducible scratch while keeping the rasteriser and its fixed state initialized.
    pub fn release(&mut self) -> u64 {
        let freed = self.bytes();
        self.texture = None;
        self.views.clear();
        self.views.shrink_to_fit();
        self.extent = (0, 0);
        self.decay.clear();
        self.publish();
        freed
    }

    /// Clears each of `layers` to transparent, in one submitted encoder.
    ///
    /// A whole layer rather than the region a pass will use: a pass smaller than the last one that
    /// used the layer would otherwise leave the difference holding what the last one wrote.
    pub fn clear(&self, gpu: &Gpu, layers: &[u32]) {
        if layers.is_empty() || self.views.is_empty() {
            return;
        }
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zgui.vector.preclear"),
            });
        for &layer in layers {
            let Some(view) = self.view(layer) else {
                continue;
            };
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zgui.vector.preclear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        gpu.queue().submit([encoder.finish()]);
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}
