//! `TreeDump`: the seam every tree dumper in the project is written against.
//!
//! Layout produces two trees whose goldens are the primary evidence that it is correct — the box
//! tree with its resolved layout, and the fragment tree — and neither of them exists in this crate,
//! nor could: this crate has no layout dependency and must not grow one, because a testkit that
//! depends on the engine it tests cannot be used to bring that engine up.
//!
//! What lives here is therefore the *shape* of a tree dump and the machinery around it: an
//! implementation writes its nodes through [`Writer`], and gets stable
//! indentation, escaping, number formatting, golden comparison and blessing for free. The trees
//! themselves implement [`TreeDump`] where they live.
//!
//! ```
//! use zgui_testkit_scene::dump::{TreeDump, to_text};
//! use zgui_testkit_scene::text::Writer;
//!
//! struct Node {
//!     name: &'static str,
//!     children: Vec<Node>,
//! }
//!
//! impl TreeDump for Node {
//!     fn dump(&self, writer: &mut Writer) {
//!         if self.children.is_empty() {
//!             writer.line(self.name);
//!         } else {
//!             writer.nested(self.name, |writer| {
//!                 for child in &self.children {
//!                     child.dump(writer);
//!                 }
//!             });
//!         }
//!     }
//! }
//!
//! let tree = Node {
//!     name: "root",
//!     children: vec![Node { name: "leaf", children: Vec::new() }],
//! };
//! assert_eq!(to_text(&tree), "root\n  leaf\n");
//! ```

pub mod diff;
pub mod golden;

use crate::text::Writer;

/// Something that can be written out as a stable, indented, diffable tree.
///
/// # What an implementation must promise
///
/// **The same tree writes the same bytes.** No hash map's iteration order may reach the page, and
/// every number goes through [`crate::text::number`]. A dump that reordered siblings between runs
/// is not a regression artifact, it is noise.
///
/// **Everything that can regress is written.** A dump is only as good as the fields it carries: a
/// value left out is a value a golden holds green while it changes. Where that would make the text
/// unreadable, prefer omitting a field *at its default value* — an absent field then means the
/// default and nothing else.
pub trait TreeDump {
    /// Writes this node, and its children one level deeper.
    fn dump(&self, writer: &mut Writer);
}

/// One tree, rendered.
pub fn to_text(tree: &dyn TreeDump) -> String {
    let mut writer = Writer::new();
    tree.dump(&mut writer);
    writer.finish()
}

#[cfg(test)]
mod tests {
    use crate::text::Writer;

    use super::{TreeDump, to_text};

    /// A tree with a value on every node, so a dump has something to lose.
    struct Node {
        /// The node's name.
        name: &'static str,
        /// A measurement a golden should see change.
        width: f32,
        /// Its children.
        children: Vec<Node>,
    }

    impl TreeDump for Node {
        fn dump(&self, writer: &mut Writer) {
            let line = format!(
                "{} width={}",
                self.name,
                crate::text::number::float(self.width)
            );
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

    /// A two-level tree.
    fn tree(width: f32) -> Node {
        Node {
            name: "root",
            width,
            children: vec![Node {
                name: "child",
                width: 8.0,
                children: Vec::new(),
            }],
        }
    }

    #[test]
    fn a_dump_is_stable_across_renderings() {
        let first = to_text(&tree(16.0));
        for _ in 0..16 {
            assert_eq!(to_text(&tree(16.0)), first);
        }
        assert_eq!(first, "root width=16\n  child width=8\n");
    }

    #[test]
    fn a_dump_changes_when_the_tree_does() {
        // The control for the test above: a dump that never changed would be perfectly stable and
        // perfectly useless.
        assert_ne!(to_text(&tree(16.0)), to_text(&tree(17.0)));
    }
}
