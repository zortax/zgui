//! What the shaper is told about the things on a line that are not glyphs.
//!
//! Two different things reach a shaper as one opaque box. An atomic inline is a box with content of
//! its own, and its size is a whole nested layout; the edges of a nested inline box are its margin,
//! border and padding on that side, which occupy width on the line and hold nothing at all.
//!
//! Everything here is resolved afresh on every measure call, and that is the point rather than an
//! oversight. An atomic can come out at a different size under a different constraint, a percentage
//! margin resolves against a containing block that the algorithm is still deciding, and a
//! `vertical-align` shift is a function of both — none of which a shaper can notice, because all
//! three are baked into numbers it was handed once. Recomputing them here, and letting the break
//! key cover them, is what stops a re-style against a warm cache from being a silent no-op.

use cephal::{AvailableSpace, Rect, Size};
use zgui_css::ComputedStyle;
use zgui_css::values::text::{AlignmentBaseline, BaselineShift, BaselineSource};
use zgui_dom::side::BoxKey;
use zgui_geom::CssPx;
use zgui_text::{InlineBoxGeometry, StrutMetrics};

use crate::inline::atomic;
use crate::inline::content::{Generated, Role};
use crate::inline::insets::edges_of;
use crate::inline::vertical_align::{self, Alignment, scale_strut};
use crate::measure::MeasureContent;
use crate::node::kind::FormattingContext;
use crate::tree::LayoutTree;

/// What every inline box on the line currently measures, and how each is aligned.
#[derive(Clone, Debug, Default)]
pub(crate) struct Boxes {
    /// The geometry the shaper is handed, in item order.
    pub(crate) geometry: Vec<InlineBoxGeometry>,
    /// How each of them is aligned, parallel to `geometry`.
    pub(crate) alignments: Vec<Alignment>,
    /// The result an atomic inline's own layout produced, parallel to `geometry`.
    ///
    /// Kept because nothing else writes it: the algorithms position a box they laid out
    /// themselves, and an atomic inline is laid out by the line it sits on.
    pub(crate) frames: Vec<Option<Frame>>,
    /// How far each nested inline box's edge raises the box's text above the line's baseline,
    /// summed over the boxes it is inside, by item index.
    ///
    /// Empty when no nested inline box is shifted, and shorter than `geometry` when the last
    /// shifted edge comes before the last item: a missing entry is zero.
    pub(crate) shifts: Vec<f32>,
}

impl Boxes {
    /// The capacity of the vectors these boxes hold, in bytes.
    pub(crate) fn heap_bytes(&self) -> usize {
        use core::mem::size_of;
        self.geometry.capacity() * size_of::<InlineBoxGeometry>()
            + self.alignments.capacity() * size_of::<Alignment>()
            + self.frames.capacity() * size_of::<Option<Frame>>()
            + self.shifts.capacity() * size_of::<f32>()
    }
}

/// One atomic inline's own resolved box, for writing back into the tree.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    /// The border box.
    pub(crate) size: Size<f32>,
    /// The resolved margins, which the line has already consumed.
    pub(crate) margin: Rect<f32>,
    /// The resolved padding.
    pub(crate) padding: Rect<f32>,
    /// The resolved border widths.
    pub(crate) border: Rect<f32>,
}

impl Boxes {
    /// Whether any box has to wait for its line box before it can be placed.
    pub(crate) fn needs_line_box(&self) -> bool {
        self.alignments
            .iter()
            .any(|alignment| alignment.needs_line_box())
    }
}

/// Lays out every atomic inline and resolves every edge, under one constraint.
pub(crate) fn resolve<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    generated: &Generated,
    strut: &StrutMetrics,
    available: Size<AvailableSpace>,
    basis: Option<f32>,
    basis_height: Option<f32>,
) -> Boxes {
    let mut boxes = Boxes::default();
    // The nested inline boxes open at this item: the shift of each, summed over the boxes it is
    // inside, and the raised geometry its edges carry when it moves off its parent's baseline.
    let mut open: smallvec::SmallVec<[Raise; 4]> = smallvec::SmallVec::new();
    for (index, item) in generated.items.iter().enumerate() {
        let key = item.role.box_();
        let (geometry, alignment, frame) = match item.role {
            Role::Atomic(_) => atomic_box(
                tree,
                key,
                item.id,
                item.offset,
                available,
                Size {
                    width: basis,
                    height: basis_height,
                },
                strut,
            ),
            Role::StartEdge(_) => {
                let mut geometry = edge(tree, key, item.id, item.offset, basis, true);
                let raise = raise_of(tree, key, open.last(), strut);
                raise.apply(&mut geometry);
                boxes.record_shift(index, raise.shift);
                open.push(raise);
                (geometry, Alignment::Baseline, None)
            }
            Role::EndEdge(_) => {
                let mut geometry = edge(tree, key, item.id, item.offset, basis, false);
                if let Some(raise) = open.pop() {
                    raise.apply(&mut geometry);
                    boxes.record_shift(index, raise.shift);
                }
                (geometry, Alignment::Baseline, None)
            }
        };
        boxes.geometry.push(geometry);
        boxes.alignments.push(alignment);
        boxes.frames.push(frame);
    }
    boxes
}

impl Boxes {
    /// Records the summed shift of the nested inline box whose edge is item `index`.
    fn record_shift(&mut self, index: usize, shift: f32) {
        if shift == 0.0 {
            return;
        }
        if self.shifts.len() <= index {
            self.shifts.resize(index + 1, 0.0);
        }
        self.shifts[index] = shift;
    }
}

/// How one nested inline box moves its text off the line's baseline.
#[derive(Clone, Copy, Debug)]
struct Raise {
    /// The box's own shift plus the shifts of every box it is inside, positive upwards.
    shift: f32,
    /// The box's own strut, when the box moves off its parent's baseline; a box inside it
    /// resolves its alignment against it.
    own: Option<StrutMetrics>,
}

impl Raise {
    /// Gives an edge the box's line-height-tall extent at its shift, so the line grows to hold
    /// the shifted text. An edge of a box on its parent's baseline keeps no height, exactly as an
    /// unshifted box's does.
    fn apply(&self, geometry: &mut InlineBoxGeometry) {
        if let Some(strut) = self.own {
            geometry.height = strut.line_height;
            geometry.ascent = strut.ascent();
            geometry.shift = CssPx(self.shift);
        }
    }
}

/// How a nested inline box is raised, given the box it is inside and the context's strut.
///
/// A box at the initial `vertical-align` inside no raised box is the common case, and it is
/// answered without measuring a strut. `top` and `bottom` align to the line box, which a nested
/// inline box does not wait for; such a box stays on its parent's baseline.
fn raise_of<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    parent: Option<&Raise>,
    root: &StrutMetrics,
) -> Raise {
    let inherited = parent.map_or(0.0, |parent| parent.shift);
    let style = &tree.structure().node(key).style;
    let group = style.get_box();
    let baseline = group.alignment_baseline == AlignmentBaseline::Baseline
        && group.baseline_shift == BaselineShift::zero();
    if baseline {
        return Raise {
            shift: inherited,
            own: None,
        };
    }
    let style = style.clone();
    let scale = tree.device().scale;
    let lowered = tree.text_styles_mut().get(&style);
    let own = scale_strut(tree.content().strut(&lowered.text), scale);
    let alignment = match vertical_align::of(&style, own.line_height.0, scale) {
        alignment if alignment.needs_line_box() => Alignment::Baseline,
        alignment => alignment,
    };
    let against = parent.and_then(|parent| parent.own).unwrap_or(*root);
    let shift = vertical_align::resolve(alignment, own.ascent().0, own.line_height.0, &against);
    if shift == 0.0 {
        return Raise {
            shift: inherited,
            own: None,
        };
    }
    Raise {
        shift: inherited + shift,
        own: Some(own),
    }
}

/// One atomic inline's margin box, its own baseline, and its shift.
fn atomic_box<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    id: u64,
    offset: usize,
    available: Size<AvailableSpace>,
    basis: Size<Option<f32>>,
    strut: &StrutMetrics,
) -> (InlineBoxGeometry, Alignment, Option<Frame>) {
    let (margins, padding, border) = crate::inline::insets::frame_of(tree, key, basis.width);
    let measured = atomic::measure(tree, key, Size::NONE, available, basis.height);
    let node = tree.structure().node(key);
    let style = node.style.clone();
    let replaced = node.fc == FormattingContext::Replaced;
    let scale = tree.device().scale;

    let width = measured.size.width + margins.left + margins.right;
    let height = measured.size.height + margins.top + margins.bottom;
    // A replaced box has no baseline of its own, so its bottom margin edge is the baseline. So does
    // a box that scrolls or clips its own content: its last line can be scrolled out of sight, and
    // a line of text outside it aligned to a baseline that is not on screen would sit anywhere at
    // all. Everything else is aligned by a line box of its own, and which one is what
    // `baseline-source` chooses: the last for something in normal flow, which is what an
    // `inline-block` is, and the first when the style asks for it.
    let own = if replaced || clips_its_own_content(tree, key) {
        None
    } else {
        match style.get_box().baseline_source {
            BaselineSource::First => measured.first_baseline,
            BaselineSource::Last | BaselineSource::Auto => {
                measured.last_baseline.or(measured.first_baseline)
            }
        }
    };
    let ascent = own.map_or(height, |baseline| baseline + margins.top);
    let line_height = resolved_line_height(tree, &style, scale);
    let alignment = vertical_align::of(&style, line_height, scale);
    let shift = vertical_align::resolve(alignment, ascent, height, strut);

    (
        InlineBoxGeometry {
            id,
            offset,
            width: CssPx(width),
            height: CssPx(height),
            ascent: CssPx(ascent),
            shift: CssPx(shift),
        },
        alignment,
        Some(Frame {
            size: measured.size,
            margin: margins,
            padding,
            border,
        }),
    )
}

/// Whether a box keeps its content to itself on either axis.
///
/// Anything but `visible` on either axis does: `hidden`, `clip`, `scroll` and `auto` all cut the
/// content off at the box's own edges, and a baseline taken from a line that can be cut off is not
/// a baseline anything outside the box can align to.
fn clips_its_own_content<C: MeasureContent>(tree: &LayoutTree<'_, C>, key: BoxKey) -> bool {
    let overflow = tree.engine_style(key).overflow;
    overflow.x != cephal::style::Overflow::Visible || overflow.y != cephal::style::Overflow::Visible
}

/// One edge of a nested inline box: the width it occupies, and nothing else.
fn edge<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    id: u64,
    offset: usize,
    basis: Option<f32>,
    start: bool,
) -> InlineBoxGeometry {
    let edges = edges_of(tree, key, basis);
    let width = if start { edges.0 } else { edges.1 };
    InlineBoxGeometry {
        id,
        offset,
        width: CssPx(width),
        height: CssPx::ZERO,
        ascent: CssPx::ZERO,
        shift: CssPx::ZERO,
    }
}

/// One box's own resolved `line-height`, which a percentage shift is measured against.
///
/// Taken from the box's own strut rather than from its style alone, because `line-height: normal`
/// is a property of the face and not of the style — and because asking for it here is free: a strut
/// is held against the style it was measured from.
fn resolved_line_height<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    style: &ComputedStyle,
    scale: f32,
) -> f32 {
    let lowered = tree.text_styles_mut().get(style);
    tree.content().strut(&lowered.text).line_height.0 * scale
}
