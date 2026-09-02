//! The seam a style sheet's shader name is resolved through.
//!
//! A style sheet names an effect as a word — `--zgui-shader: cursor-trail` — and the paint stage
//! has to turn that word into the handle a display list carries. What an effect *is* belongs to
//! the renderer's own companion crate, well above this one, so what crosses here is a lookup and
//! nothing else: a name in, a handle and the two facts the paint stage decides with out.

use zgui_scene::{ShaderField, ShaderId, ShaderMode, ShaderReads};

/// Where a style sheet's shader names are resolved.
pub trait ShaderSource {
    /// The effect `name` names, or `None` when nothing was registered under it.
    ///
    /// A name nothing answers draws nothing rather than drawing wrongly: a style sheet naming an
    /// effect the application never declared is a misspelling, and the box keeps the appearance it
    /// would have had.
    ///
    /// This is also where a device that cannot draw an effect at all says so — an implementation
    /// answers `None` throughout when
    /// [`RenderCapabilities::custom_shaders`](zgui_render::RenderCapabilities::custom_shaders) is
    /// false, and every box falls back to the painting it would have had. Deciding it here rather
    /// than in the renderer is what keeps the fallback a *picture* rather than an omission: a
    /// smoothed corner becomes a rounded one, not a hole.
    fn effect(&self, name: &str) -> Option<ShaderBinding>;

    /// The value of the custom property `property` on the element being painted.
    ///
    /// An effect's parameters come from the cascade — `--squircle-n: 4` beside
    /// `--zgui-corner-shape: squircle` — and reading them is the caller's, because only it knows
    /// which style is being lowered.
    fn parameter(&self, _name: &str) -> Option<f32> {
        None
    }
}

/// What the paint stage learns about one registered effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShaderBinding {
    /// The handle a display list carries.
    pub id: ShaderId,
    /// What the effect does, which decides whether its rectangle replaces a background or covers
    /// it.
    pub mode: ShaderMode,
    /// What the effect reads that changes on its own, which decides what has to invalidate it.
    pub reads: ShaderReads,
    /// The effect's own parameters, so a style sheet's `--<effect>-<field>` can be put where the
    /// effect declares that field is.
    pub fields: &'static [ShaderField],
    /// How far outside its own box a filter effect reads, in CSS pixels.
    pub reach: f32,
}

impl ShaderBinding {
    /// Where the effect declares `name` is, or `None` for a field it does not declare.
    pub fn field(&self, name: &str) -> Option<ShaderField> {
        self.fields.iter().copied().find(|held| held.name == name)
    }
}

/// A source with no effects registered.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoShaders;

impl ShaderSource for NoShaders {
    fn effect(&self, _name: &str) -> Option<ShaderBinding> {
        None
    }
}

/// The effects the process declared, if the device can draw one.
///
/// The capability is held here rather than asked per box, because it decides what the display list
/// should *contain* rather than how it is drawn: a device that cannot draw an effect makes every
/// box fall back to the painting it would have had, which is a picture rather than a hole.
#[derive(Clone, Copy, Debug)]
pub struct DeclaredShaders {
    /// Whether the device can draw an application's own shader.
    enabled: bool,
}

impl DeclaredShaders {
    /// The declared effects, drawn only if `enabled`.
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

impl ShaderSource for DeclaredShaders {
    fn effect(&self, name: &str) -> Option<ShaderBinding> {
        if !self.enabled {
            return None;
        }
        let declared = zgui_scene::shader_named(name)?;
        Some(ShaderBinding {
            id: declared.id,
            mode: declared.mode,
            reads: declared.reads,
            fields: declared.fields,
            reach: declared.reach,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DeclaredShaders, NoShaders, ShaderSource};
    use zgui_scene::{ShaderMode, ShaderReads};

    #[test]
    fn a_declared_effect_is_found_by_the_name_a_style_sheet_writes() {
        zgui_scene::declare_shader(
            "paint-source-test",
            ShaderMode::Coverage,
            ShaderReads::NOTHING,
            &[],
            0.0,
        );
        let found = DeclaredShaders::new(true)
            .effect("paint-source-test")
            .expect("the declaration is found");
        assert_eq!(found.mode, ShaderMode::Coverage);
    }

    #[test]
    fn a_device_that_cannot_draw_an_effect_answers_nothing_at_all() {
        zgui_scene::declare_shader(
            "paint-source-gated",
            ShaderMode::Paint,
            ShaderReads::NOTHING,
            &[],
            0.0,
        );
        assert!(
            DeclaredShaders::new(false)
                .effect("paint-source-gated")
                .is_none(),
            "every box falls back to the painting it would have had"
        );
    }

    #[test]
    fn a_name_nothing_declared_is_answered_by_nothing() {
        assert!(
            DeclaredShaders::new(true)
                .effect("never-declared")
                .is_none()
        );
        assert!(NoShaders.effect("never-declared").is_none());
    }
}
