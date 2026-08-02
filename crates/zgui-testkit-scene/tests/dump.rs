//! The tree-dump seam, end to end: a tree, its rendering, and the golden that records it.
//!
//! The trees this seam exists for — the box tree and the fragment tree — live in the layout engine
//! and implement [`TreeDump`] there. What is checked here is the machinery they get for free, over a
//! tree small enough to read: stable indentation, a golden that is compared rather than created, and
//! a blessing path that rewrites the record and still fails, so a blessed run is never mistaken for
//! a checked one.

use std::path::PathBuf;

use zgui_testkit_scene::dump::golden::{self, Outcome};
use zgui_testkit_scene::text::{Writer, number};
use zgui_testkit_scene::{TreeDump, to_text};

/// A node of a small tree with geometry on it.
struct Node {
    /// What the node is called.
    name: &'static str,
    /// A measurement a golden must see change.
    width: f32,
    /// Its children, in order.
    children: Vec<Node>,
}

impl TreeDump for Node {
    fn dump(&self, writer: &mut Writer) {
        let line = format!("{} width={}", self.name, number::float(self.width));
        if self.children.is_empty() {
            writer.line(&line);
        } else {
            writer.nested(&line, |writer| {
                for child in &self.children {
                    child.dump(writer);
                }
            });
        }
    }
}

/// A leaf.
fn leaf(name: &'static str, width: f32) -> Node {
    Node {
        name,
        width,
        children: Vec::new(),
    }
}

/// The tree the golden records.
fn tree() -> Node {
    Node {
        name: "root",
        width: 800.0,
        children: vec![
            Node {
                name: "header",
                width: 800.0,
                children: vec![leaf("title", 240.0), leaf("actions", 120.0)],
            },
            leaf("body", 800.0),
        ],
    }
}

/// Where this crate's goldens live.
fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name)
}

/// A writable path unique to this process.
fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push("zgui-testkit-scene-dump");
    path.push(format!("{name}-{}.txt", std::process::id()));
    path
}

#[test]
fn a_tree_dump_matches_its_golden() {
    golden::assert_tree(&golden_path("tree/nested_boxes.txt"), &tree());
}

#[test]
fn the_golden_is_not_an_empty_rendering() {
    // The control for the golden above: a dumper that wrote nothing would match an empty file for
    // ever. The golden is compared against the tree's own shape rather than merely existing.
    let rendered = to_text(&tree());
    assert_eq!(rendered.lines().count(), 5);
    assert!(rendered.contains("\n  header width=800\n    title width=240\n"));
}

#[test]
fn a_tree_that_changed_no_longer_matches() {
    let mut changed = tree();
    changed.children[0].children[0].width = 241.0;
    assert!(matches!(
        golden::compare(&golden_path("tree/nested_boxes.txt"), &to_text(&changed)),
        Outcome::Differed(_)
    ));
}

#[test]
#[should_panic(expected = "no golden at")]
fn a_missing_golden_is_a_failure_and_never_a_quiet_creation() {
    // A golden that materialises on first run is a golden that has never once been checked.
    let path = scratch("never-written");
    let _ = std::fs::remove_file(&path);
    golden::assert_matches(&path, &to_text(&tree()));
    unreachable!("the assertion above must fail");
}

#[test]
fn the_report_of_a_difference_names_the_line_and_both_sides() {
    let path = scratch("difference");
    golden::write(&path, &to_text(&tree()));

    let mut changed = tree();
    changed.children[1].width = 640.0;
    let Outcome::Differed(report) = golden::compare(&path, &to_text(&changed)) else {
        panic!("the trees differ");
    };
    assert!(report.contains("line 5"));
    assert!(report.contains("body width=800"));
    assert!(report.contains("body width=640"));
    let _ = std::fs::remove_file(&path);
}
