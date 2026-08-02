//! What one shaping pass produces: a cluster per character, and nothing a font decided.

use zgui_geom::CssPx;
use zgui_text::ParagraphContent;

use crate::shaper::metrics;

/// One shaped cluster.
///
/// A cluster is one character, always. A real shaper produces clusters from a font's own mapping —
/// a ligature is one cluster of several characters, a combining mark none at all — and nothing in
/// this crate pretends otherwise. What it buys is that a paragraph's measured width is a function
/// of its length and its style, computable by hand in a test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cluster {
    /// Byte offset of the character in the generated string.
    pub offset: usize,
    /// How far the pen moves over it.
    pub advance: CssPx,
    /// Whether a soft break may be taken before it.
    pub breakable: bool,
    /// The brush slot the run this cluster belongs to is drawn with.
    pub brush: zgui_text::Brush,
}

/// What [`MonoShaper`](crate::MonoShaper) keeps between shaping and breaking.
///
/// It is the shaper's own form, carried through
/// [`ShapedParagraph::engine`](zgui_text::ShapedParagraph) and never interpreted by anything else.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonoLayout {
    /// The device pixel ratio the paragraph was shaped at.
    ///
    /// Held because the size a run reports is in device pixels — it is what a glyph key is built
    /// from, and a key carrying a CSS size asks the rasteriser for a glyph of the wrong extent —
    /// while the strut this shaper measures against is in CSS ones. Nothing else in the shaped
    /// form records the ratio, so a run visited later could not otherwise state its own size.
    pub scale: f32,
    /// The clusters, in logical order.
    pub clusters: Vec<Cluster>,
    /// The last break's lines, as half-open cluster ranges.
    pub lines: Vec<(usize, usize)>,
    /// The geometry those lines came out at, kept so that positioned glyphs can be reported
    /// without breaking a second time and getting a second opinion.
    pub geometry: Vec<zgui_text::LineGeometry>,
}

/// Shapes every character of `content` into one cluster, run by run.
///
/// The style is taken from the run the character belongs to, so a paragraph with a large first word
/// measures wider than one styled uniformly — which is what makes a run-boundary bug visible.
///
/// # The device pixel ratio
///
/// Advances come out at `font-size × ratio`, because that is what a real shaper does: it is handed
/// the ratio and it shapes at the size in *device* pixels, so an inline box measured from its own
/// glyphs is a device-pixel extent like every other length the layout algorithms are given. A
/// shaper that ignored it would report CSS-pixel advances into a device-pixel layout, and every
/// text-sized box in the document would come out at one-times width inside scaled padding — while
/// the glyphs drawn into it, whose keys carry the device size, came out at the right size.
///
/// It also decides whether a test at a fractional ratio can see anything at all. Every advance
/// here would be identical at every ratio, so a suite written against this fixture would pass
/// whatever the framework did with the number.
pub fn shape(content: &ParagraphContent<'_>) -> Vec<Cluster> {
    let mut clusters = Vec::new();
    for (offset, character) in content.text.char_indices() {
        let run = content.runs.iter().find(|run| run.text.contains(&offset));
        let style = run.map(|run| run.style.as_ref());
        let advance = match style {
            Some(style) if character == ' ' => metrics::space_advance(style),
            Some(style) => metrics::advance(style),
            // A character outside every run has no style to be measured with. The contract says the
            // runs cover the string, so this is a caller's defect: it is given zero advance rather
            // than a guessed one, so the resulting geometry is visibly wrong instead of plausible.
            None => CssPx::ZERO,
        };
        let advance = CssPx(advance.0 * content.scale);
        clusters.push(Cluster {
            offset,
            advance,
            breakable: character == ' ',
            // A cluster outside every run has no brush either; slot zero is an ordinary entry and
            // is the only answer available, which is why the contract says the runs cover the
            // string.
            brush: run.map_or(zgui_scene::PaintSlot(0), |run| run.brush),
        });
    }
    clusters
}

/// The narrowest and widest the clusters can be laid out at.
///
/// The narrow figure is the widest single word, because a break may only be taken at a space; the
/// wide figure is every cluster on one line.
pub fn content_widths(clusters: &[Cluster]) -> (CssPx, CssPx) {
    let mut widest_word: f32 = 0.0;
    let mut word: f32 = 0.0;
    let mut total: f32 = 0.0;
    for cluster in clusters {
        total += cluster.advance.0;
        if cluster.breakable {
            widest_word = widest_word.max(word);
            word = 0.0;
        } else {
            word += cluster.advance.0;
        }
    }
    (CssPx(widest_word.max(word)), CssPx(total))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_geom::CssPx;
    use zgui_scene::PaintSlot;
    use zgui_text::{ParagraphContent, StyledRun, TextMap};
    use zgui_text_style::{ParagraphStyle, TextStyle};

    use super::{content_widths, shape};

    #[test]
    fn every_character_is_one_cluster_and_a_space_carries_word_spacing() {
        let text = "ab cd";
        let map = TextMap::new();
        let style = Arc::new(TextStyle::initial());
        let runs = [StyledRun {
            text: 0..text.len(),
            style,
            brush: PaintSlot(0),
        }];
        let paragraph = ParagraphStyle::initial();
        let content = ParagraphContent {
            text,
            map: &map,
            runs: &runs,
            boxes: &[],
            paragraph: &paragraph,
            scale: 1.0,
        };

        let clusters = shape(&content);
        assert_eq!(clusters.len(), 5);
        assert!(clusters[2].breakable, "the space is the break opportunity");
        assert_eq!(clusters[0].advance, CssPx(8.0));

        let (min, max) = content_widths(&clusters);
        assert_eq!(min, CssPx(16.0), "the widest word is two clusters");
        assert_eq!(max, CssPx(40.0), "five clusters on one line");
    }

    #[test]
    fn a_cluster_advances_by_the_font_size_in_device_pixels() {
        // What a shaper is handed the ratio for. Every advance here was independent of it, so a
        // window at a fractional ratio measured its text as though it were at one — and every test
        // written against this fixture agreed with whatever the framework did, because the number
        // it compared against did not move either.
        let text = "ab";
        let map = TextMap::new();
        let style = Arc::new(TextStyle::initial());
        let runs = [StyledRun {
            text: 0..text.len(),
            style,
            brush: PaintSlot(0),
        }];
        let paragraph = ParagraphStyle::initial();
        let at = |scale: f32| {
            let content = ParagraphContent {
                text,
                map: &map,
                runs: &runs,
                boxes: &[],
                paragraph: &paragraph,
                scale,
            };
            content_widths(&shape(&content)).1
        };

        assert_eq!(at(1.0), CssPx(16.0));
        assert_eq!(at(1.25), CssPx(20.0));
        assert_eq!(at(2.0), CssPx(32.0));
    }
}
