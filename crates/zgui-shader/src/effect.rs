//! One declared effect: the program its own build produced, and the handle items draw it by.

use std::sync::{Arc, OnceLock};

use iceblink::{Prebuilt, ShaderDesc, ShaderSource};
use zgui_paint::content::shader::{Declaration, ShaderId, ShaderMode, ShaderReads};

use crate::handle::{ShaderHandle, ShaderParams};
use crate::program::EffectProgram;

/// An application shader, as `shader!` left it.
///
/// Written as a `static`, because everything in it is decided while the application is built. It
/// is declared the first time it is registered and never again, so every window draws the same
/// shader under the same handle.
#[derive(Debug)]
pub struct ShaderEffect<P: 'static> {
    /// What the application's build produced.
    program: EffectProgram,
    /// What the effect reads that changes on its own.
    reads: ShaderReads,
    /// What the effect is called.
    name: &'static str,
    /// How far outside its own box a filter effect reads, in CSS pixels.
    reach: f32,
    /// The handle it was declared under, taken the first time it is registered.
    id: OnceLock<ShaderId>,
    /// The parameter structure it is written with.
    marker: core::marker::PhantomData<fn(P)>,
}

impl<P: ShaderParams> ShaderEffect<P> {
    /// The effect `shader!` describes.
    ///
    /// Called by the macro. Writing one by hand means writing the compiled representation by hand.
    pub const fn declared(
        program: EffectProgram,
        reads: ShaderReads,
        name: &'static str,
        reach: f32,
    ) -> Self {
        Self {
            program,
            reads,
            name,
            reach,
            id: OnceLock::new(),
            marker: core::marker::PhantomData,
        }
    }

    /// Registers the effect and hands back a handle on it.
    ///
    /// Registering twice returns two handles on one effect: the declaration is taken once, and the
    /// parameters belong to the handle rather than to the declaration, so two parts of an
    /// application can draw the same shader with different parameters.
    ///
    /// An effect whose parameters disagree between its shader and Rust, as
    /// [`validate`](ShaderEffect::validate) reports, is declared without a program and draws
    /// nothing.
    pub fn register(&'static self) -> ShaderHandle<P> {
        let id = *self.id.get_or_init(|| {
            let checked = self.validate();
            if let Err(reason) = &checked {
                tracing::error!(effect = self.name, "{reason}, so the effect draws nothing");
            }
            zgui_paint::content::shader::declare(Declaration {
                name: self.name,
                mode: self.program.mode,
                desc: checked.ok().map(|()| self.desc()),
                reads: self.reads,
                fields: P::LAYOUT.fields,
                reach: self.reach,
            })
        });
        ShaderHandle::new(id, self.reads)
    }

    /// Whether the parameters this effect declares agree between its shader and Rust.
    ///
    /// A disagreement makes the effect draw nothing, so an application that would rather find out
    /// early than look at an empty rectangle can ask here.
    ///
    /// # Errors
    ///
    /// Returns the first difference between the two layouts.
    pub fn validate(&self) -> Result<(), String> {
        self.program.check_params()
    }

    /// What the effect is called, which is the name a driver error and a style sheet use.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// What the effect reads that changes on its own.
    pub fn reads(&self) -> ShaderReads {
        self.reads
    }

    /// How far outside its own box this effect reads, in CSS pixels.
    ///
    /// Zero for everything but a filter that samples a neighbourhood. It is what the damage a
    /// filtered box owes is grown by: reading further than this feeds the filter its own previous
    /// output wherever a partial redraw stopped short.
    pub fn reach(&self) -> f32 {
        self.reach
    }

    /// Returns the program the compositor creates the shader from.
    fn desc(&self) -> ShaderDesc {
        let program = &self.program;
        ShaderDesc {
            mode: match program.mode {
                ShaderMode::Paint => iceblink::ShaderMode::PAINT,
                ShaderMode::Coverage => iceblink::ShaderMode::COVERAGE,
                ShaderMode::Filter => iceblink::ShaderMode::FILTER,
            },
            source: ShaderSource {
                wgsl: Arc::from(program.source),
                prebuilt: Some(Prebuilt {
                    format: Arc::from(program.prebuilt_format),
                    bytes: Arc::from(program.prebuilt),
                }),
            },
            params_size: P::LAYOUT.size as u32,
            reads: if self.reads.time {
                iceblink::ShaderReads::TIME
            } else {
                iceblink::ShaderReads::default()
            },
        }
    }
}
