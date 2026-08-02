//! The bridge between this framework's interned names and the style engine's own atoms.
//!
//! Both sides intern, and both are cheap to clone and compare — but they are separate tables, so a
//! name that exists in one is not automatically the same handle in the other. Crossing between
//! them is therefore a lookup, and it happens here rather than at every call site, in one direction
//! at a time.

/// The style engine's interned string.
pub use stylo_atoms::Atom;
/// The bound set a cheaply cloned name has to satisfy.
///
/// A layout engine handed a custom-identifier type clones those names freely while it resolves a
/// track list, and [`Ident`] is the type it is handed — so it is re-exported beside the name it
/// constrains, and a consumer needs neither the engine nor the interning crate to write the bound.
pub use zgui_interned::CheapCloneStr;
/// An identifier written in a style sheet: an element's id, a named grid line or area, an
/// animation or counter name, a font family name.
///
/// This is the identifier type on both sides of the bridge below, and the one a layout pass is
/// handed for named grid lines and areas.
pub use zgui_interned::Ident;

/// This framework's name for an engine atom.
///
/// ```
/// use zgui_css::{atom_to_ident, ident_to_atom};
/// use zgui_interned::Ident;
///
/// let round_tripped = atom_to_ident(&ident_to_atom(Ident::new("sans-serif")));
/// assert_eq!(round_tripped, Ident::new("sans-serif"));
/// ```
pub fn atom_to_ident(atom: &Atom) -> Ident {
    Ident::new(atom)
}

/// The engine's atom for one of this framework's names.
pub fn ident_to_atom(ident: Ident) -> Atom {
    Atom::from(ident.as_str())
}
