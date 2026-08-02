//! The filled rectangles drawn with a line of text but not made of glyphs: the caret, and the
//! bands a selection is painted as.
//!
//! # Why this is a seam and not a property of the fragment
//!
//! Where the caret is depends on an editing model, on which element holds focus and on a blink
//! phase, and none of those belong to the fragment tree — a tree that carried them would have to be
//! rebuilt whenever the caret blinked, which is twice a second for as long as a field is focused.
//! So a line fragment says only which paragraph and which line it draws, exactly as it already does
//! for glyphs, and whoever owns the caret answers for it here.
//!
//! # Why a fingerprint travels with it
//!
//! The emit walk replays an unchanged fragment's recorded primitives rather than encoding them
//! again, and *unchanged* is decided from the fragment's style, geometry, clip, transform and
//! animation. A caret that blinked moves none of those. Without a fingerprint the record still
//! stands, the previous frame's range is replayed, and the caret is painted in whichever phase it
//! happened to be in when the line was last encoded — for ever.

use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_layout::fragment::ParagraphId;
use zgui_scene::{ClipId, Quad, Scene, SpatialId};

/// Whether a rectangle is drawn under the glyphs or over them.
///
/// Both exist and neither can stand in for the other. A selection band under opaque glyphs would
/// hide the text if it were drawn over them; a caret under them would disappear behind the letter
/// it is sitting on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HighlightLayer {
    /// Behind the glyphs: what a selection band is.
    Behind,
    /// In front of them: what a caret is.
    InFront,
}

/// One filled rectangle drawn with a line of text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Highlight {
    /// Where it lands, in absolute device pixels.
    pub bounds: Rect<DevicePx, Device>,
    /// What fills it.
    pub color: Color,
    /// Whether it goes under the glyphs or over them.
    pub layer: HighlightLayer,
}

/// Where one line's highlights are wanted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HighlightRequest {
    /// The line box's top-left corner, in absolute device pixels.
    ///
    /// The same origin the glyphs of that line are placed against, so a caret computed from the
    /// cluster advances lands exactly between the two letters it was asked for rather than a
    /// rounding away from them.
    pub origin: Point<DevicePx, Device>,
    /// How many device pixels one CSS pixel is.
    pub scale: f32,
}

/// Where a line's caret and selection bands come from.
pub trait HighlightSource {
    /// A fingerprint of everything this source would draw over one line.
    ///
    /// Folded into the record that decides whether a fragment may be replayed, so it has to move
    /// whenever the painting would: a caret that blinked, a selection that grew, a caret that moved
    /// to another offset on the same line. A source that answers with a constant makes every caret
    /// in the document permanent at whatever phase it was first drawn in.
    fn fingerprint(&self, paragraph: ParagraphId, line: u16) -> u64;

    /// Visits every rectangle to be drawn with one line.
    ///
    /// A line with no caret and nothing selected on it visits nothing, which is every line of every
    /// document nobody is typing into.
    fn visit_line(
        &self,
        paragraph: ParagraphId,
        line: u16,
        request: HighlightRequest,
        visit: &mut dyn FnMut(Highlight),
    );
}

/// A source that draws no caret and no selection anywhere.
///
/// What a document nobody is editing is painted through, and what a test that is not about text
/// selection uses.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHighlights;

impl HighlightSource for NoHighlights {
    fn fingerprint(&self, _paragraph: ParagraphId, _line: u16) -> u64 {
        0
    }

    fn visit_line(
        &self,
        _paragraph: ParagraphId,
        _line: u16,
        _request: HighlightRequest,
        _visit: &mut dyn FnMut(Highlight),
    ) {
    }
}

/// Emits every rectangle of one line that belongs on `layer`, and returns how many were pushed.
///
/// A rectangle with nothing to draw — fully transparent, or of no extent — is skipped rather than
/// pushed: a zero-width caret is what a caller that forgot to give it a width produces, and a quad
/// of no extent draws nothing while still costing a batch.
#[allow(clippy::too_many_arguments)]
pub fn emit(
    scene: &mut Scene,
    highlights: &dyn HighlightSource,
    paragraph: ParagraphId,
    line: u16,
    layer: HighlightLayer,
    request: HighlightRequest,
    clip: ClipId,
    transform: SpatialId,
    alpha: f32,
) -> usize {
    let mut pushed = 0;
    highlights.visit_line(paragraph, line, request, &mut |highlight| {
        if highlight.layer != layer {
            return;
        }
        if highlight.bounds.size.width.0 <= 0.0 || highlight.bounds.size.height.0 <= 0.0 {
            return;
        }
        let color = highlight
            .color
            .with_alpha(highlight.color.alpha() * alpha.clamp(0.0, 1.0));
        if color.alpha() == 0.0 {
            return;
        }
        let paint = scene.paints.add(zgui_scene::Paint::Solid(color));
        let quad = Quad::filled(highlight.bounds, paint)
            .clipped(clip)
            .transformed(transform);
        if scene.push_quad(quad).is_some() {
            pushed += 1;
        }
    });
    pushed
}
