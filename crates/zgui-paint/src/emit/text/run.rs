//! One shaped run recorded as a compositor glyph run.

use iceblink::{ArenaRange, ClipRef, FontId, Glyph, GlyphFlags, GlyphRun, PaintRef, SpaceRef};
use smallvec::SmallVec;
use zgui_text::{SUBPIXEL_MAX_SIZE, ShapedRun};

use crate::emit::record::Recorder;

/// How one run is drawn.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunDraw {
    /// The device position the run's glyph positions are measured from.
    pub(crate) origin: [f32; 2],
    pub(crate) paint: PaintRef,
    pub(crate) clip: ClipRef,
    pub(crate) space: SpaceRef,
    pub(crate) flags: GlyphFlags,
}

/// Records `run` as one glyph run drawn as `draw` says, at the run's variation coordinates and
/// with its synthesis.
pub(crate) fn record(rec: &mut Recorder<'_>, run: &ShapedRun<'_>, draw: RunDraw) {
    let [x, y] = draw.origin;
    let glyphs: SmallVec<[Glyph; 32]> = run
        .glyphs
        .iter()
        .map(|glyph| Glyph {
            id: u32::from(glyph.glyph),
            x: x + glyph.x,
            y: y + glyph.y,
        })
        .collect();
    crate::content::probe::placed(glyphs.len());
    rec.glyph_run(
        GlyphRun {
            // A face id is the font id, because the text engine's face table is the font registry.
            font: FontId(run.face.0),
            size: run.size,
            glyphs: ArenaRange::default(),
            coords: ArenaRange::default(),
            paint: draw.paint,
            embolden: run.synthetic_bold * run.size,
            skew: run.synthetic_slant,
            clip: draw.clip,
            space: draw.space,
            flags: draw.flags,
            _pad: [0; 1],
            palette: 0,
        },
        &glyphs,
        run.coords,
    );
}

/// Returns the flags of a run `size` device pixels large.
///
/// Every run is hinted. A run asks for per-channel coverage when `subpixel` allows it and it is at
/// most [`SUBPIXEL_MAX_SIZE`] device pixels. A `silhouette` run, such as a shadow, draws no colour
/// glyph.
pub(crate) fn flags(size: f32, subpixel: bool, silhouette: bool) -> GlyphFlags {
    let mut flags = GlyphFlags::HINT;
    if subpixel && size <= SUBPIXEL_MAX_SIZE {
        flags |= GlyphFlags::SUBPIXEL;
    }
    if silhouette {
        flags |= GlyphFlags::SILHOUETTE;
    }
    flags
}

#[cfg(test)]
mod tests {
    use iceblink::GlyphFlags;
    use zgui_text::SUBPIXEL_MAX_SIZE;

    #[test]
    fn a_run_above_the_subpixel_limit_asks_for_one_coverage_per_pixel() {
        assert!(super::flags(SUBPIXEL_MAX_SIZE, true, false).contains(GlyphFlags::SUBPIXEL));
        assert!(!super::flags(SUBPIXEL_MAX_SIZE + 0.5, true, false).contains(GlyphFlags::SUBPIXEL));
    }

    #[test]
    fn a_run_records_its_synthesis_in_device_pixels_and_degrees() {
        let glyphs = [zgui_text::ShapedGlyph {
            glyph: 4,
            x: 2.0,
            y: 12.0,
        }];
        let run = zgui_text::ShapedRun {
            face: zgui_text::FaceId(1),
            size: 50.0,
            coords: &[],
            synthetic_bold: zgui_text::SYNTHETIC_BOLD_RATIO,
            synthetic_slant: 14.0,
            brush: zgui_space::PaintSlot(0),
            glyphs: &glyphs,
            text_start: 0,
        };
        let mut tables = crate::Tables::new();
        let mut recording = crate::emit::record::Recording::new();
        let mut rec = recording.recorder(
            &mut tables,
            zgui_space::SpatialId::VIEWPORT,
            zgui_space::ClipId::ROOT,
        );
        super::record(
            &mut rec,
            &run,
            super::RunDraw {
                origin: [10.0, 20.0],
                paint: iceblink::PaintRef::color([0, 0, 0, 255]),
                clip: iceblink::ClipRef::INHERIT,
                space: iceblink::SpaceRef::INHERIT,
                flags: GlyphFlags::HINT,
            },
        );
        let recorded = recording.glyph_runs()[0];
        assert_eq!(recorded.embolden, 1.0, "two hundredths of fifty pixels");
        assert_eq!(recorded.skew, 14.0);
        let glyph = recording.glyphs(&recorded)[0];
        assert_eq!((glyph.x, glyph.y), (12.0, 32.0));
    }

    #[test]
    fn a_silhouette_is_hinted_and_asks_for_no_per_channel_coverage() {
        let flags = super::flags(16.0, false, true);
        assert_eq!(flags, GlyphFlags::HINT | GlyphFlags::SILHOUETTE);
    }
}
