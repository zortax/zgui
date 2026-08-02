//! Identifiers, and the table that turns a copyable handle back into a borrowed atom.
//!
//! Two requirements meet here and only one shape satisfies both. The record stores an element's
//! `id` in a plain cell, so the stored form has to be [`Copy`]; the style engine asks for an
//! element's `id` as a *borrowed* atom, so something with a longer life than the call has to own
//! one. The stored form is therefore a handle — [`Ident`], eight bytes, copyable — and the atom it
//! resolves to lives in a table the document owns for as long as the document does.
//!
//! A handle that is a newtype *over* the atom satisfies neither half: the engine's atom is not
//! [`Copy`], because a dynamically allocated one decrements a reference count when it is dropped.

use rustc_hash::FxHashMap;
use stylo_atoms::Atom;
use zgui_interned::Ident;

/// The identifiers one document has used, each resolvable to a borrowed atom.
///
/// Entries are never removed. An identifier that has been used once is cheap to keep and may well
/// be used again, and removal would break the one property the table exists to provide: that a
/// resolved reference stays valid for as long as the document does.
#[derive(Default)]
pub struct IdentTable {
    /// Handle to the atom the style engine matches against.
    atoms: FxHashMap<Ident, Atom>,
}

impl IdentTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many distinct identifiers the table holds.
    pub fn len(&self) -> usize {
        self.atoms.len()
    }

    /// Whether the table holds no identifiers.
    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }

    /// Records `ident` if it is not recorded already, and returns it unchanged.
    ///
    /// Interning on the write path rather than the read path is what keeps resolution a lookup:
    /// nothing that runs while selectors are being matched ever has to build an atom.
    pub fn intern(&mut self, ident: Ident) -> Ident {
        self.atoms
            .entry(ident)
            .or_insert_with(|| Atom::from(ident.as_str()));
        ident
    }

    /// The atom `ident` names, if this table has seen it.
    ///
    /// The reference borrows the table, not the handle, which is exactly the lifetime the style
    /// engine's element trait asks for.
    pub fn resolve(&self, ident: Ident) -> Option<&Atom> {
        self.atoms.get(&ident)
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::Ident;

    use super::IdentTable;

    #[test]
    fn a_handle_resolves_to_an_atom_with_the_tables_lifetime() {
        let mut table = IdentTable::new();
        let ident = table.intern(Ident::new("submit"));
        assert_eq!(
            table.resolve(ident).map(|atom| atom.to_string()).as_deref(),
            Some("submit")
        );
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn interning_the_same_identifier_twice_adds_one_entry() {
        let mut table = IdentTable::new();
        table.intern(Ident::new("submit"));
        table.intern(Ident::new("submit"));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn an_identifier_the_table_has_not_seen_resolves_to_nothing() {
        let table = IdentTable::new();
        assert!(table.is_empty());
        assert!(table.resolve(Ident::new("never-written")).is_none());
    }

    #[test]
    fn the_stored_form_is_copyable_and_optional_in_one_word() {
        assert_eq!(size_of::<Option<Ident>>(), size_of::<usize>());
    }
}
