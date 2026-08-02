//! `clip-path`: what a box cuts its own painting to.
//!
//! Overflow clipping is not here. That one is layout's, because the chain a descendant is drawn
//! under has to exist before a fragment does — a fragment carries its chain, and this stage reads
//! it rather than building a second one.
//!
//! What is here is the other clip, the one that cuts a box's *own* pixels to a shape. A shape is
//! only a clip a draw call can apply once it has been rasterised into a coverage tile, and nothing
//! in this stage rasterises anything, so what this answers is the question the emission actually
//! needs: does this box need a target of its own for a mask to be applied to.

use zgui_css::parity::Support;
use zgui_css::values::effect::ClipPathValue;
use zgui_css::{ComputedStyle, register_properties};

register_properties! {
    clip_path => Support::Ignored("a clip-path shape needs a rasterised coverage tile"),
    clip_rule => Support::Absent(zgui_css::parity::AbsentReason::GeckoOnly),
}

/// What a box's `clip-path` asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipShape {
    /// Nothing is clipped, which is what nearly every box says.
    #[default]
    None,
    /// A shape whose coverage has to be rasterised before it can be applied.
    ///
    /// A box carrying one composites into a target of its own, because that is where the mask is
    /// applied. Until something rasterises the shape the target is composited unmasked, which draws
    /// too much rather than too little — visible, and never a silently missing element.
    Masked,
}

impl ClipShape {
    /// Whether this shape needs a target of its own.
    pub fn needs_target(self) -> bool {
        self == Self::Masked
    }
}

/// Lowers a style's `clip-path`.
///
/// A `url()` reference names a shape defined by a document this engine does not resolve, and a
/// shape it cannot resolve clips nothing — which is what every engine does with a reference that
/// does not resolve.
pub fn of(style: &ComputedStyle) -> ClipShape {
    match style.get_svg().clip_path {
        ClipPathValue::None | ClipPathValue::Url(_) => ClipShape::None,
        ClipPathValue::Shape(..) | ClipPathValue::Box(_) => ClipShape::Masked,
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::{ClipShape, of};

    #[test]
    fn a_box_with_no_clip_path_needs_no_target() {
        let style = StyleDraft::initial().build();
        assert_eq!(of(&style), ClipShape::None);
        assert!(!of(&style).needs_target());
    }

    #[test]
    fn only_a_masked_shape_needs_a_target() {
        assert!(ClipShape::Masked.needs_target());
        assert!(!ClipShape::None.needs_target());
    }
}
