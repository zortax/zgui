//! A keyed list against a reference implementation, over ten thousand random edit sequences.
//!
//! The reference is a plain `Vec`: whatever it holds after a sequence of edits is exactly what the
//! rendered rows must read. A keyed list is the one piece of this crate whose bugs are invisible
//! in any small example and obvious in aggregate — a wrong move shows up as two rows swapped after
//! the ninth edit of a particular sequence, and no hand-written case finds that.

use std::rc::Rc;

use zgui_interned::ElementName;
use zgui_reactive::prelude::*;
use zgui_reactive::{Mounted, RwSignal, flush, install};
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::{Anchor, AnyView, BuildCxOwned, DocumentId, DomHandle, For, HostHandle, View};

/// A deterministic generator, so a failure is reproducible from its seed alone.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }
}

/// One change to the list.
#[derive(Debug, Clone, Copy)]
enum Edit {
    /// Insert a fresh key at this position.
    Insert(usize),
    /// Remove the key at this position.
    Remove(usize),
    /// Move the key at the first position to the second.
    Move(usize, usize),
    /// Swap the keys at two positions.
    Swap(usize, usize),
    /// Reverse the whole list.
    Reverse,
    /// Empty the list.
    Clear,
}

/// Applies `edit` to `keys`, minting fresh keys from `next`.
fn apply(keys: &mut Vec<u32>, edit: Edit, next: &mut u32) {
    match edit {
        Edit::Insert(at) => {
            let at = at.min(keys.len());
            keys.insert(at, *next);
            *next += 1;
        }
        Edit::Remove(at) => {
            if !keys.is_empty() {
                keys.remove(at % keys.len());
            }
        }
        Edit::Move(from, to) => {
            if !keys.is_empty() {
                let from = from % keys.len();
                let key = keys.remove(from);
                let to = to % (keys.len() + 1);
                keys.insert(to, key);
            }
        }
        Edit::Swap(first, second) => {
            if !keys.is_empty() {
                let first = first % keys.len();
                let second = second % keys.len();
                keys.swap(first, second);
            }
        }
        Edit::Reverse => keys.reverse(),
        Edit::Clear => keys.clear(),
    }
}

/// A random edit.
fn random_edit(rng: &mut Rng, len: usize) -> Edit {
    match rng.below(10) {
        0..=3 => Edit::Insert(rng.below(len + 1)),
        4..=5 => Edit::Remove(rng.below(len.max(1))),
        6..=7 => Edit::Move(rng.below(len.max(1)), rng.below(len.max(1) + 1)),
        8 => Edit::Swap(rng.below(len.max(1)), rng.below(len.max(1))),
        _ => {
            if rng.below(4) == 0 {
                Edit::Clear
            } else {
                Edit::Reverse
            }
        }
    }
}

/// What the rows should read for a given key list.
fn expected(keys: &[u32]) -> String {
    keys.iter().map(|key| format!("<{key}>")).collect()
}

#[test]
fn a_keyed_list_matches_a_reference_vector_over_ten_thousand_edit_sequences() {
    install().ok();

    let backend = Rc::new(StubDom::new(DocumentId::FIRST));
    let dom = DomHandle::from_rc(backend.clone());
    let window = Mounted::new();
    let cx = BuildCxOwned::new(
        dom.clone(),
        HostHandle::new(StubHost::default()),
        window.owner().clone(),
        DocumentId::FIRST,
    );
    let root = dom.create_element(ElementName::new("column"));

    let keys = window.with(|| RwSignal::new(Vec::<u32>::new()));
    let mut state = window.with(|| {
        For::new(
            move || keys.get(),
            |key: &u32| *key,
            |key| AnyView::new(format!("<{key}>")),
        )
        .build(&mut cx.cx())
    });
    state.mount(&dom, root, None);

    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut reference: Vec<u32> = Vec::new();
    let mut next_key = 0u32;

    // Ten thousand sequences, each a handful of edits applied to whatever the last one left.
    for sequence in 0..10_000u32 {
        let edits = 1 + rng.below(4);
        for _ in 0..edits {
            let edit = random_edit(&mut rng, reference.len());
            apply(&mut reference, edit, &mut next_key);
        }

        keys.set(reference.clone());
        flush();

        assert_eq!(
            backend.text_content(root),
            expected(&reference),
            "sequence {sequence} left the rows out of order"
        );
    }

    // The reference and the rendered rows agreed at every one of those steps, and the list is
    // still one subtree rather than a pile of orphans.
    state.unmount(&dom);
    assert_eq!(backend.text_content(root), "");
    window.unmount();
}

#[test]
fn the_sequences_actually_exercise_every_kind_of_edit() {
    // A property test whose generator never produced a removal would pass vacuously, so the
    // generator itself is checked against the same seed the test above uses.
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut inserts, mut removes, mut moves, mut swaps, mut reverses, mut clears) =
        (0, 0, 0, 0, 0, 0);
    let mut length = 8;
    for _ in 0..10_000 {
        match random_edit(&mut rng, length) {
            Edit::Insert(_) => {
                inserts += 1;
                length += 1;
            }
            Edit::Remove(_) => {
                removes += 1;
                length = length.saturating_sub(1);
            }
            Edit::Move(..) => moves += 1,
            Edit::Swap(..) => swaps += 1,
            Edit::Reverse => reverses += 1,
            Edit::Clear => {
                clears += 1;
                length = 0;
            }
        }
    }
    for (count, name) in [
        (inserts, "insert"),
        (removes, "remove"),
        (moves, "move"),
        (swaps, "swap"),
        (reverses, "reverse"),
        (clears, "clear"),
    ] {
        assert!(
            count > 50,
            "the generator produced only {count} {name} edits"
        );
    }
}

#[test]
fn a_row_that_did_not_move_keeps_its_node() {
    install().ok();
    let backend = Rc::new(StubDom::new(DocumentId::FIRST));
    let dom = DomHandle::from_rc(backend.clone());
    let window = Mounted::new();
    let cx = BuildCxOwned::new(
        dom.clone(),
        HostHandle::new(StubHost::default()),
        window.owner().clone(),
        DocumentId::FIRST,
    );
    let root = dom.create_element(ElementName::new("column"));

    let keys = window.with(|| RwSignal::new(vec![1u32, 2, 3]));
    let mut state = window.with(|| {
        For::new(
            move || keys.get(),
            |key: &u32| *key,
            |key| AnyView::new(key.to_string()),
        )
        .build(&mut cx.cx())
    });
    state.mount(&dom, root, None);
    let before = backend.node_count();

    // Prepending is one new row, not three rewrites.
    keys.set(vec![0, 1, 2, 3]);
    flush();
    assert_eq!(backend.text_content(root), "0123");
    assert_eq!(
        backend.node_count(),
        before + 2,
        "one row is one marker plus one text node"
    );
    window.unmount();
}

#[test]
fn rebuilding_the_list_itself_adopts_the_rows_that_are_already_there() {
    // A list inside a reactive hole is rebuilt whenever that hole re-runs. The collection closure
    // is a new one, so the effect watching the old one is replaced — but the rows it built are
    // still mounted, and replacing them would make every enclosing conditional throw the list
    // away and build it again.
    install().ok();
    let backend = Rc::new(StubDom::new(DocumentId::FIRST));
    let dom = DomHandle::from_rc(backend.clone());
    let window = Mounted::new();
    let cx = BuildCxOwned::new(
        dom.clone(),
        HostHandle::new(StubHost::default()),
        window.owner().clone(),
        DocumentId::FIRST,
    );
    let root = dom.create_element(ElementName::new("column"));

    let keys = window.with(|| RwSignal::new(vec![1u32, 2, 3]));
    let mut state = window.with(|| {
        For::new(
            move || keys.get(),
            |key: &u32| *key,
            |key| AnyView::new(format!("<{key}>")),
        )
        .build(&mut cx.cx())
    });
    state.mount(&dom, root, None);
    assert_eq!(backend.text_content(root), "<1><2><3>");
    let nodes = backend.node_count();
    let first = state.first_node();

    window.with(|| {
        For::new(
            move || keys.get(),
            |key: &u32| *key,
            |key| AnyView::new(format!("<{key}>")),
        )
        .rebuild(&mut state, &mut cx.cx());
    });

    assert_eq!(backend.text_content(root), "<1><2><3>");
    assert_eq!(state.first_node(), first, "the rows kept their nodes");
    assert_eq!(
        backend.node_count(),
        nodes,
        "not one node was created: the replacement effect adopted what was there"
    );

    // ... and the replacement effect is the one that is now watching the collection.
    keys.set(vec![3, 1]);
    flush();
    assert_eq!(backend.text_content(root), "<3><1>");
    window.unmount();
}
