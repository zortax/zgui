//! `background-color` and `background-image`, lowered into what a fill needs.
//!
//! Only the parts that do not depend on the box are resolved here, because this is what the
//! lowering cache holds and that cache is shared by every element with the same style. A gradient's
//! *line* depends on the box it fills, so a gradient keeps its direction in CSS terms and its stop
//! positions unresolved, and both are settled where the gradient is emitted.

use smallvec::SmallVec;
use zgui_color::{Color, GradientStop, Interpolation};
use zgui_css::parity::Support;
use zgui_css::values::color::{AbsoluteColor, ColorValue, current, to_color, to_interpolation};
use zgui_css::values::image::{
    AngleOrPercentageValue, GradientFlags, GradientItemValue, GradientValue,
    HorizontalPositionKeyword, ImageValue, LineDirectionValue, VerticalPositionKeyword,
};
use zgui_css::values::length::{LengthPercentage, evaluate_at};
use zgui_css::{ComputedStyle, register_properties};
use zgui_geom::CssPx;

register_properties! {
    background_color      => Support::Implemented("zgui-paint::lower::background"),
    background_image      => Support::Implemented("zgui-paint::lower::background"),
    background_position_x => Support::Ignored("a background layer fills the box it is painted on"),
    background_position_y => Support::Ignored("a background layer fills the box it is painted on"),
    background_size       => Support::Ignored("a background layer fills the box it is painted on"),
    background_repeat     => Support::Ignored("a background layer fills the box it is painted on"),
    background_attachment => Support::Ignored("there is no separately scrolling background layer"),
    background_clip       => Support::Ignored("a background is painted to the border box"),
    background_origin     => Support::Ignored("a background layer fills the box it is painted on"),
    background_blend_mode => Support::Ignored("background layers do not blend with each other"),
}

/// Everything painted behind a box's own content.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundStyle {
    /// The colour painted under every layer.
    pub color: Color,
    /// The gradient layers, in the order they were written: the first is painted last, on top.
    pub layers: SmallVec<[GradientSpec; 1]>,
}

impl Default for BackgroundStyle {
    /// A background that paints nothing: no layers, and a fully transparent colour.
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            layers: SmallVec::new(),
        }
    }
}

impl BackgroundStyle {
    /// Whether this paints nothing at all.
    pub fn is_invisible(&self) -> bool {
        self.layers.is_empty() && self.color.alpha() == 0.0
    }
}

/// Which shape a gradient's ramp follows, in the terms CSS states it in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientShape {
    /// A ramp along a line at `angle` radians clockwise from pointing up.
    Linear {
        /// The gradient line's direction, in radians clockwise from twelve o'clock.
        angle: f32,
    },
    /// A ramp outwards from the box's centre, reaching its end at the farthest corner.
    Radial,
    /// A ramp swept from `from_angle` radians clockwise from twelve o'clock.
    Conic {
        /// Where the sweep starts, in radians.
        from_angle: f32,
    },
}

/// One gradient, with everything that does not depend on the box resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientSpec {
    /// The shape the ramp follows.
    pub shape: GradientShape,
    /// The stops, in the order they were written.
    pub stops: SmallVec<[SpecStop; 4]>,
    /// The space and hue direction the ramp is interpolated in.
    pub interpolation: Interpolation,
    /// Whether the ramp repeats outside its extent instead of clamping.
    pub repeating: bool,
}

/// One gradient stop before its position has been resolved against the gradient line.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecStop {
    /// The colour at this stop.
    pub color: Color,
    /// Where it sits, or `None` when it takes the even share between its neighbours.
    pub position: Option<StopOffset>,
}

/// Where a stop sits, in the form the gradient's own grammar states it in.
#[derive(Clone, Debug, PartialEq)]
pub enum StopOffset {
    /// A distance along the gradient line, or a percentage of it.
    Along(LengthPercentage),
    /// A fraction of the ramp, which is what an angle around a conic sweep resolves to.
    Fraction(f32),
}

impl StopOffset {
    /// The fraction of a gradient line of `length` CSS pixels this offset is at.
    ///
    /// A zero-length line is degenerate — there is no ramp to be a fraction of — and is evaluated
    /// against a line of one pixel instead. A percentage still answers the fraction it wrote; a
    /// distance answers past the end of the ramp, which is where a fixed distance along a line of
    /// no length is, and the ordering rule then keeps the stops in the sequence they were written.
    pub fn fraction(&self, length: CssPx) -> f32 {
        match self {
            Self::Fraction(fraction) => *fraction,
            Self::Along(value) if length.0 == 0.0 => evaluate_at(value, CssPx(1.0)).0,
            Self::Along(value) => evaluate_at(value, length).0 / length.0,
        }
    }
}

/// Lowers a style's background.
pub fn of(style: &ComputedStyle) -> BackgroundStyle {
    let background = style.get_background();
    let current = current(style);
    let mut layers: SmallVec<[GradientSpec; 1]> = SmallVec::new();
    for image in &*background.background_image.0 {
        if let ImageValue::Gradient(gradient) = image {
            layers.push(gradient_of(gradient, current));
        }
    }
    BackgroundStyle {
        color: to_color(&background.background_color.resolve_to_absolute(current)),
        layers,
    }
}

/// The custom property that makes a box's background paint the text inside it instead of the box.
///
/// It exists because `background-clip` is a property this engine build discards, so there is no
/// cascade result to read `text` out of. The value is the keyword [`TEXT_FILL_BACKGROUND`], the
/// property inherits like any custom property, and the ramp it names is the box's own
/// `background-image` — so a gradient heading is written with the two declarations everybody
/// already knows and one that says where the ramp goes.
pub const TEXT_FILL: &str = "zgui-text-fill";

/// The one value [`TEXT_FILL`] takes.
pub const TEXT_FILL_BACKGROUND: &str = "background";

/// The ramp the text inside a box is painted with, if any.
///
/// The *first* background layer, which is the one painted on top of the others and so the one a
/// reader would call the background. A box with no gradient layer paints its text in `color`
/// however the property is set: a solid colour needs no promotion off the atlas, and promoting one
/// would cost a run its hinting for a picture nobody could tell apart.
pub fn text_fill(style: &ComputedStyle) -> Option<GradientSpec> {
    let asked = zgui_css::values::custom::text(style, TEXT_FILL)?;
    if asked.trim() != TEXT_FILL_BACKGROUND {
        return None;
    }
    of(style).layers.into_iter().next()
}

/// Lowers one gradient.
fn gradient_of(gradient: &GradientValue, current: &AbsoluteColor) -> GradientSpec {
    match gradient {
        GradientValue::Linear {
            direction,
            color_interpolation_method,
            items,
            flags,
            ..
        } => GradientSpec {
            shape: GradientShape::Linear {
                angle: line_angle(direction),
            },
            stops: stops_of(items, current, along),
            interpolation: to_interpolation(color_interpolation_method),
            repeating: flags.contains(GradientFlags::REPEATING),
        },
        GradientValue::Radial {
            color_interpolation_method,
            items,
            flags,
            ..
        } => GradientSpec {
            shape: GradientShape::Radial,
            stops: stops_of(items, current, along),
            interpolation: to_interpolation(color_interpolation_method),
            repeating: flags.contains(GradientFlags::REPEATING),
        },
        GradientValue::Conic {
            angle,
            color_interpolation_method,
            items,
            flags,
            ..
        } => GradientSpec {
            shape: GradientShape::Conic {
                from_angle: angle.radians(),
            },
            stops: stops_of(items, current, around),
            interpolation: to_interpolation(color_interpolation_method),
            repeating: flags.contains(GradientFlags::REPEATING),
        },
    }
}

/// The angle a linear gradient's line points in, in radians clockwise from twelve o'clock.
///
/// A corner keyword's true angle depends on the box's proportions, which is deliberately not known
/// here: putting it in the lowered style would put a per-box number in a cache shared by every box
/// with this style. The diagonal of a square is used instead, which is exact for a square box and
/// within forty-five degrees for any other.
fn line_angle(direction: &LineDirectionValue) -> f32 {
    match direction {
        LineDirectionValue::Angle(angle) => angle.radians(),
        LineDirectionValue::Horizontal(HorizontalPositionKeyword::Left) => (270.0f32).to_radians(),
        LineDirectionValue::Horizontal(HorizontalPositionKeyword::Right) => (90.0f32).to_radians(),
        LineDirectionValue::Vertical(VerticalPositionKeyword::Top) => 0.0,
        LineDirectionValue::Vertical(VerticalPositionKeyword::Bottom) => core::f32::consts::PI,
        LineDirectionValue::Corner(horizontal, vertical) => {
            let across = match horizontal {
                HorizontalPositionKeyword::Left => -1.0f32,
                HorizontalPositionKeyword::Right => 1.0,
            };
            let down = match vertical {
                VerticalPositionKeyword::Top => 1.0f32,
                VerticalPositionKeyword::Bottom => -1.0,
            };
            across.atan2(down)
        }
    }
}

/// Lowers a gradient's items, dropping the interpolation hints.
///
/// A hint moves the midpoint between its neighbours rather than being a stop of its own, so
/// honouring one means re-parameterising the ramp. Dropping it draws the same colours with an even
/// midpoint — the same gradient with a different easing, rather than a different gradient.
fn stops_of<P>(
    items: &[GradientItemValue<ColorValue, P>],
    current: &AbsoluteColor,
    position: impl Fn(&P) -> StopOffset,
) -> SmallVec<[SpecStop; 4]> {
    items
        .iter()
        .filter_map(|item| match item {
            GradientItemValue::SimpleColorStop(color) => Some(SpecStop {
                color: to_color(&color.resolve_to_absolute(current)),
                position: None,
            }),
            GradientItemValue::ComplexColorStop {
                color,
                position: at,
            } => Some(SpecStop {
                color: to_color(&color.resolve_to_absolute(current)),
                position: Some(position(at)),
            }),
            GradientItemValue::InterpolationHint(_) => None,
        })
        .collect()
}

/// A linear or radial gradient's stop position.
fn along(position: &LengthPercentage) -> StopOffset {
    StopOffset::Along(position.clone())
}

/// A conic gradient's stop position, as a fraction of the whole turn.
fn around(position: &AngleOrPercentageValue) -> StopOffset {
    StopOffset::Fraction(match position {
        AngleOrPercentageValue::Percentage(percentage) => percentage.0,
        AngleOrPercentageValue::Angle(angle) => angle.radians() / (2.0 * core::f32::consts::PI),
    })
}

/// The stops of a gradient, resolved against a line of `length` CSS pixels and put in order.
///
/// CSS's own three rules are applied here and nowhere else: a stop with no position takes the even
/// share between its positioned neighbours, the two ends default to zero and one, and a stop never
/// sits before the one written ahead of it — which is what makes `red 60%, blue 20%` a hard edge at
/// sixty percent rather than a reordered ramp.
pub fn resolved_stops(spec: &GradientSpec, length: CssPx) -> SmallVec<[GradientStop; 4]> {
    let count = spec.stops.len();
    if count == 0 {
        return SmallVec::new();
    }
    let mut offsets: SmallVec<[Option<f32>; 4]> = spec
        .stops
        .iter()
        .map(|stop| stop.position.as_ref().map(|at| at.fraction(length)))
        .collect();
    if offsets[0].is_none() {
        offsets[0] = Some(0.0);
    }
    if offsets[count - 1].is_none() {
        offsets[count - 1] = Some(1.0);
    }
    let mut index = 1;
    while index < count {
        if offsets[index].is_some() {
            index += 1;
            continue;
        }
        let start = index - 1;
        let mut end = index;
        while offsets[end].is_none() {
            end += 1;
        }
        let from = offsets[start].unwrap_or(0.0);
        let to = offsets[end].unwrap_or(1.0);
        let steps = (end - start) as f32;
        for (step, slot) in offsets.iter_mut().enumerate().take(end).skip(index) {
            *slot = Some(from + (to - from) * ((step - start) as f32 / steps));
        }
        index = end;
    }
    let mut highest = f32::NEG_INFINITY;
    spec.stops
        .iter()
        .zip(offsets)
        .map(|(stop, offset)| {
            let offset = offset.unwrap_or(0.0).max(highest);
            highest = offset;
            GradientStop::new(offset, stop.color)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;
    use zgui_color::{Color, ColorSpace, Interpolation};
    use zgui_css::values::length::{Length, LengthPercentage, percent};
    use zgui_geom::CssPx;

    use super::{GradientShape, GradientSpec, SpecStop, StopOffset, resolved_stops};

    /// A gradient with the given stop positions and black stops throughout.
    fn spec(positions: [Option<StopOffset>; 4]) -> GradientSpec {
        GradientSpec {
            shape: GradientShape::Linear { angle: 0.0 },
            stops: positions
                .into_iter()
                .map(|position| SpecStop {
                    color: Color::BLACK,
                    position,
                })
                .collect(),
            interpolation: Interpolation::new(ColorSpace::Srgb),
            repeating: false,
        }
    }

    /// A stop at a distance in CSS pixels along the line.
    fn at(px: f32) -> Option<StopOffset> {
        Some(StopOffset::Along(LengthPercentage::new_length(
            Length::new(px),
        )))
    }

    /// A stop at a fraction of the line.
    fn fraction(value: f32) -> Option<StopOffset> {
        Some(StopOffset::Along(percent(value)))
    }

    #[test]
    fn positionless_stops_are_spread_evenly_between_their_neighbours() {
        let resolved = resolved_stops(&spec([None, None, None, None]), CssPx(100.0));
        let offsets: Vec<f32> = resolved.iter().map(|stop| stop.offset).collect();
        assert_eq!(offsets, vec![0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]);
    }

    #[test]
    fn a_length_position_is_a_fraction_of_the_gradient_line() {
        let resolved = resolved_stops(
            &spec([fraction(0.0), at(25.0), at(50.0), fraction(1.0)]),
            CssPx(100.0),
        );
        let offsets: Vec<f32> = resolved.iter().map(|stop| stop.offset).collect();
        assert_eq!(offsets, vec![0.0, 0.25, 0.5, 1.0]);
    }

    #[test]
    fn a_stop_never_sits_before_the_one_written_ahead_of_it() {
        let resolved = resolved_stops(
            &spec([fraction(0.6), fraction(0.2), fraction(0.3), fraction(1.0)]),
            CssPx(100.0),
        );
        let offsets: Vec<f32> = resolved.iter().map(|stop| stop.offset).collect();
        assert_eq!(offsets, vec![0.6, 0.6, 0.6, 1.0]);
    }

    #[test]
    fn a_zero_length_line_puts_every_distance_past_the_end_and_keeps_the_order() {
        let resolved = resolved_stops(
            &spec([fraction(0.0), at(20.0), at(30.0), fraction(1.0)]),
            CssPx(0.0),
        );
        let offsets: Vec<f32> = resolved.iter().map(|stop| stop.offset).collect();
        assert_eq!(offsets, vec![0.0, 20.0, 30.0, 30.0]);
        assert!(
            offsets.windows(2).all(|pair| pair[0] <= pair[1]),
            "a rasteriser walks these in order, whatever the geometry did"
        );
    }

    #[test]
    fn an_empty_gradient_resolves_to_no_stops() {
        let empty = GradientSpec {
            stops: smallvec![],
            ..spec([None, None, None, None])
        };
        assert!(resolved_stops(&empty, CssPx(100.0)).is_empty());
    }
}
