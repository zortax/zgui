//! What the parser says fills a shape, in this crate's own paint vocabulary.

use smallvec::SmallVec;
use zgui_color::Color;

use crate::document::Unsupported;
use crate::document::gradient::{Gradient, GradientKind, Stop};
use crate::document::ink::Ink;
use crate::document::place::axis_scales;
use crate::document::shape::Paint;
use crate::parse::geometry;

/// The paint `source` is, placed by `placement`, or `None` for one this model cannot carry.
///
/// `opacity` is everything the document folded above this paint — the element's own fill or stroke
/// opacity and the opacity of every group it sits inside — and it multiplies the alpha rather than
/// replacing it, so a half-opaque colour inside a half-opaque group is a quarter opaque.
///
/// `placement` is the shape's own transform composed with everything above it, which is the same
/// matrix the outline was placed with. A ramp is defined in the same user space as the outline it
/// paints, so applying anything else here would slide the ramp off the shape.
pub(crate) fn of(
    source: &usvg::Paint,
    opacity: f32,
    placement: kurbo::Affine,
    unsupported: &mut Unsupported,
) -> Option<Paint> {
    Some(unfaded(source, placement, unsupported)?.faded(opacity))
}

/// The paint before any opacity above it is folded in.
fn unfaded(
    source: &usvg::Paint,
    placement: kurbo::Affine,
    unsupported: &mut Unsupported,
) -> Option<Paint> {
    match source {
        usvg::Paint::Color(color) => Some(Paint::Solid(Ink::Solid(solid(*color, 1.0)))),
        usvg::Paint::LinearGradient(ramp) => {
            let matrix = placement * geometry::affine(ramp.transform());
            let kind = GradientKind::Linear {
                start: matrix * kurbo::Point::new(f64::from(ramp.x1()), f64::from(ramp.y1())),
                end: matrix * kurbo::Point::new(f64::from(ramp.x2()), f64::from(ramp.y2())),
            };
            Some(Paint::Gradient(spread(
                ramp.spread_method(),
                kind,
                stops(ramp.stops()),
            )))
        }
        usvg::Paint::RadialGradient(ramp) => {
            let matrix = placement * geometry::affine(ramp.transform());
            let (x, y) = axis_scales(matrix);
            let radius = f64::from(ramp.r().get());
            let kind = GradientKind::Radial {
                center: matrix * kurbo::Point::new(f64::from(ramp.cx()), f64::from(ramp.cy())),
                radius_x: radius * x,
                radius_y: radius * y,
            };
            Some(Paint::Gradient(spread(
                ramp.spread_method(),
                kind,
                stops(ramp.stops()),
            )))
        }
        usvg::Paint::Pattern(_) => {
            unsupported.patterns += 1;
            None
        }
    }
}

/// One colour of the document, at the given extra alpha.
fn solid(color: usvg::Color, alpha: f32) -> Color {
    Color::srgb_u8(color.red, color.green, color.blue, 255).with_alpha(alpha)
}

/// The stops of a ramp, with each stop's own opacity folded into its colour.
fn stops(source: &[usvg::Stop]) -> SmallVec<[Stop; 4]> {
    source
        .iter()
        .map(|stop| Stop {
            offset: stop.offset().get(),
            color: Ink::Solid(solid(stop.color(), stop.opacity().get())),
        })
        .collect()
}

/// The ramp one spread method produces.
fn spread(method: usvg::SpreadMethod, kind: GradientKind, stops: SmallVec<[Stop; 4]>) -> Gradient {
    match method {
        usvg::SpreadMethod::Pad => Gradient::padded(kind, stops),
        usvg::SpreadMethod::Repeat => Gradient::repeating(kind, stops),
        usvg::SpreadMethod::Reflect => Gradient::reflecting(kind, stops),
    }
}
