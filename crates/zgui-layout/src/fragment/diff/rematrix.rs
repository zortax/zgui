//! Carrying a subtree whose only change is the matrix it is drawn through.
//!
//! A pan or a zoom writes one transform on one box, and everything below that box is laid out in
//! the box's own space. Every piece below keeps its rectangles, its clip chains and its hit
//! entries, because all of them are measured in a space the write did not move. What the write
//! does move is each piece's device ink and the fingerprint of the matrix it is drawn through, and
//! the folds made from those two. This walk rewrites exactly those, from the same calls the
//! composing walk makes, so a frame produced this way and a frame produced by composing every box
//! hold the same fragments.
//!
//! The walk is taken only under a matrix that keeps rectangles upright: a scale and a translation.
//! Under that matrix the device rectangle of a set of pieces is the matrix applied to their union,
//! so every overlap answer in the subtree stays what it was and is recomputed only to be exact.

use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Matrix4, Rect};
use zgui_profile::{Counter, counter};
use zgui_scene::Content;

use crate::fragment::diff::damage::{absorb, overlaps, pairwise_disjoint};
use crate::fragment::diff::dirty::FrameDirty;
use crate::fragment::diff::{Folded, Pass};
use crate::fragment::transform;

/// Whether `matrix` keeps every rectangle upright and the right way round.
///
/// `None` is the identity, which does.
pub(super) fn keeps_rectangles(matrix: Option<Matrix4>) -> bool {
    matrix.is_none_or(|matrix| {
        matrix.to_affine2().is_some_and(|affine| {
            affine.b == 0.0 && affine.c == 0.0 && affine.a > 0.0 && affine.d > 0.0
        })
    })
}

impl<D: FrameDirty> Pass<'_, '_, D> {
    /// Carries one clean subtree onto the matrix `outer`, and reports what it folds up exactly as
    /// visiting it would have.
    ///
    /// The damage is raised once for the whole subtree, where it was and where it is now.
    pub(super) fn rematrix(&mut self, key: BoxKey, outer: Option<Matrix4>) -> Folded {
        let before = self
            .store
            .fragments_of_box(key)
            .first()
            .and_then(|frag| self.store.fragment(*frag))
            .map(|fragment| fragment.subtree_ink);
        let folded = self.rematrix_box(key, outer);
        if let Some(before) = before {
            absorb(self.damage, before);
        }
        absorb(self.damage, folded.subtree_ink);
        folded
    }

    /// Rewrites one box's pieces under `outer`, then everything below it.
    fn rematrix_box(&mut self, key: BoxKey, outer: Option<Matrix4>) -> Folded {
        counter::bump(Counter::NodesVisited);
        let scale = self.tables.device.scale;
        let first = self.store.fragments_of_box(key).first().copied();
        // The same matrix the composing walk arrives at: the box's own transform, or the one an
        // animation is drawing it under, composed onto the matrix above it.
        let matrix = match first.and_then(|frag| self.store.fragment(frag)) {
            Some(fragment) => {
                let node = self.store.node(key);
                let placement = transform::animated::of(self.tables.placements, node.source);
                let box_ = placement.map_or_else(
                    || node.style.get_box(),
                    zgui_dom::side::AnimPlacement::group,
                );
                match (
                    outer,
                    transform::matrix_of(box_, fragment.border_box, scale),
                ) {
                    (None, None) => None,
                    (Some(outer), None) => Some(outer),
                    (None, Some(own)) => Some(own),
                    (Some(outer), Some(own)) => Some(own.then(&outer)),
                }
            }
            None => outer,
        };
        let hash = matrix.unwrap_or(Matrix4::IDENTITY).content_hash();

        // The box's own piece is its ink; every later piece is a piece of the subtree, exactly as
        // the composing walk folds them.
        let mut ink = Rect::ZERO;
        let mut inks: Vec<Rect<DevicePx, Device>> = Vec::new();
        let count = self.store.fragments_of_box(key).len();
        for position in 0..count {
            let Some(&frag) = self.store.fragments_of_box(key).get(position) else {
                break;
            };
            let Some(fragment) = self.store.fragment_mut(frag) else {
                continue;
            };
            fragment.transform_hash = hash;
            // Every piece is drawn through the box's matrix: its own, its lines and its bars.
            fragment.ink = match matrix {
                Some(matrix) => transform::transformed_bounds(&matrix, fragment.local_ink),
                None => fragment.local_ink,
            };
            if position == 0 {
                ink = fragment.ink;
            } else {
                inks.push(fragment.ink);
            }
        }

        let mut folded = self.cached(key);
        let mut disjoint = true;
        let mut position = 0;
        while let Some(&child) = self.store.node(key).children.get(position) {
            let below = self.rematrix_box(child, matrix);
            disjoint &= below.disjoint;
            inks.push(below.subtree_ink);
            if first.is_none() {
                // A box with no piece of its own answers for what is below it.
                folded.blending |= below.blending;
                folded.rigid &= below.rigid;
                folded.order = (
                    folded.order.0.min(below.order.0),
                    folded.order.1.max(below.order.1),
                );
            }
            position += 1;
        }
        disjoint &= !inks.iter().any(|piece| overlaps(ink, *piece));
        disjoint &= pairwise_disjoint(&inks);
        let subtree_ink = inks.iter().fold(ink, |held, piece| held.union(*piece));

        let count = self.store.fragments_of_box(key).len();
        for position in 0..count {
            let Some(&frag) = self.store.fragments_of_box(key).get(position) else {
                break;
            };
            if let Some(fragment) = self.store.fragment_mut(frag) {
                fragment.subtree_ink = subtree_ink;
                fragment.subtree_disjoint = disjoint;
            }
        }
        folded.subtree_ink = subtree_ink;
        folded.disjoint = disjoint;
        folded
    }
}
