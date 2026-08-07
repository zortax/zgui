//! The seam a custom element's layout half plugs into.
//!
//! A custom element is a leaf to CSS — the shell arithmetic around it (box-sizing, min and max,
//! aspect ratio, the inline atom machinery) is exactly a replaced box's — and a container to
//! itself: its implementation measures its content and places its children, reached through the
//! same nested-layout door an atomic inline uses. This module is the engine's side of that
//! bargain: an object-safe view of the tree ([`LayoutAccess`]), the context one measurement runs
//! in ([`CustomLayoutCx`]), and the source the pass resolves elements through
//! ([`CustomLayoutSource`]).
//!
//! Nothing here names the application's trait. The bridge crate that owns the user-facing
//! `CustomElement` implements [`CustomLayoutSource`] over its registry, which is the same
//! arms-length arrangement every other content seam has: the engine states what it needs, and who
//! answers is a decision made two crates up.

use taffy::{AvailableSpace, LayoutInput, LayoutPartialTree, RunMode, Size, SizingMode};
use zgui_css::ComputedStyle;
use zgui_dom::side::BoxKey;

use crate::key::to_node_id;
use crate::measure::{MeasureContent, Measured};
use crate::tree::LayoutTree;

/// What a custom element's layout produced: its content size, and its own baselines.
///
/// Everything is in device pixels, as every length the layout algorithms speak is. The size is
/// the *content* size — the shell adds padding, borders and the style's own constraints around
/// it, so an implementation never re-derives CSS.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CustomMeasured {
    /// The content's width.
    pub width: f32,
    /// The content's height.
    pub height: f32,
    /// Where the first baseline sits, down from the content's top, if the element has one.
    pub first_baseline: Option<f32>,
    /// Where the last baseline sits, measured the same way.
    pub last_baseline: Option<f32>,
}

/// The space offered on one axis, in the seam's own vocabulary.
///
/// The layout algorithms speak their own type for this, and that type is deliberately named by
/// this crate alone; a custom element written two crates up speaks this one, which converts
/// losslessly at the seam.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Space {
    /// This many device pixels are available.
    Definite(f32),
    /// The probe asking how small the content can be.
    MinContent,
    /// The probe asking how large it wants to be.
    MaxContent,
}

impl Space {
    /// The algorithms' own form.
    fn to_taffy(self) -> AvailableSpace {
        match self {
            Self::Definite(space) => AvailableSpace::Definite(space),
            Self::MinContent => AvailableSpace::MinContent,
            Self::MaxContent => AvailableSpace::MaxContent,
        }
    }

    /// This form, from the algorithms' own.
    fn from_taffy(space: AvailableSpace) -> Self {
        match space {
            AvailableSpace::Definite(value) => Self::Definite(value),
            AvailableSpace::MinContent => Self::MinContent,
            AvailableSpace::MaxContent => Self::MaxContent,
        }
    }
}

/// An object-safe view of the tree, for a custom element that measures and places its children.
///
/// Children are the element's ordinary document children: they were built into boxes exactly as
/// any container's are, and only their *placement* is the element's. The recipe is an atomic
/// inline's, behind indices so the trait stays object-safe: measure whichever children the answer
/// needs, and on the final pass lay each out and place it.
pub trait LayoutAccess {
    /// How many child boxes the element has.
    fn child_count(&self) -> usize;

    /// Measures child `index` under the given constraints, without keeping the result.
    ///
    /// The probe form: cheap to repeat, answered from the layout cache when the same question was
    /// asked before, and committed to nothing. `known` is what the element has already decided,
    /// per axis; `available` is the space it is offering.
    fn measure_child(
        &mut self,
        index: usize,
        known: (Option<f32>, Option<f32>),
        available: (Space, Space),
    ) -> ChildMeasure;

    /// Lays child `index` out at the given constraints, keeping the result.
    ///
    /// The kept form, for the final pass: the child's own subtree is laid out for real, and what
    /// this returns is what [`LayoutAccess::place_child`] places.
    fn layout_child(
        &mut self,
        index: usize,
        known: (Option<f32>, Option<f32>),
        available: (Space, Space),
    ) -> ChildMeasure;

    /// Writes child `index`'s origin, in device pixels from the element's border-box corner.
    ///
    /// Nothing above the element will move the child again: a custom element's children are laid
    /// out by it the way a line box's atoms are laid out by the line.
    fn place_child(&mut self, index: usize, x: f32, y: f32);
}

/// What measuring or laying out one child produced, in device pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChildMeasure {
    /// The child's width.
    pub width: f32,
    /// Its height.
    pub height: f32,
    /// Its first baseline, down from its own top, when it has one.
    pub first_baseline: Option<f32>,
    /// Its last baseline, measured the same way.
    pub last_baseline: Option<f32>,
}

impl ChildMeasure {
    /// The seam's form of one measured answer.
    fn of(measured: Measured) -> Self {
        Self {
            width: measured.size.width,
            height: measured.size.height,
            first_baseline: measured.first_baseline,
            last_baseline: measured.last_baseline,
        }
    }
}

/// Everything one custom layout call is given.
pub struct CustomLayoutCx<'a> {
    /// The tree, behind the object-safe view.
    pub access: &'a mut dyn LayoutAccess,
    /// The element's computed style, for whatever the implementation sizes by.
    pub style: &'a ComputedStyle,
    /// The width the engine has already fixed, which is authoritative when present.
    pub known_width: Option<f32>,
    /// The height, the same way.
    pub known_height: Option<f32>,
    /// The space available on each axis, insets already taken off.
    pub available: (Space, Space),
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// Whether this answer will be kept — the pass to place children on — or is a probe.
    pub final_pass: bool,
}

/// The source a pass resolves custom elements through.
pub trait CustomLayoutSource {
    /// Measures — and, on the final pass, lays out — the element `token` names.
    ///
    /// `None` when the token no longer resolves, which lays out as an empty leaf: a custom
    /// element whose implementation is gone is a box with nothing to say, not a panic.
    fn layout(&self, token: u32, cx: &mut CustomLayoutCx<'_>) -> Option<CustomMeasured>;
}

/// The source a tree has until a window installs one: no element answers.
pub struct NoCustomLayout;

impl CustomLayoutSource for NoCustomLayout {
    fn layout(&self, _token: u32, _cx: &mut CustomLayoutCx<'_>) -> Option<CustomMeasured> {
        None
    }
}

/// Measures one custom leaf, routing through the tree's installed source.
///
/// The counterpart of [`measure_leaf`](crate::inline::measure_leaf)'s replaced arm: called from
/// the same place, wrapped by the same shell arithmetic.
pub(crate) fn measure<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
    final_pass: bool,
) -> Measured {
    let Some((token, _, _)) = tree.store().node(key).custom else {
        return Measured::default();
    };
    let source = tree.custom();
    let style = tree.store().node(key).style.clone();
    let scale = tree.device().scale;
    let mut access = TreeAccess { tree, box_: key };
    let mut cx = CustomLayoutCx {
        access: &mut access,
        style: &style,
        known_width: known.width,
        known_height: known.height,
        available: (
            Space::from_taffy(available.width),
            Space::from_taffy(available.height),
        ),
        scale,
        final_pass,
    };
    let Some(measured) = source.layout(token, &mut cx) else {
        return Measured::default();
    };
    Measured {
        size: Size {
            width: known.width.unwrap_or(measured.width),
            height: known.height.unwrap_or(measured.height),
        },
        first_baseline: measured.first_baseline,
        last_baseline: measured.last_baseline,
    }
}

/// The tree, viewed through [`LayoutAccess`] for one element's children.
struct TreeAccess<'a, 'b, C> {
    /// The pass.
    tree: &'a mut LayoutTree<'b, C>,
    /// The element's own box, whose children are the indices.
    box_: BoxKey,
}

impl<C: MeasureContent> TreeAccess<'_, '_, C> {
    /// The child box at `index`, if the element has that many.
    fn child(&self, index: usize) -> Option<BoxKey> {
        self.tree.store().node(self.box_).children.get(index).copied()
    }

    /// One nested layout of `child`, at `run_mode`.
    fn compute(
        &mut self,
        child: BoxKey,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Measured {
        let output = self.tree.compute_child_layout(
            to_node_id(child),
            LayoutInput {
                run_mode,
                sizing_mode: SizingMode::InherentSize,
                axis: taffy::RequestedAxis::Both,
                known_dimensions: known,
                parent_size: available.into_options(),
                available_space: available,
                vertical_margins_are_collapsible: taffy::Line::FALSE,
            },
        );
        let last = self
            .tree
            .store()
            .state(child)
            .and_then(|state| state.last_baseline);
        Measured {
            size: output.size,
            first_baseline: output.first_baselines.y,
            last_baseline: last.or(output.first_baselines.y),
        }
    }
}

impl<C: MeasureContent> LayoutAccess for TreeAccess<'_, '_, C> {
    fn child_count(&self) -> usize {
        self.tree.store().node(self.box_).children.len()
    }

    fn measure_child(
        &mut self,
        index: usize,
        known: (Option<f32>, Option<f32>),
        available: (Space, Space),
    ) -> ChildMeasure {
        let Some(child) = self.child(index) else {
            return ChildMeasure::default();
        };
        ChildMeasure::of(self.compute(child, sized(known), spaced(available), RunMode::ComputeSize))
    }

    fn layout_child(
        &mut self,
        index: usize,
        known: (Option<f32>, Option<f32>),
        available: (Space, Space),
    ) -> ChildMeasure {
        let Some(child) = self.child(index) else {
            return ChildMeasure::default();
        };
        let measured = self.compute(
            child,
            sized(known),
            spaced(available),
            RunMode::PerformLayout,
        );
        // The half a parent algorithm normally performs: computing a child answers *how big*,
        // and it is the parent that writes the answer into the child's kept state. The custom
        // element is the parent here, and this is its pen.
        let state = self.tree.store_mut().state_mut(child);
        state.unrounded.size = measured.size;
        state.unrounded.content_size = measured.size;
        state.snapped = state.unrounded;
        ChildMeasure::of(measured)
    }

    fn place_child(&mut self, index: usize, x: f32, y: f32) {
        let Some(child) = self.child(index) else {
            return;
        };
        // The whole placement, exactly as a line places its atoms: nothing above the element will
        // write this child's location again.
        let state = self.tree.store_mut().state_mut(child);
        state.unrounded.location = taffy::Point { x, y };
        state.snapped = state.unrounded;
    }
}

/// The algorithms' size from the seam's pair.
fn sized(known: (Option<f32>, Option<f32>)) -> Size<Option<f32>> {
    Size {
        width: known.0,
        height: known.1,
    }
}

/// The algorithms' available space from the seam's pair.
fn spaced(available: (Space, Space)) -> Size<AvailableSpace> {
    Size {
        width: available.0.to_taffy(),
        height: available.1.to_taffy(),
    }
}
