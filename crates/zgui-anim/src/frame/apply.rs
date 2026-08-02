//! What one animating element is given this frame.
//!
//! Three writes, and each of the three is load-bearing in a way the other two are not:
//!
//! * the **override**, into the node's own column, because everywhere else is shared;
//! * the **obligations**, because a value that changed and nothing owing to draw it again is a
//!   value nobody sees;
//! * the **animating bit**, because it is the only thing that brings the loop back for the next
//!   frame — and it is marked whichever path the element takes.
//!
//! The last two are marked on different conditions, and the difference is what a *held* value
//! costs. An animation whose fill mode keeps its last keyframe in force goes on owning the
//! element's values after it has stopped moving: the override still has to be written, so that the
//! frame after it ended does not snap the element back to its base style, and the animating bit
//! must nonetheless not be marked, or the loop wakes at the refresh rate for ever over a value that
//! no longer changes.
//!
//! # Which obligation each path owes
//!
//! A repaint-only animation owes a repaint, because the rectangle it changed is one that already
//! exists and already sits where it sits. A **placement** animation owes whatever the caller could
//! not already do for it, and that is decided before this runs: a box whose coordinate system was
//! written is somewhere else already and owes nothing at all, while a box that could not be written
//! owes a *re-fragmenting*. The second is not the first wearing another name — the fragment pass is
//! the one stage that turns a style into geometry, and only by running it does that element's new
//! ink rectangle, its damaged region and its hit entry come into being.

use zgui_bits::Dirty;
use zgui_dom::Document;
use zgui_dom::dirty::propagate;
use zgui_profile::{Counter, counter};
use zgui_style::ElementAnimation;

use crate::tier::Tier;

/// What the frame did with an element's interpolated placement.
///
/// Answered by the caller, because the caller is what holds the previous frame's table and the
/// coordinate systems the boxes established. Read on the placement path and ignored on the other
/// two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placed {
    /// The interpolation arrived back where the standing fragments were already composed.
    ///
    /// Nothing is owed. A fragment recomposed for it would be an ink rectangle damaged on every
    /// frame of a transform that is holding still.
    Held,
    /// The box's coordinate system was written, so it is already somewhere else.
    ///
    /// Nothing about the box is composed again, and nothing has to be: its fragments are in their
    /// own untransformed space, the hit entries under them are indexed in that space, and the
    /// primitives already drawn name the coordinate system by slot. All three move because the one
    /// matrix they resolve through moved.
    Written,
    /// The box has to be composed again for the placement to take effect at all.
    ///
    /// What an interactive transform costs, and what a keyframed one costs on a frame it leaves the
    /// region it declared. Correct in every case and dearer in all of them.
    Recomposed,
}

/// Applies one element's animations and reports which path it took.
pub fn element(document: &mut Document, animation: &ElementAnimation, placed: Placed) -> Tier {
    let tier = Tier::of(animation.properties);
    match tier {
        Tier::Cheap => {
            let mut bits = Dirty::empty();
            // `REHIT` alongside `REPAINT` because the fragment pass is entered on either, and it is
            // the pass that turns an obligation into a damaged rectangle. Owed only when the value
            // actually moved: a held final value is already on the screen.
            if write_override(document, animation) {
                bits |= Dirty::REPAINT | Dirty::REHIT;
            }
            // `ANIMATING` because nothing else will ask for the frame after this one.
            if animation.advancing {
                bits |= Dirty::ANIMATING;
                counter::bump(Counter::TierBTransitions);
            }
            if !bits.is_empty() {
                mark(document, animation, bits);
            }
        }
        Tier::Place => {
            let mut bits = Dirty::empty();
            // A placement animation may be fading as well as moving, and the two values are written
            // into two different places: this one into the node's own paint column, exactly as the
            // cheap path writes it.
            if write_override(document, animation) {
                bits |= Dirty::REPAINT | Dirty::REHIT;
            }
            // `REFRAGMENT` is what makes the fragment pass descend to this element at all, and
            // descending is the whole obligation for a box that could not be written: the pass
            // composes it against the new matrix, absorbs the rectangle it left and the one it
            // arrived at into the frame's damage, rewrites its hit entry, and carries the matrix
            // down to every descendant. Marking a repaint here instead would draw the element again
            // exactly where it already was.
            //
            // A box whose coordinate system *was* written owes none of it — not even the hit bit.
            // The entries are indexed per coordinate system, in the space the fragment's own
            // rectangle is in, and a query is carried down the chain of matrices as it is asked. So
            // the rectangle that moved is the one nobody stored.
            if placed == Placed::Recomposed {
                bits |= Dirty::REFRAGMENT | Dirty::REHIT;
            }
            if animation.advancing {
                bits |= Dirty::ANIMATING;
                counter::bump(Counter::TierCPlacements);
            }
            if !bits.is_empty() {
                mark(document, animation, bits);
            }
        }
        Tier::Cascade => {
            // The element is about to be styled again, so anything left in its column would be
            // composed over a style that already carries the animated value.
            clear_override(document, animation);
            // `RECASCADE` is what makes the restyle descend to it at all: the engine's own hint
            // lives in the element's data, and a traversal with no obligation of ours leading to
            // the element never looks at it. Five hundred animations marked with the hint alone
            // advance not at all.
            //
            // The frame an animation *ends* on has to cascade too, and by then nothing is left to
            // advance: that cascade is what resolves the value the fill mode goes on holding. The
            // animating bit is not marked for it, because there is nothing left to come back for.
            let mut bits = Dirty::empty();
            if animation.advancing || animation.crossed {
                bits |= Dirty::RECASCADE;
            }
            if animation.advancing {
                bits |= Dirty::ANIMATING;
            }
            if !bits.is_empty() {
                mark(document, animation, bits);
            }
        }
    }
    tier
}

/// Puts one element's interpolated values in the one place they are private to it.
///
/// Returns whether what is stored changed, which is what the repaint obligation is owed on.
fn write_override(document: &mut Document, animation: &ElementAnimation) -> bool {
    let slot = document
        .store_mut()
        .columns_mut()
        .anim
        .get_mut(animation.node);
    if animation.values.is_empty() {
        return slot.take().is_some();
    }
    match slot {
        Some(held) if **held == animation.values => false,
        Some(held) => {
            **held = animation.values.clone();
            true
        }
        None => {
            *slot = Some(Box::new(animation.values.clone()));
            true
        }
    }
}

/// Drops one element's override, if it had one.
fn clear_override(document: &mut Document, animation: &ElementAnimation) {
    document
        .store_mut()
        .columns_mut()
        .anim
        .clear(animation.node);
}

/// Marks one element's obligations, if it is still in the document.
fn mark(document: &mut Document, animation: &ElementAnimation, bits: Dirty) {
    propagate::mark(document.store_mut(), animation.index, bits);
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_dom::side::AnimOverride;
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;
    use zgui_style::{AnimatedProperties, ElementAnimation};

    use super::{Placed, element};
    use crate::tier::Tier;

    /// A document with one element in it, and that element's slot.
    fn one_element() -> (Document, zgui_dom::NodeIndex) {
        let mut document = Document::new();
        let index = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("box"),
        );
        (document, index)
    }

    #[test]
    fn a_cheap_animation_writes_into_the_node_and_marks_it_animating() {
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        let tier = element(
            &mut document,
            &ElementAnimation {
                node,
                index,
                properties: AnimatedProperties::OPACITY,
                values: AnimOverride {
                    opacity: Some(0.5),
                    ..AnimOverride::new()
                },
                placement: None,
                advancing: true,
                crossed: false,
            },
            Placed::Held,
        );
        assert_eq!(tier, Tier::Cheap);
        assert_eq!(
            document
                .store()
                .columns()
                .anim
                .get(node)
                .and_then(|slot| slot.as_ref())
                .and_then(|held| held.opacity),
            Some(0.5)
        );
        let dirty = document.store().core(index).dirty();
        assert!(dirty.own().contains(zgui_bits::Dirty::ANIMATING));
        assert!(dirty.own().contains(zgui_bits::Dirty::REPAINT));
    }

    #[test]
    fn a_cascading_animation_is_also_marked_animating() {
        // The defect this closes: the bit was marked on one path only, so the commonest animation
        // in the library — which takes the other one — left the loop with nothing to wake for.
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        let tier = element(
            &mut document,
            &ElementAnimation {
                node,
                index,
                properties: AnimatedProperties::CASCADED,
                values: AnimOverride::new(),
                placement: None,
                advancing: true,
                crossed: false,
            },
            Placed::Held,
        );
        assert_eq!(tier, Tier::Cascade);
        let dirty = document.store().core(index).dirty();
        assert!(dirty.own().contains(zgui_bits::Dirty::ANIMATING));
        assert!(dirty.own().contains(zgui_bits::Dirty::RECASCADE));
    }

    #[test]
    fn a_placement_animation_refragments_and_never_recascades() {
        // The defect this closes: a transform was on the cascade tier, so the gallery's progress
        // bar cascaded one element on every one of the sixty frames a second it produces, for ever.
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        let tier = element(
            &mut document,
            &ElementAnimation {
                node,
                index,
                properties: AnimatedProperties::TRANSFORM,
                values: AnimOverride::new(),
                placement: None,
                advancing: true,
                crossed: false,
            },
            Placed::Recomposed,
        );
        assert_eq!(tier, Tier::Place);
        let dirty = document.store().core(index).dirty();
        assert!(dirty.own().contains(Dirty::REFRAGMENT));
        assert!(dirty.own().contains(Dirty::REHIT));
        assert!(dirty.own().contains(Dirty::ANIMATING));
        assert!(
            !dirty.own().contains(Dirty::RECASCADE),
            "a transform asked the style engine for a cascade it does not need"
        );
        assert!(
            !dirty.own().contains(Dirty::REPAINT),
            "the repaint belongs to the fragment pass, which is what knows where the box went"
        );
    }

    #[test]
    fn a_placement_that_was_written_owes_nothing_at_all() {
        // The whole of what the write buys. Marking anything here would send the fragment pass down
        // to the element and compose it again — which is the work the write was made instead of.
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        let tier = element(
            &mut document,
            &ElementAnimation {
                node,
                index,
                properties: AnimatedProperties::TRANSFORM,
                values: AnimOverride::new(),
                placement: None,
                advancing: true,
                crossed: false,
            },
            Placed::Written,
        );
        assert_eq!(tier, Tier::Place);
        let dirty = document.store().core(index).dirty().own();
        assert!(
            !dirty.contains(Dirty::REFRAGMENT),
            "a box that had already been moved asked to be composed again"
        );
        assert!(!dirty.contains(Dirty::REHIT));
        assert!(!dirty.contains(Dirty::REPAINT));
        assert!(
            dirty.contains(Dirty::ANIMATING),
            "the loop was left with nothing to come back for"
        );
    }

    #[test]
    fn a_placement_that_did_not_move_owes_no_fragment() {
        // A transform holding still. Marking unconditionally would recompose the box, damage its
        // ink and rewrite its hit entry on every frame the window runs for any other reason.
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        element(
            &mut document,
            &ElementAnimation {
                node,
                index,
                properties: AnimatedProperties::TRANSFORM,
                values: AnimOverride::new(),
                placement: None,
                advancing: false,
                crossed: false,
            },
            Placed::Held,
        );
        assert!(document.store().core(index).dirty().own().is_empty());
    }

    #[test]
    fn a_placement_animation_that_also_fades_writes_the_fade_too() {
        // Two values, two places, one element. The transform is read while the fragment is
        // composed and the alpha while it is emitted, so neither displaces the other.
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        let tier = element(
            &mut document,
            &ElementAnimation {
                node,
                index,
                properties: AnimatedProperties::TRANSFORM | AnimatedProperties::OPACITY,
                values: AnimOverride {
                    opacity: Some(0.3),
                    ..AnimOverride::new()
                },
                placement: None,
                advancing: true,
                crossed: false,
            },
            Placed::Recomposed,
        );
        assert_eq!(tier, Tier::Place);
        assert_eq!(
            document
                .store()
                .columns()
                .anim
                .get(node)
                .and_then(|slot| slot.as_ref())
                .and_then(|held| held.opacity),
            Some(0.3)
        );
        let dirty = document.store().core(index).dirty();
        assert!(dirty.own().contains(Dirty::REPAINT));
        assert!(dirty.own().contains(Dirty::REFRAGMENT));
    }

    #[test]
    fn a_held_final_value_is_still_written_and_asks_for_no_further_frame() {
        // `animation-fill-mode: forwards` after the animation has stopped moving. The value is
        // still the element's and still has to be composed over its shared style, but nothing is
        // going to change it again — and a frame asked for over a value that no longer moves is a
        // loop that wakes at the refresh rate for ever on a finished animation.
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        let held = ElementAnimation {
            node,
            index,
            properties: AnimatedProperties::OPACITY,
            values: AnimOverride {
                opacity: Some(0.4),
                ..AnimOverride::new()
            },
            placement: None,
            advancing: false,
            crossed: false,
        };
        element(&mut document, &held, Placed::Held);
        assert_eq!(
            document
                .store()
                .columns()
                .anim
                .get(node)
                .and_then(|slot| slot.as_ref())
                .and_then(|slot| slot.opacity),
            Some(0.4),
            "the value the fill mode holds was dropped"
        );
        let dirty = document.store().core(index).dirty();
        assert!(
            !dirty.own().contains(zgui_bits::Dirty::ANIMATING),
            "a finished animation asked the loop to come back for it"
        );
        // The frame it is first written on still owes the repaint that puts it on the screen.
        assert!(dirty.own().contains(zgui_bits::Dirty::REPAINT));
    }

    #[test]
    fn a_value_that_did_not_move_owes_no_repaint() {
        // The second and every later frame of a held value. Marking a repaint unconditionally would
        // damage the node on every frame the window runs for any other reason, for ever, and every
        // assertion about the animation itself would still read correct.
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        let held = ElementAnimation {
            node,
            index,
            properties: AnimatedProperties::OPACITY,
            values: AnimOverride {
                opacity: Some(0.4),
                ..AnimOverride::new()
            },
            placement: None,
            advancing: false,
            crossed: false,
        };
        element(&mut document, &held, Placed::Held);
        let root = document.document_index();
        zgui_dom::dirty::walk::walk(
            document.store_mut(),
            root,
            zgui_bits::Dirty::all(),
            &mut |_store, _node| {},
        );
        element(&mut document, &held, Placed::Held);
        assert!(
            document.store().core(index).dirty().own().is_empty(),
            "a value that did not change asked to be drawn again"
        );
    }

    #[test]
    fn a_cascading_animation_that_stopped_moving_is_not_cascaded_again() {
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        element(
            &mut document,
            &ElementAnimation {
                node,
                index,
                properties: AnimatedProperties::CASCADED,
                values: AnimOverride::new(),
                placement: None,
                advancing: false,
                crossed: false,
            },
            Placed::Held,
        );
        let dirty = document.store().core(index).dirty();
        assert!(!dirty.own().contains(zgui_bits::Dirty::RECASCADE));
        assert!(!dirty.own().contains(zgui_bits::Dirty::ANIMATING));
    }

    #[test]
    fn a_cascading_animation_leaves_no_override_behind() {
        // An element that starts out fading and then also slides moves from one path to the other.
        // The value the first path wrote would otherwise be composed over a style that already
        // holds the animated value, and the fade would be applied twice.
        let (mut document, index) = one_element();
        let node = document.store().key_of(index);
        let fading = ElementAnimation {
            node,
            index,
            properties: AnimatedProperties::OPACITY,
            values: AnimOverride {
                opacity: Some(0.25),
                ..AnimOverride::new()
            },
            placement: None,
            advancing: true,
            crossed: false,
        };
        element(&mut document, &fading, Placed::Held);
        element(
            &mut document,
            &ElementAnimation {
                properties: AnimatedProperties::OPACITY | AnimatedProperties::CASCADED,
                ..fading
            },
            Placed::Held,
        );
        assert!(
            document
                .store()
                .columns()
                .anim
                .get(node)
                .and_then(|slot| slot.as_ref())
                .is_none()
        );
    }
}
