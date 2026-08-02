//! What a paint entry actually is.

use smallvec::SmallVec;
use zgui_atlas::AtlasTile;
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
use zgui_geom::{Device, DevicePx, Point, Rect};

use crate::content::{Content, ContentHash};
use crate::spatial::SpatialId;

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
    /// The discriminant, for hashing and for the shader.
    const fn tag(&self) -> u32 {
        match self {
            Self::Linear { .. } => 0,
            Self::Radial { .. } => 1,
            Self::Conic { .. } => 2,
        }
    }
}

/// One paint source: everything CSS can fill or stroke a shape with.
///
/// Gradients keep their stops and their interpolation space rather than being flattened to two
/// endpoints, so a ramp asked for in Oklch is interpolated in Oklch.
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
    /// A sampled image.
    Image {
        /// Where the decoded image lives.
        tile: AtlasTile,
        /// The rectangle one copy of the image covers.
        destination: Rect<DevicePx, Device>,
        /// How the image is mapped into that rectangle.
        transform: SpatialId,
        /// Whether copies repeat outside `destination`.
        repeating: bool,
    },
}

impl Paint {
    /// The discriminant a [`PaintRef`](crate::PaintRef) carries.
    pub const fn kind_tag(&self) -> u32 {
        match self {
            Self::Solid(_) => 0,
            Self::Gradient { .. } => 1,
            Self::Image { .. } => 2,
        }
    }

    /// The colour of a solid paint, or `None` for anything else.
    pub const fn solid_color(&self) -> Option<Color> {
        match self {
            Self::Solid(color) => Some(*color),
            _ => None,
        }
    }

    /// The one colour that stands in for this paint where a ramp cannot be evaluated per fragment.
    ///
    /// A solid paint is itself. A ramp is its mean colour along the gradient line, in sRGB, so that
    /// a shape filled with one is still drawn, still in roughly the right colour, by a consumer that
    /// can only fill flat — the alternative being a shape that is not drawn at all, which turns a
    /// gradient-filled icon or heading into a hole. A sampled image has no such stand-in.
    ///
    /// ```
    /// use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
    /// use zgui_geom::{Device, DevicePx, Point};
    /// use zgui_scene::{GradientKind, Paint};
    ///
    /// let black = Color::srgb(0.0, 0.0, 0.0, 1.0);
    /// assert_eq!(Paint::Solid(black).flat_color(), Some(black));
    ///
    /// let ramp = Paint::Gradient {
    ///     kind: GradientKind::Linear {
    ///         start: Point::new(DevicePx(0.0), DevicePx(0.0)),
    ///         end: Point::new(DevicePx(10.0), DevicePx(0.0)),
    ///     },
    ///     stops: [
    ///         GradientStop::new(0.0, Color::srgb(1.0, 0.0, 0.0, 1.0)),
    ///         GradientStop::new(1.0, Color::srgb(0.0, 0.0, 1.0, 1.0)),
    ///     ]
    ///     .into_iter()
    ///     .collect(),
    ///     space: ColorSpace::Srgb,
    ///     hue: HueInterpolation::Shorter,
    ///     repeating: false,
    /// };
    /// let mean = ramp.flat_color().expect("a ramp has a mean colour");
    /// assert_eq!(mean.components(), [0.5, 0.0, 0.5]);
    /// ```
    pub fn flat_color(&self) -> Option<Color> {
        match self {
            Self::Solid(color) => Some(*color),
            Self::Gradient { stops, .. } => mean_of(stops),
            Self::Image { .. } => None,
        }
    }
}

/// The mean colour along a ramp with these stops, in sRGB.
///
/// Each interval between two stops contributes the average of its endpoints, weighted by how much
/// of the ramp it covers; the flat stretches before the first stop and after the last contribute
/// their own colour over their own share. Stops are taken in the order they are held, which is the
/// order a ramp is defined in.
fn mean_of(stops: &[GradientStop]) -> Option<Color> {
    let srgb = |color: Color| {
        let converted = color.to_space(ColorSpace::Srgb);
        let [red, green, blue] = converted.components();
        [red, green, blue, converted.alpha()]
    };
    let first = stops.first()?;
    let last = stops.last()?;
    let mut total = [0.0f32; 4];
    let mut covered = 0.0f32;
    let mut add = |color: [f32; 4], weight: f32| {
        if weight <= 0.0 {
            return;
        }
        for channel in 0..4 {
            total[channel] += color[channel] * weight;
        }
        covered += weight;
    };
    add(srgb(first.color), first.offset.clamp(0.0, 1.0));
    add(srgb(last.color), 1.0 - last.offset.clamp(0.0, 1.0));
    for pair in stops.windows(2) {
        let span = (pair[1].offset - pair[0].offset).clamp(0.0, 1.0);
        let (one, two) = (srgb(pair[0].color), srgb(pair[1].color));
        add(
            [
                (one[0] + two[0]) * 0.5,
                (one[1] + two[1]) * 0.5,
                (one[2] + two[2]) * 0.5,
                (one[3] + two[3]) * 0.5,
            ],
            span,
        );
    }
    if covered <= 0.0 {
        // Every stop at one position: the ramp is that colour everywhere.
        return Some(first.color);
    }
    Some(Color::srgb(
        total[0] / covered,
        total[1] / covered,
        total[2] / covered,
        total[3] / covered,
    ))
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
            Self::Image {
                tile,
                destination,
                transform,
                repeating,
            } => hash
                .u32(tile.texture.index)
                .u32(tile.tile.0)
                .f32s(&[
                    destination.origin.x.0,
                    destination.origin.y.0,
                    destination.size.width.0,
                    destination.size.height.0,
                ])
                .u32(transform.index())
                .u32(u32::from(transform.generation().get()))
                .u32(u32::from(*repeating))
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
