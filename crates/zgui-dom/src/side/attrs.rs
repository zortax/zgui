//! Attributes, which are the largest thing most nodes do not have.
//!
//! An attribute value is a string, and the style engine's attribute interface hands one back by
//! value, so values are kept as far from the node record as the design allows. The measurement
//! that decided it: an attribute map costs twenty-four bytes per node in a dense table whether or
//! not the node has a single attribute, which in a document built from a component library is most
//! of them — the largest single sparse cost in the whole record. Paged, it costs one pointer per
//! thousand nodes instead.

use smallvec::SmallVec;
use zgui_interned::AttrName;
use zgui_vocab::SharedString;

/// One attribute.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Attr {
    /// The attribute's name.
    pub name: AttrName,
    /// The attribute's value.
    pub value: SharedString,
}

/// One node's attributes, other than the two that are not stored here.
///
/// `id` and `class` live in the node record — as a copyable identifier handle and as a span into
/// the document's class pool — because selector matching asks about them far more often than about
/// anything else and neither answer should cost a column lookup.
#[derive(Clone, Default, Debug)]
pub struct AttrMap {
    /// In insertion order. A node with attributes has a handful, so a scan beats a hash.
    entries: SmallVec<[Attr; 4]>,
}

impl AttrMap {
    /// A map with no attributes in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many attributes the map holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map holds no attributes.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The value of `name`, if the node has it.
    pub fn get(&self, name: AttrName) -> Option<&SharedString> {
        self.entries
            .iter()
            .find(|attr| attr.name == name)
            .map(|attr| &attr.value)
    }

    /// The value of the attribute whose name is `name`, matched as text.
    ///
    /// This is the form the style engine asks in, because its own name type is not the one stored
    /// here. Names are short, so the comparison is a handful of bytes.
    pub fn get_by_str(&self, name: &str) -> Option<&SharedString> {
        self.entries
            .iter()
            .find(|attr| attr.name.as_str() == name)
            .map(|attr| &attr.value)
    }

    /// Sets `name` to `value`, replacing any previous value, and returns what was there.
    pub fn set(&mut self, name: AttrName, value: SharedString) -> Option<SharedString> {
        match self.entries.iter_mut().find(|attr| attr.name == name) {
            Some(attr) => Some(core::mem::replace(&mut attr.value, value)),
            None => {
                self.entries.push(Attr { name, value });
                None
            }
        }
    }

    /// Removes `name`, returning its value if the node had it.
    pub fn remove(&mut self, name: AttrName) -> Option<SharedString> {
        let position = self.entries.iter().position(|attr| attr.name == name)?;
        Some(self.entries.remove(position).value)
    }

    /// Every attribute, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Attr> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::AttrName;
    use zgui_vocab::SharedString;

    use super::AttrMap;

    #[test]
    fn setting_an_attribute_twice_replaces_rather_than_appends() {
        let mut attrs = AttrMap::new();
        assert!(attrs.is_empty());
        assert_eq!(
            attrs.set(AttrName::new("role"), SharedString::from("tab")),
            None
        );
        assert_eq!(
            attrs
                .set(AttrName::new("role"), SharedString::from("tabpanel"))
                .as_ref()
                .map(SharedString::as_str),
            Some("tab")
        );
        assert_eq!(attrs.len(), 1);
        assert_eq!(
            attrs.get(AttrName::new("role")).map(SharedString::as_str),
            Some("tabpanel")
        );
    }

    #[test]
    fn an_attribute_is_findable_by_text_as_well_as_by_name() {
        let mut attrs = AttrMap::new();
        attrs.set(AttrName::new("data-state"), SharedString::from("open"));
        assert_eq!(
            attrs.get_by_str("data-state").map(SharedString::as_str),
            Some("open")
        );
        assert!(attrs.get_by_str("data-other").is_none());
    }

    #[test]
    fn removing_an_attribute_yields_its_value_once() {
        let mut attrs = AttrMap::new();
        attrs.set(AttrName::new("title"), SharedString::from("Save"));
        assert_eq!(
            attrs
                .remove(AttrName::new("title"))
                .as_ref()
                .map(SharedString::as_str),
            Some("Save")
        );
        assert_eq!(attrs.remove(AttrName::new("title")), None);
        assert_eq!(attrs.iter().count(), 0);
    }
}
