//! The two pipelines and the buffers they read.

use bytemuck::Pod;
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
    /// The layout its three storage buffers are bound through.
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
            entries: &[storage(0), storage(1), storage(2)],
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

/// A storage buffer that grows to the largest thing it has held.
#[derive(Debug)]
pub struct Storage {
    /// The buffer.
    buffer: wgpu::Buffer,
    /// What it is called, so a driver message names it.
    label: &'static str,
    /// How many bytes it holds.
    capacity: u64,
}

impl Storage {
    /// The smallest allocation, which is also what an empty frame gets: a bind group has to name a
    /// buffer whether or not this frame put anything in it.
    const MINIMUM: u64 = 256;

    /// An empty buffer named `label`.
    pub fn new(gpu: &Gpu, label: &'static str) -> Self {
        Self {
            buffer: allocate(gpu, label, Self::MINIMUM),
            label,
            capacity: Self::MINIMUM,
        }
    }

    /// Writes `values`, growing if they do not fit.
    pub fn write<T: Pod>(&mut self, gpu: &Gpu, values: &[T]) {
        let bytes: &[u8] = bytemuck::cast_slice(values);
        if bytes.len() as u64 > self.capacity {
            self.capacity = (bytes.len() as u64).next_power_of_two().max(Self::MINIMUM);
            self.buffer = allocate(gpu, self.label, self.capacity);
        }
        if !bytes.is_empty() {
            gpu.queue().write_buffer(&self.buffer, 0, bytes);
        }
    }

    /// The binding a bind group names.
    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }

    /// How many bytes it holds.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }
}

/// Allocates a storage buffer.
fn allocate(gpu: &Gpu, label: &'static str, size: u64) -> wgpu::Buffer {
    gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A read-only storage buffer, visible to both stages.
fn storage(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
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
