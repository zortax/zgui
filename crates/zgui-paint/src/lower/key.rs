//! The identity of everything a lowering reads.
//!
//! Two styles with the same key necessarily lower to the same paint description, because the
//! cascade hands out shared, immutable groups of computed values and two elements that cascaded to
//! the same result share the very same allocations. So the comparison is a handful of integer
//! tests, and lowering costs one pass per *distinct style* rather than one per element.

use zgui_css::{ComputedStyle, StructPtr};

/// The identity of the computed-value groups a paint lowering reads.
///
/// Every field is the identity of one shared group. Equal identities are proof that the groups hold
/// equal values — the cascade shares a group rather than copying it — so this is an exact key on
/// the hit path and never a guess.
///
/// The converse does not hold, and that is why it is not the only answer: the cascade runs on
/// several threads, and each may build its own copy of a logically identical group, so two elements
/// that look identical can hold different identities. [`PaintStyleCache`](super::cache::PaintStyleCache)
/// therefore falls back to comparing contents when a lookup by identity misses.
///
/// # Its relationship to the key that decides damage
///
/// A node also carries a key that decides whether it has to be *painted again*, and that key names
/// more groups than this one: it has to be a superset of what lowering reads, or a change to a
/// group a lowering consumes would produce no damage and no repaint at all. This key names exactly
/// the groups read here, so a group added to a lowering is added here in the same change — and the
/// wider key already covers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LoweringKey {
    /// Background colour and background images.
    pub background: StructPtr,
    /// Border colours, styles, widths and corner radii.
    pub border: StructPtr,
    /// Opacity, box shadows, filters, backdrop filters and blend mode.
    pub effects: StructPtr,
    /// Outline colour, style, width and offset.
    pub outline: StructPtr,
    /// `clip-path`.
    pub svg: StructPtr,
    /// `visibility`.
    pub inherited_box: StructPtr,
    /// Text decoration lines, colours and styles.
    pub text: StructPtr,
    /// `isolation`, and the transform properties a group composites under.
    ///
    /// Named with a trailing underscore because the group's own name is a keyword.
    pub box_: StructPtr,
    /// The element's own `color`, which every `currentColor` resolves against, and its text shadow.
    pub inherited_text: StructPtr,
    /// The two maps of custom properties in scope, out of which the vector paint properties are
    /// resolved.
    pub custom: (StructPtr, StructPtr),
}

impl LoweringKey {
    /// The key of `style`.
    pub fn of(style: &ComputedStyle) -> Self {
        Self {
            background: StructPtr::of(style.get_background()),
            border: StructPtr::of(style.get_border()),
            effects: StructPtr::of(style.get_effects()),
            outline: StructPtr::of(style.get_outline()),
            svg: StructPtr::of(style.get_svg()),
            inherited_box: StructPtr::of(style.get_inherited_box()),
            text: StructPtr::of(style.get_text()),
            box_: StructPtr::of(style.get_box()),
            inherited_text: StructPtr::of(style.get_inherited_text()),
            custom: StructPtr::custom_properties(style),
        }
    }

    /// Every property group this key names, for a caller checking it against a wider key.
    ///
    /// The custom-property maps are left out: they are a derived identity rather than a group
    /// address, so an address-by-address comparison has nothing to find them under.
    pub fn group_identities(&self) -> [StructPtr; 9] {
        [
            self.background,
            self.border,
            self.effects,
            self.outline,
            self.svg,
            self.inherited_box,
            self.text,
            self.box_,
            self.inherited_text,
        ]
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::LoweringKey;

    #[test]
    fn one_style_answers_the_same_key_twice() {
        let style = StyleDraft::initial().build();
        assert_eq!(LoweringKey::of(&style), LoweringKey::of(&style));
    }

    #[test]
    fn a_shared_style_answers_the_same_key_as_its_clone() {
        // Two elements that cascaded to the same result share the allocation, and the clone here
        // is exactly that sharing. If this ever stopped holding, the whole memoisation would too.
        let style = StyleDraft::initial().build();
        let shared = style.clone();
        assert_eq!(LoweringKey::of(&style), LoweringKey::of(&shared));
    }

    #[test]
    fn every_named_group_has_a_real_identity() {
        let style = StyleDraft::initial().build();
        let key = LoweringKey::of(&style);
        assert!(
            key.group_identities().iter().all(|group| group.0 != 0),
            "an identity of zero would compare equal to a default key"
        );
    }

    #[test]
    fn no_two_groups_share_an_identity() {
        let style = StyleDraft::initial().build();
        let mut groups = LoweringKey::of(&style).group_identities();
        groups.sort_unstable();
        let before = groups.len();
        groups.sort_unstable();
        let mut unique = groups.to_vec();
        unique.dedup();
        assert_eq!(
            unique.len(),
            before,
            "two fields naming one group would make the key blind to one of them"
        );
    }
}
