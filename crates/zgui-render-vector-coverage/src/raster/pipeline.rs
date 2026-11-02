//! The two pipelines and the buffers they read.

use zgui_render_wgpu::Gpu;

use crate::raster::scratch::FORMAT;

/// The fill.
const COVERAGE: &str = include_str!("../shader/coverage.wgsl");
/// The conversion out of premultiplied form.
const RESOLVE: &str = include_str!("../shader/resolve.wgsl");

/// Everything built once and used every frame.
#[derive(Debug)]
pub struct Pipelines {
    /// Fills one outline into the accumulation texture.
    pub coverage: wgpu::RenderPipeline,
    /// The layout its four tables are bound through.
    pub coverage_layout: wgpu::BindGroupLayout,
    /// Converts one accumulated layer into the straight one.
    pub resolve: wgpu::RenderPipeline,
    /// The layout it reads the accumulated layer through.
    pub resolve_layout: wgpu::BindGroupLayout,
}

impl Pipelines {
    /// Builds both on `gpu`.
    pub fn new(gpu: &Gpu) -> Self {
        let device = gpu.device();
        let coverage_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zgui.vector.coverage"),
            entries: &[storage(0), storage(1), storage(2), storage(3)],
        });
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zgui.vector.coverage.resolve"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let coverage = build(
            gpu,
            &coverage_layout,
            COVERAGE,
            "zgui.vector.coverage",
            ("vs_coverage", "fs_coverage"),
            // Outlines within one pass composite over each other, and that is only a
            // fixed-function blend in premultiplied form.
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        );
        let resolve = build(
            gpu,
            &resolve_layout,
            RESOLVE,
            "zgui.vector.coverage.resolve",
            ("vs_resolve", "fs_resolve"),
            // A conversion replaces; blending it would make the result depend on what the layer
            // happened to hold.
            None,
        );
        Self {
            coverage,
            coverage_layout,
            resolve,
            resolve_layout,
        }
    }
}

/// Where this rasteriser's lookups live.
///
/// The same table texture the rest of the renderer reads through, for the same reason: a device
/// with no storage buffers is precisely the device that falls back to this rasteriser.
pub use zgui_render_wgpu::buffer::tables::TableTexture as Storage;

/// A lookup table, read one texel at a time with no sampler, visible to both stages.
fn storage(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Uint,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

/// Builds one pipeline.
fn build(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    source: &str,
    label: &'static str,
    entries: (&str, &str),
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let device = gpu.device();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some(entries.0),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(entries.1),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
