//! Does a second mark, with no phase walk between it and the first, still get descended into?
//!
//! This is the one property nothing had run before. Marking returns at its *first* early-out — the
//! node's own invalidation word — before it touches any ancestor, so a second mark of a node whose
//! own bits have not yet been retired restores nothing anywhere. Whether that matters depends
//! entirely on whether the flag the traversal descends by is the same storage: if the engine keeps a
//! flag of its own, cleared on the engine's schedule, then the second traversal stops above the node
//! and the change is lost silently. If the descent question is answered from the invalidation word
//! itself, the union is still there and the traversal still descends.
//!
//! The hazard and the ancestor marking have each been shown on their own, the first against a
//! stand-in flag and the second against a traversal that never re-marked. Neither exercised both at
//! once against a real traversal. This does.
//!
//! The last case is the same question from the other side. The engine raises a descent flag of its
//! own — on the *sibling* whose descendant an invalidation reached — and it raises it in the same
//! storage, so a walk of the marked set never visits that node and never retires it. A mark of that
//! node afterwards must still reach the ancestors, which it only does if the early-out tests the
//! node's own bits rather than its subtree union.

use zgui_bits::Dirty;
use zgui_dom::{Document, NodeIndex, NodeKind};
use zgui_interned::{ClassName, ElementName};

use crate::support::edit;
use crate::support::engine::Engine;
use crate::support::fixture;
use crate::support::read::color;

/// Whether `pass` visited the element at `index`.
fn visited(pass: &crate::support::engine::Pass, index: NodeIndex) -> bool {
    pass.visited.contains(&index.get())
}

#[test]
fn a_repeat_mark_across_a_real_traversal_still_descends() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".item { color: rgb(1, 1, 1) }
         .item.one { color: rgb(2, 0, 0) }
         .item.two { color: rgb(3, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);

    let target = tree.at("i1");

    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("one")],
    );
    let first = engine.restyle(&mut tree.document, None);
    assert!(visited(&first, target), "the first traversal reaches it");
    assert_eq!(color(&tree.document, target), (2, 0, 0));

    // No retirement here. This is the whole point: the node's own invalidation word still carries
    // the obligation from the first mark, so the second mark returns immediately and restores no
    // ancestor state at all.
    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("two")],
    );
    let second = engine.restyle(&mut tree.document, None);

    assert!(
        visited(&second, target),
        "the second traversal has to reach it too; a descent flag retired on a schedule of its own \
         would have been cleared by the first traversal and never restored"
    );
    assert_eq!(
        color(&tree.document, target),
        (3, 0, 0),
        "and the second change has to actually take effect"
    );
}

#[test]
fn retiring_between_the_two_marks_changes_nothing_about_the_answer() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        ".item { color: rgb(1, 1, 1) }
         .item.one { color: rgb(2, 0, 0) }
         .item.two { color: rgb(3, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);

    let target = tree.at("i1");
    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("one")],
    );
    engine.restyle(&mut tree.document, None);
    edit::retire(&mut tree.document);

    edit::set_classes(
        &tree.document,
        target,
        &[ClassName::new("item"), ClassName::new("two")],
    );
    let second = engine.restyle(&mut tree.document, None);
    assert!(visited(&second, target));
    assert_eq!(color(&tree.document, target), (3, 0, 0));
}

/// Retires the invalidation the way a phase walk does: only at the nodes it descends to.
///
/// A walk starts at the document node and descends by the dirty-child records, so a node whose
/// subtree union was raised by something other than a mark is never visited and never retired.
fn walk_retire(document: &Document) {
    fn visit(document: &Document, index: NodeIndex) {
        let record = document.store().core(index);
        let children: Vec<NodeIndex> = record
            .dirty_children()
            .iter(document.store(), index)
            .collect();
        for child in children {
            visit(document, child);
        }
        record.dirty().clear_own(Dirty::all());
        record.dirty().retire_phase(Dirty::all(), Dirty::empty());
        record.dirty_children().clear();
    }
    visit(document, document.document_index());
}

/// `root > (x, y > z)`, the smallest shape in which an invalidation crosses to a sibling's subtree.
struct Sideways {
    /// The document.
    document: Document,
    /// The element whose class change starts the invalidation.
    x: NodeIndex,
    /// Its next sibling, which the engine raises its own descent flag on.
    y: NodeIndex,
    /// The element the combinator actually reaches.
    z: NodeIndex,
}

/// Builds the sideways shape.
fn sideways() -> Sideways {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let x = document.append(root, NodeKind::Element, ElementName::new("box"));
    let y = document.append(root, NodeKind::Element, ElementName::new("box"));
    document.set_classes(y, &[ClassName::new("b")]);
    let z = document.append(y, NodeKind::Element, ElementName::new("box"));
    document.set_classes(z, &[ClassName::new("c")]);
    Sideways { document, x, y, z }
}

#[test]
fn a_mark_of_a_node_the_engine_left_a_descent_flag_on_still_reaches_the_root() {
    let mut shape = sideways();
    let mut engine = Engine::new(&shape.document);
    engine.add_author_sheet(
        ".c { color: rgb(1, 1, 1) }
         .a + .b .c { color: rgb(9, 0, 0) }
         .b.q { color: rgb(5, 0, 0) }",
    );
    engine.restyle(&mut shape.document, None);
    walk_retire(&shape.document);
    assert_eq!(color(&shape.document, shape.z), (1, 1, 1));

    // A class change on `x` invalidates its *sibling's* descendant through the combinator, so the
    // engine raises its own descent flag on `y` — a node the marked set never leads to.
    edit::set_classes(&shape.document, shape.x, &[ClassName::new("a")]);
    engine.restyle(&mut shape.document, None);
    assert_eq!(
        color(&shape.document, shape.z),
        (9, 0, 0),
        "the combinator applied, which is what makes the engine descend into `y` at all"
    );
    walk_retire(&shape.document);
    assert!(
        shape
            .document
            .store()
            .core(shape.y)
            .dirty()
            .subtree()
            .contains(Dirty::RESTYLE),
        "the engine's descent flag on `y` outlives the walk, because the walk never visits `y`"
    );

    // Now `y` itself changes. Its own bits are clean, so this mark has real work to do — and a mark
    // that early-returned on the subtree union would do none of it.
    edit::set_classes(
        &shape.document,
        shape.y,
        &[ClassName::new("b"), ClassName::new("q")],
    );
    let pass = engine.restyle(&mut shape.document, None);
    assert!(
        pass.traversed,
        "the mark on `y` has to reach the root, or nothing is traversed at all"
    );
    assert_eq!(
        color(&shape.document, shape.y),
        (5, 0, 0),
        "and the failure is a silently stale colour, not an error"
    );
}
