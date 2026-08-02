//! Which text decorations are in force at a line box, and where they came from.
//!
//! `text-decoration-line` is not an inherited property, and that is not an oversight in CSS: a
//! decoration belongs to the box that declared it, is drawn across every in-flow descendant of that
//! box in *its* colour, style and thickness, and several ancestors may each contribute one. An
//! inherited property could express none of that — a descendant would redraw the line in its own
//! colour, and a descendant that set its own would replace its ancestor's instead of adding to it.
//!
//! So the propagation happens here, on the way down the emit walk, and a line box draws the whole
//! list rather than its own box's value. That is also why a decoration declared on a box with text
//! directly inside it drew nothing before this existed: the line box belongs to an anonymous inline
//! root generated *below* the declaring element, and an anonymous box inherits nothing that is not
//! an inherited property.
//!
//! # Where propagation stops
//!
//! At a box that is not in its parent's flow, because such a box is not part of the text the
//! decoration was drawn across: a float, an absolutely or fixed positioned box, and an atomic
//! inline-level box — `inline-block` and friends, whose inside is a formatting context of its own.
//! Each of those starts a fresh list, so a `text-decoration: underline` on a paragraph does not
//! reach into an `inline-block` sitting in the middle of it.

use zgui_color::Color;
use zgui_css::ComputedStyle;
use zgui_css::values::size::{DisplayInside, DisplayOutside, FloatValue, PositionValue};

use crate::emit::text::DecorationStyle;

/// The decorations in force as the walk descends, outermost first.
///
/// The list is a stack with a *floor*: entering a box that interrupts propagation moves the floor
/// up to the current length, so everything an ancestor contributed is hidden for the subtree
/// without being discarded, and leaving the box puts it back.
#[derive(Debug, Default)]
pub struct Decorations {
    /// Every decoration contributed by an ancestor still on the walk, outermost first.
    contributed: Vec<DecorationStyle>,
    /// The index the visible list starts at, which a box that interrupts propagation raises.
    floor: usize,
    /// What to restore on the way out: the length and the floor as they were on the way in.
    frames: Vec<(usize, usize)>,
}

impl Decorations {
    /// A walk that has entered nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enters a box, contributing whatever decoration it declares.
    ///
    /// `style` is the box's own computed style, which is what decides both halves: what the box
    /// contributes, and whether it is in its parent's flow at all.
    pub fn enter(&mut self, style: &ComputedStyle, decoration: &DecorationStyle) {
        self.frames.push((self.contributed.len(), self.floor));
        if interrupts_propagation(style) {
            self.floor = self.contributed.len();
        }
        if decoration.draws_anything() {
            self.contributed.push(*decoration);
        }
    }

    /// Leaves the box most recently entered.
    ///
    /// # Panics
    ///
    /// Panics if more boxes are left than were entered, which would mean the walk's enter and leave
    /// calls are not paired — the same failure that leaves a group's target open.
    pub fn leave(&mut self) {
        let (length, floor) = self
            .frames
            .pop()
            .expect("every leave follows its own enter");
        self.contributed.truncate(length);
        self.floor = floor;
    }

    /// The decorations a line box here draws, outermost first.
    pub fn in_force(&self) -> &[DecorationStyle] {
        &self.contributed[self.floor..]
    }

    /// The same list with every colour's alpha scaled by `alpha`.
    ///
    /// A folded group opacity applies to a decoration exactly as it applies to a glyph, and the
    /// decoration in force may have been contributed above the group as easily as below it.
    pub fn faded(&self, alpha: f32) -> smallvec::SmallVec<[DecorationStyle; 2]> {
        self.in_force()
            .iter()
            .map(|decoration| DecorationStyle {
                color: fade(decoration.color, alpha),
                ..*decoration
            })
            .collect()
    }
}

/// A fingerprint of the decorations in force, for the record a replay is checked against.
///
/// Two frames can agree about a fragment's own style, box, clip and transform and still have to
/// draw different lines, because the lines came from somewhere above it. Without this in the
/// record, changing `text-decoration` on a paragraph would replay every line inside it exactly as
/// it was.
pub fn signature(decorations: &[DecorationStyle]) -> u64 {
    let mut hash = zgui_scene::ContentHash::new().u32(decorations.len() as u32);
    for decoration in decorations {
        hash = decoration.fold_into(hash);
    }
    hash.finish()
}

/// The same colour with its alpha scaled.
fn fade(color: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        return color;
    }
    color.with_alpha(color.alpha() * alpha)
}

/// Whether a box is outside its parent's flow, and so outside the text an ancestor decorated.
pub fn interrupts_propagation(style: &ComputedStyle) -> bool {
    let box_ = style.get_box();
    if box_.float != FloatValue::None {
        return true;
    }
    if matches!(
        box_.position,
        PositionValue::Absolute | PositionValue::Fixed
    ) {
        return true;
    }
    let display = box_.display;
    display.outside() == DisplayOutside::Inline && display.inside() != DisplayInside::Flow
}

#[cfg(test)]
mod tests {
    use zgui_color::Color;
    use zgui_css::StyleDraft;
    use zgui_scene::prim::decoration::DecorationStyle as Line;

    use super::{Decorations, interrupts_propagation};
    use crate::emit::text::DecorationStyle;

    /// A decoration that draws an underline in the given colour.
    fn underline(color: Color) -> DecorationStyle {
        DecorationStyle {
            underline: true,
            color,
            style: Line::Solid,
            thickness: 1.0,
            ..DecorationStyle::default()
        }
    }

    /// The initial computed style, which is an in-flow block.
    fn in_flow() -> zgui_css::ComputedStyle {
        StyleDraft::initial().build()
    }

    #[test]
    fn a_decoration_declared_on_a_box_reaches_the_line_boxes_below_it() {
        // This is the whole defect: the line box belongs to an anonymous inline root generated
        // under the declaring element, and nothing an anonymous box inherits carries a decoration.
        let mut held = Decorations::new();
        held.enter(&in_flow(), &underline(Color::srgb(1.0, 0.0, 0.0, 1.0)));
        assert_eq!(held.in_force().len(), 1);
        held.enter(&in_flow(), &DecorationStyle::default());
        assert_eq!(
            held.in_force().len(),
            1,
            "a box that declares nothing still draws what its ancestor declared"
        );
        held.leave();
        held.leave();
        assert!(held.in_force().is_empty());
    }

    #[test]
    fn two_ancestors_each_contribute_their_own_line() {
        let mut held = Decorations::new();
        held.enter(&in_flow(), &underline(Color::srgb(1.0, 0.0, 0.0, 1.0)));
        held.enter(&in_flow(), &underline(Color::srgb(0.0, 0.0, 1.0, 1.0)));
        let lines = held.in_force();
        assert_eq!(
            lines.len(),
            2,
            "a nested decoration adds rather than replaces"
        );
        assert_eq!(lines[0].color, Color::srgb(1.0, 0.0, 0.0, 1.0));
        assert_eq!(lines[1].color, Color::srgb(0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn a_box_outside_its_parents_flow_starts_a_fresh_list() {
        let mut draft = StyleDraft::initial();
        draft.box_group().float = zgui_css::values::size::FloatValue::Left;
        let floated = draft.build();
        assert!(interrupts_propagation(&floated));

        let mut held = Decorations::new();
        held.enter(&in_flow(), &underline(Color::srgb(1.0, 0.0, 0.0, 1.0)));
        held.enter(&floated, &DecorationStyle::default());
        assert!(
            held.in_force().is_empty(),
            "a float is not part of the text its parent decorated"
        );
        held.leave();
        assert_eq!(
            held.in_force().len(),
            1,
            "and leaving it puts the ancestor's line back"
        );
    }

    #[test]
    fn an_in_flow_block_does_not_interrupt_propagation() {
        assert!(
            !interrupts_propagation(&in_flow()),
            "if this were true nothing would ever propagate and every case above would pass \
             vacuously"
        );
    }

    #[test]
    fn a_folded_alpha_scales_every_line_in_force() {
        let mut held = Decorations::new();
        held.enter(&in_flow(), &underline(Color::srgb(1.0, 0.0, 0.0, 0.8)));
        let faded = held.faded(0.5);
        assert_eq!(faded.len(), 1);
        assert!((faded[0].color.alpha() - 0.4).abs() < 1e-6);
    }
}
