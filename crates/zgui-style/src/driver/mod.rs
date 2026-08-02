//! Running the restyle: one traversal, what it produced, and what that costs the frame.
//!
//! | Module | Contents |
//! |---|---|
//! | [`snapshots`] | the pre-mutation records the traversal compares against |
//! | [`context`] | everything the workers share |
//! | [`traversal`] | what each worker does with each element, and the set it collects |
//! | [`animations`] | the running animations, the tick that advances them, and what it sampled |

pub mod animations;
pub mod context;
pub mod snapshots;
pub mod traversal;

use std::time::{Duration, Instant};

use rayon::ThreadPool;
use style::animation::DocumentAnimationSet;
use style::selector_parser::SnapshotMap;
use style::shared_lock::SharedRwLock;
use style::stylist::Stylist;
use style::traversal::DomTraversal;
use style::traversal_flags::TraversalFlags;
use zgui_dom::{Document, Node};

use crate::driver::animations::AnimationTime;
use crate::driver::traversal::{RecalcStyle, Restyled};
use crate::engine::guards;

/// What one restyle did.
///
/// Both counts of elements are here because they mean different things and only one of them is
/// ever what a budget is written against: an element being styled for the *first* time is not
/// "restyled", so the initial pass over a fresh document restyles none of it however much work it
/// did.
#[derive(Clone, Default, Debug)]
pub struct Restyle {
    /// Whether the traversal ran at all.
    pub traversed: bool,
    /// Elements the traversal styled, first-time cascades included.
    pub styled: usize,
    /// Elements that already had a style and were styled again.
    pub restyled: usize,
    /// Elements that ran selector matching, as opposed to only re-running the cascade.
    ///
    /// This is the number an "it restyled one element and looked at almost nothing" budget is
    /// about: a recascade is cheap and a match is not.
    pub matched: usize,
    /// Elements that came out with a non-empty obligation.
    pub damaged: usize,
    /// How many distinct workers ran at least one element.
    pub workers: usize,
    /// How many ordinary traversals ran, which is two when the root font metrics moved under it.
    pub passes: u8,
    /// Whether the animation-only traversal ran ahead of them.
    ///
    /// True exactly on the frames where some element's animation could not be composed as a
    /// repaint and asked for its cascade to be run again. It is reported rather than inferred
    /// because an animation-only traversal that silently stops running is invisible from
    /// everywhere else: the values keep advancing, the counters keep counting, and the elements
    /// are simply never restyled.
    pub animation_pass: bool,
    /// Elements the pre-mutation records described.
    pub snapshots: usize,
    /// What each styled element's data said, in the order the workers collected it.
    pub records: Vec<Restyled>,
    /// Time spent inside the style engine.
    pub engine_time: Duration,
}

impl Restyle {
    /// The elements this restyle styled, by slot number, in ascending order.
    ///
    /// Sorted, because the order the workers collected them in is whatever the pool decided, and a
    /// test that asserts *which* elements were restyled is asserting a set.
    pub fn styled_nodes(&self) -> Vec<zgui_dom::NodeIndex> {
        let mut nodes: Vec<_> = self.records.iter().map(|record| record.index).collect();
        nodes.sort_unstable();
        nodes.dedup();
        nodes
    }

    /// The elements that already had a style and were styled again, by slot number, ascending.
    pub fn restyled_nodes(&self) -> Vec<zgui_dom::NodeIndex> {
        let mut nodes: Vec<_> = self
            .records
            .iter()
            .filter(|record| !record.initial)
            .map(|record| record.index)
            .collect();
        nodes.sort_unstable();
        nodes.dedup();
        nodes
    }
}

/// What one traversal is run over, other than the rule set it matches against.
///
/// A frame runs the traversal more than once — the animation-only pass, then the ordinary one,
/// then the ordinary one again if the root font metrics moved — and every one of them is over the
/// same document, the same pre-mutation records, the same workers and the same instant. Naming
/// them once is what keeps the passes from differing in a way no reader would notice.
pub(crate) struct Pass<'a> {
    /// The document being styled.
    pub(crate) document: &'a Document,
    /// The pre-mutation records the traversal compares against.
    pub(crate) snapshots: &'a SnapshotMap,
    /// The workers it may run across, if any.
    pub(crate) pool: Option<&'a ThreadPool>,
    /// The animations the document is running.
    pub(crate) animations: DocumentAnimationSet,
    /// The instant animation-derived values are resolved at.
    pub(crate) now: AnimationTime,
}

/// Runs one traversal over the document and reports what it styled.
///
/// Returns nothing at all when the engine decides there is nothing to traverse, which is the
/// common case for a frame in which no style input changed.
///
/// `flags` chooses the traversal. [`TraversalFlags::empty`] is the ordinary one;
/// [`TraversalFlags::AnimationOnly`] is the pass that replaces animation and transition
/// declarations, descends only where one of those is pending, and must run before the ordinary one
/// in any frame that has any pending — the ordinary traversal refuses to process an animation hint
/// and asserts when it finds one.
pub(crate) fn run_pass(
    stylist: &mut Stylist,
    lock: &SharedRwLock,
    pass: Pass<'_>,
    flags: TraversalFlags,
) -> (Vec<Restyled>, usize, bool, Duration) {
    let Pass {
        document,
        snapshots,
        pool,
        animations,
        now,
    } = pass;
    let Some(root) = document.root() else {
        return (Vec::new(), 0, false, Duration::ZERO);
    };
    let start = Instant::now();

    // The rule set is flushed first, and what it reports is the set of elements the sheet changes
    // themselves invalidate — which is how a sheet that was added, replaced or re-matched against
    // a new device reaches a document in which nothing else changed.
    guards::with_guards(lock, |guards| {
        stylist.flush(guards).process_style(root, Some(snapshots));
    });

    let read = lock.read();
    let context = context::build(
        stylist,
        guards::guards(&read),
        snapshots,
        animations,
        now,
        flags,
    );
    let token = <RecalcStyle<'_> as DomTraversal<Node<'_>>>::pre_traverse(root, &context);
    if !token.should_traverse() {
        drop(context);
        return (Vec::new(), 0, false, start.elapsed());
    }

    let traverser = RecalcStyle::new(context);
    // A worker that panics leaves per-element bookkeeping in a state no later traversal can
    // interpret, and the worker that panicked holds nothing anyone can inspect, so the document is
    // poisoned on the way out and the panic keeps going.
    document
        .guarded(|| style::driver::traverse_dom(&traverser, token, pool))
        .expect("the document is not poisoned");
    let (records, workers) = traverser.finish();
    drop(read);
    (records, workers, true, start.elapsed())
}

/// Whether anything at or below the root owes the style engine work.
pub(crate) fn document_owes_restyle(document: &Document) -> bool {
    document
        .root()
        .is_some_and(|root| root.has_style_work_below())
}
