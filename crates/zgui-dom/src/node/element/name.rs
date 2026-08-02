//! An element's tag name, in the one representation the style engine can borrow.

use crate::plain_data;

/// An element's local name.
///
/// A newtype over the style engine's own interned-string type, held by value in the record so that
/// the engine can be handed a reference *into* the record rather than a freshly constructed atom
/// on every selector test. Nothing above this crate ever names the type inside: callers speak
/// [`zgui_interned::ElementName`], and the conversion happens here, once, when the node is created.
#[derive(Clone, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct ElementName(pub(crate) web_atoms::LocalName);

impl ElementName {
    /// The local name for `name`.
    pub fn new(name: zgui_interned::ElementName) -> Self {
        Self(web_atoms::LocalName::from(name.as_str()))
    }

    /// The name as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The name in the form the interned-name vocabulary uses.
    pub fn to_interned(&self) -> zgui_interned::ElementName {
        zgui_interned::ElementName::new(self.as_str())
    }
}

// SAFETY: shape 1 — the field is written when the record is created and never again, and the
// interned-string type behind it is itself immutable and shareable. Declared through the macro
// rather than by hand only because it is not `Copy`.
unsafe impl crate::node::discipline::CellDisciplined for ElementName {}

plain_data!(zgui_interned::NamespaceId);

#[cfg(test)]
mod tests {
    use super::ElementName;

    #[test]
    fn a_name_round_trips_through_the_interned_vocabulary() {
        let name = ElementName::new(zgui_interned::ElementName::new("button"));
        assert_eq!(name.as_str(), "button");
        assert_eq!(
            name.to_interned(),
            zgui_interned::ElementName::new("button")
        );
        assert_ne!(name, ElementName::new(zgui_interned::ElementName::new("a")));
    }
}
