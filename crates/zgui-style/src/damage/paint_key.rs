//! The identity of everything one element's painted appearance depends on.
//!
//! Two elements with the same key necessarily paint the same, because the cascade hands out
//! shared, immutable groups of computed values and two elements that cascaded to the same result
//! share the very same allocations. Comparing this frame's key with last frame's is therefore a
//! handful of integer tests, and it is the *only* predicate that fires for a paint-only change:
//! border colours, corner radii, visibility, masks and box shadows all arrive from the engine
//! carrying no damage at all.

use style::custom_properties_map::CustomPropertiesMap;
use zgui_css::ComputedStyle;
use zgui_dom::side::paint_key::PaintStyleKey;

/// The paint key of `style`, with the identities of the element's generated-content styles.
///
/// `pseudos` is `[::before, ::after]`, each the identity of that pseudo-element's cascade result
/// or zero when it generates nothing. They are passed in rather than read here because they live
/// on the element's engine data, which is only borrowable while the element is being visited.
pub fn paint_key(style: &ComputedStyle, pseudos: [usize; 2]) -> PaintStyleKey {
    let custom = style.custom_properties();
    PaintStyleKey {
        background: address(style.get_background()),
        border: address(style.get_border()),
        effects: address(style.get_effects()),
        outline: address(style.get_outline()),
        svg: address(style.get_svg()),
        inherited_ui: address(style.get_inherited_ui()),
        inherited_box: address(style.get_inherited_box()),
        text: address(style.get_text()),
        box_: address(style.get_box()),
        position: address(style.get_position()),
        inherited_text: inherited_text(style),
        pseudo_before: pseudos[0],
        pseudo_after: pseudos[1],
        custom: (
            map_identity(&custom.inherited),
            map_identity(&custom.non_inherited),
        ),
    }
}

/// The identity of the group every text colour and text shadow lives in.
///
/// Named on its own because it is asked for twice: once as a field of the key, and once by whoever
/// is collecting the elements whose *text* paint moved. The two answers have to be the same
/// number — computed by two different routes they would never compare equal, and every restyled
/// element would look like a text-colour change.
pub fn inherited_text(style: &ComputedStyle) -> usize {
    address(style.get_inherited_text())
}

/// The address of one shared group of computed values.
///
/// Two styles that agree on a group point at the very same allocation, which is what makes this a
/// proof of equality rather than a guess: the cascade shares the group, so equal addresses cannot
/// hold different values.
fn address<T>(group: &T) -> usize {
    core::ptr::from_ref(group) as usize
}

/// The identity of one custom-property map.
///
/// The map is shared with the parent when an element declares nothing of its own, and the shared
/// storage is what this reads: the address of the map's first entry lives inside that storage, so
/// two elements holding the same map answer the same and a map built freshly answers differently.
/// The length participates as well, so that a map emptied in place is not mistaken for the map it
/// used to be.
///
/// The engine exposes no accessor for the map's own allocation, which is what this would otherwise
/// be. Like every other field of the key this over-fires and never under-fires: a fresh allocation
/// holding the same properties repaints an element that did not need it, and the set of elements
/// that pay is the set that declares custom properties of their own.
fn map_identity(map: &CustomPropertiesMap) -> usize {
    let first = map
        .get_index(0)
        .map_or(0, |(name, _value)| core::ptr::from_ref(name) as usize);
    first.wrapping_mul(31) ^ map.len()
}

/// The identity of a pseudo-element's cascade result, or zero when it generates nothing.
pub fn pseudo_identity(style: Option<&ComputedStyle>) -> usize {
    style.map_or(0, |style| style.heap_ptr() as usize)
}

/// Whether two keys differ only in the identity of a generated-content style.
///
/// A generated-content style is *cloned into the box* that carries it, so a change to one has to
/// rebuild that box rather than merely repaint the element it hangs off.
pub fn pseudos_moved(before: PaintStyleKey, after: PaintStyleKey) -> bool {
    (before.pseudo_before, before.pseudo_after) != (after.pseudo_before, after.pseudo_after)
}
