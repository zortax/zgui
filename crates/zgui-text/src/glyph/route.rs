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
    /// How many device pixels one CSS pixel is.
    ///
    /// A run states its size in device pixels, so this is what turns that back into the size the
    /// document asked for — and the size threshold is about what a document asked for.
    pub scale: f32,
}

impl RunSurface {
    /// An upright run of one colour at one device pixel per CSS pixel — what nearly every run is.
    pub const PLAIN: Self = Self {
        translated_only: true,
        solid_brush: true,
        scale: 1.0,
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
    /// // The same heading on a display of twice the density is the same heading: ninety-six CSS
    /// // pixels is a hundred and ninety-two device ones, and the atlas still serves it.
    /// let dense = RunProfile {
    ///     size: 128.0,
    ///     surface: RunSurface { scale: 2.0, ..RunSurface::PLAIN },
    ///     ..RunProfile::BODY_TEXT
    /// };
    /// assert_eq!(RasterPath::of(&dense), RasterPath::Atlas);
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
        let cacheable = profile.size <= atlas_max_size(surface.scale)
            && surface.translated_only
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
    use super::{ATLAS_MAX_CSS_SIZE, ATLAS_MAX_SIZE, RasterPath, RunProfile, RunSurface, atlas_max_size};

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
                    ..RunSurface::PLAIN
                },
                ..RunProfile::BODY_TEXT
            },
        ] {
            assert_eq!(RasterPath::of(&profile), RasterPath::Atlas, "{profile:?}");
        }
    }
}
