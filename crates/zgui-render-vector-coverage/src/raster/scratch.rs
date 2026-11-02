//! The two textures one pass passes through.

use zgui_profile::{Counter, counter};
use zgui_render::vector::decay::{Decay, Extent};
use zgui_render_wgpu::Gpu;

/// The format both textures hold, and the one a composite reads.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// One array texture per stage, with room for one round of a frame's passes.
///
/// There are two of them and not one because compositing several outlines over each other is only a
/// fixed-function blend in premultiplied form, while the rasteriser contract says what a composite
/// reads holds *straight* colour. So outlines accumulate premultiplied in the first and one draw
/// converts the whole layer into the second.
///
/// Each pass is rasterised into a bin of its own size in some layer, so the depth follows the area
/// of a frame's passes. Both textures cost the same, which is why this rasteriser cares about the
/// depth twice as much as the other one does.
#[derive(Debug)]
pub struct Scratch {
    /// Where outlines accumulate, premultiplied.
    accumulation: Option<Textures>,
    /// What a composite reads, straight.
    straight: Option<Textures>,
    /// The extent every layer is allocated at.
    extent: (u32, u32),
    /// How many layers are allocated.
    layers: u32,
    /// What is held, and how long it has been more than any frame needed.
    decay: Decay,
}

/// One array texture and a view per layer.
#[derive(Debug)]
struct Textures {
    /// One texture per layer, held because dropping one would take its view with it.
    ///
    /// A layer each rather than one array texture with a layer per pass. Rendering into a layer of
    /// an array texture writes nothing at all on an OpenGL 3.3 device — no error, no warning, an
    /// attachment that silently keeps whatever it held — and this rasteriser is the one such a
    /// device falls back to, so it is the last thing that may depend on that working.
    #[expect(
        dead_code,
        reason = "held so the views outlive the textures they are of"
    )]
    textures: Vec<wgpu::Texture>,
    /// One view per layer.
    views: Vec<wgpu::TextureView>,
}

impl Scratch {
    /// How many layers a scratch starts with, and the fewest it ever shrinks to.
    pub const LAYERS: u32 = 1;

    /// The most layers one round may have.
    ///
    /// A frame whose passes need more is rasterised in rounds, and each round reuses the layers
    /// after the composites of the round before it have read them.
    pub const MAX_LAYERS: u32 = 64;

    /// An unallocated scratch.
    pub fn new() -> Self {
        Self {
            accumulation: None,
            straight: None,
            extent: (0, 0),
            layers: 0,
            decay: Decay::new(),
        }
    }

    /// How many layers there are.
    pub fn layers(&self) -> u32 {
        self.layers
    }

    /// Makes sure there is room for `layers` layers of at least `width` by `height`.
    ///
    /// Growth is immediate; the way back down is not, because a scroll's demand swings frame to
    /// frame and a texture reallocated on the quiet frame between two busy ones costs more than it
    /// saves — and here there are two textures to throw away rather than one.
    pub fn ensure(&mut self, gpu: &Gpu, width: u32, height: u32, layers: u32) {
        let want = Extent::new(
            width.max(1),
            height.max(1),
            layers.clamp(Self::LAYERS, Self::MAX_LAYERS),
        );
        let Some(extent) = self.decay.wants(want) else {
            return;
        };
        let size = (extent.width.max(1), extent.height.max(1));
        let depth = extent.layers.clamp(Self::LAYERS, Self::MAX_LAYERS);
        self.accumulation = Some(allocate(
            gpu,
            size,
            depth,
            "zgui.vector.coverage.accumulation",
        ));
        self.straight = Some(allocate(gpu, size, depth, "zgui.vector.coverage.straight"));
        self.extent = size;
        self.layers = depth;
        self.publish();
    }

    /// Records what the scratch is holding, so that the largest texture in the process is a number
    /// somebody can see.
    ///
    /// Both figures are gauges rather than totals: what is wanted is what the textures cost *now*,
    /// which is the quantity a memory report has to reconcile against and the one a growth check
    /// compares between two moments of the same run.
    fn publish(&self) {
        counter::set(Counter::ScratchLayers, u64::from(self.layers));
        counter::set(Counter::ScratchBytes, self.bytes());
    }

    /// The view outlines accumulate into.
    pub fn accumulation(&self, layer: u32) -> Option<&wgpu::TextureView> {
        self.accumulation
            .as_ref()
            .and_then(|held| held.views.get(layer as usize))
    }

    /// The view a composite reads.
    pub fn straight(&self, layer: u32) -> Option<&wgpu::TextureView> {
        self.straight
            .as_ref()
            .and_then(|held| held.views.get(layer as usize))
    }

    /// The extent every layer is allocated at.
    pub fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// The bytes of video memory both textures occupy.
    pub fn bytes(&self) -> u64 {
        let one = u64::from(self.extent.0) * u64::from(self.extent.1) * u64::from(self.layers) * 4;
        if self.accumulation.is_some() {
            2 * one
        } else {
            0
        }
    }

    /// Drops both reproducible scratch textures after wall-clock idleness.
    pub fn release(&mut self) -> u64 {
        let freed = self.bytes();
        self.accumulation = None;
        self.straight = None;
        self.extent = (0, 0);
        self.layers = 0;
        self.decay.clear();
        self.publish();
        freed
    }

    /// Clears each of `layers` in both textures, in one submitted encoder.
    ///
    /// Both, and not only the one that is written last: a pass that fails part way through leaves
    /// what a composite reads holding the previous frame's paths otherwise, which is wrong pixels
    /// rather than missing ones and has nothing to notice it by.
    pub fn clear(&self, gpu: &Gpu, layers: &[u32]) {
        if layers.is_empty() {
            return;
        }
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zgui.vector.coverage.preclear"),
            });
        for &layer in layers {
            for view in [self.accumulation(layer), self.straight(layer)]
                .into_iter()
                .flatten()
            {
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("zgui.vector.coverage.preclear"),
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
        }
        gpu.queue().submit([encoder.finish()]);
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}

/// Allocates one texture per layer, and the view each is drawn into and read through.
fn allocate(gpu: &Gpu, extent: (u32, u32), layers: u32, label: &'static str) -> Textures {
    let textures: Vec<wgpu::Texture> = (0..layers)
        .map(|_| {
            gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: extent.0,
                    height: extent.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        })
        .collect();
    let views = textures
        .iter()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    Textures { textures, views }
}
