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
    /// One mark draw's block, and the payload of the kind it draws.
    ///
    /// The block names the item and its bin. The payload is a table walked by the draw's
    /// instances, one prim each.
    pub marks: wgpu::BindGroupLayout,
    /// One mark draw's block, and the bin pages a union item's composite reads.
    pub mark_composite: wgpu::BindGroupLayout,
}

impl Layouts {
    /// Builds the layouts on `gpu`.
    pub fn new(gpu: &Gpu) -> Self {
        let device = gpu.device();
        Self {
            frame: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.frame"),
                entries: &[dynamic_uniform(0), table(1), table(2), table(3), table(4)],
            }),
            effect: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.effect"),
                entries: &[dynamic_uniform(0)],
            }),
            marks: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.marks"),
                entries: &[dynamic_uniform(0), table(1)],
            }),
            mark_composite: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.mark_composite"),
                entries: &[
                    dynamic_uniform(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            }),
            instances: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("zgui.bind.instances"),
                // The instance arena, which keeps push order, and the draw order beside it. The
                // order is vertex input for every lane but one, so this binding of it is for the
                // mark lane alone: a mark's draw names its position in the order rather than its
                // instance index, and indexing needs the stream bound. The chunk offsets that
                // used to be a third binding are resolved into the order on the host.
                entries: &[table(0), table(1)],
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
                    table(0),
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

/// One side table, as a `rgba32uint` texture rather than a storage buffer.
///
/// Storage buffers arrive in OpenGL 4.3 and OpenGL ES 3.1, so a GL 3.3 context has none; a
/// texture read one texel at a time gives the same random access on every device.
fn table(binding: u32) -> wgpu::BindGroupLayoutEntry {
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
