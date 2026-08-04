//! The document as a tree, in either of the two shapes worth looking at.
//!
//! **Components**, which is the tree somebody wrote: a few dozen boundaries nested the way the
//! source nests them, each against the file and line it was declared on. **All nodes**, which is
//! what those components turned into — every element and run of text, with the component
//! boundaries still marked among them so a box nobody remembers writing can be traced back to the
//! component that wrote it.
//!
//! One walk produces either. The modes are projections of the same depth-first pass rather than two
//! traversals, because the expensive part is walking a document of a few thousand nodes and doing
//! it twice to answer one question would be doing it twice.
//!
//! **The panel's own subtree is skipped.** This is not tidiness either: the rows this produces
//! become elements, elements are nodes, and nodes are what this reads — so a tree that included the
//! panel would grow by its own size every time it was drawn, which is the same runaway the timeline
//! is bounded against and arrives faster.

use std::rc::Rc;

use zgui::runtime::Window;
use zgui::view::NodeId;
use zgui::view::instrument::{self, MarkerRole};
use zgui_dom::NodeKind;

use crate::state::TreeMode;

/// How many rows the tree will show.
///
/// A document can be arbitrarily large and the panel is one column of a window, so this is the
/// bound that keeps a sample proportional to what can be read rather than to what exists. It is
/// also the bound that matters for the same reason the timeline has one: rows are elements, and a
/// sampler with no ceiling is a document whose size is a function of its own size.
const MAX_ROWS: usize = 4096;

/// How much of a run of text a row shows.
const TEXT: usize = 80;

/// What one row of the tree is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    /// An element.
    Element,
    /// A run of text.
    Text,
    /// The start of one component's content.
    Component,
}

/// One row of the tree.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TreeRow {
    /// The node this row is: an element, a text node, or a component's open marker.
    ///
    /// Keys the row, and is what a click on it picks. A component's marker is a stable handle for
    /// as long as the instance lives, which an element inside it is not — the first thing a
    /// component renders is replaced whenever its own content changes.
    pub(crate) node: NodeId,
    /// What kind of row it is.
    pub(crate) kind: RowKind,
    /// How deep it sits.
    pub(crate) depth: u16,
    /// The element's name, the component's short name, or `text` for a run of text.
    pub(crate) label: String,
    /// Where the component was declared, for a component row.
    pub(crate) source: Option<(&'static str, u32)>,
    /// The element's `id`, when it has one.
    pub(crate) id: Option<String>,
    /// The element's classes.
    pub(crate) classes: Vec<String>,
    /// What a run of text says, clamped to something a row can hold.
    pub(crate) text: Option<String>,
    /// Whether anything is nested under it.
    pub(crate) has_children: bool,
    /// The markers bracketing a component's content, which is what its outline is measured over.
    pub(crate) extent: Option<(NodeId, NodeId)>,
}

/// The document, as rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Tree {
    /// The rows, in document order, each carrying its own depth.
    ///
    /// Flat rather than nested because every question the panel asks of it is a question about a
    /// range: which rows are under this one, which of its ancestors are open, where does this
    /// component end. All of those are a scan over a slice here and a recursive walk in a nested
    /// tree, and the panel asks them on every render.
    pub(crate) rows: Vec<TreeRow>,
    /// Whether the document had more rows than [`MAX_ROWS`] and the rest were dropped.
    pub(crate) truncated: bool,
    /// Whether this build records component boundaries at all.
    ///
    /// A tree with no components in it is ambiguous — a program with none, or a build with no
    /// instrumentation — and the tab says which rather than showing an empty panel.
    pub(crate) instrumented: bool,
}

/// Reads `window`'s document into the tree `mode` asks for, skipping the subtrees rooted at `skip`.
///
/// What is skipped is everything the inspector itself drew — the panel, and the outline it portals
/// over the application. Both have to go: the rows this produces are elements of the very document
/// it walks, so a tree that included them would grow every time it was drawn.
pub(crate) fn sample_tree(
    window: &Window,
    mode: TreeMode,
    skip: &[NodeId],
    start: Option<NodeId>,
) -> Rc<Tree> {
    let document = window.document().borrow();
    let dom = window.dom();
    let mut rows = Vec::new();
    let mut truncated = false;
    // A document with no root element has nothing to draw, which is the state between a window
    // being made and its first view being mounted.
    let Some(root) = document.root_index() else {
        return Rc::new(Tree {
            rows,
            truncated,
            instrumented: instrument::is_recording(),
        });
    };

    // Where the walk starts. The application's own wrapper when the panel has published one, and
    // the document root when it has not — which is the first frame, and any caller that has no
    // panel at all.
    //
    // Starting inside is what makes the tree readable: the document root, the overlay root, its
    // four layers, the inspector's own boundary and its two wrapper columns are five or six levels
    // of scaffolding nobody wrote, and every row of the tree was indented behind all of it. The
    // overlay layers are walked as roots of their own so a portalled dialog or toast is still in
    // the tree, at the top level where a reader can find it rather than nested under plumbing.
    let mut roots: Vec<zgui_dom::NodeIndex> = Vec::new();
    match start.and_then(|node| {
        zgui_view_dom::id::is_live(&document, node)
            .then(|| zgui_view_dom::id::resolve(&document, node))
    }) {
        Some(app) => {
            // The children of the wrapper, not the wrapper: `.zgui-devtools-app` is the inspector's
            // own box and the last piece of scaffolding between a reader and what they wrote.
            roots.extend(children_of(&document, app));
            roots.extend(overlay_layers(&document, root));
        }
        None => roots.push(root),
    }

    // The walk, iteratively: a document is as deep as somebody's markup and a recursive walk over
    // one is a stack this has no reason to risk.
    //
    // Each entry is a node still to visit and the depth it sits at. `closes` carries the component
    // instances whose content the walker is currently inside, so a close marker can be matched to
    // the open that raised the depth.
    let mut stack: Vec<(zgui_dom::NodeIndex, u16)> = Vec::new();
    for from in roots.into_iter().rev() {
        stack.push((from, 0));
    }
    let mut closes: Vec<(u64, u16)> = Vec::new();
    while let Some((index, depth)) = stack.pop() {
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        let node = document.store().key_of(index);
        let id = zgui_view_dom::id::to_view(node);
        // Everything the inspector itself drew.
        if skip.contains(&id) {
            continue;
        }

        // A close marker lowers the depth again rather than drawing anything.
        let mut depth = depth;
        if let Some(MarkerRole::Close(instance)) = instrument::at(id) {
            if let Some(at) = closes.iter().rposition(|(open, _)| *open == instance) {
                closes.truncate(at);
            }
            continue;
        }
        // A depth is only ever as deep as the boundaries still open around it, which is what makes
        // a component's content indent under it and go back out again afterwards.
        if let Some((_, opened)) = closes.last() {
            depth = depth.max(*opened);
        }

        let record = document.node(index);
        let kind = record.kind();
        let row = match kind {
            NodeKind::Marker => match instrument::at(id) {
                Some(MarkerRole::Open(tag)) => {
                    closes.push((tag.instance, depth.saturating_add(1)));
                    Some(TreeRow {
                        node: id,
                        kind: RowKind::Component,
                        depth,
                        label: short(tag.name).to_owned(),
                        source: Some((tag.file, tag.line)),
                        id: None,
                        classes: Vec::new(),
                        text: None,
                        // Filled in below, once the walk knows whether anything followed it.
                        has_children: false,
                        extent: close_of(&document, index, tag.instance).map(|close| (id, close)),
                    })
                }
                // A conditional, a list or a hole: a position rather than a boundary.
                _ => None,
            },
            NodeKind::Text if mode == TreeMode::Full => {
                let said = dom.text_content(id);
                let said = said.trim();
                // A text node of nothing but layout whitespace is not a row anybody wants.
                (!said.is_empty()).then(|| TreeRow {
                    node: id,
                    kind: RowKind::Text,
                    depth,
                    label: "text".to_owned(),
                    source: None,
                    id: None,
                    classes: Vec::new(),
                    text: Some(clamp(said)),
                    has_children: false,
                    extent: None,
                })
            }
            NodeKind::Element if mode == TreeMode::Full => Some(TreeRow {
                node: id,
                kind: RowKind::Element,
                depth,
                label: record.record().local_name().as_str().to_owned(),
                source: None,
                id: record.record().id_attr().map(|id| id.to_string()),
                classes: document
                    .store()
                    .classes_of(index)
                    .iter()
                    .map(|class| class.0.as_ref().to_owned())
                    .collect(),
                text: None,
                has_children: !record.record().has_no_children(),
                extent: None,
            }),
            _ => None,
        };
        let descend = match row {
            Some(row) => {
                let at = rows.len();
                rows.push(row);
                // Children of a drawn row sit under it; children of a row this mode does not draw
                // sit wherever their parent would have.
                Some((at, depth.saturating_add(1)))
            }
            None => None,
        };
        let child_depth = descend.map_or(depth, |(_, deeper)| deeper);

        // Pushed in reverse so the first child is the next one popped.
        let mut children = Vec::new();
        let mut next = document.store().core(index).first_child();
        while let Some(child) = next {
            children.push(child);
            next = document.store().core(child).next_sibling();
        }
        if let Some((at, _)) = descend
            && !children.is_empty()
        {
            rows[at].has_children = true;
        }
        for child in children.into_iter().rev() {
            stack.push((child, child_depth));
        }
    }

    // What is nested under a row is decided from the rows themselves rather than from the document,
    // for every row and in both modes.
    //
    // A component nests by *boundary*: its content are the siblings between its open marker and its
    // close, not children of the marker — a marker has none, so asking the document leaves every
    // component row saying it has nothing under it and gives it no chevron to fold. And a node
    // whose children this mode does not draw — the text inside an element, in the components tree —
    // is a row that would offer a chevron and unfold to nothing.
    //
    // The flat list already answers it exactly: the next row is nested under this one when it is
    // deeper, whatever put it there.
    for at in 0..rows.len() {
        let deeper = rows
            .get(at + 1)
            .is_some_and(|next| next.depth > rows[at].depth);
        rows[at].has_children = deeper;
    }

    Rc::new(Tree {
        rows,
        truncated,
        instrumented: instrument::is_recording(),
    })
}

/// The overlay layer nodes of the window rooted at `root`.
///
/// Portalled content — a dialog, a menu, a toast — is mounted under one of these rather than where
/// it was written, so a tree that only walked the application's own wrapper would not show it at
/// all. The layers themselves are not drawn; what is on them is.
fn overlay_layers(
    document: &zgui_dom::Document,
    root: zgui_dom::NodeIndex,
) -> Vec<zgui_dom::NodeIndex> {
    let mut content = Vec::new();
    let mut next = document.store().core(root).first_child();
    while let Some(child) = next {
        if document.node(child).record().local_name().as_str() == "overlay_root" {
            for layer in children_of(document, child) {
                // The layers themselves are never drawn. There are four of them in every window,
                // they are empty almost all of the time, and four empty rows at the top level would
                // be the tree's most prominent feature.
                content.extend(children_of(document, layer));
            }
        }
        next = document.store().core(child).next_sibling();
    }
    content
}

/// A node's children, in order.
fn children_of(
    document: &zgui_dom::Document,
    parent: zgui_dom::NodeIndex,
) -> Vec<zgui_dom::NodeIndex> {
    let mut found = Vec::new();
    let mut next = document.store().core(parent).first_child();
    while let Some(child) = next {
        found.push(child);
        next = document.store().core(child).next_sibling();
    }
    found
}

/// The component whose content `node` is part of, innermost first.
///
/// A component's boundary is a pair of markers among its content's *siblings*, so "which component
/// built this" is not a question about ancestors — it is asked one level at a time: among the
/// siblings before this one, which boundary is still open? A stack answers that in one pass, and
/// the first level that has one is the innermost component containing the node.
///
/// `None` for anything outside every component, which is the document's own root scaffolding.
pub(crate) fn component_of(window: &Window, node: NodeId) -> Option<NodeId> {
    let document = window.document().borrow();
    if !zgui_view_dom::id::is_live(&document, node) {
        return None;
    }
    let mut at = zgui_view_dom::id::resolve(&document, node);
    loop {
        if let Some(found) = open_before(&document, at) {
            return Some(found);
        }
        at = document.store().core(at).parent()?;
    }
}

/// The innermost component boundary still open among the siblings before `index`.
fn open_before(document: &zgui_dom::Document, index: zgui_dom::NodeIndex) -> Option<NodeId> {
    let parent = document.store().core(index).parent()?;
    let mut open: Vec<(u64, NodeId)> = Vec::new();
    let mut next = document.store().core(parent).first_child();
    while let Some(sibling) = next {
        if sibling == index {
            break;
        }
        let id = zgui_view_dom::id::to_view(document.store().key_of(sibling));
        match instrument::at(id) {
            Some(MarkerRole::Open(tag)) => open.push((tag.instance, id)),
            Some(MarkerRole::Close(instance)) => {
                if let Some(found) = open.iter().rposition(|(held, _)| *held == instance) {
                    open.truncate(found);
                }
            }
            None => {}
        }
        next = document.store().core(sibling).next_sibling();
    }
    open.last().map(|(_, id)| *id)
}

/// The close marker of the component instance opened at `open`, if it is a sibling after it.
///
/// A pair is always mounted into one parent and in order, so this is a walk along one sibling list
/// rather than a search of the document.
fn close_of(
    document: &zgui_dom::Document,
    open: zgui_dom::NodeIndex,
    instance: u64,
) -> Option<NodeId> {
    let mut next = document.store().core(open).next_sibling();
    while let Some(index) = next {
        let id = zgui_view_dom::id::to_view(document.store().key_of(index));
        if instrument::at(id) == Some(MarkerRole::Close(instance)) {
            return Some(id);
        }
        next = document.store().core(index).next_sibling();
    }
    None
}

/// A component's path, cut down to the name at the end of it.
///
/// The full path is kept on the row and shown where there is room for it: `Page` is what a reader
/// is scanning for, and `zgui_devtools::tests::support::Page` in a 420 px column is a row that says
/// nothing at all because none of it is on screen.
fn short(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// A run of text, cut to what a row can hold.
fn clamp(said: &str) -> String {
    // Collapsed first: markup indentation is newlines and runs of spaces, and a row of those is a
    // row of nothing that still takes its eighty characters.
    let mut out = String::new();
    let mut space = false;
    for character in said.chars() {
        if character.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        if out.chars().count() >= TEXT {
            out.push('…');
            break;
        }
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{TEXT, clamp, short};

    #[test]
    fn a_component_is_named_by_the_end_of_its_path() {
        assert_eq!(short("demo::widgets::Label"), "Label");
        assert_eq!(short("Label"), "Label");
    }

    #[test]
    fn a_run_of_text_is_collapsed_and_cut() {
        assert_eq!(clamp("hello   there"), "hello there");
        assert_eq!(clamp("\n   spaced  \n out \n"), "spaced out");

        let long = "x".repeat(TEXT * 2);
        let cut = clamp(&long);
        assert!(
            cut.chars().count() <= TEXT + 1,
            "a {} character run became a {} character row",
            long.len(),
            cut.chars().count()
        );
        assert!(cut.ends_with('…'), "a cut row does not say it was cut");
    }
}
