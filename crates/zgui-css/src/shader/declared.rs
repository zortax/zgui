//! The process's effect declarations.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{OnceLock, RwLock};

use crate::shader::{ShaderId, ShaderMode, ShaderReads};

/// The custom properties a style sheet names an effect in.
pub mod property {
    /// The effect that fills a box.
    pub const SHADER: &str = "zgui-shader";
    /// The effect that shapes a box.
    ///
    /// The engine's own corner shape is `--zgui-corner-shape`, which takes a keyword or an
    /// exponent.
    pub const SHAPE: &str = "zgui-shape";
    /// The effect that filters a box's own content.
    pub const FILTER: &str = "zgui-filter";
    /// The effect that filters the content drawn beneath a box.
    pub const BACKDROP_FILTER: &str = "zgui-backdrop-filter";
}

/// One field of an effect's parameter block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderField {
    /// The name a style sheet writes after the effect's own.
    pub name: &'static str,
    /// The byte offset in the block.
    pub offset: usize,
    /// The size in bytes.
    pub size: usize,
}

/// One declared effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShaderDeclaration {
    /// The handle the effect is drawn by.
    pub id: ShaderId,
    /// The name a style sheet writes.
    pub name: &'static str,
    /// What the effect does.
    pub mode: ShaderMode,
    /// What the effect reads that changes on its own.
    pub reads: ShaderReads,
    /// The fields of the parameter block.
    pub fields: &'static [ShaderField],
    /// How far outside its box a filter effect reads, in CSS pixels, never below zero.
    pub reach: f32,
}

impl ShaderDeclaration {
    /// Returns the field `name`, or `None` if the effect declares no such field.
    pub fn field(&self, name: &str) -> Option<ShaderField> {
        self.fields.iter().copied().find(|held| held.name == name)
    }
}

/// Returns the process's declarations.
fn declarations() -> &'static RwLock<Vec<ShaderDeclaration>> {
    static DECLARED: OnceLock<RwLock<Vec<ShaderDeclaration>>> = OnceLock::new();
    DECLARED.get_or_init(|| RwLock::new(Vec::new()))
}

// Handles are never reused, so a stale handle never resolves to another effect.
static NEXT: AtomicU32 = AtomicU32::new(1);

/// Declares an effect and returns its handle.
///
/// A negative `reach` is stored as zero.
pub fn declare(
    name: &'static str,
    mode: ShaderMode,
    reads: ShaderReads,
    fields: &'static [ShaderField],
    reach: f32,
) -> ShaderId {
    let id = ShaderId(NEXT.fetch_add(1, Ordering::Relaxed));

    if let Ok(mut held) = declarations().write() {
        held.push(ShaderDeclaration {
            id,
            name,
            mode,
            reads,
            fields,
            reach: reach.max(0.0),
        });
    }
    id
}

/// Returns the latest effect declared under `name`, or `None` if no effect has that name.
pub fn named(name: &str) -> Option<ShaderDeclaration> {
    let held = declarations().read().ok()?;
    held.iter().rev().find(|held| held.name == name).copied()
}
