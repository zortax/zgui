//! The insertion point and the selection, from the model that owns them to the display list.
//!
//! Text editing is only half visible without this. The model knows where the caret is and which
//! bytes are selected; nothing below the runtime does, because where a caret is *drawn* is a
//! question about the shaped, broken, laid-out lines — and those belong to the frame rather than to
//! the model.
//!
//! So the frame computes a [`Plan`] after it has laid out and before it paints, and the emit walk
//! reads it through [`zgui_paint::HighlightSource`]. The plan holds rectangles
//! relative to each line box, never absolute ones: the line's own fragment is what carries the
//! scroll offset and the transform, so a caret expressed against it moves with the text instead of
//! staying behind when the field scrolls.

pub mod blink;
pub mod place;

use std::hash::{Hash, Hasher};
use std::time::Instant;

use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_layout::fragment::ParagraphId;
use zgui_paint::{Highlight, HighlightLayer, HighlightRequest, HighlightSource};

pub use crate::caret::blink::Blink;
pub use crate::caret::place::Located;

/// How wide the caret is, in device pixels, at a given scale.
///
/// One CSS pixel, and never less than one device pixel: a caret rounded to nothing is a field with
/// no insertion point at all, which is the defect this whole module exists to remove.
fn caret_width(scale: f32) -> f32 {
    scale.round().max(1.0)
}

/// How opaque a selection band is over the text it marks.
///
/// The band is drawn behind the glyphs in the same colour the text is written in, so it marks the
/// selection on any background the element has — a light theme and a dark one both contrast with
/// their own text by construction — and the glyphs stay legible on top of it.
const BAND_ALPHA: f32 = 0.30;

/// One rectangle the plan will draw, expressed against its own line box.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mark {
    /// Which line of the paragraph it belongs to.
    line: u16,
    /// Where it sits relative to that line box's top-left corner, in device pixels.
    offset: Point<DevicePx, Device>,
    /// How big it is.
    size: Size<DevicePx, Device>,
    /// What fills it.
    color: Color,
    /// Whether it goes under the glyphs or over them.
    layer: HighlightLayer,
}

impl Mark {
    /// Folds this into a fingerprint.
    fn hash_into(&self, hasher: &mut impl Hasher) {
        self.line.hash(hasher);
        for value in [
            self.offset.x.0,
            self.offset.y.0,
            self.size.width.0,
            self.size.height.0,
        ] {
            value.to_bits().hash(hasher);
        }
        self.color.alpha().to_bits().hash(hasher);
        for channel in self.color.components() {
            channel.to_bits().hash(hasher);
        }
        (self.layer == HighlightLayer::InFront).hash(hasher);
    }
}

/// What one frame draws for the caret and the selection.
///
/// Empty for every window nobody is typing into, which is most of them, and asked once per line
/// fragment by the emit walk.
#[derive(Debug, Default)]
pub struct Plan {
    /// The paragraph the marks belong to, and nothing when there are none.
    paragraph: Option<ParagraphId>,
    /// The rectangles, in the order they were computed.
    marks: Vec<Mark>,
    /// A fingerprint of the whole plan, which is what stops a blink from being replayed.
    fingerprint: u64,
}

impl Plan {
    /// Nothing to draw.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether the plan draws anything at all.
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// How many rectangles it draws.
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    /// The fingerprint the paint record is keyed on.
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Recomputes the fingerprint from what the plan holds.
    fn seal(mut self) -> Self {
        let mut hasher = rustc_hash::FxHasher::default();
        self.paragraph.map(ParagraphId::index).hash(&mut hasher);
        for mark in &self.marks {
            mark.hash_into(&mut hasher);
        }
        // A plan that draws nothing must not fingerprint as one that draws something, and the empty
        // hash of an empty list is a real value: it is written down rather than left at zero so
        // that "no caret" and "a caret nobody has hashed yet" cannot collide.
        self.fingerprint = if self.marks.is_empty() {
            0
        } else {
            hasher.finish() | 1
        };
        self
    }

    /// Every absolute rectangle the plan would draw for one line, for a caller checking the plan
    /// itself rather than the display list.
    pub fn rects_of(
        &self,
        paragraph: ParagraphId,
        line: u16,
        origin: Point<DevicePx, Device>,
    ) -> Vec<Rect<DevicePx, Device>> {
        let mut out = Vec::new();
        self.visit_line(
            paragraph,
            line,
            HighlightRequest { origin, scale: 1.0 },
            &mut |highlight| out.push(highlight.bounds),
        );
        out
    }
}

impl HighlightSource for Plan {
    fn fingerprint(&self, paragraph: ParagraphId, line: u16) -> u64 {
        if self.paragraph != Some(paragraph) {
            return 0;
        }
        // Per line, so that a paragraph whose caret is on its third line does not force its first
        // two to be encoded again on every blink.
        if !self.marks.iter().any(|mark| mark.line == line) {
            return 0;
        }
        self.fingerprint
    }

    fn visit_line(
        &self,
        paragraph: ParagraphId,
        line: u16,
        request: HighlightRequest,
        visit: &mut dyn FnMut(Highlight),
    ) {
        if self.paragraph != Some(paragraph) {
            return;
        }
        for mark in self.marks.iter().filter(|mark| mark.line == line) {
            visit(Highlight {
                bounds: Rect::new(
                    Point::new(
                        DevicePx(request.origin.x.0 + mark.offset.x.0),
                        DevicePx(request.origin.y.0 + mark.offset.y.0),
                    ),
                    mark.size,
                ),
                color: mark.color,
                layer: mark.layer,
            });
        }
    }
}

/// Everything a window knows about its own caret between frames.
#[derive(Debug, Default)]
pub struct Carets {
    /// The phase.
    blink: Blink,
    /// What this frame draws.
    plan: Plan,
    /// The absolute rectangles the last frame drew, so that a phase change can damage exactly them.
    drawn: Vec<Rect<DevicePx, Device>>,
}

impl Carets {
    /// A window with no caret anywhere.
    pub fn new() -> Self {
        Self::default()
    }

    /// What this frame draws.
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// The blink, for a caller asking when the next phase is due.
    pub fn blink(&self) -> &Blink {
        &self.blink
    }

    /// Restarts the blink, which every caret movement does.
    pub fn restart(&mut self, now: Instant) {
        self.blink.restart(now);
    }

    /// Stops the blink, which losing focus or losing the editable element does.
    pub fn stop(&mut self) {
        self.blink.stop();
    }

    /// When the caret next owes a frame.
    pub fn next_flip(&self, now: Instant) -> Option<Instant> {
        if self.plan.is_empty() && !self.blink.is_running() {
            return None;
        }
        self.blink.next_flip(now)
    }

    /// Replaces the plan, and reports every rectangle whose pixels have to be redrawn.
    ///
    /// Both the rectangles the last frame drew and the ones this frame will: a caret that moved
    /// leaves its old pixels behind, and a caret that has gone dark leaves all of them behind.
    /// Returning them rather than damaging from here keeps this a value: the caller owns the damage
    /// set and is the only thing that may write it.
    pub fn install(
        &mut self,
        plan: Plan,
        drawn: Vec<Rect<DevicePx, Device>>,
    ) -> Vec<Rect<DevicePx, Device>> {
        let mut owed = core::mem::take(&mut self.drawn);
        if self.plan.fingerprint() != plan.fingerprint() {
            owed.extend(drawn.iter().copied());
        } else {
            owed.clear();
        }
        self.plan = plan;
        self.drawn = drawn;
        owed
    }
}

/// Builds the plan for one editable element's selection.
///
/// `visible` is the blink phase: the caret is left out entirely while it is off, rather than drawn
/// transparent, so that a frame in the dark phase pushes no primitive at all. The selection is not
/// blinked — it marks what a keystroke would replace, and marking that intermittently would be a
/// selection that looks like it comes and goes.
pub fn plan_for(
    located: &Located,
    selection: zgui_edit::Selection,
    color: Color,
    scale: f32,
    visible: bool,
) -> Plan {
    let mut marks = Vec::new();
    for band in located.bands(selection.range()) {
        let Some(origin) = located.line_origin(band.line) else {
            continue;
        };
        marks.push(Mark {
            line: band.line as u16,
            offset: Point::new(
                DevicePx(band.origin.x.0 - origin.x.0),
                DevicePx(band.origin.y.0 - origin.y.0),
            ),
            size: Size::new(DevicePx(band.size.width.0), DevicePx(band.size.height.0)),
            color: color.with_alpha(color.alpha() * BAND_ALPHA),
            layer: HighlightLayer::Behind,
        });
    }
    if visible
        && let Some(caret) = located.caret(selection.focus, selection.affinity)
        && let Some(origin) = located.line_origin(caret.line)
    {
        marks.push(Mark {
            line: caret.line as u16,
            offset: Point::new(
                DevicePx(caret.origin.x.0 - origin.x.0),
                DevicePx(caret.origin.y.0 - origin.y.0),
            ),
            size: Size::new(DevicePx(caret_width(scale)), DevicePx(caret.height.0)),
            color,
            layer: HighlightLayer::InFront,
        });
    }
    Plan {
        paragraph: (!marks.is_empty()).then_some(located.paragraph),
        marks,
        fingerprint: 0,
    }
    .seal()
}
