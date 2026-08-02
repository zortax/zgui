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

use zgui_dom::side::AnimOverride;
use zgui_dom::{Document, NodeKey};

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

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_css::values::color::AbsoluteColor;
    use zgui_dom::side::AnimOverride;

    use super::{AnimOverrides, NoAnim, compose};
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
}
