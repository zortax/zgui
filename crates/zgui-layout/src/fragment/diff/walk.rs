//! Serving a frame whose every mark is a repaint without composing anything.
//!
//! A colour moves nothing, and the full walk still places every box on the way to it: one row of a
//! list costs as much as the cell that changed. When the style patch proved that each repainted
//! box keeps its fragments (see [`recolour`](crate::fragment::recolour)), what the frame owes is
//! the ink those fragments already hold. This walk follows the marks to them and absorbs that ink.
//!
//! Anything else hands the frame to the full walk: any mark but a repaint, a box that moved,
//! a piece that left the tree, or an element with no proof, which is how a repaint from an
//! animation or a text edit is told from a colour. The damage absorbed by then stays, because it
//! was owed either way.

use std::sync::OnceLock;

use zgui_bits::{DamageSet, Dirty};
use zgui_dom::side::BoxKey;
use zgui_profile::{Counter, counter};

use super::dirty::{FrameDirty, Owed};
use super::{Pass, REPAINTS_IN_PLACE};
use crate::fragment::{FragmentFlags, FragmentKind};

/// Whether a pass tries this walk first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Mode {
    /// Every frame takes the full walk.
    Off,
    /// A frame takes both walks, and the full walk must agree with this one.
    Shadow,
    /// A frame this walk serves takes nothing else.
    On,
}

/// The mode, from `ZGUI_REPAINT_WALK` (`off`, `shadow` or `on`).
pub(super) fn mode() -> Mode {
    static MODE: OnceLock<Mode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("ZGUI_REPAINT_WALK").as_deref() {
        Ok("off") => Mode::Off,
        Ok("shadow") => Mode::Shadow,
        _ => Mode::On,
    })
}

/// Whether `walked` reaches what `composed` holds.
///
/// Compared as bounds and area, because the two sets can hold one coverage in different rectangles.
pub(super) fn covers(walked: &DamageSet, composed: &DamageSet) -> bool {
    match (walked.bounds(), composed.bounds()) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(walked_bounds), Some(bounds)) => {
            walked_bounds.contains_rect(bounds) && walked.area() >= composed.area()
        }
    }
}

impl<D: FrameDirty> Pass<'_, '_, D> {
    /// Serves the frame, and reports whether it could.
    pub(super) fn repaint_only(&mut self, root: BoxKey) -> bool {
        if !self.store.recolours_stand()
            || !self.store.retired_fragments().is_empty()
            || self.store.fragments_of_box(root).is_empty()
        {
            return false;
        }
        let served = self.repaint_below(root, None);
        if served {
            counter::bump(Counter::RepaintWalks);
        }
        served
    }

    /// Absorbs what `key` and the boxes below it owe, and reports whether it could.
    fn repaint_below(&mut self, key: BoxKey, generator: Option<zgui_dom::NodeKey>) -> bool {
        counter::bump(Counter::RepaintWalkBoxes);
        let owed = self.owed_by(key, generator);
        // Every other bit refuses, also a bit no pass enters for. A transform in flight owes only
        // `ANIMATING`, and its fragments still change when a walk comes past.
        if !(owed.own | owed.subtree)
            .difference(Dirty::REPAINT)
            .is_empty()
            || self
                .store
                .state(key)
                .is_none_or(|state| state.unrounded != state.composed)
        {
            return false;
        }
        if owed.own.contains(Dirty::REPAINT) && !self.absorb_held(key, &owed) {
            return false;
        }
        if !owed.subtree.contains(Dirty::REPAINT) {
            return true;
        }
        let mark = self.scratch.children.len();
        self.scratch.children.extend_from_slice(
            &crate::fragment::stacking::children_in_paint_order(self.store, key),
        );
        let end = self.scratch.children.len();
        let mut served = true;
        for index in mark..end {
            let child = self.scratch.children[index];
            if !self.can_skip(child, owed.node) && !self.repaint_below(child, owed.node) {
                served = false;
                break;
            }
        }
        self.scratch.children.truncate(mark);
        served
    }

    /// Absorbs the ink the pieces of `key` hold, and reports whether they stand.
    fn absorb_held(&mut self, key: BoxKey, owed: &Owed) -> bool {
        // An anonymous box answers under its generator, and only a style can change it. The
        // check on the whole frame covers those.
        let element = self
            .store
            .get(key)
            .is_some_and(|node| node.source.is_some());
        if element
            && (!self.store.recoloured(key)
                || self.dirty.animates_background(owed.node)
                || self.dirty.animates_opacity(owed.node))
        {
            return false;
        }
        for slot in 0..self.store.fragments_of_box(key).len() {
            let frag = self.store.fragments_of_box(key)[slot];
            let Some(fragment) = self.store.fragment(frag) else {
                continue;
            };
            let (kind, flags) = (fragment.kind, fragment.flags);
            let (ink, subtree_ink, anchor) = (fragment.ink, fragment.subtree_ink, fragment.anchor);
            let admitted = self.admitted(fragment.clip);
            // As `update` reads them: a line paints the nested inline boxes on it.
            let own = if matches!(kind, FragmentKind::Line { .. })
                && flags.contains(FragmentFlags::INLINE_EFFECTS)
            {
                owed.own | (owed.subtree & REPAINTS_IN_PLACE)
            } else {
                owed.own
            };
            let is_box = kind == FragmentKind::Box;
            if own.contains(Dirty::REPAINT)
                && !(is_box && flags.contains(FragmentFlags::PAINTS_NOTHING))
            {
                self.damage_beyond_a_move(self.device_now(ink, anchor), admitted);
            }
            if is_box
                && own.contains(Dirty::REPAINT)
                && flags.contains(FragmentFlags::GROUPS_SUBTREE)
            {
                let was = self.device_before(subtree_ink, anchor);
                self.damage_drawn_beyond_a_move(was, admitted);
            }
        }
        true
    }
}
