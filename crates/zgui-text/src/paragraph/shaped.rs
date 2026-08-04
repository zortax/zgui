//! A shaped paragraph, and the one place a breaking pass is decided on.

use smallvec::SmallVec;
use zgui_geom::CssPx;
use zgui_profile::{Counter, counter};
use zgui_text_style::BreakingKey;

use crate::geometry::strut::StrutMetrics;
use crate::map::TextMap;
use crate::paragraph::break_request::BreakRequest;
use crate::paragraph::broken::BrokenParagraph;
use crate::paragraph::inline_box::InlineBoxGeometry;
use crate::paragraph::key::ParagraphKey;
use crate::paragraph::recall::Recalled;

/// How narrow and how wide a paragraph's content can be.
///
/// The narrow figure is what it measures with a break taken at every opportunity; the wide figure
/// is what it measures with none taken. Both are properties of the shaped glyphs alone, so they are
/// computed once per shape and never again — which matters, because a layout engine asks for them
/// on every intrinsic-sizing pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentWidths {
    /// The narrowest the content can be.
    pub min: CssPx,
    /// The widest it would like to be.
    pub max: CssPx,
}

/// One paragraph's shaped glyphs, plus whatever line breaking was last applied to them.
///
/// The engine's own shaped form is held in [`engine`](ShapedParagraph::engine) and is never
/// interpreted here. Everything beside it is what a caller needs without opening that form: the key
/// it is cached under, the map back to the source, the intrinsic widths, and the record of which
/// break the glyphs currently reflect.
#[derive(Clone, Debug)]
pub struct ShapedParagraph<E> {
    /// The key this result is held under.
    key: ParagraphKey,
    /// The generated string the offsets refer to.
    text: String,
    /// The way back from those offsets to the source.
    map: TextMap,
    /// The intrinsic widths, computed once per shape.
    content_widths: ContentWidths,
    /// The block's strut, which every line box is at least as tall as.
    strut: StrutMetrics,
    /// The atomic inlines, at the geometry the last break used.
    boxes: SmallVec<[InlineBoxGeometry; 2]>,
    /// Which breaking key the glyphs currently reflect, if any break has been taken.
    broken: Option<BreakingKey>,
    /// The line boxes the last few breaking passes produced, by the key they were taken under.
    recalled: Recalled,
    /// The shaper's own result.
    pub engine: E,
}

impl<E> ShapedParagraph<E> {
    /// Records a fresh shaping pass.
    ///
    /// This is the only way to build one, and it is what counts a shape — so a shaper cannot report
    /// a cache hit and perform a shape, or the reverse.
    pub fn new(
        key: ParagraphKey,
        text: String,
        map: TextMap,
        content_widths: ContentWidths,
        strut: StrutMetrics,
        boxes: impl IntoIterator<Item = InlineBoxGeometry>,
        engine: E,
    ) -> Self {
        counter::bump(Counter::TextShaped);
        counter::add(Counter::TextBytesShaped, text.len() as u64);
        Self {
            key,
            text,
            map,
            content_widths,
            strut,
            boxes: boxes.into_iter().collect(),
            broken: None,
            recalled: Recalled::default(),
            engine,
        }
    }

    /// The key this result is held under.
    pub fn key(&self) -> ParagraphKey {
        self.key
    }

    /// The generated string every reported offset indexes into.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The way back from a generated offset to the source.
    pub fn map(&self) -> &TextMap {
        &self.map
    }

    /// The intrinsic widths.
    pub fn content_widths(&self) -> ContentWidths {
        self.content_widths
    }

    /// The block's strut.
    pub fn strut(&self) -> StrutMetrics {
        self.strut
    }

    /// The atomic inlines at the geometry the last break used.
    pub fn boxes(&self) -> &[InlineBoxGeometry] {
        &self.boxes
    }

    /// Which break the glyphs currently reflect, or `None` if none has been taken yet.
    ///
    /// Equal to the key of the last [`BreakRequest`] that
    /// [`begin_break`](ShapedParagraph::begin_break) answered `true` for, so a caller can tell a
    /// result that already answers its request from one that still owes a pass.
    pub fn broken_as(&self) -> Option<BreakingKey> {
        self.broken
    }

    /// Decides how `request` is to be answered.
    ///
    /// This is the single place a breaking pass is decided on, so a shaper cannot report a cheap
    /// pass and take an expensive one, or the reverse. [`Plan::Owed`] is the only answer that lets
    /// one happen, and taking it counts the pass and adopts the request's inline-box geometry.
    ///
    /// Adopting that geometry here is what makes a `vertical-align` re-style reach the output. The
    /// shift it produces is baked into the height the shaper was told, so nothing in the shaped
    /// glyphs can notice it changed; the request carries the current shift, the key covers it, and
    /// a shift that moved therefore forces a break exactly as a width change would.
    pub fn plan_break(&mut self, request: &BreakRequest<'_>) -> Plan<'_> {
        let key = request.key();
        if self.broken == Some(key) {
            return Plan::Reflected;
        }
        if request.probe && self.recalled.get(key).is_some() {
            return Plan::Recalled(self.recalled.get(key).expect("just found"));
        }
        counter::bump(Counter::TextRebroken);
        self.boxes.clear();
        self.boxes.extend(request.boxes.iter().copied());
        self.broken = Some(key);
        Plan::Owed
    }

    /// Decides whether `request` needs a breaking pass, and records the answer.
    ///
    /// Returns `false` when the glyphs already reflect exactly this request, in which case a shaper
    /// must not break again and must report what it already has. The short form of
    /// [`plan_break`](ShapedParagraph::plan_break), for a shaper that keeps no line boxes of its
    /// own to hand back and so has nothing to recall.
    pub fn begin_break(&mut self, request: &BreakRequest<'_>) -> bool {
        !matches!(self.plan_break(request), Plan::Reflected)
    }

    /// Records the line boxes a pass produced, so that a later probe at the same width is free.
    ///
    /// Called with what [`Plan::Owed`] led to, and with nothing else: an answer filed under a key
    /// it is not the answer to would be served to a paragraph asking a different question.
    pub fn remember(&mut self, key: BreakingKey, broken: BrokenParagraph) {
        self.recalled.insert(key, broken);
    }

    /// How many passes are remembered.
    ///
    /// Bounded, and the bound is the point: a window being dragged proposes a new width every
    /// frame, and a paragraph that remembered all of them would grow for as long as the drag went
    /// on, once per paragraph on the page.
    pub fn remembered(&self) -> usize {
        self.recalled.len()
    }
}

/// How one breaking request is to be answered.
///
/// # Why recalling is only offered to a probe
///
/// A shaper's own laid-out form holds one break at a time, and it is what glyph positions are read
/// out of when the paragraph is painted. Handing back a remembered result therefore answers the
/// measurement without moving that form — which is exactly right for a question about how big the
/// paragraph *would* be, and exactly wrong for the pass whose lines are going to be drawn. So the
/// caller says which it is asking, and a pass whose answer will be kept always costs a real break
/// unless the glyphs already reflect it.
#[derive(Debug)]
pub enum Plan<'a> {
    /// The glyphs already reflect this request; the shaper must report what it has.
    Reflected,
    /// A previous pass at this width was remembered, and answers the measurement.
    Recalled(&'a BrokenParagraph),
    /// A breaking pass is owed, and has been counted.
    Owed,
}
