//! Every registered application effect, and the pipelines built from them.

use std::collections::HashMap;

use zgui_scene::ShaderId;
use zgui_wgsl::ShaderMode;

use crate::effect::EffectProgram;
use crate::gpu::device::Gpu;
use crate::gpu::pipeline_cache::PipelineCache;
use crate::pipeline::layout::Layouts;
use crate::shader::reflect::{Member, Reflected};

/// One effect that has been accepted onto a device.
#[derive(Debug)]
struct Registered {
    /// What it does, which decides the entry points its pipelines are built from.
    mode: ShaderMode,
    /// The label a driver error names it by.
    label: &'static str,
    /// The compiled module.
    module: wgpu::ShaderModule,
}

/// The effects the renderer has been told about.
///
/// Keyed by the handle the display list carries, so resolving one is a lookup and a display list
/// naming an effect the renderer never heard of draws nothing rather than drawing wrongly.
#[derive(Debug, Default)]
pub struct Effects {
    /// Each accepted effect, by the handle it was registered under.
    registered: HashMap<ShaderId, Registered>,
    /// What has already been reported about an effect that could not be drawn.
    ///
    /// A draw that cannot be issued is dropped and counted, which is the right thing to do and the
    /// wrong thing to be quiet about: an effect that stops appearing looks like an effect that
    /// decided to draw nothing. Reported once per effect and reason, because the alternative is a
    /// line every frame for as long as it lasts.
    reported: std::cell::RefCell<std::collections::HashSet<(ShaderId, &'static str)>>,
    /// One pipeline per effect and attachment format, built on demand.
    ///
    /// Built lazily for the same reason the framework's own are: a document that isolates nothing
    /// never allocates a target in the second format, and therefore never pays to build a second
    /// pipeline for an effect it draws.
    built: HashMap<(ShaderId, wgpu::TextureFormat), wgpu::RenderPipeline>,
}

impl Effects {
    /// No effects at all.
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts `program` under `id`, replacing anything registered under it.
    ///
    /// The parameters the shader declares are compared against the ones Rust declares before
    /// anything is compiled, so a mismatch is an error naming both sides rather than a rectangle
    /// full of the wrong numbers.
    pub fn register(
        &mut self,
        gpu: &Gpu,
        id: ShaderId,
        program: EffectProgram,
    ) -> Result<(), String> {
        if !id.is_some() {
            return Err("the absent shader handle names no effect".to_owned());
        }
        check_params(&program)?;
        let module = compile(gpu, &program)?;
        // Anything built for the previous program under this handle draws the previous program.
        self.built.retain(|(held, _), _| *held != id);
        self.registered.insert(
            id,
            Registered {
                mode: program.mode,
                label: program.label,
                module,
            },
        );
        Ok(())
    }

    /// Forgets the effect registered under `id`, and every pipeline built from it.
    pub fn release(&mut self, id: ShaderId) {
        self.registered.remove(&id);
        self.built.retain(|(held, _), _| *held != id);
    }

    /// Whether anything is registered under `id`.
    pub fn contains(&self, id: ShaderId) -> bool {
        self.registered.contains_key(&id)
    }

    /// How many effects are registered.
    pub fn len(&self) -> usize {
        self.registered.len()
    }

    /// Whether nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.registered.is_empty()
    }

    /// Forgets every effect, which is what a lost device leaves behind.
    pub fn clear(&mut self) {
        self.registered.clear();
        self.built.clear();
        self.reported.borrow_mut().clear();
    }

    /// How many distinct reports have been made, for a test that asserts they happen once.
    pub fn reported_count(&self) -> usize {
        self.reported.borrow().len()
    }

    /// Says once that an effect could not be drawn, and why.
    ///
    /// The reasons are all recoverable — a frame draws the rest of itself and the next one may
    /// well succeed — so this is a warning rather than a failure. What it buys is that an effect
    /// which stops appearing says so, instead of looking like one that drew nothing on purpose.
    pub fn note_undrawable(&self, id: ShaderId, why: &'static str) {
        if self.reported.borrow_mut().insert((id, why)) {
            let known = self.registered.contains_key(&id);
            tracing::warn!(
                effect = id.index(),
                registered = known,
                "an application effect could not be drawn: {why}"
            );
        }
    }

    /// The pipeline drawing `id` into an attachment of `format`, building it if needed.
    ///
    /// `None` for a handle nothing is registered under, which is a display list built against an
    /// effect that has since been released.
    pub fn pipeline(
        &mut self,
        gpu: &Gpu,
        layouts: &Layouts,
        cache: &PipelineCache,
        id: ShaderId,
        format: wgpu::TextureFormat,
    ) -> Option<&wgpu::RenderPipeline> {
        let effect = self.registered.get(&id)?;
        let key = (id, format);
        if !self.built.contains_key(&key) {
            let pipeline = build(gpu, layouts, cache, effect, format);
            self.built.insert(key, pipeline);
        }
        self.built.get(&key)
    }
}

/// Compiles one effect, preferring the representation its own build lowered.
fn compile(gpu: &Gpu, program: &EffectProgram) -> Result<wgpu::ShaderModule, String> {
    let descriptor = match decoded(program) {
        Some(module) => wgpu::ShaderModuleDescriptor {
            label: Some(program.label),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
        },
        None => {
            if program.source.is_empty() {
                return Err(format!(
                    "{}: the representation would not decode and no text was carried beside it",
                    program.label
                ));
            }
            tracing::debug!(
                effect = program.label,
                "the effect's representation did not decode; compiling its text instead"
            );
            wgpu::ShaderModuleDescriptor {
                label: Some(program.label),
                source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(program.source)),
            }
        }
    };
    Ok(gpu.device().create_shader_module(descriptor))
}

/// The effect's representation, or `None` when it was written by a different shader front end.
fn decoded(program: &EffectProgram) -> Option<wgpu::naga::Module> {
    if program.representation.is_empty() {
        return None;
    }
    bincode::deserialize(program.representation).ok()
}

/// Compares the parameters the shader declares against the ones Rust declares.
pub fn check_params(program: &EffectProgram) -> Result<(), String> {
    if program.params.size > zgui_scene::MAX_PARAMS_BYTES {
        return Err(format!(
            "{}: parameters are {} bytes and at most {} fit in the block",
            program.label,
            program.params.size,
            zgui_scene::MAX_PARAMS_BYTES
        ));
    }
    if program.source.is_empty() {
        // Nothing to compare against. An effect carrying no text is one a test assembled by hand.
        return Ok(());
    }
    if program.params.size == 0 && program.params.fields.is_empty() {
        // An effect that declares no parameters is drawn with the type the prelude supplies, and
        // there is nothing on the Rust side to compare it against.
        return Ok(());
    }
    let reflected = Reflected {
        name: "Params",
        size: program.params.size,
        members: program
            .params
            .fields
            .iter()
            .map(|field| Member {
                name: field.name,
                offset: field.offset,
                size: field.size,
            })
            .collect(),
    };
    crate::shader::reflect::check(program.source, std::slice::from_ref(&reflected))
        .map_err(|error| format!("{}: {error}", program.label))
}

/// Builds one effect's pipeline.
fn build(
    gpu: &Gpu,
    layouts: &Layouts,
    cache: &PipelineCache,
    effect: &Registered,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    // A rectangle binds what every instanced pipeline binds — the frame's tables and the lane's
    // instances — which is what makes an effect's clip the same clip a background's is. A filter
    // binds what the blur chain binds instead: it is cut to its region by the scissor rather than
    // clipped per fragment, and the content it reads sits where those tables would.
    let groups: &[Option<&wgpu::BindGroupLayout>] = if effect.mode.is_primitive() {
        &[
            Some(&layouts.frame),
            Some(&layouts.instances),
            Some(&layouts.effect),
        ]
    } else {
        &[Some(&layouts.filtered), Some(&layouts.effect)]
    };
    let pipeline_layout = gpu
        .device()
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(effect.label),
            bind_group_layouts: groups,
            immediate_size: 0,
        });
    gpu.device()
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(effect.label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &effect.module,
                entry_point: Some(zgui_wgsl::vertex_entry(effect.mode)),
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
                module: &effect.module,
                entry_point: Some(zgui_wgsl::fragment_entry(effect.mode)),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // A rectangle composites onto what is beneath it, premultiplied, which is why
                    // an effect returns a premultiplied colour. A filtering pass replaces what was
                    // there: blending it would make the result depend on whatever the target it
                    // was lent happened to hold.
                    blend: effect
                        .mode
                        .is_primitive()
                        .then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: cache.handle(),
        })
}

#[cfg(test)]
mod tests {
    use super::{EffectProgram, check_params};
    use crate::effect::{ParamsField, ParamsLayout};
    use zgui_wgsl::ShaderMode;

    /// A program declaring `fields` in Rust and `declaration` in its text.
    fn program(
        size: usize,
        fields: &'static [ParamsField],
        declaration: &'static str,
    ) -> EffectProgram {
        EffectProgram {
            mode: ShaderMode::Paint,
            label: "test",
            representation: &[],
            source: declaration,
            params: ParamsLayout { size, fields },
        }
    }

    const ONE: [ParamsField; 1] = [ParamsField {
        name: "amount",
        offset: 0,
        size: 4,
    }];

    #[test]
    fn parameters_that_agree_are_accepted() {
        let source = "struct Params {\n    amount: f32,\n}\n";
        check_params(&program(4, &ONE, source)).expect("the two sides agree");
    }

    #[test]
    fn a_field_renamed_on_one_side_alone_is_caught() {
        let source = "struct Params {\n    strength: f32,\n}\n";
        let error = check_params(&program(4, &ONE, source))
            .expect_err("a renamed field is a disagreement");
        assert!(error.contains("amount"), "{error}");
    }

    #[test]
    fn a_field_added_on_one_side_alone_is_caught() {
        let source = "struct Params {\n    amount: f32,\n    extra: f32,\n}\n";
        let error =
            check_params(&program(4, &ONE, source)).expect_err("an extra field is a disagreement");
        assert!(error.contains("Params"), "{error}");
    }

    #[test]
    fn parameters_wider_than_the_block_are_refused_before_anything_is_compiled() {
        let source = "struct Params {\n    amount: f32,\n}\n";
        let error = check_params(&program(zgui_scene::MAX_PARAMS_BYTES + 4, &ONE, source))
            .expect_err("a block that does not fit is refused");
        assert!(error.contains("at most"), "{error}");
    }

    #[test]
    fn an_effect_carrying_no_text_compares_nothing_and_says_so_by_succeeding() {
        check_params(&program(4, &ONE, "")).expect("there is nothing to compare against");
    }

    #[test]
    fn an_effect_declaring_no_parameters_is_accepted_against_the_supplied_type() {
        let supplied = "struct Params {\n    unused: vec4<f32>,\n}\n";
        check_params(&program(0, &[], supplied))
            .expect("an effect with no parameters compares nothing");
    }
}
