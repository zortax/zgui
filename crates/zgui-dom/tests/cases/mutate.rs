//! Does the batched change API leave the tree, the obligations and the document itself intact?
//!
//! Three questions, and each of them is about a failure that is silent rather than loud.
//!
//! A batch whose body panics leaves the document half-changed. If the depth counter it raised stays
//! raised, every later batch joins one that never closes and the work owed at a close never runs
//! again: the interface keeps running, keeps accepting input, and stops updating.
//!
//! A subtree built while detached accumulated obligations that stopped at its own root, because
//! there were no ancestors to tell. Linking it in has to fold them into its new ancestors, and
//! marking the root again cannot do it — marking returns at the node's own word before it reaches an
//! ancestor.
//!
//! And structural change interleaved with marking has to leave every node that owes work reachable
//! from the document node. That last one is checked against the definition rather than against an
//! expected list: run the walk, and compare what it serviced with the set of attached nodes that owe
//! something.

use zgui_bits::Dirty;
use zgui_dom::dirty::propagate;
use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKind, Poisoned};
use zgui_interned::ElementName;

use crate::support::edit;

/// A document with a root element and `count` children under it.
fn tree(count: usize) -> (Document, NodeIndex, Vec<NodeIndex>) {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let children = (0..count)
        .map(|_| document.append(root, NodeKind::Element, ElementName::new("box")))
        .collect();
    (document, root, children)
}

#[test]
fn edit_panic_poisons_document() {
    let (document, root, _) = tree(2);

    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        document.edit(&EverythingMatters, |batch| {
            batch.set_state(root, zgui_vocab::UiState::HOVER, true);
            panic!("a listener panicked halfway through a batch");
        })
    }));
    assert!(unwound.is_err());

    assert!(
        !document.is_editing(),
        "the batch counter has to be recoverable: a stranded one makes every later batch join one \
         that never closes"
    );
    assert!(document.is_poisoned());
    assert_eq!(
        document.edit(&EverythingMatters, |_| ()),
        Err(Poisoned),
        "a document that silently accepted changes after this would never update again"
    );
}

#[test]
fn a_batch_that_unwinds_does_not_run_the_work_owed_at_its_close() {
    let (document, root, children) = tree(3);
    document.take_redraw_request();

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        document.edit(&EverythingMatters, |batch| {
            batch.remove(children[1]);
            panic!("halfway through");
        })
    }));
    assert!(
        !document.redraw_requested(),
        "a frame asked for on behalf of a batch that did not finish would present the half-change"
    );
    assert_eq!(document.store().core(root).child_count(), 2);
}

#[test]
fn a_detached_subtree_reaches_the_root_the_moment_it_is_linked_in() {
    let (mut document, _root, _) = tree(1);
    edit::retire(&mut document);

    let (host, inner) = document
        .edit(&EverythingMatters, |batch| {
            let host = batch.create_element(ElementName::new("panel"));
            let inner = batch.create_element(ElementName::new("item"));
            batch.insert_before(host, inner, None);
            (host, inner)
        })
        .expect("the document is not poisoned");

    // Marked while detached: the obligation stops at `host`, because nothing above it exists.
    propagate::mark(document.store_mut(), inner, Dirty::RESHAPE);
    let root = document.root_index().expect("the document has a root");
    assert!(
        !document
            .store()
            .core(root)
            .dirty()
            .subtree()
            .contains(Dirty::RESHAPE)
    );

    document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(root, host, None);
        })
        .expect("the document is not poisoned");

    let serviced = edit::retire(&mut document);
    assert!(
        serviced.contains(&inner),
        "the obligations a subtree accumulated while detached are only reachable if the insertion \
         splices them into its new ancestors"
    );
}

#[test]
fn a_removal_records_the_subtree_that_left_and_marks_its_parent_for_repaint() {
    let (mut document, root, children) = tree(3);
    edit::retire(&mut document);

    document
        .edit(&EverythingMatters, |batch| batch.remove(children[1]))
        .expect("the document is not poisoned");

    let owed = document.store().core(root).dirty().own();
    assert!(owed.contains(Dirty::CHILDREN));
    assert!(
        owed.contains(Dirty::RELAYOUT | Dirty::REPAINT),
        "the area a removed subtree vacated is the one damage no restyle can produce"
    );
    assert_eq!(document.take_removed(), vec![children[1]]);
    assert!(document.take_removed().is_empty());
}

#[test]
fn a_move_across_the_document_node_changes_what_the_element_is() {
    let (document, root, children) = tree(2);
    let moved = children[0];
    assert!(!document.node(moved).matches_root());

    document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(document.document_index(), moved, None);
        })
        .expect("the document is not poisoned");
    assert!(
        document.node(moved).matches_root(),
        "root-ness is a fact about the parent, so a move changes it without touching the element"
    );
    assert_eq!(document.store().core(root).child_count(), 1);
}

/// A tiny reproducible generator, so a disagreement is a bug report rather than a coincidence.
struct Rng(u64);

impl Rng {
    /// The next value below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

/// Every node still attached to `document`, by slot number.
fn attached(document: &Document) -> Vec<NodeIndex> {
    (0..document.store().slot_count() as u32)
        .map(NodeIndex::new)
        .filter(|index| {
            let Some(record) = document.store().try_core(*index) else {
                return false;
            };
            let mut current = record.parent();
            let mut steps = 0;
            while let Some(parent) = current {
                if parent == document.document_index() {
                    return true;
                }
                current = document.store().core(parent).parent();
                steps += 1;
                assert!(steps < 1_000, "the tree has a cycle in it");
            }
            false
        })
        .collect()
}

/// Insertion, removal, reparenting and marking interleaved, judged against the definition.
///
/// The property is the one every stage of a frame rests on: a walk starting at the document node
/// services exactly the attached nodes that owe work. Anything left owing that the walk cannot reach
/// is a node whose obligation is silently never serviced, which is the failure mode nothing else in
/// the crate can see.
#[test]
fn structural_change_interleaved_with_marking_leaves_nothing_unreachable() {
    let bits = [Dirty::RESTYLE, Dirty::A11Y, Dirty::RESHAPE, Dirty::REPAINT];
    for seed in 1..40u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let (mut document, root, children) = tree(14);
        let mut live: Vec<NodeIndex> = children.clone();
        live.push(root);
        // Non-vacuity: the run form has to be reached, or the case only measures the exact list.
        let mut spans = 0;
        edit::retire(&mut document);

        for _ in 0..120 {
            match rng.below(4) {
                0 => {
                    // Biased towards one wide child list, because the record only degrades to the
                    // run form on the fifth distinct marked child of a single parent, and a tree
                    // that spreads its changes evenly never reaches it.
                    let parent = if rng.below(3) == 0 {
                        live[rng.below(live.len())]
                    } else {
                        root
                    };
                    let before = nth_child(&document, parent, &mut rng);
                    let fresh = document
                        .edit(&EverythingMatters, |batch| {
                            let fresh = batch.create_element(ElementName::new("box"));
                            batch.insert_before(parent, fresh, before);
                            fresh
                        })
                        .expect("the document is not poisoned");
                    live.push(fresh);
                }
                1 => {
                    let node = live[rng.below(live.len())];
                    if node != root {
                        document
                            .edit(&EverythingMatters, |batch| batch.remove(node))
                            .expect("the document is not poisoned");
                    }
                }
                2 => {
                    let node = live[rng.below(live.len())];
                    let parent = if rng.below(3) == 0 {
                        live[rng.below(live.len())]
                    } else {
                        root
                    };
                    if node != root && node != parent && !is_ancestor(&document, node, parent) {
                        // Somewhere among the parent's existing children rather than always at the
                        // end: a reorder that only ever appends never moves a child list's ends
                        // past one another, which is the shape the dirty-child run is described by.
                        let before =
                            nth_child(&document, parent, &mut rng).filter(|before| *before != node);
                        document
                            .edit(&EverythingMatters, |batch| {
                                batch.insert_before(parent, node, before);
                            })
                            .expect("the document is not poisoned");
                    }
                }
                _ => {
                    let node = if rng.below(3) == 0 {
                        live[rng.below(live.len())]
                    } else {
                        nth_child(&document, root, &mut rng).unwrap_or(root)
                    };
                    if document.store().core(root).dirty_children().is_span() {
                        spans += 1;
                    }
                    propagate::mark(document.store_mut(), node, bits[rng.below(bits.len())]);
                }
            }
        }

        assert!(
            spans > 0,
            "seed {seed}: the dirty-child record never reached the run form"
        );
        let mut owing: Vec<NodeIndex> = attached(&document)
            .into_iter()
            .filter(|index| !document.store().core(*index).dirty().own().is_clean())
            .collect();
        let mut serviced = edit::retire(&mut document);
        owing.sort();
        serviced.sort();
        assert_eq!(
            serviced, owing,
            "seed {seed}: the walk has to service exactly the attached nodes that owe work"
        );

        // And nothing is left owing anything at all, so the next frame starts clean.
        for index in attached(&document) {
            let (own, subtree) = document.store().core(index).dirty().get();
            assert!(
                (own | subtree).is_clean(),
                "seed {seed}: node {} still owes {own:?} / {subtree:?} after a full walk",
                index.get()
            );
        }
    }
}

/// One of `parent`'s children at random, or [`None`] to mean "at the end".
fn nth_child(document: &Document, parent: NodeIndex, rng: &mut Rng) -> Option<NodeIndex> {
    let mut children = Vec::new();
    let mut current = document.store().core(parent).first_child();
    while let Some(index) = current {
        children.push(index);
        current = document.store().core(index).next_sibling();
    }
    let at = rng.below(children.len() + 1);
    children.get(at).copied()
}

/// Whether `candidate` is at or above `node`.
fn is_ancestor(document: &Document, candidate: NodeIndex, node: NodeIndex) -> bool {
    let mut current = Some(node);
    while let Some(index) = current {
        if index == candidate {
            return true;
        }
        current = document.store().core(index).parent();
    }
    false
}

/// A worker that panics mid-traversal leaves per-element bookkeeping in a state no later traversal
/// can interpret, so the document is poisoned rather than reused. This is a decision to fail loudly
/// on an internal invariant violation, not a recovery path — the panic keeps going outwards.
#[test]
fn a_panicking_restyle_worker_poisons_the_document() {
    let mut table = crate::support::rows::Rows::new(4);
    let mut engine = table.styled("li { color: rgb(1, 1, 1) } .hot { color: rgb(9, 0, 0) }");
    engine.panic_in_worker_at(table.rows[2]);
    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.add_class(table.rows[2], zgui_interned::ClassName::new("hot"));
        })
        .expect("the document is not poisoned");

    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine.restyle(&mut table.document, None)
    }));
    assert!(unwound.is_err(), "the panic is not swallowed");
    assert!(table.document.is_poisoned());
    assert_eq!(
        table
            .document
            .edit(&EverythingMatters, |batch| batch.remove(table.rows[0])),
        Err(Poisoned)
    );
}

/// A dirty-child run whose *end* is moved earlier in the same child list.
///
/// Unlinking leaves the run naming a node that is no longer where the run says it is, and widening
/// for that node then takes the "already inside the run" path and returns. The run reaches from its
/// start to the end of the child list and never meets the moved node, so its obligations survive
/// with nothing leading to them: no panic, no log, no counter, and one row that never restyles
/// again. It is the reorder every list does.
#[test]
fn moving_the_end_of_a_dirty_run_to_the_front_keeps_it_reachable() {
    let (mut document, root, children) = tree(10);
    edit::retire(&mut document);

    // Five marks under one parent is what promotes the record from four exact children to the
    // inclusive run they span.
    for index in [2, 3, 5, 6, 7] {
        propagate::mark(document.store_mut(), children[index], Dirty::RESTYLE);
    }
    assert!(document.store().core(root).dirty_children().is_span());

    document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(root, children[7], Some(children[0]));
        })
        .expect("the document is not poisoned");

    let serviced = edit::retire(&mut document);
    assert!(
        serviced.contains(&children[7]),
        "the node that moved owes a restyle and nothing leads to it"
    );
    assert!(document.store().core(children[7]).dirty().own().is_clean());
}

/// The mirror: the run's *start* moved to the back. This half was already repaired, and it is here
/// so that a repair of one end alone is a failing test rather than a passing one.
#[test]
fn moving_the_start_of_a_dirty_run_to_the_back_keeps_it_reachable() {
    let (mut document, root, children) = tree(10);
    edit::retire(&mut document);
    for index in [2, 3, 5, 6, 7] {
        propagate::mark(document.store_mut(), children[index], Dirty::RESTYLE);
    }

    document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(root, children[2], None);
        })
        .expect("the document is not poisoned");

    let serviced = edit::retire(&mut document);
    for index in [2, 3, 5, 6, 7] {
        assert!(
            serviced.contains(&children[index]),
            "child {index} owes a restyle and nothing leads to it"
        );
    }
}

/// A batch nested inside another, whose body panics, where the unwind is caught before it leaves the
/// outer batch. The depth never returns to zero on the way out, so a poison conditional on that
/// would let the outer batch close over a document with half of a change applied and records
/// describing neither state — the failure the policy exists to make loud.
#[test]
fn a_nested_batch_that_unwinds_poisons_even_when_its_caller_catches_it() {
    let (document, root, children) = tree(3);

    let outcome = document.edit(&EverythingMatters, |batch| {
        batch.set_state(root, zgui_vocab::UiState::HOVER, true);
        let document = batch.document();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            document.edit(&EverythingMatters, |nested| {
                nested.remove(children[1]);
                panic!("a listener panicked inside a nested batch");
            })
        }))
        .is_err()
    });

    assert_eq!(outcome, Ok(true), "the outer batch itself did not unwind");
    assert!(document.is_poisoned());
    assert_eq!(document.edit(&EverythingMatters, |_| ()), Err(Poisoned));
}
