//! What a screen reader is holding, mid-fixture.
//!
//! Everything else a fixture asks is a question about the document or about the pixels. Neither
//! answers the question a reader's user is actually asking — *what am I on, and what is it called*
//! — because that answer is assembled by a consumer out of every update the window has published,
//! and a component can be perfectly drawn while announcing nothing.
//!
//! So the updates are taken from the surface, applied through `accesskit_consumer` — the crate every
//! platform adapter is built on — and the result is asked the two questions a reader asks first.

use accesskit_consumer::{Node, Tree, TreeChangeHandler};
use zgui_platform_headless::OffscreenSurface;

/// One node, as a reader would meet it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Announced {
    /// What it is, by the name a consumer reports.
    pub role: String,
    /// What it is called.
    pub name: String,
}

/// A change handler that records nothing: the claim is about the tree the updates leave behind.
struct Silent;

impl TreeChangeHandler for Silent {
    fn node_added(&mut self, _: &Node<'_>) {}
    fn node_updated(&mut self, _: &Node<'_>, _: &Node<'_>) {}
    fn focus_moved(&mut self, _: Option<&Node<'_>>, _: Option<&Node<'_>>) {}
    fn node_removed(&mut self, _: &Node<'_>) {}
}

/// The tree a consumer holds after applying everything `surface` has published, in order.
///
/// `None` when the window has published nothing at all, which is a window no reader has been told
/// about rather than a window whose tree is empty.
///
/// The updates are read from the surface here rather than handed in, so that the type a window
/// publishes is never named: it belongs to the accessibility engine, and this package is a
/// consumer of the public API like any other.
pub fn consumed(surface: &OffscreenSurface) -> Option<Tree> {
    let mut updates = surface.a11y_log().into_iter();
    let mut tree = Tree::new(updates.next()?, true);
    for update in updates {
        tree.update_and_process_changes(update, &mut Silent);
    }
    Some(tree)
}

/// What a reader would say about whatever holds the keyboard.
pub fn focused(surface: &OffscreenSurface) -> Option<Announced> {
    let tree = consumed(surface)?;
    let state = tree.state();
    let node = state.focus()?;
    Some(Announced {
        role: format!("{:?}", node.role()),
        name: node.label().unwrap_or_default(),
    })
}

/// Every node a reader would meet, in tree order.
pub fn everything(surface: &OffscreenSurface) -> Vec<Announced> {
    let Some(tree) = consumed(surface) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    walk(&tree.state().root(), &mut found);
    found
}

/// Walks `node` and everything under it.
fn walk(node: &Node<'_>, found: &mut Vec<Announced>) {
    found.push(Announced {
        role: format!("{:?}", node.role()),
        name: node.label().unwrap_or_default(),
    });
    for child in node.children() {
        walk(&child, found);
    }
}
