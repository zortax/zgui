//! Every effect the process has declared, by the name a style sheet writes.
//!
//! # Why the declaration is split
//!
//! An effect is two things. It is a *program*, which is one graphics API's language and belongs
//! below the render seam beside the device that compiles it. And it is a *declaration* — a name, a
//! mode, what it reads, where its parameters are — which is what the paint stage decides with:
//! whether a rectangle replaces a background or covers it, whether the element carrying it has to
//! be redrawn every refresh, and which byte a style sheet's `--effect-field` lands in.
//!
//! Only the second half is here. Both halves are given out under one [`ShaderId`], so a display
//! list built from the declaration is drawn by the program.
//!
//! # Why a list and not a call
//!
//! An application declares its effects while it is starting, which is before any document exists.
//! The declarations outlive every document and every device, and each catches up with them rather
//! than being told about them. The list only grows: an effect is a `static` in the application's
//! binary, so there is nothing to free.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{OnceLock, RwLock};

use crate::shader::{ShaderId, ShaderMode, ShaderReads};

/// The custom properties a style sheet names an effect through.
///
/// There is no `@property` registration in this build, so an effect is named through a custom
/// property. The names live here rather than beside either reader because both the paint stage and
/// the layout stage need them: one to draw the effect, the other to know how far it reads, and a
/// second copy of either name is a box that is drawn filtered and damaged as though it were not.
pub mod property {
    /// The effect that fills a box.
    pub const SHADER: &str = "zgui-shader";
    /// The effect that shapes a box.
    ///
    /// Distinct from `--zgui-corner-shape`, which is the engine's own corner shape and takes a
    /// keyword or an exponent rather than the name of an effect. A smoothed corner wants the
    /// engine's: it reaches the shadow, the outline and the clip a box gives its children, none of
    /// which an effect over the background can. This is for a shape the engine has no name for.
    pub const SHAPE: &str = "zgui-shape";
    /// The effect that filters a box's own content.
    pub const FILTER: &str = "zgui-filter";
    /// The effect that filters whatever is drawn beneath a box.
    pub const BACKDROP_FILTER: &str = "zgui-backdrop-filter";
}

/// One field of an effect's parameters, as the effect declares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderField {
    /// The field's name, which is what a style sheet writes after the effect's own.
    pub name: &'static str,
    /// Its byte offset in the block.
    pub offset: usize,
    /// How many bytes it occupies.
    pub size: usize,
}

/// What one declared effect is, apart from its program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShaderDeclaration {
    /// The handle a display list carries.
    pub id: ShaderId,
    /// What a style sheet names it by.
    pub name: &'static str,
    /// What it does, which decides where its rectangle goes and what fills it.
    pub mode: ShaderMode,
    /// What it reads that changes on its own.
    pub reads: ShaderReads,
    /// Where each of its parameters is.
    pub fields: &'static [ShaderField],
    /// How far outside its own box a filter effect reads, in CSS pixels.
    ///
    /// Zero for everything else, and for a filter that reads only the pixel it writes. Nothing
    /// here can look inside a shader, so this is the effect's own statement — and it is what the
    /// damage a filtered box owes is grown by.
    pub reach: f32,
}

impl ShaderDeclaration {
    /// Where this effect declares `name` is, or `None` for a field it does not declare.
    pub fn field(&self, name: &str) -> Option<ShaderField> {
        self.fields.iter().copied().find(|held| held.name == name)
    }
}

/// The process's declarations.
fn declarations() -> &'static RwLock<Vec<ShaderDeclaration>> {
    static DECLARED: OnceLock<RwLock<Vec<ShaderDeclaration>>> = OnceLock::new();
    DECLARED.get_or_init(|| RwLock::new(Vec::new()))
}

/// The next handle, never reused, so a stale display list never resolves to a stranger's effect.
static NEXT: AtomicU32 = AtomicU32::new(1);

/// Declares an effect, and returns the handle a display list names it by.
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

/// The effect a style sheet's `name` names, or `None` when nothing was declared under it.
///
/// The most recent declaration wins where two share a name, so an application that declares an
/// effect twice gets the one it declared last rather than an arbitrary one.
pub fn named(name: &str) -> Option<ShaderDeclaration> {
    let held = declarations().read().ok()?;
    held.iter().rev().find(|held| held.name == name).copied()
}

/// The effect `id` names, or `None` for a handle nothing was declared under.
pub fn by_id(id: ShaderId) -> Option<ShaderDeclaration> {
    let held = declarations().read().ok()?;
    held.iter().find(|held| held.id == id).copied()
}

/// How many effects the process has declared.
pub fn count() -> usize {
    declarations().read().map_or(0, |held| held.len())
}

#[cfg(test)]
mod tests {
    use super::{ShaderField, by_id, count, declare, named};
    use crate::shader::{ShaderMode, ShaderReads};

    const FIELDS: [ShaderField; 1] = [ShaderField {
        name: "amount",
        offset: 0,
        size: 4,
    }];

    #[test]
    fn a_declaration_is_found_by_the_name_a_style_sheet_writes() {
        let before = count();
        let id = declare(
            "test-found",
            ShaderMode::Coverage,
            ShaderReads::NOTHING,
            &FIELDS,
            0.0,
        );
        assert_eq!(count(), before + 1);
        let found = named("test-found").expect("the declaration is found");
        assert_eq!(found.id, id);
        assert_eq!(found.mode, ShaderMode::Coverage);
        assert_eq!(found.field("amount").map(|field| field.offset), Some(0));
        assert_eq!(found.field("missing"), None);
    }

    /// A reach below zero would shrink the region a filter may read, which is a filter reading
    /// texels the pass never wrote.
    #[test]
    fn a_reach_below_zero_is_kept_as_none() {
        declare(
            "test-negative-reach",
            ShaderMode::Filter,
            ShaderReads::NOTHING,
            &[],
            -3.0,
        );
        assert_eq!(named("test-negative-reach").map(|held| held.reach), Some(0.0));
    }

    #[test]
    fn a_name_nothing_declared_is_found_by_nothing() {
        assert!(named("test-never-declared").is_none());
    }

    #[test]
    fn handles_are_never_reused_and_resolve_back_to_their_declaration() {
        let first = declare("test-one", ShaderMode::Paint, ShaderReads::NOTHING, &[], 0.0);
        let second = declare("test-two", ShaderMode::Paint, ShaderReads::NOTHING, &[], 0.0);
        assert_ne!(first, second);
        assert_eq!(by_id(first).map(|held| held.name), Some("test-one"));
    }

    #[test]
    fn the_last_declaration_of_a_name_is_the_one_it_resolves_to() {
        declare("test-twice", ShaderMode::Paint, ShaderReads::NOTHING, &[], 0.0);
        let second = declare(
            "test-twice",
            ShaderMode::Coverage,
            ShaderReads::NOTHING,
            &[],
            0.0,
        );
        assert_eq!(named("test-twice").map(|held| held.id), Some(second));
    }
}
