//! Composing a running animation's values over the shared lowered style.
//!
//! A lowering is shared: a thousand identically styled buttons resolve to one of them, and that
//! sharing is what makes a large document cheap to paint. An animation therefore cannot write into
//! one. What it writes into is the node's own override, and this is where the two are put together
//! — on a copy, for one box, at the moment that box is emitted.
//!
//! The copy is why hover-*out* works. As soon as the pointer leaves, the element's computed style
//! has reverted to the one it shares with its siblings, so it is being drawn through the very same
//! lowering they are; a value written into that lowering would fade all of them together.

use zgui_color::Color;
use zgui_dom::side::AnimOverride;
use zgui_dom::{Document, NodeKey};
use zgui_scene::{Paint, PaintOwner, PaintRole, PropertyOwner};

use crate::lower::PaintStyle;

/// Where the emitter asks what an element's animations are currently overriding.
///
/// A trait rather than a table, because the answer lives in the document and this crate walks a
/// fragment tree: the alternative is copying a column into the paint stage every frame, most of
/// which is empty on every frame of every document that is not animating.
pub trait AnimOverrides {
    /// What `node`'s running animations override right now, if anything.
    fn get(&self, node: NodeKey) -> Option<&AnimOverride>;
}

/// Nothing animating anywhere.
///
/// The default a caller that is not driving animations uses, and the one the static path costs a
/// null check for.
pub struct NoAnim;

impl AnimOverrides for NoAnim {
    fn get(&self, _node: NodeKey) -> Option<&AnimOverride> {
        None
    }
}

impl AnimOverrides for Document {
    fn get(&self, node: NodeKey) -> Option<&AnimOverride> {
        self.store()
            .columns()
            .anim
            .get(node)
            .and_then(Option::as_ref)
            .map(Box::as_ref)
    }
}

/// Applies `over` to a copy of a shared lowering.
///
/// Only the values an animation is actually driving are replaced, so a transition moving one colour
/// leaves every other property of the style exactly as the cascade produced it.
pub fn compose(style: &mut PaintStyle, over: &AnimOverride) {
    if let Some(opacity) = over.opacity {
        style.group.opacity = opacity.clamp(0.0, 1.0);
    }
    if let Some(color) = &over.background_color {
        style.background.color = zgui_css::values::color::to_color(color);
    }
    if let Some(colors) = &over.border_colors {
        style.border.colors = [
            zgui_css::values::color::to_color(&colors[0]),
            zgui_css::values::color::to_color(&colors[1]),
            zgui_css::values::color::to_color(&colors[2]),
            zgui_css::values::color::to_color(&colors[3]),
        ];
        // A border whose every side was transparent is skipped outright, and an animation fading
        // one in has to be able to bring it back.
        style.border.invisible = style.border.colors.iter().all(|color| color.alpha() == 0.0);
    }
    if let Some(color) = &over.outline_color
        && let Some(outline) = &mut style.outline
    {
        outline.color = zgui_css::values::color::to_color(color);
    }
}

/// Which of a box's paints an animation drives through slots the box owns.
///
/// A box in this mode draws its animated colours through [`PaintTable::owned`] slots, and its
/// paint signature holds this set in place of the animated values. A frame that moves only the
/// colours therefore replays the box's chunk, and the animation stage rewrites the slots the chunk
/// names.
///
/// The border bit has a second bit for the paint family: four equal colours are one solid
/// stroke, and anything else is one colour per side. A primitive names the family, so a change of
/// family changes the signature and encodes the box again.
///
/// [`PaintTable::owned`]: zgui_scene::PaintTable::owned
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnedPaints(u8);

impl OwnedPaints {
    /// The background colour.
    const BACKGROUND: u8 = 1;
    /// The border colours.
    const BORDER: u8 = 1 << 1;
    /// The border colours differ, so the stroke is one colour per side.
    const BORDER_SIDES: u8 = 1 << 2;
    /// The outline colour.
    const OUTLINE: u8 = 1 << 3;
    /// Marks a paint signature as an owned set rather than a fingerprint of values.
    const OWNED: u64 = 1 << 63;

    /// The paints `over` drives, or `None` when it drives a value no slot holds.
    ///
    /// The opacity is such a value: it is folded into every colour the subtree draws.
    pub fn of(over: &AnimOverride) -> Option<Self> {
        if over.opacity.is_some() {
            return None;
        }
        let mut bits = 0;
        if over.background_color.is_some() {
            bits |= Self::BACKGROUND;
        }
        if let Some(colors) = border_colors(over) {
            bits |= Self::BORDER;
            if matches!(border_paint(colors), Paint::Sides(_)) {
                bits |= Self::BORDER_SIDES;
            }
        }
        if over.outline_color.is_some() {
            bits |= Self::OUTLINE;
        }
        (bits != 0).then_some(Self(bits))
    }

    /// The paint signature of a box in this mode.
    pub fn signature(self) -> u64 {
        Self::OWNED | u64::from(self.0)
    }

    /// Whether the background colour is in an owned slot.
    pub fn background(self) -> bool {
        self.0 & Self::BACKGROUND != 0
    }

    /// Whether the border stroke is in an owned slot.
    pub fn border(self) -> bool {
        self.0 & Self::BORDER != 0
    }

    /// Whether the outline stroke is in an owned slot.
    pub fn outline(self) -> bool {
        self.0 & Self::OUTLINE != 0
    }

    /// Whether the box is drawn even where its shared style draws nothing.
    ///
    /// A fully transparent background or border is skipped, and an animation from transparent
    /// needs the primitive there to rewrite.
    pub fn forces_box(self) -> bool {
        self.background() || self.border()
    }

    /// The paint each owned slot holds for `over`, named after the box `owner` names.
    ///
    /// What the animation stage writes. The emission draws the same values from the style
    /// [`compose`] makes, so the slot holds one value whoever wrote it last.
    pub fn paints(
        self,
        owner: PropertyOwner,
        over: &AnimOverride,
    ) -> impl Iterator<Item = (PaintOwner, Paint)> {
        let background = over
            .background_color
            .as_ref()
            .filter(|_| self.background())
            .map(|color| {
                (
                    PaintOwner::new(owner, PaintRole::Background),
                    Paint::Solid(zgui_css::values::color::to_color(color)),
                )
            });
        let border = border_colors(over).filter(|_| self.border()).map(|colors| {
            (
                PaintOwner::new(owner, PaintRole::BorderSides),
                border_paint(colors),
            )
        });
        let outline = over
            .outline_color
            .as_ref()
            .filter(|_| self.outline())
            .map(|color| {
                (
                    PaintOwner::new(owner, PaintRole::Outline),
                    Paint::Solid(zgui_css::values::color::to_color(color)),
                )
            });
        background.into_iter().chain(border).chain(outline)
    }
}

/// The animated border colours, top, right, bottom and left, as the emission draws them.
fn border_colors(over: &AnimOverride) -> Option<[Color; 4]> {
    over.border_colors
        .as_ref()
        .map(|colors| colors.each_ref().map(zgui_css::values::color::to_color))
}

/// The stroke an owned border slot holds: one colour when the four sides agree, else four.
pub fn border_paint(colors: [Color; 4]) -> Paint {
    if colors.iter().all(|color| *color == colors[0]) {
        Paint::Solid(colors[0])
    } else {
        Paint::Sides(colors)
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_css::values::color::AbsoluteColor;
    use zgui_dom::side::AnimOverride;

    use super::{AnimOverrides, NoAnim, OwnedPaints, compose};
    use crate::lower::lower;

    #[test]
    fn nothing_animating_answers_nothing() {
        let node = zgui_dom::Document::new()
            .store()
            .key_of(zgui_dom::Document::new().document_index());
        assert!(NoAnim.get(node).is_none());
    }

    #[test]
    fn an_overridden_opacity_replaces_only_the_opacity() {
        let mut style = lower(&StyleDraft::initial().build(), 1.0);
        let color = style.color;
        compose(
            &mut style,
            &AnimOverride {
                opacity: Some(0.25),
                ..AnimOverride::new()
            },
        );
        assert_eq!(style.group.opacity, 0.25);
        assert_eq!(style.color, color);
    }

    #[test]
    fn a_border_fading_in_stops_being_skipped() {
        // The initial border is fully transparent and is skipped as invisible. An animation that
        // fades one in has to clear that, or the frame paints nothing and the counters all agree
        // that it worked.
        let mut style = lower(&StyleDraft::initial().build(), 1.0);
        assert!(style.border.invisible);
        compose(
            &mut style,
            &AnimOverride {
                border_colors: Some([AbsoluteColor::srgb_legacy(255, 0, 0, 1.0); 4]),
                ..AnimOverride::new()
            },
        );
        assert!(!style.border.invisible);
    }

    /// An override moving every colour a box can own, with the border sides given.
    fn colours(sides: [AbsoluteColor; 4]) -> AnimOverride {
        AnimOverride {
            background_color: Some(AbsoluteColor::srgb_legacy(10, 20, 30, 0.5)),
            border_colors: Some(sides),
            outline_color: Some(AbsoluteColor::srgb_legacy(40, 50, 60, 1.0)),
            ..AnimOverride::new()
        }
    }

    #[test]
    fn an_opacity_keeps_a_box_out_of_owned_slots() {
        let red = AbsoluteColor::srgb_legacy(255, 0, 0, 1.0);
        let over = AnimOverride {
            opacity: Some(0.5),
            ..colours([red; 4])
        };
        assert_eq!(OwnedPaints::of(&over), None);
        assert_eq!(OwnedPaints::of(&AnimOverride::new()), None);
    }

    #[test]
    fn a_change_of_border_family_changes_the_signature() {
        let red = AbsoluteColor::srgb_legacy(255, 0, 0, 1.0);
        let blue = AbsoluteColor::srgb_legacy(0, 0, 255, 1.0);
        let one = OwnedPaints::of(&colours([red; 4])).expect("owned");
        let again = OwnedPaints::of(&colours([blue; 4])).expect("owned");
        let four = OwnedPaints::of(&colours([red, blue, red, blue])).expect("owned");
        assert_eq!(
            one.signature(),
            again.signature(),
            "the values are not in it"
        );
        assert_ne!(one.signature(), four.signature(), "the family is");
    }

    #[test]
    fn the_owned_paints_are_what_the_composed_style_draws() {
        let red = AbsoluteColor::srgb_legacy(255, 0, 0, 1.0);
        let blue = AbsoluteColor::srgb_legacy(0, 0, 255, 0.25);
        let over = colours([red, blue, red, blue]);
        let mut style = lower(&StyleDraft::initial().build(), 1.0);
        style.outline = Some(crate::lower::outline::OutlinePaint {
            width: 2.0,
            offset: 0.0,
            color: zgui_color::Color::BLACK,
            style: crate::lower::border::LineStyle::Solid,
        });
        compose(&mut style, &over);
        let owner = zgui_scene::PropertyOwner::new(7).expect("not the empty word");
        let owned = OwnedPaints::of(&over).expect("owned");
        let paints: Vec<_> = owned.paints(owner, &over).collect();
        assert_eq!(
            paints,
            vec![
                (
                    zgui_scene::PaintOwner::new(owner, zgui_scene::PaintRole::Background),
                    zgui_scene::Paint::Solid(style.background.color)
                ),
                (
                    zgui_scene::PaintOwner::new(owner, zgui_scene::PaintRole::BorderSides),
                    zgui_scene::Paint::Sides(style.border.colors)
                ),
                (
                    zgui_scene::PaintOwner::new(owner, zgui_scene::PaintRole::Outline),
                    zgui_scene::Paint::Solid(style.outline.expect("an outline").color)
                ),
            ]
        );
    }
}
