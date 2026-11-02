//! The widths over which a broken paragraph keeps its lines.
//!
//! # Why the engine's own pass tells the widths
//!
//! The engine's line breaker is greedy. It walks the clusters and inline boxes of a line in order,
//! adds each advance to the line, and compares the sum with the width. Each other decision it
//! makes reads only the clusters, the boxes and the decisions before it. So a pass at a second
//! width where each of those comparisons gives the same answer takes the same decisions and
//! gives the same lines, bit for bit. This module reads those comparisons back out of a finished
//! pass and keeps the widths where all of them hold. A later pass at a width inside them is
//! answered from the lines the layout already holds, and the engine does not break again.
//!
//! # What the pass is read from
//!
//! The comparisons are a function of the breaker's steps (a [`Step`] table built once per shaped
//! paragraph) and of where each line ends. Before a line's end, every step was compared. After it,
//! the breaker either stopped at once or went on until a step overflowed and then went back to the
//! line's end. In the second case the line still ends there at a second width when some step of
//! the word after it overflows before the next place a break can be taken. Only a word boundary
//! the engine reports, or the place after an inline box, can be such a place, which bounds the
//! word. For an emergency break, each cluster that does not continue a ligature can be such a
//! place.
//!
//! # What is not covered
//!
//! Right-to-left text, bands, justification, an alignment other than start or left, and a
//! paragraph that may not wrap and holds inline boxes: alignment reads the width for the line
//! offsets, and the others either change the cluster advances or take decisions this module does
//! not repeat. A paragraph with any of them breaks in the engine on each pass. A pass that moves an
//! inline box breaks in the engine too. So does each pass of a paragraph whose step table cannot
//! be built: one with a right-to-left run, an out-of-flow box, or a box out of text order or
//! inside a run.

use parley::{BreakReason, IndentOptions, InlineBox, InlineBoxKind};

use crate::shape::brush::SlotBrush;

/// The first cluster of a step is a hard line break.
const NEWLINE: u8 = 1;
/// The first cluster of a step is a space or a no-break space.
const SPACE: u8 = 2;
/// The first cluster of a step is a word boundary, which every line break opportunity is.
const WORD: u8 = 4;
/// The first cluster of a step continues a ligature.
const CONTINUES: u8 = 8;
/// The step is an inline box.
const BOX: u8 = 16;
/// The first cluster of a step starts a ligature. Used while building only.
const LIGATURE: u8 = 32;

/// One step of the engine's breaker: one cluster, a ligature start with the clusters the engine
/// adds to it, or one inline box.
#[derive(Clone, Copy, Debug)]
struct Step {
    /// What a cluster step adds to its line, summed in the engine's order. A box step reads its
    /// box instead, because a box changes its width without a new shaping.
    advance: f32,
    /// How many clusters the step takes, or the index of the box of a box step.
    count: u16,
    /// What the step is.
    flags: u8,
}

impl Step {
    fn is(self, flags: u8) -> bool {
        self.flags & flags != 0
    }

    /// How many clusters the step takes.
    fn clusters(self) -> usize {
        if self.is(BOX) {
            0
        } else {
            usize::from(self.count)
        }
    }

    /// What the step adds to its line, as the engine computes it.
    fn advance(self, boxes: &[InlineBox]) -> f32 {
        if !self.is(BOX) {
            return self.advance;
        }
        match boxes.get(usize::from(self.count)) {
            Some(inline_box) if inline_box.kind == InlineBoxKind::InFlow => inline_box.width,
            _ => 0.0,
        }
    }
}

/// A half-open interval of line widths.
#[derive(Clone, Copy, Debug)]
struct Span {
    min: f32,
    end: f32,
}

impl Span {
    const ANY: Self = Self {
        min: f32::NEG_INFINITY,
        end: f32::INFINITY,
    };

    /// Narrows the span to the widths that compare `advance` as `width` does.
    fn observe(&mut self, advance: f32, width: f32) {
        if advance <= width {
            self.min = self.min.max(advance);
        } else {
            self.end = self.end.min(advance);
        }
    }

    fn holds(self, width: f32) -> bool {
        self.min <= width && width < self.end
    }

    fn meet(self, other: Self) -> Self {
        Self {
            min: self.min.max(other.min),
            end: self.end.min(other.end),
        }
    }
}

/// What a pass was taken with besides its width, bit for bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Terms {
    indent: u32,
    each_line: bool,
    hanging: bool,
    boxes: bool,
}

impl Terms {
    fn of(indent: f32, options: IndentOptions, boxes: bool) -> Self {
        Self {
            indent: indent.to_bits(),
            each_line: options.each_line,
            hanging: options.hanging,
            boxes,
        }
    }
}

/// The widths the current break holds over.
///
/// The engine subtracts a line's indent from the width before it compares, so the lines with an
/// indent have a span of their own.
#[derive(Clone, Copy, Debug)]
struct Held {
    terms: Terms,
    plain: Span,
    indented: Span,
}

/// One paragraph's step table, and the widths its current break holds over.
#[derive(Debug)]
struct Table {
    /// Empty when the paragraph is not covered.
    steps: Box<[Step]>,
    held: Option<Held>,
}

/// What a shaped paragraph knows about the widths its lines hold over.
///
/// The step table is built at the second covered pass, so a paragraph broken once pays nothing.
#[derive(Debug, Default)]
pub(crate) struct Reach(Option<Box<Table>>);

impl Reach {
    /// Whether a pass at `max_advance` gives the lines the layout holds now.
    ///
    /// The caller makes sure that the pass is covered (see the module documentation) and that no
    /// inline box changed its size since the pass before.
    pub(crate) fn holds(
        &self,
        max_advance: Option<f32>,
        indent: f32,
        options: IndentOptions,
        boxes: bool,
    ) -> bool {
        let Some(held) = self.0.as_ref().and_then(|table| table.held) else {
            return false;
        };
        let width = max_advance.unwrap_or(f32::MAX);
        held.terms == Terms::of(indent, options, boxes)
            && held.plain.holds(width)
            && held.indented.holds(width - indent)
    }

    /// Records the widths the engine's pass that just ran holds over.
    ///
    /// `covered` is whether the pass is one this module covers. `rebroken` is whether a covered
    /// pass ran before; the step table is built only then. `boxes` is whether the pass reported
    /// box placements.
    #[expect(
        clippy::too_many_arguments,
        reason = "the terms of one breaking pass, read once"
    )]
    pub(crate) fn measure(
        &mut self,
        layout: &parley::Layout<SlotBrush>,
        covered: bool,
        rebroken: bool,
        max_advance: Option<f32>,
        indent: f32,
        options: IndentOptions,
        boxes: bool,
    ) {
        if !covered {
            if let Some(table) = &mut self.0 {
                table.held = None;
            }
            return;
        }
        if self.0.is_none() && !rebroken {
            return;
        }
        let table = self.0.get_or_insert_with(|| {
            Box::new(Table {
                steps: steps(layout).unwrap_or_default(),
                held: None,
            })
        });
        table.held = if table.steps.is_empty() {
            None
        } else {
            held(
                &table.steps,
                layout,
                max_advance.unwrap_or(f32::MAX),
                Terms::of(indent, options, boxes),
                indent,
            )
        };
    }

    /// Drops the step table. A justified pass moves cluster advances, and the engine does not
    /// restore them bit for bit.
    pub(crate) fn forget(&mut self) {
        self.0 = None;
    }

    /// The heap bytes the table holds.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.0.as_ref().map_or(0, |table| {
            core::mem::size_of::<Table>() + core::mem::size_of_val(&*table.steps)
        })
    }
}

/// A run of the engine or an inline box, in the order the breaker meets them.
enum Item {
    /// The run's cluster range.
    Run(usize, usize),
    /// The box's index.
    Box(u16),
}

/// Builds the step table of a broken layout, or `None` for a paragraph this module does not
/// cover.
fn steps(layout: &parley::Layout<SlotBrush>) -> Option<Box<[Step]>> {
    if layout.is_rtl() {
        return None;
    }
    let boxes = layout.inline_boxes();
    // The engine places a box before the run that starts at the box's text offset, and the boxes
    // after the text at its end. It meets the boxes in their order, which must follow the text.
    if boxes.windows(2).any(|pair| pair[1].index < pair[0].index)
        || boxes
            .iter()
            .any(|inline_box| inline_box.kind == InlineBoxKind::CustomOutOfFlow)
    {
        return None;
    }
    let mut advances: Vec<f32> = Vec::new();
    let mut flags: Vec<u8> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    let mut placed = 0;
    let mut text_end = 0;
    // A run broken across lines comes back in pieces, and the pieces of one run share its metrics
    // record.
    let mut last: *const parley::RunMetrics = core::ptr::null();
    for line in layout.lines() {
        for run in line.runs() {
            let range = run.cluster_range();
            if range.is_empty() {
                continue;
            }
            if run.is_rtl() || range.start != advances.len() {
                return None;
            }
            let text = run.text_range().start;
            text_end = run.text_range().end;
            while let Some(inline_box) = boxes.get(placed)
                && inline_box.index <= text
            {
                // A box inside a run would have split it.
                if inline_box.index != text {
                    return None;
                }
                items.push(Item::Box(u16::try_from(placed).ok()?));
                placed += 1;
                last = core::ptr::null();
            }
            for cluster in run.clusters() {
                let advance = cluster.advance();
                if !advance.is_finite() {
                    return None;
                }
                advances.push(advance);
                flags.push(flags_of(&cluster));
            }
            if advances.len() != range.end {
                return None;
            }
            let identity = core::ptr::from_ref(run.metrics());
            match items.last_mut() {
                Some(Item::Run(_, end)) if core::ptr::eq(identity, last) => *end = range.end,
                _ => items.push(Item::Run(range.start, range.end)),
            }
            last = identity;
        }
    }
    for (index, inline_box) in boxes.iter().enumerate().skip(placed) {
        if inline_box.index < text_end {
            return None;
        }
        items.push(Item::Box(u16::try_from(index).ok()?));
    }

    let mut steps = Vec::with_capacity(advances.len() + boxes.len());
    let mut at = 0;
    for item in items {
        let (start, end) = match item {
            Item::Run(start, end) => (start, end),
            Item::Box(index) => {
                steps.push(Step {
                    advance: 0.0,
                    count: index,
                    flags: BOX,
                });
                continue;
            }
        };
        if at != start {
            return None;
        }
        while at < end {
            let first = flags[at];
            let mut advance = advances[at];
            let mut index = at;
            // The engine reads the clusters after a ligature start with the paragraph's cluster
            // index as an index into the run, so in any run after the first it reads clusters
            // `start` places further on. This repeats that reading.
            if first & (NEWLINE | LIGATURE) == LIGATURE {
                while flags
                    .get(start + index + 1)
                    .is_some_and(|next| next & CONTINUES != 0)
                {
                    advance += advances[start + index + 1];
                    index += 1;
                }
            }
            let count = u16::try_from(index - at + 1).ok()?;
            if index >= end {
                return None;
            }
            steps.push(Step {
                advance,
                count,
                flags: first & !LIGATURE,
            });
            at = index + 1;
        }
    }
    (at == advances.len() && at > 0).then(|| steps.into_boxed_slice())
}

fn flags_of(cluster: &parley::Cluster<'_, SlotBrush>) -> u8 {
    let mut flags = 0;
    if cluster.is_hard_line_break() {
        flags |= NEWLINE;
    }
    if cluster.is_space_or_nbsp() {
        flags |= SPACE;
    }
    if cluster.is_word_boundary() {
        flags |= WORD;
    }
    if cluster.is_ligature_continuation() {
        flags |= CONTINUES;
    }
    if cluster.is_ligature_start() {
        flags |= LIGATURE;
    }
    flags
}

/// The widths the lines of `layout`, broken at `width`, hold over. `None` when a line is not one
/// this reading can account for.
fn held(
    steps: &[Step],
    layout: &parley::Layout<SlotBrush>,
    width: f32,
    terms: Terms,
    indent: f32,
) -> Option<Held> {
    let boxes = layout.inline_boxes();
    let mut plain = Span::ANY;
    let mut indented = Span::ANY;
    let mut at = 0;
    let mut cluster = 0;
    let mut previous = None;
    let count = layout.len();
    for (index, line) in layout.lines().enumerate() {
        let end = line
            .runs()
            .last()
            .map_or(cluster, |run| run.cluster_range().end);
        let on_line = line.len() - line.runs().count();
        // The engine's own choice of the lines that take the indent.
        let scope = if terms.each_line {
            index == 0 || previous == Some(BreakReason::Explicit)
        } else {
            index == 0
        };
        let line_indent = if scope != terms.hanging { indent } else { 0.0 };
        let limit = width - line_indent;
        let reason = line.break_reason();

        let first = at;
        let mut placed = 0;
        let mut advance = 0.0_f32;
        let mut before = 0.0_f32;
        let mut span = Span::ANY;
        while let Some(&step) = steps.get(at) {
            if cluster >= end && !(step.is(BOX) && placed < on_line) {
                break;
            }
            // A hard break ends its line before the comparison and adds nothing to it.
            if !step.is(NEWLINE) {
                before = advance;
                advance += step.advance(boxes);
                span.observe(advance, limit);
            }
            cluster += step.clusters();
            placed += usize::from(step.is(BOX));
            at += 1;
        }
        if cluster != end || placed != on_line {
            return None;
        }
        let empty = at == first;
        if empty {
            // Only the line after a hard break that ends the text is empty, and the engine gives
            // it the metrics of the line before it.
            if reason != BreakReason::None || index + 1 != count {
                return None;
            }
        } else if advance.to_bits() != line.metrics().advance.to_bits() {
            return None;
        }
        match reason {
            BreakReason::Explicit => {
                if empty || !steps[at - 1].is(NEWLINE) {
                    return None;
                }
            }
            BreakReason::None => {
                if at != steps.len() {
                    return None;
                }
            }
            BreakReason::Regular | BreakReason::Emergency => {
                if empty {
                    return None;
                }
                let line = Ended {
                    at,
                    advance,
                    before,
                    limit,
                    regular: reason == BreakReason::Regular,
                };
                line.narrow(steps, boxes, &mut span)?;
            }
        }
        if !span.holds(limit) {
            return None;
        }
        if line_indent == 0.0 {
            plain = plain.meet(span);
        } else {
            indented = indented.meet(span);
        }
        previous = Some(reason);
    }
    (at == steps.len()).then_some(Held {
        terms,
        plain,
        indented,
    })
}

/// A line the engine ended softly, before step `at`.
struct Ended {
    at: usize,
    /// The line's advance.
    advance: f32,
    /// The line's advance before its last step.
    before: f32,
    /// The width the pass compared the line with.
    limit: f32,
    /// Whether the engine took a regular break. False for an emergency break.
    regular: bool,
}

impl Ended {
    /// Narrows `span` to the widths at which the line still ends where it does.
    ///
    /// The steps before the line's end are already in `span`. The engine ended the line in one of
    /// these ways:
    ///
    /// 1. It hung a space that overflowed.
    /// 2. It kept a box that overflowed at the start of a line on that line.
    /// 3. It moved a box that overflowed to the next line. The box must still overflow.
    /// 4. It went on past the line's end until a step overflowed, and went back to the last break
    ///    opportunity, which is where the line ends. The line then also ends there at each width
    ///    at which some step overflows before the next opportunity. See
    ///    [`word_after`](Ended::word_after).
    ///
    /// The ways cannot always be told apart, so the span holds for each one that is possible.
    /// `None` when none of them is.
    fn narrow(&self, steps: &[Step], boxes: &[InlineBox], span: &mut Span) -> Option<()> {
        let last = steps[self.at - 1];
        let next = steps.get(self.at).copied();
        let overflowed = self.advance > self.limit;
        let mut possible = (self.regular && last.is(SPACE) && overflowed)
            || (!self.regular && last.is(BOX) && self.before == 0.0 && overflowed);
        let mut end = f32::INFINITY;
        if let Some(next) = next {
            if next.is(BOX) {
                let moved = self.advance + next.advance(boxes);
                if self.regular && self.advance != 0.0 && moved > self.limit {
                    possible = true;
                    end = end.min(moved);
                }
            } else if !next.is(NEWLINE) {
                let opportunity = if self.regular {
                    (next.is(WORD) && !next.is(CONTINUES)) || last.is(BOX)
                } else {
                    !next.is(CONTINUES)
                };
                if opportunity && let Some(word) = self.word_after(steps) {
                    possible = true;
                    span.min = span.min.max(word.min);
                    end = end.min(word.end);
                }
            }
        }
        span.end = span.end.min(end);
        possible.then_some(())
    }

    /// The widths at which some step after the line's end overflows before the next break
    /// opportunity, for a line the engine went back to the end of.
    ///
    /// - Up to the step that overflowed, the pass saw each step and took none as an opportunity.
    ///   A space among them hangs if it overflows, so it must still fit. A box or a hard break
    ///   among them would have ended the line elsewhere.
    /// - After that step, the next word boundary may be an opportunity, and the next space, box
    ///   or hard break may end the line itself. The steps stop before either.
    ///
    /// `None` when no step overflowed before such a stop, so the line did not end this way.
    fn word_after(&self, steps: &[Step]) -> Option<Span> {
        let mut x = self.advance;
        let mut widest = f32::NEG_INFINITY;
        let mut min = f32::NEG_INFINITY;
        for (index, step) in steps.iter().enumerate().skip(self.at) {
            if step.is(NEWLINE | BOX) {
                break;
            }
            if index > self.at && widest > self.limit {
                let may_break =
                    step.is(WORD) || (!self.regular && !step.is(CONTINUES)) || step.is(SPACE);
                if may_break {
                    break;
                }
            }
            x += step.advance;
            if step.is(SPACE) && x <= self.limit {
                min = min.max(x);
            }
            widest = widest.max(x);
        }
        (widest > self.limit).then_some(Span { min, end: widest })
    }
}
