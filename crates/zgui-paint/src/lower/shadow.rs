//! `box-shadow` and `text-shadow`, lowered into offsets, blurs and colours.
//!
//! A shadow's geometry is entirely relative to the box that casts it — an offset, a spread and a
//! blur — so all of it survives the lowering cache. Where the shadow lands is decided when it is
//! emitted, from the box, and the ink it reaches is decided by layout from the same numbers.

use smallvec::SmallVec;
use zgui_color::Color;
use zgui_css::parity::Support;
use zgui_css::values::color::{AbsoluteColor, current, resolve};
use zgui_css::{ComputedStyle, register_properties};

register_properties! {
    box_shadow  => Support::Implemented("zgui-paint::lower::shadow"),
    text_shadow => Support::Implemented("zgui-paint::lower::shadow"),
}

/// One shadow, in device pixels, ready to be placed against a box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    /// How far right the shadow is offset.
    pub offset_x: f32,
    /// How far down it is offset.
    pub offset_y: f32,
    /// The blur's standard deviation, which is half the radius CSS writes.
    pub deviation: f32,
    /// How far the shadow's shape grows beyond the box, or shrinks inside it when negative.
    ///
    /// Always zero for a text shadow, which has no spread in the grammar.
    pub spread: f32,
    /// The shadow's colour.
    pub color: Color,
    /// Whether the shadow is cast inwards.
    pub inset: bool,
}

impl ShadowSpec {
    /// Whether the shadow would draw nothing at all.
    pub fn is_invisible(&self) -> bool {
        self.color.alpha() == 0.0
    }
}

/// The standard deviation a CSS blur radius means.
///
/// `blur(r)` is a Gaussian of standard deviation `r / 2` — the definition the filter specification
/// gives, and the one every shadow in every stylesheet was authored against.
pub fn deviation(radius: f32) -> f32 {
    radius.max(0.0) / 2.0
}

/// Lowers a style's `box-shadow` list, outermost first.
///
/// The list is painted in the order written, with the first on top, which is the order it arrives
/// in — so nothing is reversed here and nothing downstream has to know that it was not.
pub fn box_shadows(style: &ComputedStyle, scale: f32) -> SmallVec<[ShadowSpec; 2]> {
    let current = current(style);
    style
        .get_effects()
        .box_shadow
        .0
        .iter()
        .map(|shadow| ShadowSpec {
            offset_x: shadow.base.horizontal.px() * scale,
            offset_y: shadow.base.vertical.px() * scale,
            deviation: deviation(shadow.base.blur.0.px() * scale),
            spread: shadow.spread.px() * scale,
            color: shadow_color(&shadow.base.color, current),
            inset: shadow.inset,
        })
        .collect()
}

/// Lowers a style's `text-shadow` list.
///
/// A text shadow has no spread and is never inset; the grammar has neither.
pub fn text_shadows(style: &ComputedStyle, scale: f32) -> SmallVec<[ShadowSpec; 2]> {
    let current = current(style);
    style
        .get_inherited_text()
        .text_shadow
        .0
        .iter()
        .map(|shadow| ShadowSpec {
            offset_x: shadow.horizontal.px() * scale,
            offset_y: shadow.vertical.px() * scale,
            deviation: deviation(shadow.blur.0.px() * scale),
            spread: 0.0,
            color: shadow_color(&shadow.color, current),
            inset: false,
        })
        .collect()
}

/// A shadow's colour, which defaults to the element's own `color` when none is written.
fn shadow_color(color: &zgui_css::values::color::ColorValue, current: &AbsoluteColor) -> Color {
    resolve(color, current)
}

/// How far outside its own shape a shadow's painted extent reaches, in device pixels.
///
/// Taken from the alpha the shadow is drawn with, because a fainter shadow stops moving pixels
/// sooner. It is the same function the display list sizes a shadow's own rectangle with, called
/// rather than written twice: the two rectangles have to agree exactly or a shadow clips.
pub fn reach(spec: &ShadowSpec) -> f32 {
    zgui_scene::Shadow::blur_extent(spec.color.alpha()) * spec.deviation + spec.spread
}

#[cfg(test)]
mod tests {
    use zgui_color::Color;

    use super::{ShadowSpec, deviation, reach};

    #[test]
    fn a_blur_radius_is_twice_its_standard_deviation() {
        assert_eq!(deviation(8.0), 4.0);
        assert_eq!(deviation(0.0), 0.0);
        // A negative radius cannot be written, and clamping is what stops one arriving anyway from
        // shrinking a shadow's ink.
        assert_eq!(deviation(-4.0), 0.0);
    }

    #[test]
    fn reach_counts_both_the_blur_and_the_spread() {
        let spec = |color| ShadowSpec {
            offset_x: 0.0,
            offset_y: 0.0,
            deviation: 2.0,
            spread: 5.0,
            color,
            inset: false,
        };
        assert_eq!(
            reach(&spec(Color::BLACK)),
            zgui_scene::Shadow::blur_extent(1.0) * 2.0 + 5.0,
            "an opaque shadow's blur plus the spread"
        );
    }

    #[test]
    fn a_fainter_shadow_reaches_less_far_than_an_opaque_one() {
        // The spread is geometry and is unaffected; only the blur's reach follows the alpha. Stated
        // here as well as in `zgui-scene` because this is the rectangle the painter dilates by, and
        // it has to be the one the display list sizes the shadow's own bounds with.
        let spec = |alpha| ShadowSpec {
            offset_x: 0.0,
            offset_y: 0.0,
            deviation: 2.0,
            spread: 5.0,
            color: Color::srgb(0.0, 0.0, 0.0, alpha),
            inset: false,
        };
        assert!(reach(&spec(0.55)) < reach(&spec(1.0)));
        assert!(
            reach(&spec(0.0)) == 5.0,
            "a shadow that paints nothing still spreads by its spread"
        );
    }
}
