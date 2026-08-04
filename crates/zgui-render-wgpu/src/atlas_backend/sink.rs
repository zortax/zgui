//! The device side of the atlas.

use std::collections::BTreeMap;
use std::sync::Arc;

use zgui_atlas::{SinkError, TextureFormat, TextureId, TextureSink};
use zgui_geom::{Device, Rect, Size};
use zgui_profile::{Counter, counter};

use crate::buffer::upload::UploadBelt;
use crate::gpu::device::Gpu;

/// The atlas textures, and the bind group each of them is read through.
///
/// The atlas itself decides *which* tile goes *where* and knows nothing about a device; this is
/// the whole of what a device contributes. Both formats are fixed rather than following the
/// surface, which is what removes the channel swizzle a surface-following colour format forces on
/// every upload — and what keeps a text-heavy frame's upload volume down by a factor of four,
/// since coverage is one byte per texel and not four.
#[derive(Debug)]
pub struct AtlasTextures {
    /// The device the textures live on.
    gpu: Arc<Gpu>,
    /// Each texture, its view and the bind group that reads it.
    textures: BTreeMap<TextureId, Entry>,
    /// The sampler every atlas is read through.
    sampler: wgpu::Sampler,
    /// The layout the bind groups are built against.
    layout: wgpu::BindGroupLayout,
    /// Reusable staging storage for batched atlas writes.
    uploader: UploadBelt,
    /// Encoder open between the texture sink's batch boundaries.
    upload_encoder: Option<wgpu::CommandEncoder>,
    /// Whether writes are currently being collected into a batch.
    batching: bool,
}

/// One atlas texture.
#[derive(Debug)]
struct Entry {
    /// The texture.
    texture: wgpu::Texture,
    /// The bind group reading it.
    bind_group: wgpu::BindGroup,
    /// How many bytes it occupies.
    bytes: u64,
}

impl AtlasTextures {
    /// An empty set of atlas textures on `gpu`, read through `layout`.
    pub fn new(gpu: Arc<Gpu>, layout: wgpu::BindGroupLayout) -> Self {
        // Nearest filtering, because a tile is rasterised at the size it is drawn at: filtering
        // would blur a glyph and, at a tile's edge, sample its neighbour.
        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zgui.atlas.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Self {
            gpu,
            textures: BTreeMap::new(),
            sampler,
            layout,
            uploader: UploadBelt::default(),
            upload_encoder: None,
            batching: false,
        }
    }

    /// The bind group reading `texture`, if it has been created.
    pub fn bind_group(&self, texture: TextureId) -> Option<&wgpu::BindGroup> {
        self.textures.get(&texture).map(|entry| &entry.bind_group)
    }

    /// How many bytes every atlas texture occupies.
    pub fn bytes(&self) -> u64 {
        self.textures.values().map(|entry| entry.bytes).sum()
    }

    /// Mapped and in-flight atlas upload buffers.
    pub fn staging_bytes(&self) -> u64 {
        self.uploader.bytes()
    }
}

impl TextureSink for AtlasTextures {
    fn create_texture(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
    ) -> Result<(), SinkError> {
        let extent = wgpu::Extent3d {
            width: size.width.max(1) as u32,
            height: size.height.max(1) as u32,
            depth_or_array_layers: 1,
        };
        let created = self.gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("zgui.atlas"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format(format),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = created.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self
            .gpu
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("zgui.atlas"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
        self.textures.insert(
            texture,
            Entry {
                bytes: format.bytes_for(extent.width, extent.height),
                texture: created,
                bind_group,
            },
        );
        Ok(())
    }

    fn write_texture(
        &mut self,
        texture: TextureId,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError> {
        let entry = self.textures.get(&texture).ok_or_else(|| {
            SinkError::new(format!("no atlas texture has been created for {texture:?}"))
        })?;
        let width = bounds.size.width.max(0) as u32;
        let height = bounds.size.height.max(0) as u32;
        if width == 0 || height == 0 {
            return Ok(());
        }
        if self.batching {
            let encoder = self.upload_encoder.get_or_insert_with(|| {
                self.gpu
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("zgui.atlas.upload"),
                    })
            });
            self.uploader
                .write_texture(&self.gpu, encoder, &entry.texture, bounds, format, bytes);
            counter::bump(Counter::AtlasTextureWrites);
            return Ok(());
        }
        self.gpu.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &entry.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: bounds.origin.x.max(0) as u32,
                    y: bounds.origin.y.max(0) as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * format.bytes_per_texel()),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        counter::bump(Counter::AtlasTextureWrites);
        counter::add(Counter::BytesUploaded, bytes.len() as u64);
        Ok(())
    }

    fn begin_uploads(&mut self) -> Result<(), SinkError> {
        debug_assert!(!self.batching, "atlas upload batches do not nest");
        self.batching = true;
        self.uploader.begin_frame(&self.gpu);
        Ok(())
    }

    fn finish_uploads(&mut self) {
        self.batching = false;
        let Some(encoder) = self.upload_encoder.take() else {
            return;
        };
        self.uploader.finish();
        self.gpu.queue().submit([encoder.finish()]);
        counter::bump(Counter::AtlasUploadBatches);
        self.uploader.recall();
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        self.textures.remove(&texture);
    }
}

/// The device format an atlas format is created as.
///
/// Colour tiles are premultiplied before they ever reach here, which is what keeps a soft edge
/// from blooming over a light background.
fn wgpu_format(format: TextureFormat) -> wgpu::TextureFormat {
    match format {
        TextureFormat::R8Unorm => wgpu::TextureFormat::R8Unorm,
        TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
    }
}
