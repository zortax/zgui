//! Does a parallel traversal agree with a sequential one?
//!
//! The whole cell discipline exists for this: every field a worker can reach is read while other
//! workers are standing on the same records, and two of them are written from a worker. A wrong
//! answer here would not be a crash, it would be a computed value that differs run to run.
//!
//! The comparison is only worth anything if the traversal actually fanned out, so each worker ORs
//! its own index into a witness word and the case asserts more than one bit is set. "We handed it a
//! pool" is not "it used one".

use zgui_dom::{Document, NodeKind};
use zgui_interned::{ClassName, ElementName};

use crate::support::engine::{Engine, computed_digest};
use crate::support::pool;

/// The sheet both runs are styled with.
const SHEET: &str = r"
row              { color: rgb(1, 1, 1); display: block }
.even            { color: rgb(2, 2, 2) }
.odd .leaf       { font-size: 20px }
row:nth-child(3n){ border-top-left-radius: 4px }
.even + .odd     { display: flex }
";

/// A wide, two-level document of `rows` rows each holding two leaves.
fn wide(rows: usize) -> Document {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    for index in 0..rows {
        let row = document.append(root, NodeKind::Element, ElementName::new("row"));
        let class = if index % 2 == 0 { "even" } else { "odd" };
        document.set_classes(row, &[ClassName::new(class)]);
        for _ in 0..2 {
            let leaf = document.append(row, NodeKind::Element, ElementName::new("leaf"));
            document.set_classes(leaf, &[ClassName::new("leaf")]);
        }
        document.append(row, NodeKind::Text, ElementName::new("#text"));
    }
    document
}

#[test]
fn a_parallel_traversal_computes_exactly_what_a_sequential_one_does() {
    let mut sequential = wide(2_000);
    let mut engine = Engine::new(&sequential);
    engine.add_author_sheet(SHEET);
    engine.restyle(&mut sequential, None);
    let expected = computed_digest(&sequential);

    for workers in [2, 4, 6] {
        let mut parallel = wide(2_000);
        let mut engine = Engine::new(&parallel);
        engine.add_author_sheet(SHEET);
        let pool = pool::build(workers);
        let pass = engine.restyle(&mut parallel, Some(&pool));

        assert!(
            pass.workers > 1,
            "at {workers} workers the traversal never left one thread, so this compares nothing"
        );
        assert_eq!(
            computed_digest(&parallel),
            expected,
            "the computed values at {workers} workers differ from the sequential ones"
        );
    }
}
