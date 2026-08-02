//! The damage bits this framework adds to the style engine's own four.
//!
//! The engine computes four bits of its own — repaint, rebuild the stacking context, recalculate
//! overflow, relayout — from generated per-property predicates, and reserves the top twelve bits of
//! the word for whoever is doing the layout. Those twelve describe *our* pipeline's stages, so they
//! are defined here.
//!
//! # Why they are defined here rather than beside the layout that consumes them
//!
//! The hook that contributes them is an associated function with no receiver and no context: the
//! engine calls it as `Element::compute_layout_damage(old, new)` while comparing two styles, so
//! there is nothing to install a classifier on and no way for a crate above this one to supply one.
//! The bits therefore live with the only implementation that can exist, and the stages that consume
//! them name them from here.
//!
//! # When the hook fires, and what that rules out
//!
//! Only when the engine's own relayout bit is already set. That bit is far wider than its name: the
//! engine sets it for a border *colour*, a corner radius and a box shadow, because the layout it
//! was written for caches painting fragments inside its boxes and has to rebuild them. This
//! pipeline does not, and taking the engine's word for it meant that toggling a class that changed
//! one border colour rebuilt every box in the document, threw away every layout cache, renamed
//! every fragment and so widened damage to the whole surface. What separates the
//! changes that cost this pipeline a layout from the ones that cost it a repaint is a comparison
//! of the two styles over the properties the engine's own predicate names, split by which of this
//! pipeline's stages reads each one.
//!
//! Damage that is paint-only or accessibility-only is *also* derived separately, by comparing
//! cached keys over the elements the traversal restyled, which is why returning nothing here loses
//! nothing: the repaint still happens, from the comparison that was always the authority on it.

mod classify;

#[cfg(test)]
mod tests;

use style::properties::ComputedValues;
use style::selector_parser::RestyleDamage;

use crate::node::handle::Node;
use crate::stylo::element::damage::classify::Cost;

/// This element's own box has to be built again.
pub const CONSTRUCT_BOX: RestyleDamage = RestyleDamage::from_bits_retain(1 << 4);

/// The formatting context this element establishes has to be built again.
pub const CONSTRUCT_FC: RestyleDamage = RestyleDamage::from_bits_retain(1 << 5);

/// Every box below this element has to be built again.
pub const CONSTRUCT_DESCENDANTS: RestyleDamage = RestyleDamage::from_bits_retain(1 << 6);

/// This element's text has to be shaped again.
///
/// The expensive half of text layout. A change to the font, its size, its features or the spacing
/// between its glyphs invalidates the shaped run itself.
pub const RESHAPE_TEXT: RestyleDamage = RestyleDamage::from_bits_retain(1 << 7);

/// This element's text has to be broken into lines again, but not shaped again.
///
/// The cheap half. A shaped run can be re-broken and re-aligned many times without touching the
/// shaper, so a width change or an alignment change costs only this.
pub const REBREAK_TEXT: RestyleDamage = RestyleDamage::from_bits_retain(1 << 8);

/// This element's ink or scrollable overflow moved, but nothing was laid out again.
///
/// A corner radius, a box shadow and a clip change what the element covers without changing where
/// anything is. The fragment has to be measured again and the hit index told about it; no box is
/// rebuilt and no size is computed.
pub const RECALCULATE_INK: RestyleDamage = RestyleDamage::from_bits_retain(1 << 9);

/// This element's size or position has to be computed again, out of the boxes it already has.
///
/// The narrow half of a layout-affecting change. A width, a margin, an inset or an alignment moves
/// where things are and how large they are without moving *which boxes exist*, so the box tree is
/// kept and every cached layout along the path to this element is thrown away instead.
pub const RELAYOUT_BOX: RestyleDamage = RestyleDamage::from_bits_retain(1 << 10);

/// The bits that mean a box has to be built or laid out again.
pub const ALL: RestyleDamage = RestyleDamage::from_bits_retain(
    CONSTRUCT_BOX.bits()
        | CONSTRUCT_FC.bits()
        | CONSTRUCT_DESCENDANTS.bits()
        | RELAYOUT_BOX.bits()
        | RESHAPE_TEXT.bits()
        | REBREAK_TEXT.bits(),
);

impl Node<'_> {
    /// The work a layout-affecting change from `old` to `new` costs this pipeline.
    ///
    /// Four answers, and each is the narrowest one that is still true. A change this pipeline lays
    /// nothing out for — a border colour is the ordinary case — costs nothing here, and the
    /// paint-key comparison that runs over every restyle is what draws it again. A change that
    /// moves the element's ink without moving anything's geometry costs [`RECALCULATE_INK`]. A
    /// change to an extent, a margin, an inset, an alignment or the face text is set in moves
    /// geometry inside a box tree that is still correct, and costs [`RELAYOUT_BOX`] together with
    /// the two text bits — which the stage that reads them narrows again against the element's own
    /// shaping key. Only a change to *which boxes exist* — a `display`, a `position`, a `float`, an
    /// `order`, a generated-content string, a grid template's line names — costs the whole set:
    /// the box, its formatting context and everything below it.
    ///
    /// That last answer is also what an *unexplained* difference costs. The engine calls this only
    /// after deciding something layout-affecting changed, so finding no difference at all means the
    /// property responsible is one this classification does not know about — and the safe reading
    /// of an unknown property is the widest one. An omission here therefore costs time and never
    /// correctness.
    pub fn layout_damage(old: &ComputedValues, new: &ComputedValues) -> RestyleDamage {
        match classify::cost(old, new) {
            Cost::Repaint => RestyleDamage::empty(),
            Cost::Ink => RECALCULATE_INK,
            Cost::Geometry => RELAYOUT_BOX | RESHAPE_TEXT | REBREAK_TEXT,
            Cost::Layout => RestyleDamage::reconstruct() | ALL,
        }
    }
}
