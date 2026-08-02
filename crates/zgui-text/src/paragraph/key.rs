//! The two cache keys a paragraph is held under.

use zgui_geom::CssPx;
use zgui_text_style::{BreakingKey, Digest, ShapingKey};

use crate::paragraph::break_request::BreakRequest;
use crate::paragraph::content::ParagraphContent;

/// Identifies a shaped paragraph.
///
/// It is the content, the *ordered* list of runs with their shaping keys, the atomic inlines'
/// positions in the text, the paragraph's base direction and the device scale — everything a
/// shaping pass reads. Two contexts with equal keys shape to the same glyphs, so one shaped result
/// serves both.
///
/// The run list is ordered and carries each run's extent, not only its style: moving a boundary
/// between two runs changes which characters are shaped together, and a key that hashed only the
/// set of styles would miss it.
///
/// The map back to the source is part of the key too, which is not obvious, because it changes no
/// glyph. A shaped result *carries* its map, so two contexts sharing one entry share its map — and
/// two paragraphs can generate the same string from different source text, which is exactly what
/// collapsing leading white space does. Leaving the map out would serve the second paragraph the
/// first one's provenance, putting every caret, selection and hit test in it at the wrong offset
/// with nothing to report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParagraphKey(pub u64);

impl ParagraphKey {
    /// The key one context shapes under.
    pub fn of(content: &ParagraphContent<'_>) -> Self {
        let mut digest = Digest::new();
        digest.push(content.text);
        digest.push_f32(content.scale);
        content.paragraph.hash_shaping(&mut digest);
        digest.push(content.runs.len());
        for run in content.runs {
            digest.push(run.text.start);
            digest.push(run.text.end);
            digest.push(ShapingKey::of(&run.style).0);
            // The brush is in the key because the shaped result *carries* it: every glyph run in
            // the entry names the slot it is drawn through, so two contexts holding the same
            // characters in the same style but different colours must not share an entry — shared,
            // both are drawn through whichever context shaped first, and which that is is decided
            // again every time the cache empties. A change of device scale is exactly that, which
            // is a window whose short repeated strings — a tooltip over a label that says the same
            // thing — swap colours when it is dragged onto another monitor.
            //
            // Re-theming still costs no shaping, because a theme that moves a colour moves it
            // *through* the slot: the brush index in the flattened form does not change, so neither
            // does this key. The one path that re-shapes is the one where an element leaves a slot
            // others still hold — and that path already drops the shaping, precisely because what
            // the glyphs name was baked in when they were shaped.
            digest.push(run.brush.0);
        }
        digest.push(content.boxes.len());
        for inline_box in content.boxes {
            digest.push(inline_box.id);
            digest.push(inline_box.offset);
        }
        let segments = content.map.segments();
        digest.push(segments.len());
        for segment in segments {
            digest.push(segment.generated.start);
            digest.push(segment.generated.end);
            digest.push(segment.run);
            digest.push(segment.offset);
        }
        Self(digest.finish())
    }
}

/// The breaking key of one shaped paragraph at one proposed width.
///
/// Built here rather than in the style crate because two of its four parts are not style at all:
/// the width being proposed, and the current geometry of every atomic inline. Both invalidate a
/// break without changing a glyph, and the second is what makes a `vertical-align` re-style reach
/// the output — the shift is baked into a number the shaper already holds, so nothing else would
/// notice it moved.
pub fn breaking_key(request: &BreakRequest<'_>) -> BreakingKey {
    let mut digest = Digest::new();
    request.paragraph.hash_breaking(&mut digest);
    digest.push(request.runs.len());
    for run in request.runs {
        digest.push(BreakingKey::of(&run.style).0);
    }
    digest.push(request.boxes.len());
    for inline_box in request.boxes {
        digest.push(inline_box.id);
        digest.push_length(inline_box.width);
        digest.push_length(inline_box.height);
        digest.push_length(inline_box.shaper_height());
    }
    digest.push_optional_length(request.max_advance);
    digest.push_optional_length(request.indent_basis);
    // The bands are in the key for the same reason the inline boxes are: they change where the
    // lines fall while changing nothing a shaper holds, so a break taken under one set of bands is
    // not an answer to a request carrying another.
    let bands = request.bands.as_slice();
    digest.push(bands.len());
    for band in bands {
        digest.push_length(band.offset);
        digest.push_length(band.max_advance);
    }
    BreakingKey(digest.finish())
}

/// The width a paragraph is being asked to fit into, if any.
///
/// Nothing means "as wide as it likes", which is what a max-content probe asks for.
pub type MaxAdvance = Option<CssPx>;
