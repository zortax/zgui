//! Which component each part of the document came from, for a tool that wants to say so.
//!
//! A `#[component]` is a function. It runs, it builds elements, and then it is gone — nothing it
//! leaves behind says which function wrote it, because nothing needs to: the document is what gets
//! styled, laid out and painted, and the component that produced it is of interest to exactly one
//! kind of program, which is a development tool.
//!
//! So this is that record, and it is behind the `instrument` feature because it is not free. With
//! the feature on, a scoped view built from a named body brackets its content with a pair of
//! **marker** nodes and registers what they mean here. Markers are the framework's own answer to
//! "a position in the tree that is not an element" — they take part in sibling order and in nothing
//! else, so a component boundary shifts no positional selector, adds no box, and is invisible to
//! layout and paint.
//!
//! **A pair rather than a single mark**, because the useful question is *what did this component
//! produce* and that is a range, not a point. One marker at the start would answer "where does it
//! begin" and leave the end to be guessed from a sibling that may itself be another component's.
//!
//! **A registry rather than a field on the node**, because the document is shared with the style
//! engine, which matches selectors across threads and asserts as much about what a node may hold.
//! What a marker means is a development tool's business and belongs on the tool's side of that
//! line — and being thread-local costs nothing, since the tree is only ever built on the UI thread.
//!
//! With the feature off none of this exists: no markers are made, nothing is registered, and the
//! metadata a component carries is a `&'static` nobody reads.

use core::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::id::NodeId;
use crate::view::ComponentMeta;

/// Which component an open marker belongs to, and which instance of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentTag {
    /// The component's path, as `module::path::Name`.
    pub name: &'static str,
    /// The file it was declared in.
    pub file: &'static str,
    /// The line it was declared on.
    pub line: u32,
    /// Which instance of it this is, so an open marker can be matched to its close.
    pub instance: u64,
    /// The reactive scope this instance owns, by the identity the graph gives it.
    ///
    /// Two instances of one component are two scopes, and everything either of them allocated —
    /// signals, memos, cleanups — belongs to one of these. It is the handle a tool needs to say
    /// *which* instance is holding something.
    pub owner: usize,
    /// How deep that scope sits in the ownership tree.
    ///
    /// The length of its ancestry, which is what a scope leak shows up in: a tree that keeps
    /// getting deeper is one whose scopes are not being disposed of when their views go.
    pub scope: usize,
}

/// What a marker node is, when it is a component boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerRole {
    /// The start of a component's content.
    Open(ComponentTag),
    /// The end of the content belonging to the instance of that number.
    Close(u64),
}

thread_local! {
    /// What every registered marker means.
    static MARKERS: RefCell<HashMap<NodeId, MarkerRole>> = RefCell::new(HashMap::new());
    /// The number the next component instance takes.
    static NEXT: Cell<u64> = const { Cell::new(0) };
}

/// What `node` marks, or `None` when it is not a component boundary this knows about.
///
/// Most markers are not: every conditional, list and reactive hole in the program has one, and
/// those are positions rather than boundaries. A tool walking the tree skips what this does not
/// answer for rather than treating it as an unnamed component.
#[must_use]
pub fn at(node: NodeId) -> Option<MarkerRole> {
    MARKERS
        .try_with(|markers| markers.borrow().get(&node).copied())
        .unwrap_or_default()
}

/// Whether anything at all is registered.
///
/// What a tool checks before offering to show a component tree: with the feature compiled in but
/// nothing registered, the honest answer is "this build has no component instrumentation" rather
/// than an empty tree, which reads as "this program has no components".
#[must_use]
pub fn is_recording() -> bool {
    MARKERS
        .try_with(|markers| !markers.borrow().is_empty())
        .unwrap_or(false)
}

/// Records that `open` and `close` bracket one instance of the component `meta` describes.
///
/// Returns the instance's number, which is what the close marker is registered against.
pub(crate) fn register(
    open: NodeId,
    close: NodeId,
    meta: &'static ComponentMeta,
    owner: &zgui_reactive::Owner,
) -> u64 {
    let instance = NEXT
        .try_with(|next| {
            let taken = next.get();
            next.set(taken.wrapping_add(1));
            taken
        })
        .unwrap_or_default();
    let _ = MARKERS.try_with(|markers| {
        let mut markers = markers.borrow_mut();
        markers.insert(
            open,
            MarkerRole::Open(ComponentTag {
                name: meta.name,
                file: meta.file,
                line: meta.line,
                instance,
                owner: owner.debug_id(),
                scope: owner.ancestry().len(),
            }),
        );
        markers.insert(close, MarkerRole::Close(instance));
    });
    instance
}

/// Every component instance alive right now, in no particular order.
///
/// What a tool lists to answer *what is mounted*: one entry per live boundary, each naming its
/// component, where it was written, and the reactive scope it owns.
#[must_use]
pub fn live() -> Vec<ComponentTag> {
    MARKERS
        .try_with(|markers| {
            markers
                .borrow()
                .values()
                .filter_map(|role| match role {
                    MarkerRole::Open(tag) => Some(*tag),
                    MarkerRole::Close(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// How many component instances have been built since the program started.
///
/// Monotonic, and the number [`live`] is read against: a program whose live count is steady while
/// this climbs is rebuilding rather than reusing, and one whose live count climbs with it is
/// keeping every instance it ever made.
#[must_use]
pub fn created() -> u64 {
    NEXT.try_with(Cell::get).unwrap_or_default()
}

/// Forgets a pair, which the scope that made it does as it is cleaned up.
///
/// Without this the map would hold an entry per component instance ever built, which on a list
/// whose rows come and go is unbounded growth driven by ordinary use.
pub(crate) fn deregister(open: NodeId, close: NodeId) {
    // `try_with`, because this runs from a `Drop`: a document torn down as its thread exits does so
    // while thread-locals are being destroyed, and the map may already be gone. There is nothing to
    // clean up in that case — the map it would have been removed from no longer exists.
    let _ = MARKERS.try_with(|markers| {
        let mut markers = markers.borrow_mut();
        markers.remove(&open);
        markers.remove(&close);
    });
}

#[cfg(test)]
mod tests {
    use super::{MarkerRole, at, deregister, register};
    use crate::id::{DocumentId, NodeId};
    use crate::view::ComponentMeta;

    /// A node handle of the first document, numbered as a backend would.
    fn node(bits: u64) -> NodeId {
        NodeId::new(DocumentId::FIRST, bits).expect("a node handle")
    }

    /// A component declaration, as the macro would write one.
    static META: ComponentMeta = ComponentMeta {
        name: "demo::Widget",
        file: "demo.rs",
        line: 12,
    };

    #[test]
    fn a_registered_pair_names_its_component_and_matches_its_own_close() {
        let open = node(1);
        let close = node(2);
        let instance = register(open, close, &META, &zgui_reactive::Owner::new());

        let Some(MarkerRole::Open(tag)) = at(open) else {
            panic!("the open marker was not registered as one");
        };
        assert_eq!(tag.name, "demo::Widget");
        assert_eq!((tag.file, tag.line), ("demo.rs", 12));
        assert_eq!(tag.instance, instance);
        assert_eq!(at(close), Some(MarkerRole::Close(instance)));

        deregister(open, close);
        assert_eq!(at(open), None, "the pair outlived the scope that made it");
        assert_eq!(at(close), None);
    }

    #[test]
    fn two_instances_of_one_component_are_told_apart() {
        // What makes the tree a tree rather than a list of names: two rows of the same component
        // are two boundaries, and a walker matching an open to a close by name alone would nest
        // the second inside the first.
        let first = (node(3), node(4));
        let second = (node(5), node(6));
        let one = register(first.0, first.1, &META, &zgui_reactive::Owner::new());
        let two = register(second.0, second.1, &META, &zgui_reactive::Owner::new());

        assert_ne!(one, two);
        assert_eq!(at(first.1), Some(MarkerRole::Close(one)));
        assert_eq!(at(second.1), Some(MarkerRole::Close(two)));

        deregister(first.0, first.1);
        deregister(second.0, second.1);
    }

    #[test]
    fn a_marker_nobody_registered_is_not_a_component() {
        // Every conditional and list in the program has one of these, and answering for them would
        // turn each into an unnamed component in the tree.
        let loose = node(99);
        assert_eq!(at(loose), None);
    }
}
