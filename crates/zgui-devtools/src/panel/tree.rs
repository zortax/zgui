//! The document as a tree, in either of the two shapes worth looking at.
//!
//! Two modes, one toggle. **Components** is the tree somebody wrote — the boundaries `#[component]`
//! left behind, nested the way the source nests them, each against the file and line it came from.
//! **All nodes** is what those components produced, with the boundaries still marked among the
//! elements so a box nobody remembers writing leads back to the component that wrote it.
//!
//! **The rows the panel draws are a projection of the sample, not the sample.** What is expanded
//! lives in a signal of its own, so a document that changed underneath the tree keeps whatever the
//! reader had opened — a set of open rows rebuilt with the tree would collapse the whole thing
//! every time anything moved, which on a document that is being interacted with is every frame.

use std::collections::HashSet;

use zgui::prelude::*;
use zgui::reactive::{RenderEffect, on_cleanup_local};
use zgui::view::{NodeId, NodeRef};
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};

#[allow(
    unused_imports,
    reason = "the tag names the component and the macro names its props type"
)]
use crate::panel::element::{ElementPanel, ElementPanelProps};
use crate::panel::icon;
use crate::sample::tree::{RowKind, TreeRow};
use crate::state::{DevTools, MIN_HALF, TreeMode};

/// How far one level of nesting indents a row, in CSS pixels.
const INDENT: f64 = 12.0;

/// How many rows the tab will draw at once.
///
/// Rows start open, because a tree that has to be unfolded before it says anything is a tree nobody
/// reads — and on the documents this is usually pointed at, everything fits. This is what keeps
/// that from being a promise the panel cannot keep on a document of thousands of nodes: past this
/// many rows the rest are left for somebody to reach by collapsing what they are not looking at.
const BUDGET: usize = 400;

/// The tree tab.
#[component]
pub(crate) fn TreePanel(
    /// Where the tree is published, and where a click on a row is recorded.
    tools: DevTools,
) -> impl IntoView {
    let tree = tools.tree;
    let mode = tools.tree_mode;
    let expanded = tools.expanded;
    let picked = tools.picked;
    // Where the pointer went down on the rule and how tall the detail was then, so a drag is
    // measured from where it started rather than accumulated — an accumulated one drifts by
    // whatever the clamp took off and ends up somewhere the pointer is not.
    let lifting = RwSignal::new_local(None::<(f32, f64)>);
    // The tab itself, so the clamp knows how much there is to divide.
    let split = NodeRef::new();

    // Opening the path to whatever is picked, so picking in the application shows the row for it
    // rather than leaving it collapsed somewhere out of sight. Only the ancestors that are not
    // already open are written, because a set written with what it already holds is a signal
    // written for nothing, and a signal written for nothing is a frame asked for for nothing.
    let follow = RenderEffect::new(move |_: Option<()>| {
        let Some(node) = selected(tools) else { return };
        let Some(tree) = tree.get() else { return };
        let Some(at) = tree.rows.iter().position(|row| row.node == node) else {
            return;
        };
        // The set records what somebody *closed*, so opening the path to a row means taking its
        // ancestors back out of it.
        let closed: Vec<NodeId> = ancestors(&tree.rows, at)
            .into_iter()
            .filter(|node| expanded.with_untracked(|shut| shut.contains(node)))
            .collect();
        if !closed.is_empty() {
            expanded.update(|shut| {
                for node in closed {
                    shut.remove(&node);
                }
            });
        }
    });
    // Held for as long as the tab is: a dropped effect stops running, and this one is what keeps
    // the tree following what the pointer picks in the application.
    on_cleanup_local(move || drop(follow));

    view! {
        column(class = "zgui-devtools__split", node_ref = split) {
        column(
            class = "zgui-devtools__body zgui-devtools__split-tree",
            // Leaving the tree stops outlining whatever the pointer was last over. On the
            // container rather than on each row, so crossing the gap between two rows does not
            // flicker the outline off and on again.
            on:pointer_leave = move |_| {
                if tools.highlighted.get_untracked().is_some() {
                    tools.highlighted.set(None);
                }
            }
        ) {
            row(class = "zgui-devtools__legend") {
                for which in || TreeMode::ALL, key = |which: &TreeMode| *which {
                    control(
                        class = "zgui-devtools__chip",
                        class:zgui-devtools__chip-on = move || mode.get() == which,
                        a11y:label = which.label(),
                        on:click = move |_| {
                            if mode.get_untracked() != which {
                                mode.set(which);
                            }
                        }
                    ) {
                        {which.label()}
                    }
                }
            }
            Show(
                when = move || tree.get().is_some_and(|tree| !tree.rows.is_empty()),
                fallback = move || view! {
                    text(class = "zgui-devtools__value-quiet") {
                        {move || empty(tools)}
                    }
                }
            ) {
                for row in move || visible(tools), key = |row: &TreeRow| row.node {
                    row(
                        class = "zgui-devtools__tree-row",
                        // Selected when it *is* what was picked, and also when what was picked is
                        // something this tree has no row for — a box picked with the pointer while
                        // the components view is showing selects the component that built it,
                        // because that is the row a reader can actually see.
                        class:zgui-devtools__tree-picked = {
                            let node = row.node;
                            move || selected(tools) == Some(node)
                        },
                        style:padding-left = {Some(format!("{:.0}px", f64::from(row.depth) * INDENT))},
                        on:pointer_enter = {
                            let node = row.node;
                            move |_| {
                                if tools.highlighted.get_untracked() != Some(node) {
                                    tools.highlighted.set(Some(node));
                                }
                            }
                        },
                        on:click = {
                            let node = row.node;
                            move |_| {
                                if picked.get_untracked() != Some(node) {
                                    picked.set(Some(node));
                                }
                            }
                        }
                    ) {
                        // The chevron is a button of its own: opening a row and selecting it are
                        // different intentions, and a tree that could only do both at once cannot
                        // be opened without also throwing away what the element tab was showing.
                        Show(
                            when = {let has = row.has_children; move || has},
                            fallback = || view! { box(class = "zgui-devtools__tree-chevron") }
                        ) {
                            control(
                                class = "zgui-devtools__tree-chevron",
                                a11y:label = "Expand",
                                on:click = {
                                    let node = row.node;
                                    move |ev: &mut _| {
                                        expanded.update(|open| {
                                            if !open.remove(&node) {
                                                open.insert(node);
                                            }
                                        });
                                        zgui::view::event::EventCx::stop_propagation(ev);
                                    }
                                }
                            ) {
                                vector(
                                    class = "zgui-devtools__tree-arrow",
                                    prop:d = {
                                        let node = row.node;
                                        move || PropValue::from(
                                            if expanded.with(|shut| is_open(shut, node)) {
                                                icon::CHEVRON_DOWN
                                            } else {
                                                icon::CHEVRON_RIGHT
                                            }
                                        )
                                    },
                                    prop:viewBox = icon::VIEW_BOX
                                )
                            }
                        }
                        text(
                            class = "zgui-devtools__tree-name",
                            class:zgui-devtools__tree-component = {row.kind == RowKind::Component}
                        ) {
                            {row.label.clone()}
                        }
                        Show(when = {let id = row.id.clone(); move || id.is_some()}) {
                            text(class = "zgui-devtools__tree-id") {
                                {row.id.clone().map(|id| format!("#{id}")).unwrap_or_default()}
                            }
                        }
                        Show(when = {let has = !row.classes.is_empty(); move || has}) {
                            text(class = "zgui-devtools__tree-class") {
                                {row.classes.iter().map(|class| format!(".{class}")).collect::<String>()}
                            }
                        }
                        Show(when = {let said = row.text.clone(); move || said.is_some()}) {
                            text(class = "zgui-devtools__tree-text") {
                                {row.text.clone().map(|said| format!("\"{said}\"")).unwrap_or_default()}
                            }
                        }
                        // Where it was written, which is the answer somebody reading a component
                        // tree is usually after: a name says which component, a file and a line say
                        // where to put the cursor.
                        Show(when = {let at = row.source; move || at.is_some()}) {
                            text(class = "zgui-devtools__tree-source") {
                                {row.source.map(|(file, line)| format!("{}:{line}", trim(file)))
                                    .unwrap_or_default()}
                            }
                        }
                    }
                }
            }
            Show(when = move || {
                tree.get().is_some_and(|tree| tree.truncated) || visible(tools).len() >= BUDGET
            }) {
                text(class = "zgui-devtools__note") {
                    "This document has more rows than the tree draws at once. Close what you are \
                     not looking at to reach the rest."
                }
            }
        }
        // The lower half, and only once something is picked: an empty detail pane standing open
        // under the tree would take half the panel to say nothing.
        if move || picked.get().is_some() {
            control(
                class = "zgui-devtools__split-rule",
                tabindex = {Focus::Sequential},
                a11y:label = "Resize the detail pane",
                on:key_down = move |ev| {
                    let step = match &ev.key {
                        Key::Named(NamedKey::ArrowUp) => STEP,
                        Key::Named(NamedKey::ArrowDown) => -STEP,
                        _ => return,
                    };
                    resize_detail(tools, split, tools.detail.get_untracked() + step);
                    ev.prevent_default();
                    ev.stop_propagation();
                },
                on:pointer_down = move |ev| {
                    ev.capture_pointer();
                    lifting.set(Some((ev.position.y.0, tools.detail.get_untracked())));
                    ev.stop_propagation();
                    ev.prevent_default();
                },
                on:pointer_move = move |ev| {
                    let Some((start, was)) = lifting.get_untracked() else {
                        return;
                    };
                    // Upwards is taller: the detail is below the rule, so dragging the rule towards
                    // the tree is the detail taking more of the tab.
                    resize_detail(tools, split, was + f64::from(start - ev.position.y.0));
                    ev.stop_propagation();
                },
                on:pointer_up = move |ev| {
                    lifting.set(None);
                    ev.release_pointer();
                },
                on:pointer_cancel = move |ev| {
                    lifting.set(None);
                    ev.release_pointer();
                }
            ) {
                box(class = "zgui-devtools__split-line")
            }
            ElementPanel(tools = tools)
        }
        }
    }
}

/// What the tab says when it has no tree to draw.
///
/// The two reasons are different answers and the panel gives whichever one is true: a build with no
/// instrumentation has no boundaries to show, which is not the same as a program with no components
/// and must not read as one.
fn empty(tools: DevTools) -> String {
    let recording = tools
        .tree
        .get()
        .is_some_and(|tree| tree.instrumented || !tree.rows.is_empty());
    match (tools.tree_mode.get(), recording) {
        (TreeMode::Components, false) => {
            "This build records no component boundaries, so there is no component tree to show. \
             Switch to All nodes for the document itself."
                .to_owned()
        }
        _ => "Nothing yet.".to_owned(),
    }
}

/// How far one arrow key moves the rule between the two halves, in CSS pixels.
const STEP: f64 = 16.0;

/// Sets the detail pane's height to `wanted`, as far as the tab allows.
///
/// Clamped so neither half can be squeezed out of existence: a detail pane taller than the tab
/// leaves no tree to pick from, and one of no height is a rule somebody has to find again before
/// they can read what they just picked.
fn resize_detail(tools: DevTools, split: NodeRef, wanted: f64) {
    let scale = split.scale();
    let ceiling = split
        .bounds()
        .map(|tab| f64::from(tab.size.height.0 / scale) - MIN_HALF)
        .unwrap_or(f64::MAX)
        .max(MIN_HALF);
    let next = wanted.clamp(MIN_HALF, ceiling);
    if tools.detail.get_untracked() != next {
        tools.detail.set(next);
    }
}

/// Which row is selected: what was picked, or the component that built it.
///
/// Picking happens in the application, where what is under the pointer is an element. The
/// components tree has no row for one, so a tree that only ever selected the picked node would
/// answer a pointer click by highlighting nothing at all — which reads as the click having missed.
fn selected(tools: DevTools) -> Option<NodeId> {
    let picked = tools.picked.get();
    let Some(tree) = tools.tree.get() else {
        return picked;
    };
    if picked.is_some_and(|node| tree.rows.iter().any(|row| row.node == node)) {
        return picked;
    }
    tools.picked_component.get().or(picked)
}

/// The rows that are showing: every row whose ancestors are all open.
///
/// Recovered from the flat sample by walking backwards over decreasing depth, which is what the
/// flat shape is for — a row's ancestors are the nearest preceding row of each smaller depth.
fn visible(tools: DevTools) -> Vec<TreeRow> {
    let Some(tree) = tools.tree.get() else {
        return Vec::new();
    };
    let shut = tools.expanded.get();
    let mut rows = Vec::new();
    // The depth below which everything is hidden, when the walk is inside a closed row.
    let mut hidden: Option<u16> = None;
    for row in &tree.rows {
        if let Some(depth) = hidden {
            if row.depth > depth {
                continue;
            }
            hidden = None;
        }
        if rows.len() >= BUDGET {
            break;
        }
        rows.push(row.clone());
        if row.has_children && !is_open(&shut, row.node) {
            hidden = Some(row.depth);
        }
    }
    rows
}

/// Whether the row for `node` is open.
///
/// Rows start open and the set records what has been *closed*, which is the right way round for
/// this tree: the interesting part of a document is several levels down inside wrappers nobody
/// wrote, so a tree that started folded would have to be unfolded through them before it said
/// anything. [`BUDGET`] is what stops that being unbounded on a large document.
fn is_open(shut: &HashSet<NodeId>, node: NodeId) -> bool {
    !shut.contains(&node)
}

/// The nodes of every row `at` is nested under.
fn ancestors(rows: &[TreeRow], at: usize) -> Vec<NodeId> {
    let mut wanted = rows[at].depth;
    let mut found = Vec::new();
    for row in rows[..at].iter().rev() {
        if wanted == 0 {
            break;
        }
        if row.depth < wanted {
            found.push(row.node);
            wanted = row.depth;
        }
    }
    found
}

/// A path cut down to the file at the end of it.
fn trim(file: &str) -> &str {
    file.rsplit('/').next().unwrap_or(file)
}

#[cfg(test)]
mod tests {
    use zgui::view::{DocumentId, NodeId};

    use super::{ancestors, trim};
    use crate::sample::tree::{RowKind, TreeRow};

    /// A row at `depth`, numbered so it can be told from the others.
    fn row(bits: u64, depth: u16) -> TreeRow {
        TreeRow {
            node: NodeId::new(DocumentId::FIRST, bits).expect("a node handle"),
            kind: RowKind::Element,
            depth,
            label: "box".to_owned(),
            source: None,
            id: None,
            classes: Vec::new(),
            text: None,
            has_children: false,
            extent: None,
        }
    }

    #[test]
    fn a_rows_ancestors_are_the_nearest_shallower_rows_before_it() {
        // The flat sample's whole point: nesting is depth plus order, and the ancestors of a row
        // are recovered rather than stored.
        let rows = vec![row(1, 0), row(2, 1), row(3, 2), row(4, 1), row(5, 2)];

        let deep = ancestors(&rows, 4);
        assert_eq!(deep, vec![rows[3].node, rows[0].node]);

        assert!(
            ancestors(&rows, 0).is_empty(),
            "the first row is under nothing"
        );
    }

    #[test]
    fn a_declaration_is_shown_by_its_file_rather_than_its_whole_path() {
        assert_eq!(trim("crates/demo/src/widgets.rs"), "widgets.rs");
        assert_eq!(trim("widgets.rs"), "widgets.rs");
    }
}
