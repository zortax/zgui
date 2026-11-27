//! Paint sources.

use smallvec::SmallVec;
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
use zgui_geom::{Device, DevicePx, Point};
use zgui_space::{Content, ContentHash};

/// Which shape a gradient's ramp follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    /// A ramp along the line from `start` to `end`.
    Linear {
        /// Where the ramp begins.
        start: Point<DevicePx, Device>,
        /// Where it ends.
        end: Point<DevicePx, Device>,
    },
    /// A ramp outwards from `center`, along an ellipse with the given radii.
    Radial {
        /// The centre.
        center: Point<DevicePx, Device>,
        /// The horizontal radius the ramp reaches its end at.
        radius_x: f32,
        /// The vertical radius the ramp reaches its end at.
        radius_y: f32,
    },
    /// A ramp swept around `center`, starting at `from_angle` radians clockwise from twelve
    /// o'clock.
    Conic {
        /// The centre.
        center: Point<DevicePx, Device>,
        /// Where the sweep starts, in radians.
        from_angle: f32,
    },
}

impl GradientKind {
    /// Returns the discriminant of the shape.
    const fn tag(&self) -> u32 {
        match self {
            Self::Linear { .. } => 0,
            Self::Radial { .. } => 1,
            Self::Conic { .. } => 2,
        }
    }
}

/// A paint source: what CSS fills or strokes a shape with.
///
/// A gradient keeps its stops and its interpolation space.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// One colour everywhere.
    Solid(Color),
    /// A ramp between colour stops.
    Gradient {
        /// The shape the ramp follows.
        kind: GradientKind,
        /// The stops, in increasing position order.
        stops: SmallVec<[GradientStop; 4]>,
        /// The space the ramp is interpolated in.
        space: ColorSpace,
        /// Which way round a hue circle a polar space interpolates.
        hue: HueInterpolation,
        /// Whether the ramp repeats outside its extent instead of clamping.
        repeating: bool,
    },
    /// One colour per side of a border stroke: top, right, bottom and left.
    Sides([Color; 4]),
}

impl Paint {
    /// Returns the discriminant of the paint's family.
    pub const fn kind_tag(&self) -> u32 {
        match self {
            Self::Solid(_) => 0,
            Self::Gradient { .. } => 1,
            Self::Sides(_) => 2,
        }
    }
}

impl Content for Paint {
    fn content_hash(&self) -> u64 {
        let hash = ContentHash::new().u32(self.kind_tag());
        match self {
            Self::Solid(color) => hash_color(hash, *color).finish(),
            Self::Gradient {
                kind,
                stops,
                space,
                hue,
                repeating,
            } => {
                let mut hash = hash.u32(kind.tag());
                hash = match kind {
                    GradientKind::Linear { start, end } => {
                        hash.f32s(&[start.x.0, start.y.0, end.x.0, end.y.0])
                    }
                    GradientKind::Radial {
                        center,
                        radius_x,
                        radius_y,
                    } => hash.f32s(&[center.x.0, center.y.0, *radius_x, *radius_y]),
                    GradientKind::Conic { center, from_angle } => {
                        hash.f32s(&[center.x.0, center.y.0, *from_angle])
                    }
                };
                for stop in stops {
                    hash = hash_color(hash, stop.color).f32(stop.offset);
                }
                hash.u32(*space as u32)
                    .u32(*hue as u32)
                    .u32(u32::from(*repeating))
                    .finish()
            }
            Self::Sides(colors) => colors
                .iter()
                .fold(hash, |hash, color| hash_color(hash, *color))
                .finish(),
        }
    }
}

/// Folds a colour's space and channels into a hash.
fn hash_color(hash: ContentHash, color: Color) -> ContentHash {
    hash.u32(color.space() as u32)
        .f32s(&color.components())
        .f32(color.alpha())
}
