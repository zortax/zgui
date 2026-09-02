//! What shape a box's corners are cut to, read off its computed style.
//!
//! # Why a custom property
//!
//! `corner-shape` is a CSS draft this build's style engine does not implement, and a custom
//! property's computed value is a token stream rather than a typed value — which is what makes it
//! the non-forking way to feed the engine something it has no property for. The same route
//! `zgui-fill` and `zgui-text-fill` take.
//!
//! # Why it is read here
//!
//! Two stages need the answer and neither may ask the other. Layout needs it to cut the clip a
//! box gives its children, so that content inside a squircle is kept inside the squircle. Paint
//! needs it for the background, the border, the shadow and the outline. Reading it in one place
//! that both depend on is what stops the four disagreeing.

use zgui_css::ComputedStyle;
use zgui_css::values::custom;
use zgui_scene::CornerShape;

/// The property a box names its corner shape through.
pub const CORNER_SHAPE: &str = "zgui-corner-shape";

/// The shape `style` cuts its corners to.
///
/// [`CornerShape::ROUND`] — the ellipse a corner radius has always drawn — for a box that names
/// nothing, and for one whose value is not a shape. An unreadable value keeps the appearance the
/// box would have had, which is the same thing an unknown property does anywhere in CSS.
pub fn shape(style: &ComputedStyle) -> CornerShape {
    let Some(written) = custom::text(style, CORNER_SHAPE) else {
        return CornerShape::ROUND;
    };
    parse(written).unwrap_or(CornerShape::ROUND)
}

/// The shape one written value names.
///
/// The keywords are the CSS draft's, and every one of them is an exponent: a corner is the
/// superellipse `|x/rx|^n + |y/ry|^n = 1`, and the names are places along `n`.
fn parse(written: &str) -> Option<CornerShape> {
    let written = written.trim();
    if written.eq_ignore_ascii_case("round") || written.eq_ignore_ascii_case("none") {
        return Some(CornerShape::ROUND);
    }
    if written.eq_ignore_ascii_case("squircle") {
        return Some(CornerShape::SQUIRCLE);
    }
    if written.eq_ignore_ascii_case("bevel") {
        return Some(CornerShape::BEVEL);
    }
    if written.eq_ignore_ascii_case("scoop") {
        return Some(CornerShape::SCOOP);
    }
    if written.eq_ignore_ascii_case("notch") || written.eq_ignore_ascii_case("square") {
        return Some(CornerShape::NOTCH);
    }
    // `superellipse(<number>)`, and a bare number as the same thing written shorter.
    let inner = written
        .strip_prefix("superellipse(")
        .or_else(|| written.strip_prefix("SUPERELLIPSE("))
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or(written);
    inner.trim().parse::<f32>().ok().map(CornerShape::new)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use zgui_scene::CornerShape;

    #[test]
    fn every_keyword_the_draft_names_is_a_place_along_the_exponent() {
        assert_eq!(parse("round"), Some(CornerShape::ROUND));
        assert_eq!(parse("bevel"), Some(CornerShape::BEVEL));
        assert_eq!(parse("squircle"), Some(CornerShape::SQUIRCLE));
        assert_eq!(parse("scoop"), Some(CornerShape::SCOOP));
        assert_eq!(parse("notch"), Some(CornerShape::NOTCH));
    }

    /// A subtree opts out of a shape it inherited the way it opts out of an effect: by writing
    /// `none`, which is the shape a corner radius has always had.
    #[test]
    fn none_is_the_shape_a_corner_radius_always_had() {
        assert_eq!(parse("none"), Some(CornerShape::ROUND));
    }

    #[test]
    fn a_keyword_is_read_however_it_is_capitalised() {
        assert_eq!(parse("Squircle"), Some(CornerShape::SQUIRCLE));
        assert_eq!(parse("  SCOOP  "), Some(CornerShape::SCOOP));
    }

    #[test]
    fn an_exponent_can_be_written_out_in_full_or_on_its_own() {
        assert_eq!(parse("superellipse(4)"), Some(CornerShape::SQUIRCLE));
        assert_eq!(parse("superellipse( 2.5 )").map(|s| s.get()), Some(2.5));
        assert_eq!(parse("3"), Some(CornerShape::new(3.0)));
    }

    /// A value that is not a shape leaves the box the shape it would have had, rather than
    /// becoming a degenerate exponent the shading code has to defend against.
    #[test]
    fn a_value_that_is_not_a_shape_names_nothing() {
        assert_eq!(parse("wobbly"), None);
        assert_eq!(parse("superellipse(wobbly)"), None);
        assert_eq!(parse(""), None);
    }
}
