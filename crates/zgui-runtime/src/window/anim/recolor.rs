//! Moving an animated colour by rewriting the paint slots a box owns.
//!
//! A box whose animated colours are drawn through owned slots records which slots in its paint
//! signature, and none of the values. So a frame that moves only the colours replays the box's
//! chunk, and the work is here: the slots rewritten in place, and the box's ink put into the
//! damage. Nothing sends the fragment pass to the box, because a colour moves no geometry and no
//! hit entry.
//!
//! The ink is damaged for every override that [`OwnedPaints`] accepts, also when the slots could
//! not take the colour. The fragment pass damages nothing for a box whose shared style paints
//! nothing, and a transition to transparent is exactly such a box. An override it refuses, an
//! opacity for example, gets no damage here: the fragment pass damages it.

use zgui_dom::NodeKey;
use zgui_dom::side::{AnimOverride, BoxKey};
use zgui_geom::{DevicePx, Edges};
use zgui_layout::fragment::diff::pixels;
use zgui_paint::OwnedPaints;
use zgui_scene::{PaintRewrite, PropertyOwner};

use crate::window::anim::place::Writing;

impl Writing<'_> {
    /// Writes `values` into the slots `node`'s boxes own, and reports whether that is all the
    /// frame owes them.
    ///
    /// True only while every fragment of the boxes has a record encoded through exactly the slots
    /// `values` drives, and each of those slots took its colour. A box with no record, or a nested
    /// inline box that the lines it is on paint, takes the fragment pass.
    pub(super) fn write_colors(&mut self, node: NodeKey, values: &AnimOverride) -> bool {
        if !OwnedPaints::any(values) {
            return false;
        }
        self.damage_own_ink(node);
        let boxes = self.layout.boxes_of(node);
        if boxes.is_empty() {
            return false;
        }
        let cache = self.painter.cache();
        let recorded_owned = boxes.iter().all(|key| {
            let fragments = self.layout.fragments_of_box(*key);
            let Some(owned) = OwnedPaints::of(values, self.border_of(*key)) else {
                return false;
            };
            let signature = owned.signature();
            self.layout
                .get(*key)
                .is_some_and(|box_| !box_.is_nested_inline())
                && !fragments.is_empty()
                && fragments
                    .iter()
                    .all(|frag| cache.anim_signature(*frag) == Some(signature))
        });
        if !recorded_owned {
            return false;
        }
        let mut written = true;
        for key in boxes {
            let border = self.border_of(*key);
            let Some(owned) = OwnedPaints::of(values, border) else {
                return false;
            };
            for (name, paint) in owned.paints(PropertyOwner::of(*key), values, border) {
                written &= self.scene.paints.rewrite(name, paint) == PaintRewrite::Written;
            }
        }
        written
    }

    /// The border widths of the box `key`, read off its first fragment as the emission reads them.
    fn border_of(&self, key: BoxKey) -> Edges<DevicePx> {
        self.layout
            .fragments_of_box(key)
            .first()
            .and_then(|frag| self.layout.fragment(*frag))
            .map_or(Edges::ZERO, |fragment| fragment.border)
    }

    /// Damages `node`'s ink when the override it dropped held colours [`OwnedPaints`] accepts.
    pub(super) fn damage_uncolored(&mut self, node: NodeKey, was: &AnimOverride) {
        if OwnedPaints::any(was) {
            self.damage_own_ink(node);
        }
    }

    /// Damages the ink of every fragment of `node`'s boxes, where it is on the device now.
    ///
    /// Cut to what each fragment's clip chain admits, and rounded outwards to whole pixels, as the
    /// fragment pass does for a repainted box.
    fn damage_own_ink(&mut self, node: NodeKey) {
        let scene = &*self.scene;
        let resolve = |id| scene.spatial.resolve(id);
        for key in self.layout.boxes_of(node) {
            for frag in self.layout.fragments_of_box(*key) {
                let Some(fragment) = self.layout.fragment(*frag) else {
                    continue;
                };
                let ink = fragment.on_device_in(fragment.ink, &scene.spatial);
                let admitted = scene.clips.bounds_placed(fragment.clip, &resolve);
                if let Some(rect) = ink.intersection(admitted)
                    && !rect.is_empty()
                {
                    self.damage.absorb(pixels(rect));
                }
            }
        }
    }
}
