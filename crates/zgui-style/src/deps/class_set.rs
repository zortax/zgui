//! Which class names any active selector depends on.

use rustc_hash::FxHashSet;
use style::stylist::Stylist;
use zgui_interned::ClassName;

/// Every class name mentioned by any selector in any installed sheet.
///
/// Read from the rule set's *dependency* index rather than from the map it matches with. The
/// matching map buckets a rule by its rightmost compound only, so `.theme-dark .btn { … }` files
/// under `btn` and never mentions `theme-dark` — and a filter built from it would report that
/// toggling `theme-dark` on the root cannot matter. That is a wrong pixel with nothing to notice
/// it by, which is the one direction a filter may never be wrong in.
pub(crate) fn build(stylist: &Stylist) -> FxHashSet<ClassName> {
    let mut classes = FxHashSet::default();
    for (data, _origin) in stylist.iter_origins() {
        for (atom, _dependencies) in data.invalidation_map().class_to_selector.iter() {
            classes.insert(ClassName::new(atom));
        }
    }
    classes
}
