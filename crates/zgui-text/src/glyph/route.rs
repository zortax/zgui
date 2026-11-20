//! Which of the two drawing paths a run of glyphs takes.
//!
//! # Why the answer belongs to the run
//!
//! Both paths draw the same glyphs and neither is a fallback for the other. The atlas rasterises
//! once on the processor and draws a quad per glyph, which is what makes a page of body text cost
//! almost nothing and is the only path that can be hinted; outlines are filled by the frame's path
//! rasteriser, which costs more per glyph and is the only path that survives a rotation, a large
//! scale, a size no cache should hold, or a brush that is not one colour.
//!
//! So the choice is a property of *the run and the surface it lands on*, decided once where both
//! are known, and carried with the run. A renderer that decided it for itself would decide it
//! differently from the stage that measured the ink, and the two disagreeing is a run drawn twice
//! or not at all.

use zgui_geom::Affine2;

use crate::glyph::run::ShapedRun;

/// The largest size, in CSS pixels, a run may be and still go through the glyph atlas.
///
/// Above it the tiles stop being worth caching: a handful of display-sized headings would evict
/// every body-text glyph in the atlas between them, and the outlines are cheap to fill directly at
/// that size.
///
/// Stated in CSS pixels because that is the size a document asked for. A cap in device pixels is a
/// different cap on every display — the same heading is atlas text on one monitor and filled
/// outlines on the next, which is the one thing a size threshold must not be.
pub const ATLAS_MAX_CSS_SIZE: f32 = 96.0;

/// The largest coverage tile, in device pixels, anything in this engine caches.
///
/// The same number a vector mask may have on one edge, and for the same reason: one byte per texel
/// makes it sixty-four kibibytes, and several of those fit one atlas page. It is what stops a dense
/// display from turning the cap above into tiles nothing can hold.
pub const ATLAS_MAX_SIZE: f32 = 256.0;

/// The largest run, in device pixels, the atlas serves at `scale` device pixels per CSS pixel.
///
/// 96 at one, 192 at two, 256 from three upwards.
fn atlas_max_size(scale: f32) -> f32 {
    (ATLAS_MAX_CSS_SIZE * scale.max(1.0)).min(ATLAS_MAX_SIZE)
}

/// The most an upright scale may stretch or shrink a cached tile, on each axis.
///
/// A tile resampled by a factor between four fifths and five quarters stays within a texel of the
/// exact raster. That covers the scale a card, a dialog or a pressed control animates through, so
/// such an animation draws from tiles that stay in the atlas. Past it, a resampled tile is visibly
/// soft or aliased, and the outlines are filled at the scale instead.
///
/// A resampled tile is softer and lighter than the exact raster at every factor. That passes
/// unseen while the scale moves and shows once it rests, so only a run whose transform an
/// animation moves draws from resampled tiles; see [`RunSurface::moving`].
pub const ATLAS_MAX_STRETCH: f32 = 1.25;

/// What the transform in force does to a run's glyphs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RunTransform {
    /// A translation. It moves a cached tile and changes no texel.
    Translation,
    /// A positive scale on each axis and a translation. The run stays upright, and a cached tile
    /// is resampled by the two factors.
    Upright {
        /// The factor along the x axis.
        x: f32,
        /// The factor along the y axis.
        y: f32,
    },
    /// A turn, a skew, a flip or a projection.
    Other,
}

impl RunTransform {
    /// What `affine` does to a run.
    ///
    /// ```
    /// use zgui_geom::Affine2;
    /// use zgui_text::RunTransform;
    ///
    /// assert_eq!(RunTransform::of(Affine2::translation(3.5, 0.0)), RunTransform::Translation);
    /// assert_eq!(
    ///     RunTransform::of(Affine2::scale(0.96, 0.96)),
    ///     RunTransform::Upright { x: 0.96, y: 0.96 }
    /// );
    /// assert_eq!(RunTransform::of(Affine2::rotation(0.1)), RunTransform::Other);
    /// assert_eq!(RunTransform::of(Affine2::scale(-1.0, 1.0)), RunTransform::Other);
    /// ```
    pub fn of(affine: Affine2) -> Self {
        if affine.is_translation() {
            Self::Translation
        } else if affine.b == 0.0 && affine.c == 0.0 && affine.a > 0.0 && affine.d > 0.0 {
            Self::Upright {
                x: affine.a,
                y: affine.d,
            }
        } else {
            Self::Other
        }
    }

    /// Whether a cached tile drawn through this transform stays close to the exact raster.
    ///
    /// True for a translation, and for an upright scale whose factors are both within
    /// [`ATLAS_MAX_STRETCH`] of one. See [`RunSurface::keeps_tiles`] for whether a run draws from
    /// tiles, which also asks whether the transform moves.
    pub fn keeps_tiles(self) -> bool {
        let near = |factor: f32| (1.0 / ATLAS_MAX_STRETCH..=ATLAS_MAX_STRETCH).contains(&factor);
        match self {
            Self::Translation => true,
            Self::Upright { x, y } => near(x) && near(y),
            Self::Other => false,
        }
    }
}

/// What the surface a run lands on does to it.
///
/// Both fields are things the run cannot know and whoever is drawing it cannot avoid knowing: the
/// transform in force and the brush the run is painted with.
///
/// A clip is deliberately not among them. Every clip this pipeline can express — a rounded
/// rectangle, or a mask sampled from a tile — is evaluated per pixel by whatever draws the run,
/// and a quad reading a coverage tile is cut by one exactly as well as a filled curve is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunSurface {
    /// What the transform in force does to the run.
    ///
    /// A translation moves already rasterised pixels, and everything else resamples them. A
    /// cached tile survives a small upright resample and nothing more; see
    /// [`RunTransform::keeps_tiles`].
    pub transform: RunTransform,
    /// Whether an animation moves the transform in force.
    ///
    /// A run under a slight scale draws from cached tiles only while this is true. At rest its
    /// outlines are filled at the scale, which is the exact raster.
    pub moving: bool,
    /// Whether the run's brush is a single colour rather than a gradient or an image.
    ///
    /// A coverage tile carries no colour of its own: it is multiplied by one, so a brush that
    /// varies across the run has nothing to multiply.
    pub solid_brush: bool,
    /// How many device pixels one CSS pixel is.
    ///
    /// A run states its size in device pixels, so this is what turns that back into the size the
    /// document asked for — and the size threshold is about what a document asked for.
    pub scale: f32,
}

impl RunSurface {
    /// An upright run of one colour at one device pixel per CSS pixel — what nearly every run is.
    pub const PLAIN: Self = Self {
        transform: RunTransform::Translation,
        moving: false,
        solid_brush: true,
        scale: 1.0,
    };

    /// Whether a run on this surface draws from cached tiles as far as its transform decides.
    ///
    /// True under a translation. True under a slight upright scale only while an animation moves
    /// the transform.
    ///
    /// ```
    /// use zgui_text::{RunSurface, RunTransform};
    ///
    /// let scaled = RunSurface {
    ///     transform: RunTransform::Upright { x: 0.96, y: 0.96 },
    ///     ..RunSurface::PLAIN
    /// };
    /// assert!(!scaled.keeps_tiles(), "a scale at rest is filled from outlines");
    /// assert!(RunSurface { moving: true, ..scaled }.keeps_tiles());
    /// assert!(RunSurface::PLAIN.keeps_tiles());
    /// ```
    pub fn keeps_tiles(&self) -> bool {
        match self.transform {
            RunTransform::Translation => true,
            transform => self.moving && transform.keeps_tiles(),
        }
    }
}

/// What a run of glyphs is about to be drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunProfile {
    /// The size the run is drawn at, in device pixels.
    pub size: f32,
    /// Whether the face carries colour glyphs.
    pub color_glyphs: bool,
    /// What the surface does to it.
    pub surface: RunSurface,
}

impl RunProfile {
    /// The profile of ordinary body text: small, upright, one colour.
    pub const BODY_TEXT: Self = Self {
        size: 16.0,
        color_glyphs: false,
        surface: RunSurface::PLAIN,
    };
}

/// How a run's glyphs reach the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RasterPath {
    /// Rasterised on the processor into atlas tiles and drawn as one quad per glyph.
    ///
    /// The only path that can deliver hinted, subpixel-antialiased text, and the one static text
    /// takes — which in a component library is nearly all of it.
    Atlas,
    /// Filled as outlines in the frame's vector scene.
    ///
    /// What everything the atlas cannot serve takes: sizes too large to cache, transforms that
    /// turn the run or scale it past [`ATLAS_MAX_STRETCH`], and brushes that are not one colour.
    Vector,
}

impl RasterPath {
    /// The path a run takes.
    ///
    /// A colour run takes the atlas whatever else is true, and that is not a heuristic: a colour
    /// glyph is a picture — layered outlines, or a bitmap strike — and there is no single outline
    /// to fill. Sending one down the vector path would draw its first layer's silhouette in the
    /// text colour, which is a worse picture than a resampled one.
    ///
    /// ```
    /// use zgui_text::{RasterPath, RunProfile, RunSurface, RunTransform};
    ///
    /// assert_eq!(RasterPath::of(&RunProfile::BODY_TEXT), RasterPath::Atlas);
    ///
    /// let heading = RunProfile { size: 128.0, ..RunProfile::BODY_TEXT };
    /// assert_eq!(RasterPath::of(&heading), RasterPath::Vector);
    ///
    /// // The same heading on a display of twice the density is the same heading: ninety-six CSS
    /// // pixels is a hundred and ninety-two device ones, and the atlas still serves it.
    /// let dense = RunProfile {
    ///     size: 128.0,
    ///     surface: RunSurface { scale: 2.0, ..RunSurface::PLAIN },
    ///     ..RunProfile::BODY_TEXT
    /// };
    /// assert_eq!(RasterPath::of(&dense), RasterPath::Atlas);
    ///
    /// // A card that opens from 96 % draws its text from the same tiles at every step.
    /// let opening = RunProfile {
    ///     surface: RunSurface {
    ///         transform: RunTransform::Upright { x: 0.96, y: 0.96 },
    ///         moving: true,
    ///         ..RunSurface::PLAIN
    ///     },
    ///     ..RunProfile::BODY_TEXT
    /// };
    /// assert_eq!(RasterPath::of(&opening), RasterPath::Atlas);
    ///
    /// // A card that rests at 96 % has its text filled at that scale.
    /// let resting = RunProfile {
    ///     surface: RunSurface { moving: false, ..opening.surface },
    ///     ..opening
    /// };
    /// assert_eq!(RasterPath::of(&resting), RasterPath::Vector);
    ///
    /// let turned = RunProfile {
    ///     surface: RunSurface { transform: RunTransform::Other, ..RunSurface::PLAIN },
    ///     ..RunProfile::BODY_TEXT
    /// };
    /// assert_eq!(RasterPath::of(&turned), RasterPath::Vector);
    ///
    /// // Emoji at any size, under any transform, stay pictures.
    /// let emoji = RunProfile { color_glyphs: true, ..turned };
    /// assert_eq!(RasterPath::of(&emoji), RasterPath::Atlas);
    /// ```
    pub fn of(profile: &RunProfile) -> Self {
        if profile.color_glyphs {
            return Self::Atlas;
        }
        let surface = profile.surface;
        let cacheable = profile.size <= atlas_max_size(surface.scale)
            && surface.keeps_tiles()
            && surface.solid_brush;
        if cacheable { Self::Atlas } else { Self::Vector }
    }
}

impl ShapedRun<'_> {
    /// What this run is about to be drawn with, on a surface that does `surface` to it.
    pub fn profile(&self, surface: RunSurface) -> RunProfile {
        RunProfile {
            size: self.size,
            color_glyphs: self.has_color,
            surface,
        }
    }

    /// How this run's glyphs reach the screen.
    ///
    /// ```
    /// use zgui_scene::PaintSlot;
    /// use zgui_text::{FaceId, RasterPath, RunSurface, ShapedRun, VariationInstance};
    ///
    /// let run = ShapedRun {
    ///     face: FaceId(0),
    ///     size: 160.0,
    ///     instance: VariationInstance::DEFAULT,
    ///     synthetic_bold: 0.0,
    ///     synthetic_slant: 0.0,
    ///     has_color: false,
    ///     brush: PaintSlot(0),
    ///     glyphs: &[],
    ///     text_start: 0,
    /// };
    /// assert_eq!(run.raster_path(RunSurface::PLAIN), RasterPath::Vector);
    /// ```
    pub fn raster_path(&self, surface: RunSurface) -> RasterPath {
        RasterPath::of(&self.profile(surface))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ATLAS_MAX_CSS_SIZE, ATLAS_MAX_SIZE, ATLAS_MAX_STRETCH, RasterPath, RunProfile, RunSurface,
        RunTransform, atlas_max_size,
    };

    #[test]
    fn the_atlas_keeps_what_it_can_serve_and_gives_up_the_rest() {
        assert_eq!(RasterPath::of(&RunProfile::BODY_TEXT), RasterPath::Atlas);
        assert_eq!(
            RasterPath::of(&RunProfile {
                size: ATLAS_MAX_CSS_SIZE,
                ..RunProfile::BODY_TEXT
            }),
            RasterPath::Atlas,
            "the limit is the largest size the atlas still serves"
        );

        let leaving = [
            RunProfile {
                size: ATLAS_MAX_CSS_SIZE + 1.0,
                ..RunProfile::BODY_TEXT
            },
            RunProfile {
                surface: RunSurface {
                    transform: RunTransform::Other,
                    ..RunSurface::PLAIN
                },
                ..RunProfile::BODY_TEXT
            },
            RunProfile {
                surface: RunSurface {
                    solid_brush: false,
                    ..RunSurface::PLAIN
                },
                ..RunProfile::BODY_TEXT
            },
        ];
        for profile in leaving {
            assert_eq!(RasterPath::of(&profile), RasterPath::Vector, "{profile:?}");
        }
    }

    /// The threshold is a size a document asked for, so it says the same thing on every display.
    ///
    /// Stated in device pixels it did the opposite: the same heading was atlas text at one device
    /// pixel per CSS pixel and filled outlines at two, so a component library's own type scale
    /// changed which rasteriser it used according to the monitor it was dragged onto.
    ///
    /// The device cap is a second answer and not a hedge. A tile has to be a size an atlas page can
    /// hold whatever a document asked for, so past about two and a half device pixels per CSS pixel
    /// it is the cap that answers — and it answers for a heading at the very top of the scale,
    /// which is the case the outlines were always going to be cheaper for.
    #[test]
    fn the_same_heading_takes_the_same_path_on_every_display() {
        let at = |scale: f32, css: f32| {
            RasterPath::of(&RunProfile {
                size: css * scale,
                surface: RunSurface {
                    scale,
                    ..RunSurface::PLAIN
                },
                ..RunProfile::BODY_TEXT
            })
        };
        for scale in [1.0, 1.25, 1.5, 2.0, 3.0, 4.0] {
            // Every size a component library actually sets, at every density a display reports.
            for css in [12.0, 16.0, 24.0, 30.0, 48.0, 64.0] {
                assert_eq!(at(scale, css), RasterPath::Atlas, "{css} css px at {scale}");
            }
        }
        // The very top of the scale, up to where a tile stops being a size a page can hold.
        assert_eq!(at(2.0, ATLAS_MAX_CSS_SIZE), RasterPath::Atlas);
        assert_eq!(at(3.0, ATLAS_MAX_CSS_SIZE), RasterPath::Vector);

        assert_eq!(atlas_max_size(1.0), ATLAS_MAX_CSS_SIZE);
        assert_eq!(atlas_max_size(2.0), 192.0);
        assert_eq!(atlas_max_size(4.0), ATLAS_MAX_SIZE);
        assert_eq!(atlas_max_size(0.5), ATLAS_MAX_CSS_SIZE, "never below one");
    }

    /// An opening card animates its scale through every frame. Tiles stretched by a few percent
    /// keep its text off the path rasteriser, and a large scale fills the outlines at that scale.
    #[test]
    fn a_slight_upright_scale_in_motion_keeps_the_atlas_and_a_large_one_leaves_it() {
        let path = |x: f32, y: f32, moving: bool| {
            RasterPath::of(&RunProfile {
                surface: RunSurface {
                    transform: RunTransform::Upright { x, y },
                    moving,
                    ..RunSurface::PLAIN
                },
                ..RunProfile::BODY_TEXT
            })
        };
        let under = |x: f32, y: f32| path(x, y, true);
        // A resampled tile is soft, which shows once the scale rests.
        for factor in [0.8, 0.96, 1.05, ATLAS_MAX_STRETCH] {
            assert_eq!(path(factor, factor, false), RasterPath::Vector, "{factor}");
        }
        for factor in [0.8, 0.94, 0.96, 1.0, 1.05, ATLAS_MAX_STRETCH] {
            assert_eq!(under(factor, factor), RasterPath::Atlas, "{factor}");
        }
        assert_eq!(under(0.96, 1.0), RasterPath::Atlas);
        for factor in [0.5, 0.79, 1.26, 2.0] {
            assert_eq!(under(factor, factor), RasterPath::Vector, "{factor}");
        }
        assert_eq!(
            under(1.0, 2.0),
            RasterPath::Vector,
            "one axis past the limit is enough"
        );
    }

    #[test]
    fn a_transform_is_read_as_a_move_an_upright_scale_or_anything_else() {
        use zgui_geom::Affine2;

        assert_eq!(
            RunTransform::of(Affine2::IDENTITY),
            RunTransform::Translation
        );
        assert_eq!(
            RunTransform::of(Affine2::scale(0.96, 0.96).then(Affine2::translation(4.0, -8.0))),
            RunTransform::Upright { x: 0.96, y: 0.96 }
        );
        for other in [
            Affine2::rotation(0.01),
            Affine2::skew(0.1, 0.0),
            Affine2::scale(1.0, -1.0),
            Affine2::scale(0.0, 1.0),
        ] {
            assert_eq!(RunTransform::of(other), RunTransform::Other, "{other:?}");
            assert!(!RunTransform::of(other).keeps_tiles());
        }
    }

    /// A picture has no outline to fill, so nothing about the surface promotes one.
    #[test]
    fn a_colour_run_stays_on_the_atlas_under_every_condition_that_promotes_another() {
        for profile in [
            RunProfile {
                color_glyphs: true,
                size: 400.0,
                ..RunProfile::BODY_TEXT
            },
            RunProfile {
                color_glyphs: true,
                surface: RunSurface {
                    transform: RunTransform::Other,
                    solid_brush: false,
                    ..RunSurface::PLAIN
                },
                ..RunProfile::BODY_TEXT
            },
        ] {
            assert_eq!(RasterPath::of(&profile), RasterPath::Atlas, "{profile:?}");
        }
    }
}
