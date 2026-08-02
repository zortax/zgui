//! Which of the two drawing paths a run of glyphs takes.
//!
//! # Why the answer belongs to the run
//!
//! Both paths draw the same glyphs and neither is a fallback for the other. The atlas rasterises
//! once on the processor and draws a quad per glyph, which is what makes a page of body text cost
//! almost nothing and is the only path that can be hinted; outlines are filled by the frame's path
//! rasteriser, which costs more per glyph and is the only path that survives a rotation, a size no
//! cache should hold, or a brush that is not one colour.
//!
//! So the choice is a property of *the run and the surface it lands on*, decided once where both
//! are known, and carried with the run. A renderer that decided it for itself would decide it
//! differently from the stage that measured the ink, and the two disagreeing is a run drawn twice
//! or not at all.

use crate::glyph::run::ShapedRun;

/// The largest size, in device pixels, a run may be and still go through the glyph atlas.
///
/// Above it the tiles stop being worth caching: a handful of display-sized headings would evict
/// every body-text glyph in the atlas between them, and the outlines are cheap to fill directly at
/// that size.
pub const ATLAS_MAX_SIZE: f32 = 96.0;

/// What the surface a run lands on does to it.
///
/// Both fields are things the run cannot know and whoever is drawing it cannot avoid knowing: the
/// transform in force and the brush the run is painted with.
///
/// A clip is deliberately not among them. Every clip this pipeline can express — a rounded
/// rectangle, or a mask sampled from a tile — is evaluated per pixel by whatever draws the run,
/// and a quad reading a coverage tile is cut by one exactly as well as a filled curve is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunSurface {
    /// Whether the transform in force is a pure translation.
    ///
    /// A translation moves already rasterised pixels and everything else resamples them, so this
    /// is the whole of the question a cached tile has to answer.
    pub translated_only: bool,
    /// Whether the run's brush is a single colour rather than a gradient or an image.
    ///
    /// A coverage tile carries no colour of its own: it is multiplied by one, so a brush that
    /// varies across the run has nothing to multiply.
    pub solid_brush: bool,
}

impl RunSurface {
    /// An upright run of one colour — what nearly every run is.
    pub const PLAIN: Self = Self {
        translated_only: true,
        solid_brush: true,
    };
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
    /// would need a re-raster per frame, and brushes that are not one colour.
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
    /// use zgui_text::{RasterPath, RunProfile, RunSurface};
    ///
    /// assert_eq!(RasterPath::of(&RunProfile::BODY_TEXT), RasterPath::Atlas);
    ///
    /// let heading = RunProfile { size: 128.0, ..RunProfile::BODY_TEXT };
    /// assert_eq!(RasterPath::of(&heading), RasterPath::Vector);
    ///
    /// let turned = RunProfile {
    ///     surface: RunSurface { translated_only: false, ..RunSurface::PLAIN },
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
        let cacheable =
            profile.size <= ATLAS_MAX_SIZE && surface.translated_only && surface.solid_brush;
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
    /// use zgui_text::{FaceId, RasterPath, RunSurface, ShapedRun};
    ///
    /// let run = ShapedRun {
    ///     face: FaceId(0),
    ///     size: 160.0,
    ///     synthetic_bold: 0.0,
    ///     synthetic_slant: 0.0,
    ///     has_color: false,
    ///     brush: PaintSlot(0),
    ///     glyphs: &[],
    /// };
    /// assert_eq!(run.raster_path(RunSurface::PLAIN), RasterPath::Vector);
    /// ```
    pub fn raster_path(&self, surface: RunSurface) -> RasterPath {
        RasterPath::of(&self.profile(surface))
    }
}

#[cfg(test)]
mod tests {
    use super::{ATLAS_MAX_SIZE, RasterPath, RunProfile, RunSurface};

    #[test]
    fn the_atlas_keeps_what_it_can_serve_and_gives_up_the_rest() {
        assert_eq!(RasterPath::of(&RunProfile::BODY_TEXT), RasterPath::Atlas);
        assert_eq!(
            RasterPath::of(&RunProfile {
                size: ATLAS_MAX_SIZE,
                ..RunProfile::BODY_TEXT
            }),
            RasterPath::Atlas,
            "the limit is the largest size the atlas still serves"
        );

        let leaving = [
            RunProfile {
                size: ATLAS_MAX_SIZE + 1.0,
                ..RunProfile::BODY_TEXT
            },
            RunProfile {
                surface: RunSurface {
                    translated_only: false,
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
                    translated_only: false,
                    solid_brush: false,
                },
                ..RunProfile::BODY_TEXT
            },
        ] {
            assert_eq!(RasterPath::of(&profile), RasterPath::Atlas, "{profile:?}");
        }
    }
}
