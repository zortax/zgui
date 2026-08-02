//! What each worker does with each element it is handed.
//!
//! # Why the restyled set is collected here and not read back afterwards
//!
//! The engine records "this element was restyled" as a flag inside each element's own data, with
//! no list behind it, so reading the set back means scanning every element in the document. On a
//! ten-thousand-node document that scan costs about fifteen times the incremental restyle it is
//! reporting on — and it is paid on every frame, including frames where one class was toggled.
//!
//! The traversal already visits exactly the elements that were restyled, so each worker appends to
//! a vector of its own and the vectors are concatenated when it finishes. Nothing contends: a
//! worker only ever touches its own slot.
//!
//! # Why each entry copies values out rather than remembering where to look
//!
//! Two of the three things a damage translation needs are destroyed by the very call that produces
//! the styles. Whether the element had a style *before* this pass is destroyed by giving it one,
//! and the accumulated damage is cleared on a schedule that depends on which traversal flags were
//! set. Both are read while the worker owns the element, which is correct under every combination.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use style::Atom;
use style::context::{
    RegisteredSpeculativePainter, RegisteredSpeculativePainters, SharedStyleContext, StyleContext,
};
use style::data::RestyleKind;
use style::dom::TNode;
use style::selector_parser::{PseudoElement, RestyleDamage};
use style::traversal::{DomTraversal, PerLevelTraversalData, recalc_style_at};
use zgui_css::MAX_STYLE_THREADS;
use zgui_dom::{Node, NodeIndex, NodeKey};

/// One element the traversal styled, captured while it was being visited.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Restyled {
    /// The element, as a name that stays valid past the frame it was taken in.
    pub node: NodeKey,
    /// The element's slot, which is what an invalidation mark is written against.
    pub index: NodeIndex,
    /// The damage the engine accumulated for it, copied out during the visit.
    pub damage: RestyleDamage,
    /// Whether this was a first-time cascade rather than a restyle.
    ///
    /// Read *before* the call that gives the element its styles, because that call is what
    /// destroys the answer. A first-time cascade accumulates no damage at all, so this is the only
    /// signal that content which has never been styled needs laying out.
    pub initial: bool,
    /// Whether the element ran selector matching, as opposed to only re-running the cascade.
    pub matched: bool,
    /// The identities of the element's `::before` and `::after` cascade results, or zero for a
    /// pseudo-element that generates nothing.
    ///
    /// A pseudo-element has no node of its own, so it has no row in any per-node table: without
    /// these, a rule that changes only the colour of generated content produces no damage at all.
    pub pseudos: [usize; 2],
}

/// No custom paint sources.
///
/// A paint worklet is script that draws into a background, and there is no script here.
pub(crate) struct NoPainters;

impl RegisteredSpeculativePainters for NoPainters {
    fn get(&self, _name: &Atom) -> Option<&dyn RegisteredSpeculativePainter> {
        None
    }
}

/// One vector per worker, so that appending to it contends with nothing.
struct PerWorker {
    /// The slots, one per worker index the engine can produce plus one for the calling thread.
    slots: Vec<Mutex<Vec<Restyled>>>,
}

impl PerWorker {
    /// Empty slots.
    fn new() -> Self {
        Self {
            slots: (0..=MAX_STYLE_THREADS)
                .map(|_| Mutex::new(Vec::new()))
                .collect(),
        }
    }

    /// Appends `entry` to the calling worker's own slot.
    fn push(&self, entry: Restyled) {
        let slot = rayon::current_thread_index().unwrap_or(MAX_STYLE_THREADS);
        self.slots[slot.min(MAX_STYLE_THREADS)]
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(entry);
    }

    /// Every slot's entries, concatenated.
    fn into_vec(self) -> Vec<Restyled> {
        self.slots
            .into_iter()
            .flat_map(|slot| slot.into_inner().unwrap_or_else(|held| held.into_inner()))
            .collect()
    }
}

/// The traversal, holding what every worker shares.
pub(crate) struct RecalcStyle<'a> {
    /// The rule set, the guards and the snapshot map.
    context: SharedStyleContext<'a>,
    /// What each worker styled.
    restyled: PerWorker,
    /// One bit per worker index that ran at least one element.
    ///
    /// Evidence rather than trust: the traversal stays on one thread until a level is wide enough
    /// to be worth splitting, so "a pool was handed to it" is not "it used one".
    workers: AtomicU32,
}

impl<'a> RecalcStyle<'a> {
    /// A traversal over `context`.
    pub(crate) fn new(context: SharedStyleContext<'a>) -> Self {
        Self {
            context,
            restyled: PerWorker::new(),
            workers: AtomicU32::new(0),
        }
    }

    /// What the traversal styled, and how many distinct workers ran.
    pub(crate) fn finish(self) -> (Vec<Restyled>, usize) {
        let workers = self.workers.load(Ordering::Relaxed).count_ones() as usize;
        (self.restyled.into_vec(), workers)
    }
}

impl<'doc> DomTraversal<Node<'doc>> for RecalcStyle<'_> {
    fn process_preorder<F: FnMut(Node<'doc>)>(
        &self,
        traversal_data: &PerLevelTraversalData,
        context: &mut StyleContext<Node<'doc>>,
        node: Node<'doc>,
        note_child: F,
    ) {
        let worker = rayon::current_thread_index().unwrap_or(31).min(31);
        self.workers.fetch_or(1 << worker, Ordering::Relaxed);

        let Some(element) = node.as_element() else {
            return;
        };
        let mut data = element.ensure_style_data();
        // Both of these are answers about the element as it is *now*, and the call below is what
        // changes it.
        let initial = !data.has_styles();
        let matched = matches!(
            data.restyle_kind(context.shared),
            Some(RestyleKind::MatchAndCascade)
        );

        recalc_style_at(
            self,
            traversal_data,
            context,
            element,
            &mut data,
            note_child,
        );

        // The animation-only traversal's descent flag is retired here, by the traversal that read
        // it: the call above has already asked whether to descend and has already noted the
        // children it is descending to, so what is left is a flag nothing reads again this frame.
        // Left raised, it takes every later frame's animation traversal down a subtree that has
        // had nothing to do in it since.
        if context.shared.traversal_flags.for_animation_only() {
            crate::engine::animate::descent::clear(element);
        }

        // Visited is not styled. The traversal descends *through* every element on the path to the
        // work and calls this for each of them; an element that had nothing to do is not part of
        // the set damage is read from, and counting it would turn every budget into a statement
        // about how deep the document is.
        if !initial && !data.is_restyle() {
            return;
        }

        self.restyled.push(Restyled {
            node: element.key(),
            index: element.index(),
            damage: data.damage,
            initial,
            matched,
            pseudos: [
                pseudo_identity(&data, &PseudoElement::Before),
                pseudo_identity(&data, &PseudoElement::After),
            ],
        });

        // The engine's descent flag is not cleared here, because this document does not store one:
        // "is there work below me" is a view of the invalidation word, and that word is retired
        // exactly once per frame by the walk that also retires the obligations it summarises.
        // Clearing it on a second schedule is how a mark taken between the two is silently lost.
    }

    /// The post-order pass exists to build the engine's own flow tree, which is not the tree this
    /// framework lays out.
    fn needs_postorder_traversal() -> bool {
        false
    }

    fn process_postorder(&self, _context: &mut StyleContext<Node<'doc>>, _node: Node<'doc>) {
        unreachable!("the post-order traversal is switched off")
    }

    fn shared_context(&self) -> &SharedStyleContext<'_> {
        &self.context
    }
}

/// The identity of one eagerly cascaded pseudo-element's style, or zero when it generates nothing.
fn pseudo_identity(data: &style::data::ElementData, pseudo: &PseudoElement) -> usize {
    data.styles
        .pseudos
        .get(pseudo)
        .map_or(0, |style| style.heap_ptr() as usize)
}
