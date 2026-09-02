//! The bind-group layouts, and what each of them is for.

use crate::gpu::device::Gpu;

/// The layouts every pipeline is built from.
///
/// Three of them read a texture, and they are deliberately not one layout. A copy reads exactly
/// one texel at the fragment's own coordinate and binds no sampler at all, which is what keeps it
/// out of the filtering restrictions; an atlas reads through a filtering sampler and nothing else;
/// and anything that magnifies — a half-resolution target composited back up to size — needs both
/// a filtering sampler and a block of its own describing what it is magnifying.
#[derive(Debug)]
pub struct Layouts {
    /// The block describing the target being drawn into, and the side tables every instance
    /// indexes into.
    ///
    /// The block is read through a dynamic offset because it is per *target* rather than per
    /// frame: a frame writes into the composed target and into one for every isolated group, and a
    /// single rewritten block would give every pass of the frame the last one written.
    pub frame: wgpu::BindGroupLayout,
    /// One pipeline's instances.
    pub instances: wgpu::BindGroupLayout,
    /// A texture read through a filtering sampler.
    pub sampled: wgpu::BindGroupLayout,
    /// A texture read one texel at a time, with no sampler.
    pub loaded: wgpu::BindGroupLayout,
    /// A block of its own, a texture, and a filtering sampler.
    pub filtered: wgpu::BindGroupLayout,
    /// A frame's vector-composite instances, and the scratch they read.
    ///
    /// It is a *storage* array rather than a block addressed by a dynamic offset, because a pass
    /// composited one item at a time is one draw call over many instances and each instance needs
    /// its own quad and its own clip. The scratch is read one texel at a time with no sampler, so
    /// this layout carries none.
    pub vector: wgpu::BindGroupLayout,
    /// One application effect's parameters.
    ///
    /// A block addressed by a dynamic offset rather than a storage array read per instance,
    /// because the parameters are the same for every rectangle of a draw: two rectangles that
    /// disagree about them are two draws, and the batcher breaks the run where they do.
    pub effect: wgpu::BindGroupLayout,
}

impl Layouts {
    /// Builds the layouts on `gpu`.
    pub fn new(gpu: &Gpu) -> Self {
        let device = gpu.device();
        Self {
            frame: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.frame"),
                entries: &[
                    dynamic_uniform(0),
                    storage(1),
                    storage(2),
                    storage(3),
                    storage(4),
                ],
            }),
            effect: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.effect"),
                entries: &[dynamic_uniform(0)],
            }),
            instances: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.instances"),
                // The instances, the remap list the shader reads them through — the array keeps
                // push order, and the sorted list beside it is the draw order — and the frame's
                // chunk offsets, named by the remap entries' high bits.
                entries: &[storage(0), storage(1), storage(2)],
            }),
            sampled: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.sampled"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            }),
            filtered: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.filtered"),
                entries: &[
                    dynamic_uniform(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            }),
            vector: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.vector"),
                entries: &[
                    storage(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            }),
            loaded: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.loaded"),
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
            }),
        }
    }
}

/// A uniform block addressed by a dynamic offset, visible to both stages because the target's
/// extent is needed in one and its scale in both.
fn dynamic_uniform(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: None,
        },
        count: None,
    }
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
