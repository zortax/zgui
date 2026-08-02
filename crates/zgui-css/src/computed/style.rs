//! The computed style itself, and the allocation identities a cache keys on.

use servo_arc::Arc as ServoArc;
use style::properties::ComputedValues;

/// Property groups, each of which is shared separately between styles that agree on it.
///
/// A style holds one shared pointer per group, so two elements that differ only in their
/// background share the same font group. That is what makes [`StructPtr`] a usable cache key: two
/// styles with the same font pointer resolve to the same fonts, sizes and spacings, whatever else
/// differs between them.
pub use style::properties::style_structs;

/// One element's fully resolved style: every property with a value, no keywords, no inheritance
/// and no relative units left.
///
/// It is a shared pointer, and cloning one is cheap by design. Elements that cascade to the same
/// result share the allocation, so a consumer keys its own work on the pointer and does that work
/// once per *distinct style* rather than once per element — in a component library, one to two
/// orders of magnitude fewer.
///
/// Property groups are read straight through it, because it dereferences to the record holding
/// them:
///
/// ```
/// use zgui_css::StyleDraft;
///
/// let style = StyleDraft::initial().build();
/// assert_eq!(style.get_font().font_size.computed_size().px(), 16.0);
/// ```
pub type ComputedStyle = ServoArc<ComputedValues>;

/// The identity of one property group's allocation.
///
/// Two styles whose groups have equal identities agree on every property in that group, with no
/// comparison performed. A cache keyed on this therefore costs a pointer test on the hit path,
/// which is the path a document full of similar elements takes.
///
/// The converse does not hold and must not be assumed: two groups may hold equal values in
/// separate allocations, because the cascade runs on several threads and each may build its own
/// copy of a logically identical result. A cache keyed on identity is therefore a fast path with a
/// content-hashed fallback behind it, never the only answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructPtr(pub usize);

impl StructPtr {
    /// The identity of one shared property group, whichever group it is.
    ///
    /// The named constructors below exist because their groups are asked for by more than one
    /// consumer and a second spelling of the same question is how two consumers come to disagree.
    /// This is for the rest: a consumer reading a group nothing else keys on names it here.
    ///
    /// ```
    /// use zgui_css::{StructPtr, StyleDraft};
    ///
    /// let style = StyleDraft::initial().build();
    /// assert_eq!(StructPtr::of(style.get_border()), StructPtr::of(style.get_border()));
    /// assert_ne!(StructPtr::of(style.get_border()), StructPtr::of(style.get_outline()));
    /// ```
    pub fn of<T>(group: &T) -> Self {
        Self(core::ptr::from_ref(group) as usize)
    }

    /// The identities of the two custom-property maps in scope on an element, inherited first.
    ///
    /// A map is shared with the parent when an element declares nothing of its own, and it is that
    /// sharing this reads. The engine exposes no accessor for the map's own allocation, so the
    /// identity is the address of its first entry combined with its length — which over-fires,
    /// never under-fires, and costs only the elements that declare custom properties of their own.
    ///
    /// ```
    /// use zgui_css::{StructPtr, StyleDraft};
    ///
    /// let style = StyleDraft::initial().build();
    /// // A style declaring nothing has two empty maps, and they answer stably.
    /// assert_eq!(StructPtr::custom_properties(&style), StructPtr::custom_properties(&style));
    /// ```
    pub fn custom_properties(style: &ComputedStyle) -> (Self, Self) {
        let custom = style.custom_properties();
        (
            Self(map_identity(&custom.inherited)),
            Self(map_identity(&custom.non_inherited)),
        )
    }

    /// The identity of the font group — family, size, weight, style, width, variations, features
    /// and line height.
    pub fn font(style: &ComputedStyle) -> Self {
        Self(style.clone_font().heap_ptr() as usize)
    }

    /// The identity of the inherited-text group — colour, spacing, alignment, indent, wrapping and
    /// white-space handling.
    pub fn inherited_text(style: &ComputedStyle) -> Self {
        Self(style.clone_inherited_text().heap_ptr() as usize)
    }

    /// The identity of the inherited-box group, which carries the writing direction.
    pub fn inherited_box(style: &ComputedStyle) -> Self {
        Self(style.clone_inherited_box().heap_ptr() as usize)
    }
}

/// The identity of one custom-property map.
///
/// The address of the map's first entry lives inside the storage the map shares with its parent, so
/// two elements holding the same map answer the same and a map built freshly answers differently.
/// The length participates as well, so that a map emptied in place is not mistaken for the map it
/// used to be.
fn map_identity(map: &style::custom_properties_map::CustomPropertiesMap) -> usize {
    let first = map
        .get_index(0)
        .map_or(0, |(name, _value)| core::ptr::from_ref(name) as usize);
    first.wrapping_mul(31) ^ map.len()
}
