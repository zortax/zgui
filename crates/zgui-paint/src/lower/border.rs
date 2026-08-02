//! The four border sides and the four corners.
//!
//! Widths are already resolved and snapped onto the fragment, because layout had to resolve them to
//! know where the content box was; what is lowered here is what layout has no reason to look at —
//! the colours and the line styles.
//!
//! Corner radii are *not* lowered, and that is deliberate: a percentage radius is a percentage of
//! the box's own extent, so two boxes with one style have different radii and a cache shared
//! between them cannot hold either. [`radii_of`] resolves them against a box, once, through the
//! same function that decides what a box clips its children to — so a background painted to a curve
//! and a child clipped to it cannot round differently.

use zgui_color::Color;
use zgui_css::parity::Support;
use zgui_css::values::border::BorderStyleValue;
use zgui_css::values::color::{ColorValue, current, resolve};
use zgui_css::{ComputedStyle, register_properties};
use zgui_geom::{Corners, Device, DevicePx, Rect, Vec2};
use zgui_scene::prim::quad::BorderStyle;

register_properties! {
    border_top_color           => Support::Implemented("zgui-paint::lower::border"),
    border_right_color         => Support::Implemented("zgui-paint::lower::border"),
    border_bottom_color        => Support::Implemented("zgui-paint::lower::border"),
    border_left_color          => Support::Implemented("zgui-paint::lower::border"),
    border_top_style           => Support::Implemented("zgui-paint::lower::border"),
    border_right_style         => Support::Implemented("zgui-paint::lower::border"),
    border_bottom_style        => Support::Implemented("zgui-paint::lower::border"),
    border_left_style          => Support::Implemented("zgui-paint::lower::border"),
    border_top_left_radius     => Support::Implemented("zgui-paint::lower::border"),
    border_top_right_radius    => Support::Implemented("zgui-paint::lower::border"),
    border_bottom_right_radius => Support::Implemented("zgui-paint::lower::border"),
    border_bottom_left_radius  => Support::Implemented("zgui-paint::lower::border"),
    border_image_source        => Support::Ignored("border images are not painted"),
    border_image_slice         => Support::Ignored("border images are not painted"),
    border_image_width         => Support::Ignored("border images are not painted"),
    border_image_outset        => Support::Ignored("border images are not painted"),
    border_image_repeat        => Support::Ignored("border images are not painted"),
}

/// How the four sides of a box's border are drawn.
///
/// One line style is carried rather than four, because a quad is drawn by one shader with one dash
/// pattern running round it. The four sides' styles are reduced to that one: `dotted` or `dashed`
/// anywhere wins over `solid`, because those two change the shape of the line rather than only its
/// shade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderPaint {
    /// The colours of the top, right, bottom and left sides, in that order.
    pub colors: [Color; 4],
    /// How the line is drawn.
    pub style: LineStyle,
    /// Whether every side that has a width is invisible — `none`, `hidden`, or fully transparent.
    pub invisible: bool,
}

impl Default for BorderPaint {
    /// A border that draws nothing: four fully transparent sides.
    fn default() -> Self {
        Self {
            colors: [Color::TRANSPARENT; 4],
            style: LineStyle::Solid,
            invisible: true,
        }
    }
}

/// How a border line is drawn, including the two CSS styles that are drawn as something else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineStyle {
    /// One continuous line.
    #[default]
    Solid,
    /// Dashes with gaps, continuing round the corners.
    Dashed,
    /// Round dots with gaps.
    Dotted,
}

impl LineStyle {
    /// The scene's own spelling of this style.
    pub fn to_scene(self) -> BorderStyle {
        match self {
            Self::Solid => BorderStyle::Solid,
            Self::Dashed => BorderStyle::Dashed,
            Self::Dotted => BorderStyle::Dotted,
        }
    }
}

/// Lowers a style's border paint.
pub fn of(style: &ComputedStyle) -> BorderPaint {
    let border = style.get_border();
    let current = current(style);
    let sides = [
        (border.border_top_style, &border.border_top_color),
        (border.border_right_style, &border.border_right_color),
        (border.border_bottom_style, &border.border_bottom_color),
        (border.border_left_style, &border.border_left_color),
    ];
    let colors = sides.map(|(line, color)| side_color(line, color, current));
    BorderPaint {
        colors,
        style: dominant(sides.map(|(line, _)| line)),
        invisible: colors.iter().all(|color| color.alpha() == 0.0),
    }
}

/// One side's colour, which is fully transparent when the side draws no line at all.
///
/// The width of a `none` or `hidden` side is already zero on the fragment, so this is belt and
/// braces — but a transparent colour is what makes the *quad* stop drawing, and the quad carries
/// one stroke for four sides.
fn side_color(
    line: BorderStyleValue,
    color: &ColorValue,
    current: &zgui_css::values::color::AbsoluteColor,
) -> Color {
    match line {
        BorderStyleValue::None | BorderStyleValue::Hidden => Color::TRANSPARENT,
        _ => resolve(color, current),
    }
}

/// The one line style a quad draws with, chosen from the four sides'.
///
/// `dashed` and `dotted` are the two that change the shape of the line rather than only its shade,
/// so either of them anywhere wins over `solid`; the rest — `double`, `groove`, `ridge`, `inset`,
/// `outset` — are drawn as a solid line of the same width, which is what a border of one pixel
/// looks like in every engine anyway.
fn dominant(styles: [BorderStyleValue; 4]) -> LineStyle {
    if styles.contains(&BorderStyleValue::Dotted) {
        return LineStyle::Dotted;
    }
    if styles.contains(&BorderStyleValue::Dashed) {
        return LineStyle::Dashed;
    }
    LineStyle::Solid
}

/// The four elliptical corner radii of a box's border box, in device pixels.
///
/// This is the layout stage's own resolution rather than a second one: a percentage radius is a
/// percentage of the box's extent on that axis, the two radii of a corner may differ, and radii
/// that would overlap are shrunk together rather than clamped one at a time. A second
/// implementation here would let a background curve and a clip curve disagree.
pub fn radii_of(
    style: &ComputedStyle,
    border_box: Rect<DevicePx, Device>,
    scale: f32,
) -> Corners<Vec2<DevicePx>> {
    zgui_layout::fragment::clip::radii(style, border_box, scale)
}

/// The four corner radii of the *inner* edge of a border: the outer radii less the border widths.
///
/// A curve inside a border is concentric with it rather than parallel, so the width is subtracted
/// from the radius. A radius smaller than the border it sits inside collapses to a square corner.
pub fn inner_radii(
    radii: Corners<Vec2<DevicePx>>,
    border: zgui_geom::Edges<DevicePx>,
) -> Corners<Vec2<DevicePx>> {
    let inset = |radius: Vec2<DevicePx>, horizontal: DevicePx, vertical: DevicePx| {
        Vec2::new(
            DevicePx((radius.x.0 - horizontal.0).max(0.0)),
            DevicePx((radius.y.0 - vertical.0).max(0.0)),
        )
    };
    Corners {
        top_left: inset(radii.top_left, border.left, border.top),
        top_right: inset(radii.top_right, border.right, border.top),
        bottom_right: inset(radii.bottom_right, border.right, border.bottom),
        bottom_left: inset(radii.bottom_left, border.left, border.bottom),
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::values::border::BorderStyleValue;
    use zgui_geom::{Corners, DevicePx, Edges, Vec2};

    use super::{LineStyle, dominant, inner_radii};

    #[test]
    fn a_dotted_side_makes_the_whole_border_dotted() {
        let styles = [
            BorderStyleValue::Solid,
            BorderStyleValue::Dotted,
            BorderStyleValue::Solid,
            BorderStyleValue::Dashed,
        ];
        assert_eq!(dominant(styles), LineStyle::Dotted);
    }

    #[test]
    fn the_styles_drawn_as_a_solid_line_are_a_solid_line() {
        for style in [
            BorderStyleValue::Double,
            BorderStyleValue::Groove,
            BorderStyleValue::Ridge,
            BorderStyleValue::Inset,
            BorderStyleValue::Outset,
            BorderStyleValue::None,
        ] {
            assert_eq!(dominant([style; 4]), LineStyle::Solid, "{style:?}");
        }
    }

    #[test]
    fn an_inner_radius_is_the_outer_one_less_the_border_and_never_negative() {
        let radii = Corners::uniform(Vec2::new(DevicePx(6.0), DevicePx(10.0)));
        let border = Edges {
            top: DevicePx(4.0),
            right: DevicePx(4.0),
            bottom: DevicePx(4.0),
            left: DevicePx(20.0),
        };
        let inner = inner_radii(radii, border);
        assert_eq!(inner.top_right, Vec2::new(DevicePx(2.0), DevicePx(6.0)));
        assert_eq!(
            inner.top_left,
            Vec2::new(DevicePx(0.0), DevicePx(6.0)),
            "a radius narrower than its border collapses rather than going negative"
        );
    }
}
