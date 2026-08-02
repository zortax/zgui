//! The identity of everything a node's painted appearance depends on.
//!
//! Comparing two of these is how a repaint is decided. The alternative — asking the style engine
//! whether it thinks anything changed — does not work, because a border colour, a corner radius, a
//! visibility flip, a mask and a box shadow all arrive from the engine carrying *no* damage at all.
//! A `:hover` that changes only a border colour would therefore never repaint anything.
//!
//! Every field is the address of a shared, immutable group of computed values. Identically styled
//! elements share those groups, so two equal keys are proof of two equal appearances, and the
//! comparison is a handful of integer tests rather than a walk over properties.
//!
//! **The key must be a superset of what painting reads.** A group that painting starts reading and
//! this key does not name is a change that produces no repaint — silently, and only for some
//! properties. So a field is added here in the same change that starts reading the group.

/// Identity of the computed-value groups a node's painted appearance depends on.
///
/// Written once per node per restyle by the stage that computes styles, and compared against the
/// previous frame's value to decide whether the node has to be painted again. Over-firing is a cost
/// and under-firing is a wrong pixel, so every field is chosen to over-fire.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
pub struct PaintStyleKey {
    /// Background colour, images, position, repeat and size.
    pub background: usize,
    /// Border colours, styles, widths and radii.
    pub border: usize,
    /// Opacity, box shadows, filters, backdrop filters, clip and blend mode.
    pub effects: usize,
    /// Outline colour, style, width and offset.
    pub outline: usize,
    /// Clip paths and masks.
    pub svg: usize,
    /// Cursor, colour scheme and pointer events.
    pub inherited_ui: usize,
    /// Visibility and image rendering.
    pub inherited_box: usize,
    /// Text decorations.
    pub text: usize,
    /// Transforms, transform origin, perspective, backface visibility, isolation and will-change.
    ///
    /// Named with a trailing underscore because the group's own name is a keyword.
    pub box_: usize,
    /// Stacking order.
    pub position: usize,
    /// Colour and text shadow.
    ///
    /// The whole group participates rather than a hash of the colour: a text shadow is invisible to
    /// a colour hash, and `currentcolor` resolves against the colour, so a change to either can
    /// move any painted pixel of the node.
    pub inherited_text: usize,
    /// The styles of the node's generated content before its own, or zero when it generates none.
    ///
    /// Generated content has no node of its own, so it has no row in this column. Without these two
    /// fields a rule that changes only the colour of a node's generated content produces no damage
    /// at all.
    pub pseudo_before: usize,
    /// The styles of the node's generated content after its own, or zero when it generates none.
    pub pseudo_after: usize,
    /// The two maps of custom properties in scope on this node.
    ///
    /// Painting resolves the framework's own custom properties out of these, so a theme that
    /// changes nothing but a custom property must still produce damage. The maps are shared with
    /// the parent when a node declares none, so this over-fires and never under-fires.
    pub custom: (usize, usize),
}

impl PaintStyleKey {
    /// The key of a node whose appearance has never been computed.
    ///
    /// Never equal to the key of a node that has one, because a computed-value group's address is
    /// never null — so the first comparison after a node is styled always reports a change.
    pub const UNSTYLED: Self = Self {
        background: 0,
        border: 0,
        effects: 0,
        outline: 0,
        svg: 0,
        inherited_ui: 0,
        inherited_box: 0,
        text: 0,
        box_: 0,
        position: 0,
        inherited_text: 0,
        pseudo_before: 0,
        pseudo_after: 0,
        custom: (0, 0),
    };
}

#[cfg(test)]
mod tests {
    use super::PaintStyleKey;

    #[test]
    fn the_unstyled_key_is_the_default_and_differs_from_any_styled_one() {
        assert_eq!(PaintStyleKey::default(), PaintStyleKey::UNSTYLED);
        let styled = PaintStyleKey {
            background: 0x1000,
            ..PaintStyleKey::UNSTYLED
        };
        assert_ne!(styled, PaintStyleKey::UNSTYLED);
    }

    #[test]
    fn a_change_to_any_single_group_changes_the_key() {
        let base = PaintStyleKey {
            background: 1,
            border: 2,
            effects: 3,
            outline: 4,
            svg: 5,
            inherited_ui: 6,
            inherited_box: 7,
            text: 8,
            box_: 9,
            position: 10,
            inherited_text: 11,
            pseudo_before: 12,
            pseudo_after: 13,
            custom: (14, 15),
        };
        let mutations: [fn(&mut PaintStyleKey); 14] = [
            |key| key.background += 1,
            |key| key.border += 1,
            |key| key.effects += 1,
            |key| key.outline += 1,
            |key| key.svg += 1,
            |key| key.inherited_ui += 1,
            |key| key.inherited_box += 1,
            |key| key.text += 1,
            |key| key.box_ += 1,
            |key| key.position += 1,
            |key| key.inherited_text += 1,
            |key| key.pseudo_before += 1,
            |key| key.pseudo_after += 1,
            |key| key.custom.0 += 1,
        ];
        for mutate in mutations {
            let mut changed = base;
            mutate(&mut changed);
            assert_ne!(changed, base, "every field participates in the comparison");
        }
    }
}
