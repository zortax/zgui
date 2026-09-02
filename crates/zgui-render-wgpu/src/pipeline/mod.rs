//! Building the render pipelines, and keeping them.

pub mod blur;
pub mod composite;
pub mod effect_filter;
pub mod external;
pub mod kind;
pub mod layout;
pub mod vector;

use std::collections::{BTreeMap, HashMap};

use crate::effect::{EffectProgram, Effects};
use crate::gpu::device::Gpu;
use crate::gpu::pipeline_cache::PipelineCache;
use crate::pipeline::kind::PipelineKind;
use crate::pipeline::layout::Layouts;
use crate::shader::{self, Module};
use zgui_scene::ShaderId;

/// Every pipeline built so far, and everything they are built from.
///
/// A pipeline is keyed by its kind *and* by the attachment format it draws into, because a
/// pipeline's colour target must match the attachment. They are built on demand rather than up
/// front: a document that isolates nothing never allocates a target in the second format, and
/// therefore never pays to compile a single pipeline for it.
#[derive(Debug)]
pub struct Pipelines {
    /// The bind-group layouts.
    layouts: Layouts,
    /// One compiled shader module per source module.
    modules: BTreeMap<Module, wgpu::ShaderModule>,
    /// The pipelines, by kind and attachment format.
    built: HashMap<(PipelineKind, wgpu::TextureFormat), wgpu::RenderPipeline>,
    /// The persisted compilation cache, when the driver offers one.
    cache: PipelineCache,
    /// Whether the device can blend against a second colour output.
    dual_source_blending: bool,
    /// The application effects this device has been told about, and their pipelines.
    ///
    /// Held here rather than beside here because an effect's pipeline is built from the same
    /// layouts and the same compilation cache the framework's own are, and a second owner of
    /// either would be a second answer to what an effect's clip is.
    effects: Effects,
    /// How many of the process's declarations this device has caught up with.
    declared: usize,
}

impl Pipelines {
    /// Prepares to build pipelines on `gpu`.
    ///
    /// # Panics
    ///
    /// Panics in a debug build when a shader's declaration of an instance structure disagrees with
    /// the Rust one. That check runs here, once, because the alternative is a rendering artefact
    /// with no error attached to it.
    pub fn new(gpu: &Gpu) -> Self {
        if cfg!(debug_assertions)
            && let Err(error) = shader::check_layouts()
        {
            panic!("{error}");
        }
        Self {
            layouts: Layouts::new(gpu),
            modules: BTreeMap::new(),
            built: HashMap::new(),
            cache: PipelineCache::load(gpu),
            dual_source_blending: gpu.capabilities().subpixel_text,
            effects: Effects::new(),
            declared: 0,
        }
    }

    /// Registers every effect the process has declared that this device has not seen.
    ///
    /// Called at the top of a frame. On the frame after an application declares nothing new it is
    /// one comparison, which is what makes it cheap enough to be unconditional — and being
    /// unconditional is what makes a second window and a rebuilt device end up with the effects
    /// the first one had.
    pub fn sync_effects(&mut self, gpu: &Gpu) {
        let declared = crate::effect::declared_count();
        if declared == self.declared {
            return;
        }
        let mut pending: Vec<(ShaderId, EffectProgram)> = Vec::new();
        crate::effect::declared_since(self.declared, |id, program| pending.push((id, program)));
        for (id, program) in pending {
            if let Err(error) = self.effects.register(gpu, id, program) {
                // Drawing nothing is the answer. The alternative is a pipeline built from a
                // structure the host writes a different shape into, which is a rectangle full of
                // the wrong numbers and no error anywhere.
                tracing::error!(effect = program.label, "{error}");
            }
        }
        self.declared = declared;
    }

    /// Forgets every effect and re-registers them, which is what a lost device needs.
    pub fn rebuild_effects(&mut self, gpu: &Gpu) {
        self.effects.clear();
        self.declared = 0;
        self.sync_effects(gpu);
    }

    /// Accepts an application effect under `id`, replacing anything registered under it.
    pub fn register_effect(
        &mut self,
        gpu: &Gpu,
        id: ShaderId,
        program: EffectProgram,
    ) -> Result<(), String> {
        self.effects.register(gpu, id, program)
    }

    /// Forgets the application effect registered under `id`.
    pub fn release_effect(&mut self, id: ShaderId) {
        self.effects.release(id);
    }

    /// Forgets every application effect, which is what a lost device leaves behind.
    pub fn clear_effects(&mut self) {
        self.effects.clear();
    }

    /// Whether anything is registered under `id`.
    pub fn has_effect(&self, id: ShaderId) -> bool {
        self.effects.contains(id)
    }

    /// How many application effects are registered.
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    /// Says once that an effect could not be drawn, and why.
    pub fn note_undrawable_effect(&self, id: ShaderId, why: &'static str) {
        self.effects.note_undrawable(id, why);
    }

    /// The pipeline drawing the effect `id` into an attachment of `format`, building it if needed.
    ///
    /// `None` for a handle nothing is registered under, which is a display list built against an
    /// effect that has since been released. Drawing nothing is the answer there: the alternative
    /// is drawing the rectangle with whatever pipeline happens to be bound.
    pub fn effect(
        &mut self,
        gpu: &Gpu,
        id: ShaderId,
        format: wgpu::TextureFormat,
    ) -> Option<&wgpu::RenderPipeline> {
        self.effects
            .pipeline(gpu, &self.layouts, &self.cache, id, format)
    }

    /// The bind-group layouts.
    pub fn layouts(&self) -> &Layouts {
        &self.layouts
    }

    /// The pipeline for `kind` drawing into an attachment of `format`, building it if needed.
    ///
    /// Returns `None` for a pipeline the device cannot support, which is the per-channel coverage
    /// pipeline on a device without dual-source blending. That is a fallback rather than a
    /// failure: the display list is built knowing which coverage it can use.
    pub fn get(
        &mut self,
        gpu: &Gpu,
        kind: PipelineKind,
        format: wgpu::TextureFormat,
    ) -> Option<&wgpu::RenderPipeline> {
        if kind.needs_dual_source_blending() && !self.dual_source_blending {
            return None;
        }
        if !kind.suits(format) {
            return None;
        }
        let key = (kind, format);
        if !self.built.contains_key(&key) {
            let source = kind.module();
            let module = self.modules.entry(source).or_insert_with(|| {
                gpu.device()
                    .create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some(source.label()),
                        // Already parsed and lowered, at build time. What a launch does here is
                        // hand a driver a representation, not compile a language.
                        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(
                            source.representation(),
                        )),
                    })
            });
            let pipeline = build(gpu, &self.layouts, &self.cache, kind, format, module);
            self.built.insert(key, pipeline);
        }
        self.built.get(&key)
    }

    /// Writes the compilation cache back, if there is one to write.
    pub fn persist(&self) {
        self.cache.store();
    }
}

/// Builds one pipeline.
fn build(
    gpu: &Gpu,
    layouts: &Layouts,
    cache: &PipelineCache,
    kind: PipelineKind,
    format: wgpu::TextureFormat,
    module: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let mut bind_group_layouts: Vec<Option<&wgpu::BindGroupLayout>> = Vec::new();
    if kind.uses_tables() {
        bind_group_layouts.push(Some(&layouts.frame));
    }
    if kind.is_instanced() {
        bind_group_layouts.push(Some(&layouts.instances));
        if kind.samples_atlas() {
            bind_group_layouts.push(Some(&layouts.sampled));
        }
    } else if kind.composites_vector() {
        bind_group_layouts.push(Some(&layouts.vector));
    } else if kind.samples_through_block() {
        bind_group_layouts.push(Some(&layouts.filtered));
    } else if matches!(kind, PipelineKind::Blit | PipelineKind::BlitUndoSrgb) {
        bind_group_layouts.push(Some(&layouts.loaded));
    }
    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(kind.label()),
            bind_group_layouts: &bind_group_layouts,
            immediate_size: 0,
        });
    gpu.device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(kind.label()),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(kind.vertex_entry()),
                compilation_options: Default::default(),
                // Every primitive is four corners of a unit square expanded in the vertex stage,
                // so there is no vertex data at all: an instance is read straight out of storage.
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(kind.fragment_entry()),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: kind.blend(),
                    write_mask: kind.write_mask(),
                })],
            }),
            multiview_mask: None,
            cache: cache.handle(),
        })
}
