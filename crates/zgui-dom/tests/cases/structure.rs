//! Do the siblings of an inserted or removed child stop matching what they used to match?
//!
//! This is the invalidation nothing else supplies. The style engine records, while it matches, that
//! a selector under this parent depends on the child list — and then consumes none of it, because
//! the code that would is in the browser this engine was carved out of. So a document that does not
//! expand those flags itself has sibling and positional selectors that match once, at the first
//! frame, and never change again. There is no error, no log and no counter: the rows simply keep
//! the stripes they had.
//!
//! Every case here is judged against a document built to the final shape directly and styled once.
//! That is the only instrument that can see the failure, because two incremental renders share the
//! path that would be broken.

use zgui_dom::{EverythingMatters, NodeIndex, NodeKind};
use zgui_interned::{ClassName, ElementName};

use crate::support::edit;
use crate::support::engine::Engine;
use crate::support::read::{color, radius};
use crate::support::rows::{Rows, oracle};

/// Stripes, which is the shape whose invalidation is a suffix of the child list.
const STRIPED: &str = "li:nth-child(even) { border-top-left-radius: 3px }";

#[test]
fn prepending_a_row_to_a_striped_table_restyles_the_later_siblings() {
    let mut table = Rows::new(20);
    let mut engine = table.styled(STRIPED);
    let before = table.radii();

    let fresh = table.new_row();
    let first = table.rows[0];
    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(table.container, fresh, Some(first));
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut table.document, None);

    let after = table.radii();
    assert_ne!(after, before, "every stripe after the first has moved");
    assert_eq!(after, oracle(21, STRIPED));
}

/// Batch-close anchor resolution, in the first of the two shapes a single change cannot
/// answer: the earliest of two anchors recorded by two different changes wins, and it is chosen
/// after the one renumber at the close of the batch.
#[test]
fn one_batch_that_removes_a_row_and_inserts_an_earlier_one() {
    let mut table = Rows::new(40);
    let mut engine = table.styled(STRIPED);

    let fresh = table.new_row();
    let removed = table.rows[24];
    let anchor = table.rows[8];
    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.remove(removed);
            batch.insert_before(table.container, fresh, Some(anchor));
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut table.document, None);

    assert_eq!(table.radii(), oracle(40, STRIPED));
}

/// The second shape: the first removal records the element that followed it as its anchor, and the
/// second removal unlinks exactly that element. Asking it for a position would renumber a child
/// list it is no longer in, so the liveness test has to run first.
#[test]
fn two_adjacent_removes_in_one_batch_do_not_dangle_the_anchor() {
    let mut table = Rows::new(40);
    let mut engine = table.styled(STRIPED);

    let (first, second) = (table.rows[16], table.rows[17]);
    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.remove(first);
            batch.remove(second);
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut table.document, None);

    assert_eq!(table.radii(), oracle(38, STRIPED));
}

/// `dirty_children` is keyed by identity and never by position: the insertion has already
/// invalidated every position under the container by the time the class change is marked, so a
/// record keyed by position would descend into the wrong child and the class change would be lost.
#[test]
fn prepend_and_mark_an_existing_sibling_in_one_batch() {
    const SHEET: &str = "li:nth-child(even) { border-top-left-radius: 3px }
                         .picked { color: rgb(9, 0, 0) }";
    let mut table = Rows::new(40);
    let mut engine = table.styled(SHEET);

    let old_first = table.rows[0];
    let before = color(&table.document, old_first);
    let fresh = table.new_row();
    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(table.container, fresh, Some(old_first));
            batch.add_class(old_first, ClassName::new("picked"));
        })
        .expect("the document is not poisoned");
    let pass = engine.restyle(&mut table.document, None);

    assert!(
        pass.visited.contains(&old_first.get()),
        "the element the class change was made on has to be reached"
    );
    assert_ne!(color(&table.document, old_first), before);
    assert_eq!(color(&table.document, old_first), (9, 0, 0));
    assert_eq!(table.radii(), oracle(41, SHEET));
}

#[test]
fn last_child_across_an_append() {
    const SHEET: &str = ".list > :last-child { border-top-left-radius: 5px }";
    let mut group = Rows::new(3);
    let mut engine = group.styled(SHEET);
    assert_eq!(group.radii(), vec![0.0, 0.0, 5.0]);

    let fresh = group.new_row();
    group
        .document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(group.container, fresh, None);
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);

    assert_eq!(
        group.radii(),
        vec![0.0, 0.0, 0.0, 5.0],
        "the element that was last has to stop being styled as the last"
    );
    assert_eq!(group.radii(), oracle(4, SHEET));
}

/// The prepend, which is the case the stored pre-batch edge pair exists for: the old first child
/// ends up neither the new first nor the last, so nothing in "the edges as they are now" reaches
/// it, and the engine sets the edge flag exclusively of the two whole-child-list flags, so no other
/// arm covers it either. The removal twin passes with or without the stored pair, which is why it
/// is not the case that ships.
#[test]
fn first_child_across_a_prepend() {
    const SHEET: &str = ".list > :first-child { border-top-left-radius: 5px }";
    let mut group = Rows::new(3);
    let mut engine = group.styled(SHEET);
    assert_eq!(group.radii(), vec![5.0, 0.0, 0.0]);

    let fresh = group.new_row();
    let old_first = group.rows[0];
    group
        .document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(group.container, fresh, Some(old_first));
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);

    assert_eq!(
        radius(&group.document, old_first),
        0.0,
        "the element that was first keeps its rounded corner unless the pre-batch edge pair \
         reaches it"
    );
    assert_eq!(group.radii(), vec![5.0, 0.0, 0.0, 0.0]);
    assert_eq!(group.radii(), oracle(4, SHEET));
}

#[test]
fn only_child_across_an_insertion() {
    const SHEET: &str = ".list > :only-child { border-top-left-radius: 5px }";
    let mut group = Rows::new(1);
    let mut engine = group.styled(SHEET);
    assert_eq!(group.radii(), vec![5.0]);

    let fresh = group.new_row();
    group
        .document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(group.container, fresh, None);
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);

    assert_eq!(group.radii(), vec![0.0, 0.0]);
    assert_eq!(group.radii(), oracle(2, SHEET));
}

#[test]
fn an_adjacent_sibling_rule_sees_an_element_inserted_between_two_siblings() {
    const SHEET: &str = ".a + .b { border-top-left-radius: 7px }";
    let mut group = Rows::new(0);
    let mut engine = group.styled(SHEET);

    // `.b`, then `.a` prepended before it: the rule only matches once `.a` is its predecessor.
    let target = group
        .document
        .edit(&EverythingMatters, |batch| {
            let target = batch.create_element(ElementName::new("li"));
            batch.set_classes(target, &[ClassName::new("b")]);
            batch.insert_before(group.container, target, None);
            target
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);
    assert_eq!(radius(&group.document, target), 0.0);
    edit::retire(&mut group.document);

    group
        .document
        .edit(&EverythingMatters, |batch| {
            let inserted = batch.create_element(ElementName::new("li"));
            batch.set_classes(inserted, &[ClassName::new("a")]);
            batch.insert_before(group.container, inserted, Some(target));
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);

    assert_eq!(
        radius(&group.document, target),
        7.0,
        "the element the combinator reaches is the one after the change, and nothing else marks it"
    );
}

#[test]
fn an_empty_placeholder_follows_an_insertion_and_a_removal() {
    const SHEET: &str = ".list:empty { border-top-left-radius: 9px }";
    let mut group = Rows::new(0);
    let mut engine = group.styled(SHEET);
    let container = group.container;
    assert_eq!(radius(&group.document, container), 9.0);

    let fresh = group.new_row();
    group
        .document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(container, fresh, None);
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);
    assert_eq!(radius(&group.document, container), 0.0);
    edit::retire(&mut group.document);

    group
        .document
        .edit(&EverythingMatters, |batch| batch.remove(fresh))
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);
    assert_eq!(radius(&group.document, container), 9.0);
}

/// The case an application actually writes, and the one an insert-and-remove golden misses
/// entirely: an element counts as empty when it has no element child *and* no text child of
/// non-zero length, so a message going from nothing to something changes what it matches with no
/// node inserted and no node removed.
#[test]
fn an_empty_placeholder_follows_a_change_of_text_content() {
    const SHEET: &str = ".list:empty { border-top-left-radius: 9px }";
    let mut group = Rows::new(0);
    let container = group.container;
    let text = group
        .document
        .append(container, NodeKind::Text, ElementName::new("#text"));
    let mut engine = group.styled(SHEET);
    assert_eq!(
        radius(&group.document, container),
        9.0,
        "a zero-length text child leaves the element empty"
    );
    edit::retire(&mut group.document);

    group
        .document
        .edit(&EverythingMatters, |batch| batch.set_text(text, "Saved"))
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);
    assert_eq!(radius(&group.document, container), 0.0);
    edit::retire(&mut group.document);

    group
        .document
        .edit(&EverythingMatters, |batch| batch.set_text(text, ""))
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);
    assert_eq!(radius(&group.document, container), 9.0);
}

/// A parent no selector depends on costs one atomic load and records nothing, which is what keeps a
/// list append from becoming quadratic.
#[test]
fn a_parent_no_structural_selector_names_records_nothing() {
    const SHEET: &str = "li { border-top-left-radius: 1px }";
    let mut table = Rows::new(8);
    let mut engine = table.styled(SHEET);

    let fresh = table.new_row();
    let first = table.rows[0];
    table
        .document
        .edit(&EverythingMatters, |batch| {
            batch.insert_before(table.container, fresh, Some(first));
        })
        .expect("the document is not poisoned");
    let pass = engine.restyle(&mut table.document, None);

    let siblings: Vec<NodeIndex> = table.rows.clone();
    let touched = siblings
        .iter()
        .filter(|row| pass.visited.contains(&row.get()))
        .count();
    assert_eq!(
        touched, 0,
        "no selector here depends on the child list, so no sibling has anything to re-match"
    );
    assert!(pass.visited.contains(&fresh.get()));
}

/// A text node has no position among element siblings, so inserting one moves none of them. Recorded
/// as an anchor it compares as the earliest child of the parent — nothing renumbers it, so it holds
/// the zero it was born with — and then expands along the element chain to nothing at all, because a
/// text node has no element sibling links. The suffix a real anchor recorded in the same batch is
/// swallowed, and every stripe after the insertion keeps the value it had.
#[test]
fn a_text_node_inserted_beside_a_prepend_does_not_swallow_the_prepend() {
    let mut table = Rows::new(20);
    let mut engine = table.styled(STRIPED);

    let first = table.rows[0];
    let fresh = table.new_row();
    table
        .document
        .edit(&EverythingMatters, |batch| {
            let text = batch.create_text("hello");
            batch.insert_before(table.container, text, None);
            batch.insert_before(table.container, fresh, Some(first));
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut table.document, None);

    assert_eq!(table.radii(), oracle(21, STRIPED));
}

/// The mirror, and the one whose cost is the point: taking a text node out moves no element either,
/// so it must not record a change to the child list. It has no later element sibling to anchor on,
/// which makes the entry's anchor list empty and sends the whole child list down the conservative
/// arm — O(children) of restyling for a change no positional selector can see.
#[test]
fn a_text_node_removed_from_a_striped_list_restyles_no_sibling() {
    let mut table = Rows::new(8);
    let mut engine = table.styled(STRIPED);
    let text = table
        .document
        .edit(&EverythingMatters, |batch| {
            let text = batch.create_text("hello");
            batch.insert_before(table.container, text, None);
            text
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut table.document, None);
    edit::retire(&mut table.document);

    table
        .document
        .edit(&EverythingMatters, |batch| batch.remove(text))
        .expect("the document is not poisoned");
    let pass = engine.restyle(&mut table.document, None);

    let touched = table
        .rows
        .iter()
        .filter(|row| pass.visited.contains(&row.get()))
        .count();
    assert_eq!(
        touched, 0,
        "no element moved, so no element has to re-match"
    );
    assert_eq!(table.radii(), oracle(8, STRIPED));
}

/// And the emptiness a text node *does* carry is still recorded, through the arm that is about the
/// parent itself rather than about its children's positions.
#[test]
fn an_empty_placeholder_follows_a_text_node_arriving_and_leaving() {
    const SHEET: &str = ".list:empty { border-top-left-radius: 9px }";
    let mut group = Rows::new(0);
    let mut engine = group.styled(SHEET);
    let container = group.container;
    assert_eq!(radius(&group.document, container), 9.0);

    let text = group
        .document
        .edit(&EverythingMatters, |batch| {
            let text = batch.create_text("Saved");
            batch.insert_before(container, text, None);
            text
        })
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);
    assert_eq!(radius(&group.document, container), 0.0);
    edit::retire(&mut group.document);

    group
        .document
        .edit(&EverythingMatters, |batch| batch.remove(text))
        .expect("the document is not poisoned");
    engine.restyle(&mut group.document, None);
    assert_eq!(radius(&group.document, container), 9.0);
}

/// Every structural flag at once, so one disagreement names whichever arm is wrong.
///
/// `:nth-child` and the two combinators set the later-siblings flag, `:nth-last-child` sets the
/// whole-child-list one, the two edge rules set the edge flag, and `:empty` sets the parent's own.
const FUZZ_SHEET: &str = "
li:nth-child(even) { border-top-left-radius: 3px }
.list > :first-child { color: rgb(1, 0, 0) }
.list > :last-child { font-size: 20px }
li:nth-last-child(2) { display: inline }
.a + .b { border-top-left-radius: 7px }
.a ~ .c { color: rgb(0, 9, 0) }
.list:empty { border-top-left-radius: 9px }
";

/// A tiny reproducible generator, so a disagreement is a bug report rather than a coincidence.
struct FzRng(u64);
impl FzRng {
    /// The next value below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

/// The classes a row can carry.
const KINDS: [&str; 3] = ["a", "b", "c"];

/// One child of the container, as the model records it.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    /// An element carrying one of [`KINDS`].
    Elem(usize),
    /// A text node holding this body.
    Text(String),
}

/// A document whose container holds exactly `model`, styled by nothing yet.
fn fz_build(model: &[Item]) -> (zgui_dom::Document, NodeIndex, Vec<NodeIndex>) {
    let mut document = zgui_dom::Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let container = document.append(root, NodeKind::Element, ElementName::new("ul"));
    document.set_classes(container, &[ClassName::new("list")]);
    let kids = model
        .iter()
        .map(|item| match item {
            Item::Elem(kind) => {
                let row = document.append(container, NodeKind::Element, ElementName::new("li"));
                document.set_classes(row, &[ClassName::new(KINDS[*kind])]);
                row
            }
            Item::Text(body) => {
                let text = document.append(container, NodeKind::Text, ElementName::new("#text"));
                zgui_dom::text::node::set_text(document.store_mut(), text, body);
                text
            }
        })
        .collect();
    (document, container, kids)
}

/// Four computed values off every element child of `container`, plus the container's own.
fn fz_digest(document: &zgui_dom::Document, container: NodeIndex) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = document.store().core(container).first_element_child();
    while let Some(index) = current {
        let style = document.node(index).primary_style().expect("styled");
        out.push(format!(
            "{:?}|{:?}|{:?}|{:?}",
            crate::support::read::radius(document, index),
            style
                .get_inherited_text()
                .clone_color()
                .into_srgb_legacy()
                .raw_components(),
            style.get_font().clone_font_size().computed_size().px(),
            style.get_box().clone_display(),
        ));
        current = document.store().core(index).next_element();
    }
    out.push(format!(
        "container:{:?}",
        crate::support::read::radius(document, container)
    ));
    out
}

/// Insertion, removal, reordering, text edits and class changes, interleaved in batches and judged
/// against a document built to the same shape and styled once.
///
/// The goldens above each name one flag and one shape. This names none of them: it applies whatever
/// the generator produces and asks only that the incrementally-invalidated document agree with one
/// that never had a stale value to keep. That is the only instrument that can see an arm which is
/// right for the shape it was written against and wrong for the one beside it.
#[test]
fn incremental_invalidation_agrees_with_a_from_scratch_document() {
    for seed in 1..120u64 {
        let mut rng = FzRng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut model: Vec<Item> = (0..6).map(|i| Item::Elem(i % 3)).collect();
        let (mut document, container, kids) = fz_build(&model);
        let mut live = kids;
        let mut engine = Engine::new(&document);
        engine.add_author_sheet(FUZZ_SHEET);
        engine.restyle(&mut document, None);
        edit::retire(&mut document);

        for round in 0..12 {
            let ops = 1 + rng.below(3);
            document
                .edit(&EverythingMatters, |batch| {
                    for _ in 0..ops {
                        let choice = if live.is_empty() { 0 } else { rng.below(6) };
                        match choice {
                            0 | 5 => {
                                let at = rng.below(live.len() + 1);
                                let before = live.get(at).copied();
                                if choice == 5 {
                                    let body = ["", "hi", "there"][rng.below(3)].to_owned();
                                    let text = batch.create_text(&body);
                                    batch.insert_before(container, text, before);
                                    live.insert(at, text);
                                    model.insert(at, Item::Text(body));
                                } else {
                                    let kind = rng.below(KINDS.len());
                                    let fresh = batch.create_element(ElementName::new("li"));
                                    batch.set_classes(fresh, &[ClassName::new(KINDS[kind])]);
                                    batch.insert_before(container, fresh, before);
                                    live.insert(at, fresh);
                                    model.insert(at, Item::Elem(kind));
                                }
                            }
                            1 => {
                                let at = rng.below(live.len());
                                batch.remove(live[at]);
                                live.remove(at);
                                model.remove(at);
                            }
                            2 => {
                                let from = rng.below(live.len());
                                let to = rng.below(live.len());
                                let node = live[from];
                                let item = model[from].clone();
                                live.remove(from);
                                model.remove(from);
                                let to = to.min(live.len());
                                let before = live.get(to).copied();
                                batch.insert_before(container, node, before);
                                live.insert(to, node);
                                model.insert(to, item);
                            }
                            3 => {
                                let at = rng.below(live.len());
                                if let Item::Text(_) = model[at] {
                                    let body = ["", "hi", "there"][rng.below(3)].to_owned();
                                    batch.set_text(live[at], &body);
                                    model[at] = Item::Text(body);
                                }
                            }
                            _ => {
                                let at = rng.below(live.len());
                                if let Item::Elem(_) = model[at] {
                                    let kind = rng.below(KINDS.len());
                                    batch.set_classes(live[at], &[ClassName::new(KINDS[kind])]);
                                    model[at] = Item::Elem(kind);
                                }
                            }
                        }
                    }
                })
                .expect("not poisoned");
            engine.restyle(&mut document, None);
            edit::retire(&mut document);

            let (mut fresh_doc, fresh_container, _) = fz_build(&model);
            let mut fresh_engine = Engine::new(&fresh_doc);
            fresh_engine.add_author_sheet(FUZZ_SHEET);
            fresh_engine.restyle(&mut fresh_doc, None);

            assert_eq!(
                fz_digest(&document, container),
                fz_digest(&fresh_doc, fresh_container),
                "seed {seed} round {round}: model {model:?}"
            );
        }
    }
}
