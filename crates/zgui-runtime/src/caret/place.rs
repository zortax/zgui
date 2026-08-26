//! One editable element's laid-out text, and the two directions everything asks it about.
//!
//! A caret is a question in three coordinate systems at once. The editing model counts bytes of the
//! text the document holds; the shaper counts bytes of the string it was handed, which is not the
//! same string; and the screen counts device pixels. Nothing here decides anything about editing —
//! it only relates the three, using the layout the frame actually produced rather than a second
//! opinion derived from the characters.

use zgui_dom::NodeKey;
use zgui_edit::hit::{Band, Caret, LineMap};
use zgui_edit::select::Affinity;
use zgui_geom::{Css, CssPx, Device, DevicePx, Point};
use zgui_layout::LayoutStore;
use zgui_layout::fragment::ParagraphId;
use zgui_text::{LineGeometry, ShapedClusters, SourcePos, TextMap};

/// The box that holds an element's lines.
///
/// Not the element's own box. A block element with text in it generates a block box whose *child*
/// establishes the inline formatting context, and the lines belong to that child — so a caller that
/// looked only at the element's own box would find no lines at all and place no caret, which is a
/// field that types perfectly and shows nothing.
///
/// The first one found in document order, which for an editable element is the one holding its
/// text: an element with two inline formatting contexts under it is a structure the editing model
/// does not describe either.
pub fn text_box(layout: &LayoutStore, node: NodeKey) -> Option<zgui_dom::side::BoxKey> {
    let root = *layout.boxes_of(node).first()?;
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        if layout.inline_resolution(key).is_some() {
            return Some(key);
        }
        let Some(record) = layout.get(key) else {
            continue;
        };
        for child in record.children.iter().rev() {
            stack.push(*child);
        }
    }
    None
}

/// One editable element's text as it was laid out, ready to be asked where things are.
///
/// Built per frame and never held: it is a reading of the current layout, and a reading kept across
/// a frame that re-broke the paragraph would place carets on lines that are no longer there.
pub struct Located {
    /// The paragraph the element's lines belong to.
    pub paragraph: ParagraphId,
    /// The top-left corner of the inline formatting context's content box, in device pixels.
    ///
    /// In the space the *fragment* records, which is the space before its own transform: a rotated
    /// paragraph's corner is where the corner would be if it were not rotated.
    pub origin: Point<DevicePx, Device>,
    /// The transform between that space and the surface, if the paragraph carries one.
    ///
    /// A pointer arrives in the surface's coordinates and the lines are measured in the
    /// paragraph's, and those are two different spaces the moment anything above the text is
    /// turned, scaled or skewed. Keeping the name of the matrix here is what lets a hit be asked in
    /// the space the glyphs were actually drawn in rather than in the one layout wrote down.
    pub transform: Option<zgui_scene::SpatialId>,
    /// The lines and their clusters.
    pub lines: LineMap,
    /// How to get between the shaper's string and the document's.
    pub map: std::sync::Arc<TextMap>,
    /// How many bytes each source run holds, in document order.
    ///
    /// The editing model's offsets count these one after another, with nothing in between: a
    /// paragraph's text node holds the break that ends it, and the model's buffer is built by
    /// reading those nodes straight through. So this is the whole of the way between a model offset
    /// and a run-and-offset pair.
    pub runs: Vec<usize>,
}

impl Located {
    /// Reads one element's laid-out text out of the frame that produced it.
    ///
    /// Nothing is answered for an element with no inline formatting context of its own — one that
    /// has not been laid out yet, or whose box generates none — because there is no line for a
    /// caret to sit on and inventing one would put it at the window's corner.
    pub fn of(layout: &LayoutStore, clusters: &dyn ShapedClusters, node: NodeKey) -> Option<Self> {
        let box_ = text_box(layout, node)?;
        let resolution = layout.inline_resolution(box_)?;
        // The *fragment's* content box and not the layout result's. A layout result is measured
        // against the box's own parent; a fragment is where the composition pass put it on the
        // surface, which is the space a pointer arrives in and the space the glyphs were drawn in.
        // Reading the layout result places every caret at the paragraph's offset from its parent
        // instead of at its offset from the window, which for a field near the middle of a page is
        // a hit test that answers a completely different letter.
        let placed = layout
            .fragments_of_box(box_)
            .iter()
            .filter_map(|frag| layout.fragment(*frag))
            .find(|fragment| fragment.kind == zgui_layout::FragmentKind::Box)
            .map(|fragment| (fragment.content_box.origin, fragment.transform))?;
        let (origin, transform) = placed;
        let geometry: Vec<LineGeometry> = resolution
            .lines
            .iter()
            .map(|line| LineGeometry {
                text: line.text.clone(),
                top: CssPx(line.top),
                baseline: CssPx(line.baseline()),
                height: CssPx(line.height()),
                width: CssPx(line.width),
                offset: CssPx(line.offset),
            })
            .collect();
        let key = resolution.key;
        let lines = LineMap::of_lines(&geometry, |line, visit| {
            clusters.visit_clusters(key, line, visit);
        });
        let runs = resolution
            .sources
            .iter()
            .map(|source| {
                layout
                    .get(*source)
                    .and_then(|record| record.text.as_deref())
                    .map_or(0, str::len)
            })
            .collect();
        Some(Self {
            paragraph: resolution.paragraph,
            origin,
            transform,
            lines,
            map: resolution.map.clone(),
            runs,
        })
    }

    /// The source position an offset in the editing model's own text names.
    ///
    /// The model's text is the runs written out one after another, break characters and all: a
    /// paragraph's node holds the break that ends it, so there is nothing between two runs to
    /// account for. An offset that names the boundary between two runs resolves to the end of the
    /// earlier one, which is the reading a caret after a line break wants — the break itself is the
    /// last character of the run before it.
    pub fn to_source(&self, offset: usize) -> SourcePos {
        let mut remaining = offset;
        for (run, length) in self.runs.iter().enumerate() {
            if remaining <= *length {
                return SourcePos {
                    run,
                    offset: remaining,
                };
            }
            remaining -= length;
        }
        SourcePos {
            run: self.runs.len().saturating_sub(1),
            offset: self.runs.last().copied().unwrap_or(0),
        }
    }

    /// The offset in the editing model's own text a source position names.
    pub fn to_model(&self, position: SourcePos) -> usize {
        let before: usize = self.runs.iter().take(position.run).sum();
        before
            + position
                .offset
                .min(self.runs.get(position.run).copied().unwrap_or(0))
    }

    /// The offset in the shaper's string a model offset names.
    pub fn to_generated(&self, offset: usize) -> Option<usize> {
        self.map.to_generated_snapped(self.to_source(offset))
    }

    /// The model offset a point lands on, and which of that offset's two carets it means.
    ///
    /// This is the whole of click-to-place-caret: the point is brought into the paragraph's own
    /// space, the line and cluster it landed in are found from the shaping, and the offset that
    /// comes back is mapped through to the text the document holds.
    ///
    /// "The paragraph's own space" is two steps, not one. A pointer is reported in the surface's
    /// coordinates; the lines were measured before any transform above them was applied. So the
    /// point goes *backwards* through that transform first and only then has the paragraph's corner
    /// taken off it. Skipping the first step costs nothing on an upright page — the matrix is a
    /// translation the fragment's own origin already accounts for — and puts the caret in front of
    /// the wrong character on every turned, scaled or skewed one, by an amount that grows with the
    /// distance from the transform's origin. Which is a field that reads, draws and reports
    /// perfectly and answers the pointer with a letter nobody clicked.
    ///
    /// Nothing at all when the transform collapses the plane, because a paragraph scaled to zero
    /// covers no pixel and no point is on it.
    ///
    /// The affinity comes back with it because an offset alone does not say where the caret goes.
    /// At a soft line break one offset is both the end of one line and the start of the next, and
    /// at a direction boundary it is two places on the same line. Dropping the affinity here and
    /// letting the selection default to one of the two readings puts the caret at the end of the
    /// previous line for every click on the left edge of a wrapped one.
    pub fn hit(
        &self,
        point: Point<DevicePx, Device>,
        spatial: &zgui_scene::SpatialTree,
    ) -> Option<(usize, Affinity)> {
        let placed =
            zgui_layout::fragment::hit::transform::into_local(point, self.transform, spatial)?;
        let local = Point::<CssPx, Css>::new(
            CssPx(placed.x.0 - self.origin.x.0),
            CssPx(placed.y.0 - self.origin.y.0),
        );
        let hit = self.lines.hit(local)?;
        let source = self
            .map
            .to_source(hit.offset)
            .or_else(|| self.map.to_source_snapped(hit.offset))?;
        Some((self.to_model(source), hit.affinity))
    }

    /// Where the caret for a model offset is drawn, in the paragraph's own coordinates.
    ///
    /// A field a person has just emptied is the one paragraph no offset maps through. The map
    /// records the stretches of source text that survived generation, and a paragraph holding
    /// nothing produced no stretches at all — so the offset the model reports, which can only be
    /// zero, maps to nothing and the field shows no insertion point at all. Not a missing line: the
    /// line box is there, with the height its strut gives it, and it holds no cluster for an offset
    /// to be found against. There is exactly one place a caret can go on such a paragraph, and it
    /// is the start of that line, which is where the first character typed will appear.
    pub fn caret(&self, offset: usize, affinity: Affinity) -> Option<Caret> {
        let generated = match self.to_generated(offset) {
            Some(generated) => generated,
            None if self.map.is_empty() => 0,
            None => return None,
        };
        self.lines.caret(generated, affinity)
    }

    /// The bands a model range is painted as, in the paragraph's own coordinates.
    ///
    /// An empty range paints nothing, and a range whose ends do not both survive generation paints
    /// nothing rather than something arbitrary: a band drawn from a guessed offset covers text
    /// nobody selected.
    pub fn bands(&self, range: core::ops::Range<usize>) -> Vec<Band> {
        if range.is_empty() {
            return Vec::new();
        }
        let (Some(start), Some(end)) =
            (self.to_generated(range.start), self.to_generated(range.end))
        else {
            return Vec::new();
        };
        self.lines.highlight(start.min(end)..start.max(end))
    }

    /// Where the caret lands when it moves one line up or down, and the column it aimed for.
    ///
    /// The column is `goal` when one is held and the caret's own x otherwise; it comes back with
    /// the answer so the caller can keep it for the next step. Passing a short line without the
    /// goal would walk the caret leftwards line by line, losing the column the motion started in.
    ///
    /// Nothing when there is no line in that direction, which the caller reads as the document's
    /// own edge: a single-line field answers every vertical motion that way.
    pub fn line_step(
        &self,
        offset: usize,
        affinity: Affinity,
        down: bool,
        goal: Option<f32>,
    ) -> Option<(usize, Affinity, f32)> {
        let caret = self.caret(offset, affinity)?;
        let x = goal.unwrap_or(caret.origin.x.0);
        let target = if down {
            caret.line.checked_add(1)?
        } else {
            caret.line.checked_sub(1)?
        };
        let line = self.lines.lines().get(target)?;
        let y = line.geometry.top.0 + line.geometry.height.0 / 2.0;
        let hit = self.lines.hit(Point::new(CssPx(x), CssPx(y)))?;
        let source = self
            .map
            .to_source(hit.offset)
            .or_else(|| self.map.to_source_snapped(hit.offset))?;
        Some((self.to_model(source), hit.affinity, x))
    }

    /// Where one line's box sits relative to the paragraph's top-left corner.
    ///
    /// The same corner the line's own fragment was placed at, which is what makes a rectangle
    /// expressed against it land where the glyphs of that line landed however the paragraph is
    /// scrolled or transformed.
    pub fn line_origin(&self, line: usize) -> Option<Point<CssPx, Css>> {
        let geometry = &self.lines.lines().get(line)?.geometry;
        Some(Point::new(geometry.offset, geometry.top))
    }
}
