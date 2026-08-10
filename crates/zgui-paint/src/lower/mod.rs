//! Turning a computed style into a description of what it paints.
//!
//! A cascade result holds every CSS property. Painting reads a few dozen of them, so it lowers the
//! result into a shape of its own — and does that once per *distinct style*, not once per element,
//! because two elements that cascaded to the same result share the very same allocations.
//!
//! # What a lowering may hold, and what it may not
//!
//! Everything here is independent of the box it will be painted on. A colour is; a corner radius is
//! not, because a percentage radius is a percentage of the box's own extent, and a gradient's line
//! is not, because it runs across the box. Those are resolved where they are emitted, against a
//! rectangle. Putting one of them in a lowering would put a per-box number in a table shared by
//! every box with that style, and every box after the first would be painted with the first one's
//! geometry.

pub mod anim;
pub mod background;
pub mod border;
pub mod cache;
pub mod clip;
pub mod filter;
pub mod key;
pub mod outline;
pub mod shadow;
pub mod transform;

use smallvec::SmallVec;
use zgui_color::Color;
use zgui_css::parity::Support;
use zgui_css::values::color::{current, to_color};
use zgui_css::values::size::{ObjectFitValue, ObjectPositionValue, VisibilityValue};
use zgui_css::{ComputedStyle, register_properties};

pub use crate::lower::background::BackgroundStyle;
pub use crate::lower::border::BorderPaint;
pub use crate::lower::clip::ClipShape;
pub use crate::lower::filter::GroupPaint;
pub use crate::lower::outline::OutlinePaint;
pub use crate::lower::shadow::ShadowSpec;

register_properties! {
    color           => Support::Implemented("zgui-paint::lower"),
    visibility      => Support::Implemented("zgui-paint::lower"),
    object_fit      => Support::Implemented("zgui-paint::emit::replaced"),
    object_position => Support::Implemented("zgui-paint::emit::replaced"),
}

/// Everything one computed style says about what it paints, with nothing geometric in it.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintStyle {
    /// Whether the box's own painting is drawn at all.
    ///
    /// `visibility: hidden` hides the box and not its subtree, so this suppresses the box's own
    /// primitives and nothing else — a visible descendant of a hidden box is still drawn.
    pub visible: bool,
    /// The element's own `color`, which text is drawn in and every `currentColor` resolved against.
    pub color: Color,
    /// What is painted behind the box's content.
    pub background: BackgroundStyle,
    /// How the border is drawn.
    pub border: BorderPaint,
    /// The `box-shadow` list, in the order written.
    pub shadows: SmallVec<[ShadowSpec; 2]>,
    /// The `text-shadow` list, in the order written.
    pub text_shadows: SmallVec<[ShadowSpec; 2]>,
    /// The outline, when the box draws one.
    pub outline: Option<OutlinePaint>,
    /// What decorates the text inside the box.
    pub decoration: crate::emit::text::DecorationStyle,
    /// The ramp the text inside the box is painted with, when it is painted with one.
    ///
    /// A brush that varies across a run is not something a coverage tile can carry, so a run
    /// carrying one leaves the atlas and is drawn as filled curves; see
    /// [`RasterPath`](zgui_text::RasterPath).
    pub text_fill: Option<crate::lower::background::GradientSpec>,
    /// How outlines the box draws are filled and stroked.
    ///
    /// Resolved here rather than where a drawing is emitted so that a thousand identically themed
    /// icons resolve one paint between them, and so that the same custom-property lookups are not
    /// repeated per outline of a chart with two hundred marks.
    pub shape: crate::emit::vector::ShapePaint,
    /// What makes the box composite on its own.
    pub group: GroupPaint,
    /// What the box cuts its own painting to.
    pub clip_path: ClipShape,
    /// Whether the box's transform properties force it into a target of its own.
    pub transform_forces_group: bool,
    /// How replaced content meets its content box: stretched, fitted, covering, or at its own
    /// size.
    pub object_fit: ObjectFitValue,
    /// Where fitted replaced content sits within its box.
    ///
    /// Held as the computed pair rather than resolved numbers, because a percentage here is a
    /// percentage of the box's leftover space — geometry, resolved where the content is emitted.
    pub object_position: ObjectPositionValue,
}

impl PaintStyle {
    /// Whether the box's own box decorations draw nothing.
    ///
    /// A box that paints nothing still has to be walked — its descendants may paint plenty — but it
    /// contributes no primitive, and skipping the emission is what keeps a document full of plain
    /// containers cheap.
    pub fn paints_nothing(&self) -> bool {
        !self.visible
            || (self.background.is_invisible()
                && self.border.invisible
                && self.shadows.is_empty()
                && self.outline.is_none())
    }

    /// Whether this style needs its subtree composited into a target of its own.
    ///
    /// Opacity is deliberately absent: whether a partly transparent subtree needs a boundary is a
    /// question about the subtree's *geometry*, answered against the fragment tree, and answering
    /// it here would mean answering it without the information.
    pub fn needs_group(&self) -> bool {
        self.group.needs_isolation() || self.clip_path.needs_target() || self.transform_forces_group
    }
}

/// Lowers one computed style at one device scale.
///
/// This is the expensive half — resolving colours, converting filter chains, walking a gradient's
/// stops — and it is what [`PaintStyleCache`](cache::PaintStyleCache) exists to perform once per
/// distinct style rather than once per element.
pub fn lower(style: &ComputedStyle, scale: f32) -> PaintStyle {
    let text_fill = background::text_fill(style);
    PaintStyle {
        visible: style.get_inherited_box().visibility == VisibilityValue::Visible,
        color: to_color(current(style)),
        // A background painting the text is a background the box does not paint: the ramp is drawn
        // once, cut to the letters, exactly as `background-clip: text` does it.
        background: if text_fill.is_some() {
            BackgroundStyle::default()
        } else {
            background::of(style)
        },
        border: border::of(style),
        shadows: shadow::box_shadows(style, scale),
        text_shadows: shadow::text_shadows(style, scale),
        outline: outline::of(style, scale),
        decoration: crate::emit::text::DecorationStyle::of(style, scale),
        text_fill,
        shape: crate::emit::vector::shape_paint(style, scale),
        group: filter::of(style, scale),
        clip_path: clip::of(style),
        transform_forces_group: transform::forces_group(style),
        object_fit: style.get_position().object_fit,
        object_position: style.get_position().object_position.clone(),
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::lower;

    #[test]
    fn the_initial_style_paints_nothing_and_needs_no_group() {
        let style = lower(&StyleDraft::initial().build(), 1.0);
        assert!(style.visible);
        assert!(
            style.paints_nothing(),
            "no background, border, shadow or outline"
        );
        assert!(!style.needs_group());
        assert_eq!(style.group.opacity, 1.0);
    }

    #[test]
    fn the_initial_colour_is_opaque_black() {
        let style = lower(&StyleDraft::initial().build(), 1.0);
        assert_eq!(style.color.to_premultiplied_srgb(), [0.0, 0.0, 0.0, 1.0]);
    }
}
