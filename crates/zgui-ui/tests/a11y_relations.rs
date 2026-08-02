//! Every relation the gallery publishes, resolved by the consumer a reader is built on.
//!
//! A tree that parses is not a tree that can be read. What a reader does with a node is follow it:
//! it asks what names this, what describes it, what this controls, which descendant is active — and
//! every one of those is an identifier into the same tree. An identifier that resolves to nothing is
//! a question a reader asks and gets no answer to, and the update that carried it is long gone by
//! then, so nothing about the frame it was published in says which one was wrong.
//!
//! So this applies every update the window publishes, in order, through `accesskit_consumer`, and
//! then walks the result asking those questions of every node — with the surfaces that publish the
//! most relations *open*, because a menu, a dialog and a select all name their contents by
//! identifier and none of them is in the tree until it has been opened.

mod desktop;

use std::collections::BTreeSet;

use accesskit_consumer::{Node, NodeId, Tree, TreeChangeHandler, TreeState, common_filter};
use zgui::view;
use zgui::vocab::NamedKey;

use crate::desktop::stage::Stage;

#[path = "../examples/gallery/app.rs"]
#[allow(
    dead_code,
    reason = "the gallery names the window size it ships at; this fixture takes the stage's"
)]
mod app;
#[path = "../examples/gallery/section/mod.rs"]
mod section;
#[path = "../examples/gallery/shell.rs"]
mod shell;

use crate::app::GalleryProps;

/// The roles whose whole purpose is to be operated, and which therefore have to say what they do.
const OPERABLE: [&str; 12] = [
    "Button",
    "CheckBox",
    "RadioButton",
    "Switch",
    "Slider",
    "TextInput",
    "MultilineTextInput",
    "ComboBox",
    "Tab",
    "MenuItem",
    "MenuItemCheckBox",
    "MenuItemRadio",
];

/// A change handler that records nothing: the claim is about the tree the updates leave behind.
struct Silent;

impl TreeChangeHandler for Silent {
    fn node_added(&mut self, _: &Node<'_>) {}
    fn node_updated(&mut self, _: &Node<'_>, _: &Node<'_>) {}
    fn focus_moved(&mut self, _: Option<&Node<'_>>, _: Option<&Node<'_>>) {}
    fn node_removed(&mut self, _: &Node<'_>) {}
}

/// What the walk found.
#[derive(Default)]
struct Found {
    /// How many nodes were reached from the root.
    reached: usize,
    /// The identifiers reached from the root.
    ids: BTreeSet<NodeId>,
    /// Every relation that named a node the tree does not hold, as it reads in a report.
    dangling: Vec<String>,
    /// Operable, focusable nodes with nothing a reader could say about them.
    nameless: Vec<String>,
    /// How many nodes the keyboard can reach.
    focusable: usize,
    /// How many relations resolved.
    resolved: usize,
    /// Which roles were seen.
    roles: BTreeSet<String>,
}

impl Found {
    /// Takes everything `other` found into this one.
    ///
    /// Two readings of one window, taken while different surfaces were up, are two views of the
    /// same interface rather than two interfaces: what either of them reached is reachable, and
    /// what either of them found dangling is broken.
    fn absorb(&mut self, other: Self) {
        self.reached = self.reached.max(other.reached);
        self.focusable = self.focusable.max(other.focusable);
        self.resolved += other.resolved;
        self.ids.extend(other.ids);
        self.roles.extend(other.roles);
        self.dangling.extend(other.dangling);
        self.nameless.extend(other.nameless);
    }
}

/// How a node is named in a failure, so a report can be acted on without the tree beside it.
fn describe(node: &Node<'_>) -> String {
    format!("{:?} {:?}", node.role(), node.label().unwrap_or_default())
}

/// Follows every identifier `node` publishes, and records the ones that answer with nothing.
///
/// The relations are read off the node's own data rather than through the accessors that resolve
/// them, because two of those accessors *drop* an identifier that does not resolve and one panics —
/// so neither can report which relation on which node was the broken one.
fn relations(state: &TreeState, node: &Node<'_>, found: &mut Found) {
    let Some((_, tree)) = state.locate_node(node.id()) else {
        return;
    };
    let data = node.data();
    let follow = |ids: &[_], relation: &'static str, found: &mut Found| {
        for id in ids {
            if state.node_by_tree_local_id(*id, tree).is_some() {
                found.resolved += 1;
            } else {
                found.dangling.push(format!(
                    "{} names nothing through {relation}",
                    describe(node)
                ));
            }
        }
    };
    follow(data.labelled_by(), "labelled_by", found);
    follow(data.described_by(), "described_by", found);
    follow(data.controls(), "controls", found);
    follow(data.owns(), "owns", found);
    follow(data.flow_to(), "flow_to", found);
    if let Some(active) = data.active_descendant() {
        follow(&[active], "active_descendant", found);
    }
}

/// Walks `node` and everything under it, asking each one what a reader asks.
fn walk(state: &TreeState, node: &Node<'_>, found: &mut Found) {
    found.reached += 1;
    found.ids.insert(node.id());
    found.roles.insert(format!("{:?}", node.role()));
    relations(state, node, found);
    if node.is_focusable(&|candidate: &Node<'_>| common_filter(candidate)) {
        found.focusable += 1;
        let role = format!("{:?}", node.role());
        let name = node.label().unwrap_or_default();
        if OPERABLE.contains(&role.as_str()) && name.trim().is_empty() && !node.is_disabled() {
            found.nameless.push(describe(node));
        }
    }
    for child in node.children() {
        walk(state, &child, found);
    }
}

/// Applies everything the window has published and walks what a reader is left holding.
fn swept(stage: &Stage) -> Found {
    let surface = stage.surface();
    let mut updates = surface.a11y_log().into_iter();
    let mut tree = Tree::new(
        updates.next().expect("the window published a first update"),
        true,
    );
    for update in updates {
        tree.update_and_process_changes(update, &mut Silent);
    }
    let state = tree.state();
    let mut found = Found::default();
    walk(state, &state.root(), &mut found);
    found
}

/// Brings the control that says `text` into the window before anything is aimed at it.
///
/// The gallery is seven times taller than the window it opens in, so most of its triggers are below
/// the fold at any moment — and a press at the coordinate of one of those is a press outside the
/// surface, which lands on nothing and reads exactly like a control that does not answer.
fn reveal(stage: &mut Stage, text: &str) {
    let node = stage
        .census()
        .control(text)
        .unwrap_or_else(|| panic!("nothing laid out says {text:?}"))
        .id;
    stage.handles().host.scroll_to(
        node,
        zgui::view::ScrollTarget::IntoViewStart,
        zgui::view::ScrollBehavior::Instant,
    );
    stage.settle();
}

/// Opens the gallery and works the surfaces whose contents only exist once they are up.
///
/// Each is left open, because a relation that only holds while a surface is on the screen is the
/// one worth following: a menu names the item the keyboard is on, a select names its list from its
/// trigger, and a dialog names itself from a title that is not in the document until it opens.
fn driven() -> Found {
    let mut stage = Stage::open(crate::shell::SHEET, || view! { Gallery() });
    let mut found = swept(&stage);
    // One surface at a time, and the tree read while each is up. A press anywhere while a menu is
    // open belongs to the menu, so opening the second without putting the first away would click a
    // trigger that never hears the press — and then report the surface it opens as one the library
    // does not publish. Reading between them is what keeps both in the answer.
    for trigger in ["Account", "Rename…"] {
        reveal(&mut stage, trigger);
        stage.click_saying(trigger);
        stage.settle();
        found.absorb(swept(&stage));
        stage.key(NamedKey::Escape);
        stage.settle();
    }
    found
}

#[test]
fn every_relation_the_gallery_publishes_resolves_to_a_node_the_reader_holds() {
    let found = driven();
    assert!(
        found.reached > 100,
        "the consumer reached only {} nodes for a page of every component in the library",
        found.reached
    );
    assert!(
        found.resolved > 20,
        "only {} relations were followed, so this is measuring a tree that names nothing",
        found.resolved
    );
    assert!(
        found.dangling.is_empty(),
        "{} of the gallery's relations name a node the reader does not hold: {:?}",
        found.dangling.len(),
        found.dangling,
    );
}

#[test]
fn every_node_the_reader_holds_is_reachable_from_the_root() {
    let stranded = stranded_nodes();
    assert!(
        stranded.is_empty(),
        "{} nodes are in the reader's tree and under no parent it can walk to: {stranded:?}",
        stranded.len(),
    );
}

/// Every node the consumer still holds that a walk from the root never arrives at.
fn stranded_nodes() -> Vec<String> {
    let mut stage = Stage::open(crate::shell::SHEET, || view! { Gallery() });
    for trigger in ["Account", "Rename…"] {
        stage.click_saying(trigger);
        stage.settle();
    }
    let surface = stage.surface();
    let mut updates = surface.a11y_log().into_iter();
    let mut tree = Tree::new(
        updates.next().expect("the window published a first update"),
        true,
    );
    // Every node the consumer was told about and not told to take away again. A node it still
    // holds that no walk from the root can arrive at is a node no reader can ever be moved onto —
    // it is in the tree, it answers every question asked of it directly, and it is unreachable.
    let mut live: BTreeSet<NodeId> = BTreeSet::new();
    let mut census = Census { live: &mut live };
    for update in updates {
        tree.update_and_process_changes(update, &mut census);
    }
    let state = tree.state();
    let mut found = Found::default();
    walk(state, &state.root(), &mut found);
    live.iter()
        .filter(|id| !found.ids.contains(id))
        .filter_map(|id| state.node_by_id(*id).map(|node| describe(&node)))
        .collect()
}

/// A change handler that keeps the set of nodes the consumer currently holds.
struct Census<'a> {
    /// The identifiers added and not removed.
    live: &'a mut BTreeSet<NodeId>,
}

impl TreeChangeHandler for Census<'_> {
    fn node_added(&mut self, node: &Node<'_>) {
        self.live.insert(node.id());
    }

    fn node_updated(&mut self, _: &Node<'_>, _: &Node<'_>) {}

    fn focus_moved(&mut self, _: Option<&Node<'_>>, _: Option<&Node<'_>>) {}

    fn node_removed(&mut self, node: &Node<'_>) {
        self.live.remove(&node.id());
    }
}

#[test]
fn every_control_the_keyboard_can_reach_has_something_a_reader_can_say() {
    let found = driven();
    assert!(
        found.focusable > 40,
        "only {} nodes are reachable by keyboard, so this is measuring the wrong page",
        found.focusable
    );
    assert!(
        found.nameless.is_empty(),
        "{} of the gallery's keyboard-reachable controls announce their role and nothing else: \
         {:?}",
        found.nameless.len(),
        found.nameless,
    );
}

#[test]
fn the_surfaces_that_only_exist_when_open_are_published_as_what_they_are() {
    let found = driven();
    // Each is a role the gallery only publishes once something has been opened, so a run that
    // found them proves the walk above was of a driven window rather than of a resting one.
    for role in ["Dialog", "MenuItem"] {
        assert!(
            found.roles.contains(role),
            "no node in the driven gallery's tree is a {role}; the roles seen were {:?}",
            found.roles,
        );
    }
}

#[test]
fn a_keyboard_walk_leaves_every_stop_with_a_name_a_reader_would_read_out() {
    let (stops, silent) = tabbed();
    assert!(
        stops > 40,
        "only {stops} of 64 tabs landed anywhere a reader could describe",
    );
    assert!(
        silent.is_empty(),
        "{} tab stops announce their role and no name: {silent:?}",
        silent.len(),
    );
}

/// How many tab stops a reader could describe, and the ones it could not.
fn tabbed() -> (usize, Vec<String>) {
    let mut stage = Stage::open(crate::shell::SHEET, || view! { Gallery() });
    // Tab, over and over, asking after every stop what a reader would say. A stop that announces
    // nothing is a stop whose user has been moved somewhere and told only that they arrived.
    let mut silent = Vec::new();
    let mut stops = 0;
    for _ in 0..64 {
        stage.key(NamedKey::Tab);
        let Some(announced) = stage.announced_focus() else {
            continue;
        };
        stops += 1;
        if OPERABLE.contains(&announced.role.as_str()) && announced.name.trim().is_empty() {
            silent.push(format!("{} {:?}", announced.role, announced.name));
        }
    }
    (stops, silent)
}
