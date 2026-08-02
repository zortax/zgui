//! Which attribute names any active selector depends on.

use rustc_hash::FxHashSet;
use style::stylist::Stylist;
use zgui_interned::AttrName;

/// Every attribute name mentioned by any selector in any installed sheet.
///
/// There is no "matches attributes without naming one" case to carry a flag for: every attribute
/// selector this engine can parse names its attribute, so the dependency index is keyed by that
/// name and a name absent from it is a name no selector can be looking at. An identifier or a
/// class *is* an attribute in a document language, but neither reaches an element here through an
/// attribute write, so neither appears in this set.
pub(crate) fn build(stylist: &Stylist) -> FxHashSet<AttrName> {
    let mut attrs = FxHashSet::default();
    for (data, _origin) in stylist.iter_origins() {
        for name in data
            .invalidation_map()
            .other_attribute_affecting_selectors
            .keys()
        {
            attrs.insert(AttrName::new(name));
        }
    }
    attrs
}
