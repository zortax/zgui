//! The application shaders the process declared, with the programs the compositor builds them
//! from.

use std::sync::{OnceLock, RwLock};

use compositor_types::ShaderDesc;
use zgui_scene::{ShaderField, ShaderId, ShaderMode, ShaderReads};

/// One application shader as an application declares it.
#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    /// The name a style sheet writes.
    pub name: &'static str,
    /// What the shader does.
    pub mode: ShaderMode,
    /// The program, in the form the compositor creates shaders from, or `None` for a shader that
    /// draws nothing.
    pub desc: Option<ShaderDesc>,
    /// What the shader reads that changes on its own.
    pub reads: ShaderReads,
    /// The shader's own parameters, so a style sheet's `--<shader>-<field>` lands where the
    /// field is.
    pub fields: &'static [ShaderField],
    /// How far outside its own box a filter shader reads, in CSS pixels.
    pub reach: f32,
}

/// Declares `declaration` and returns the handle items name the shader by.
///
/// Every window's compositor client creates the shader from its program the first time an item
/// draws with it. Handles are never reused.
pub fn declare(declaration: Declaration) -> ShaderId {
    let id = zgui_scene::declare_shader(
        declaration.name,
        declaration.mode,
        declaration.reads,
        declaration.fields,
        declaration.reach,
    );
    if let Some(desc) = declaration.desc
        && let Ok(mut held) = programs().write()
    {
        held.push((id, desc));
    }
    id
}

/// Returns the program of the shader `id`, or `None` for a handle nothing was declared under.
pub(crate) fn program(id: ShaderId) -> Option<ShaderDesc> {
    let held = programs().read().ok()?;
    held.iter()
        .find(|(declared, _)| *declared == id)
        .map(|(_, desc)| desc.clone())
}

/// The programs of every declared shader, by handle.
fn programs() -> &'static RwLock<Vec<(ShaderId, ShaderDesc)>> {
    static PROGRAMS: OnceLock<RwLock<Vec<(ShaderId, ShaderDesc)>>> = OnceLock::new();
    PROGRAMS.get_or_init(|| RwLock::new(Vec::new()))
}
