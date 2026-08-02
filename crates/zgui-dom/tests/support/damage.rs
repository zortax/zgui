//! Turning the style engine's own per-element damage into obligations.
//!
//! The engine ships four damage bits and reserves the rest of the word for the embedder, but the
//! hook that fills the embedder's bits is only called when the engine's own relayout bit is already
//! set — so paint-only and accessibility-only changes never reach it, and *first-time* cascades
//! produce nothing at all.
//!
//! That last one is the case this module exists for, and it is the one an insertion runs into. The
//! engine returns before accumulating any damage when there is no old style to compare against, and
//! its per-element damage starts empty, so a freshly inserted subtree comes out of a restyle with
//! **no** damage from any source. Nothing else supplies it either: the child-list protocol marks the
//! parent, not the new content, and the rule that every layout and paint obligation comes from the
//! style engine forbids the insertion from marking one itself. Without the branch below, a mounted
//! subtree is never laid out, never painted, and never appears.
//!
//! The stage that owns this translation ships beside the style engine's driver. What is here is the
//! part the driver cannot infer afterwards — whether each element already had a style — plus the
//! arms, so that the property can be measured against a real traversal now.

use style::selector_parser::RestyleDamage;
use zgui_bits::Dirty;

use crate::support::traversal::Restyled;

/// The obligations one restyled element's damage implies.
///
/// The engine's four bits are a *nested* lattice — relayout contains recalculate-overflow, which
/// contains rebuild-stacking-context, which contains repaint — so the arms are tested widest first
/// and each level implies the ones below it. A flat sequence of tests fires every arm for every
/// relayout; omitting the middle arms gives an empty answer for `transform`, `z-index` and every
/// other property whose damage level is one of them.
pub(crate) fn translate(record: &Restyled) -> Dirty {
    let mut owed = Dirty::empty();

    // A first-time cascade accumulates no damage at all. This branch is the only source of layout
    // work for content that has never been styled, and it covers more than an insertion: a subtree
    // coming back out of `display: none` has had its style data thrown away, so every element in it
    // is styled for the first time again with no mutation at any of them.
    if record.initial {
        owed |= Dirty::RELAYOUT | Dirty::REBUILD_BOX;
    }

    if record.damage.contains(RestyleDamage::RELAYOUT) {
        owed |= Dirty::RELAYOUT;
    } else if record.damage.contains(RestyleDamage::RECALCULATE_OVERFLOW) {
        owed |= Dirty::REFRAGMENT | Dirty::REHIT | Dirty::REPAINT;
    } else if record
        .damage
        .contains(RestyleDamage::REBUILD_STACKING_CONTEXT)
    {
        owed |= Dirty::RESTACK | Dirty::REHIT | Dirty::REPAINT;
    }
    owed
}

/// The same, with the first-time-cascade branch switched off.
///
/// The negative control: it is what the translation looks like when the branch is left out, and it
/// is the only way to show that nothing else supplies the missing obligations.
pub(crate) fn translate_without_initial_branch(record: &Restyled) -> Dirty {
    translate(&Restyled {
        initial: false,
        ..*record
    })
}
