//! One declared effect: the program its own build produced, and the handle a device draws it by.

use std::sync::OnceLock;

use zgui_render_wgpu::EffectProgram;
use zgui_scene::ShaderId;
use zgui_wgsl::ShaderReads;

use crate::handle::{ShaderHandle, ShaderParams};

/// An application shader, as `shader!` left it.
///
/// Written as a `static`, because everything in it is decided while the application is built. It
/// is registered the first time it is used and never again: the handle it takes is the process's,
/// and every device catches up with it, so a second window and a device lost and rebuilt both end
/// up drawing the same effect.
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
    pub fn register(&'static self) -> ShaderHandle<P> {
        let id = *self.id.get_or_init(|| {
            // The handle is minted where the declaration is, so one handle names both halves of
            // the effect: the vocabulary the paint stage decides with, and the program the device
            // compiles. A style sheet reaches the first by name; a display list reaches the second
            // by this.
            let id = zgui_scene::declare_shader(
                self.name,
                self.program.mode,
                self.reads,
                P::LAYOUT.fields,
                self.reach,
            );
            zgui_render_wgpu::declare(id, self.program);
            id
        });
        ShaderHandle::new(id, self.reads)
    }

    /// Whether the parameters this effect declares agree between its shader and Rust.
    ///
    /// The same comparison a device makes when it registers the effect, made without one. A
    /// disagreement makes the effect draw nothing, so an application that would rather find out
    /// early than look at an empty rectangle can ask here.
    pub fn validate(&self) -> Result<(), String> {
        zgui_render_wgpu::check_effect(&self.program)
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
    /// filtered box owes is grown by, and nothing but the effect can state it: reading further
    /// than this feeds the filter its own previous output wherever a partial redraw stopped short.
    pub fn reach(&self) -> f32 {
        self.reach
    }
}
