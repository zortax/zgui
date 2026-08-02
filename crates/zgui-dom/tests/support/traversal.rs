//! What each worker does with each element it is handed.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use style::context::{
    RegisteredSpeculativePainter, RegisteredSpeculativePainters, SharedStyleContext, StyleContext,
};
use style::dom::{TElement, TNode};
use style::selector_parser::RestyleDamage;
use style::traversal::{DomTraversal, PerLevelTraversalData, recalc_style_at};
use stylo_atoms::Atom;

/// One element the traversal restyled, captured while it was being visited.
///
/// Copied out during the visit rather than read afterwards. The engine clears its damage only under
/// one traversal flag combination, so "read it before the tail clears it" is not a rule that holds
/// in general; and the answer to "did this element have a style before this pass" is destroyed by
/// the very call that gives it one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Restyled {
    /// The element, by slot number.
    pub(crate) node: u32,
    /// The damage the engine accumulated for it.
    pub(crate) damage: RestyleDamage,
    /// Whether this was a first-time cascade rather than a restyle.
    pub(crate) initial: bool,
}

/// No custom paint sources.
pub(crate) struct NoPainters;

impl RegisteredSpeculativePainters for NoPainters {
    fn get(&self, _name: &Atom) -> Option<&dyn RegisteredSpeculativePainter> {
        None
    }
}

/// The traversal, holding the context every worker shares.
pub(crate) struct RecalcStyle<'a> {
    /// Everything the workers read: the rule set, the guards, the snapshot map.
    pub(crate) context: SharedStyleContext<'a>,
    /// One bit per worker index that ran at least one element.
    ///
    /// Evidence rather than trust: a traversal handed a pool still runs on one thread until a level
    /// is wide enough, so a claim of "parallel" that never left one thread would be a claim about
    /// nothing.
    pub(crate) workers: AtomicU32,
    /// Every element the traversal actually visited, by slot number, in visit order.
    ///
    /// "Did the traversal reach this node" is the question a descent flag exists to answer, and it
    /// cannot be answered from the tree afterwards: an element that was reached and needed no work
    /// looks exactly like one that was never reached.
    pub(crate) visited: Mutex<Vec<u32>>,
    /// What each visited element's style data said, copied out while the worker owned it.
    pub(crate) restyled: Mutex<Vec<Restyled>>,
    /// The element a worker is to panic on, so the failure policy can be exercised on demand.
    pub(crate) panic_at: Option<u32>,
}

impl<E> DomTraversal<E> for RecalcStyle<'_>
where
    E: TElement,
{
    fn process_preorder<F: FnMut(E::ConcreteNode)>(
        &self,
        traversal_data: &PerLevelTraversalData,
        context: &mut StyleContext<E>,
        node: E::ConcreteNode,
        note_child: F,
    ) {
        let worker = rayon::current_thread_index().unwrap_or(31).min(31);
        self.workers.fetch_or(1 << worker, Ordering::Relaxed);
        if let Some(element) = node.as_element() {
            if self.panic_at == Some(element.as_node().opaque().0 as u32) {
                panic!("a restyle worker panicked");
            }
            self.visited
                .lock()
                .expect("no worker panicked while recording a visit")
                .push(element.as_node().opaque().0 as u32);
            // SAFETY: the traversal owns this element for the duration of this call, which is the
            // contract the engine's own traversal is written against.
            let mut data = unsafe { element.ensure_data() };
            // Read before the call that gives the element its styles, because that call is what
            // destroys the answer.
            let initial = !data.has_styles();
            recalc_style_at(
                self,
                traversal_data,
                context,
                element,
                &mut data,
                note_child,
            );
            self.restyled
                .lock()
                .expect("no worker panicked while recording damage")
                .push(Restyled {
                    node: element.as_node().opaque().0 as u32,
                    damage: data.damage,
                    initial,
                });
            // SAFETY: as above.
            unsafe { element.unset_dirty_descendants() };
        }
    }

    /// The post-order pass exists to build the engine's own flow tree, which we do not use.
    fn needs_postorder_traversal() -> bool {
        false
    }

    fn process_postorder(&self, _context: &mut StyleContext<E>, _node: E::ConcreteNode) {
        unreachable!("the post-order traversal is switched off")
    }

    fn shared_context(&self) -> &SharedStyleContext<'_> {
        &self.context
    }
}
