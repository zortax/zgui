//! What a screen reader is told about a running application, and what it can do back.
//!
//! Every assertion here reads the update the *surface was actually handed*, never one the test
//! assembled. That distinction is the whole point: an accessibility projection is exactly the kind
//! of subsystem that can be entirely dead in a real program while a test which hand-builds its
//! input passes. So the document is built by a view, styled by the cascade, laid out, painted and
//! drawn, and what is asserted on is what came out of the far end of the frame.
//!
//! The transcripts are text because a relation is invisible in a screenshot: a combobox that has
//! stopped pointing at its active option looks identical and is unusable.

mod support;

use std::path::{Path, PathBuf};

use zgui_a11y::{Action, ActionRequest, NodeId, TreeId, TreeUpdate};
use zgui_platform::{PlatformCx, WakeReason};
use zgui_platform_headless::Harness;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, GetUntracked, Set, Update};
use zgui_runtime::Runtime;
use zgui_view::{A11yBinding, IntoView, NodeRef, View, events};
use zgui_vocab::{HasPopup, Role};

/// Where the transcripts live.
fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens/a11y")
        .join(name)
}

/// The last update the window's surface was handed.
fn published(harness: &Harness<Runtime>) -> TreeUpdate {
    harness
        .platform()
        .offscreens()
        .first()
        .expect("a surface was created")
        .last_a11y_update()
        .expect("the frame published an accessibility update")
}

/// The accessibility identifier of whatever `node` is bound to, tracked.
///
/// Zero while the reference is unbound, which is a node no document ever issued — so a relation
/// written before its target exists is dropped rather than dangling, and the binding re-runs and
/// writes the real identifier the moment the target mounts. That is the ordinary case, not an edge
/// one: a field is written before the label below it.
fn a11y_id(node: NodeRef) -> NodeId {
    NodeId(node.get().map_or(0, |id| id.as_u64()))
}

/// The same, once the frame has settled and nothing is being tracked.
fn bound_id(node: NodeRef) -> NodeId {
    NodeId(
        node.get_untracked()
            .expect("the node reference is bound after a frame")
            .as_u64(),
    )
}

/// Asserts that nothing the surface was ever handed names a node a consumer could not resolve.
///
/// A crash guard rather than a tidiness check: `accesskit_consumer` resolves an explicit relation
/// with `node_by_id(..).unwrap()`, on a thread this process does not own and cannot catch.
///
/// It is answered by applying the whole sequence to a real consumer and then reading it, rather
/// than by a set of identifiers this file keeps: an update is a *difference*, so a relation into a
/// node sent three frames ago is correct, and a node sent three frames ago can also have left the
/// tree since. Only the consumer knows which.
fn assert_every_update_resolves(harness: &Harness<Runtime>) {
    support::replay_a11y(harness);
}

/// Delivers an accessibility action the way the window backend does: through the waker.
fn send(harness: &mut Harness<Runtime>, action: Action, target: NodeId) {
    harness
        .platform()
        .waker()
        .wake(WakeReason::A11yAction(ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: target,
            data: None,
        }));
    harness.settle(8);
}

const CSS: &str = "
root { display: block; width: 400px; height: 300px }
control { display: block; width: 120px; height: 24px }
.spacer { display: block; width: 10px; height: 10px }
.spacer.tall { height: 80px }
label { display: block; width: 160px; height: 20px }
.field { display: block; width: 160px; height: 24px }
.dialog { display: block; width: 300px; height: 200px }
.listbox { display: block; width: 200px; height: 120px }
.option { display: block; width: 200px; height: 24px }
";

#[test]
fn a_form_relates_each_field_to_the_text_that_names_it() {
    let name_label = NodeRef::new();
    let hint = NodeRef::new();
    let error = NodeRef::new();

    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::label()
                        .node_ref(name_label)
                        .child("Full name"),
                )
                .child(
                    zgui_elements::control().class("field").a11y(
                        A11yBinding::new(Role::TextInput)
                            .labelled_by(move || a11y_id(name_label))
                            .described_by(move || a11y_id(hint))
                            .error_message(move || a11y_id(error))
                            .required(true),
                    ),
                )
                .child(
                    zgui_elements::label()
                        .node_ref(hint)
                        .child("As it appears on your passport"),
                )
                .child(zgui_elements::label().node_ref(error).child("Required"))
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    let update = published(&harness);
    assert_every_update_resolves(&harness);
    zgui_testkit_scene::dump::golden::assert_matches(
        &golden("form_with_labels.txt"),
        &zgui_a11y::dump(&update),
    );
}

#[test]
fn a_counter_that_ticks_over_changes_one_accessibility_node() {
    let count = RwSignal::new_local(0);
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::label().child(move || count.get().to_string()))
                .child(
                    zgui_elements::control()
                        .on(events::CLICK, move |_| count.update(|n| *n += 1))
                        .a11y(A11yBinding::new(Role::Button).label("Increment")),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    assert!(zgui_a11y::dump(&published(&harness)).contains("Increment"));

    count.update(|n| *n += 1);
    harness.settle(8);

    let update = published(&harness);
    assert_eq!(
        update.nodes.len(),
        1,
        "a number ticking over is one node; the parent is re-projected to keep its child list \
         honest and must not be *sent* when that list did not move:\n{}",
        zgui_a11y::dump(&update)
    );
    assert_every_update_resolves(&harness);
    zgui_testkit_scene::dump::golden::assert_matches(
        &golden("counter_increment.txt"),
        &zgui_a11y::dump(&update),
    );
}

#[test]
fn a_combobox_names_the_option_that_is_active_without_moving_focus() {
    let trigger = NodeRef::new();
    let list = NodeRef::new();
    let first = NodeRef::new();
    let second = NodeRef::new();
    let active = RwSignal::new_local(0usize);

    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::control().node_ref(trigger).a11y(
                        A11yBinding::new(Role::ComboBox)
                            .label("Fruit")
                            .expanded(true)
                            .has_popup(HasPopup::Listbox)
                            .controls(move || a11y_id(list))
                            .owns(move || a11y_id(list))
                            .active_descendant(move || {
                                if active.get() == 0 {
                                    a11y_id(first)
                                } else {
                                    a11y_id(second)
                                }
                            }),
                    ),
                )
                .child(
                    zgui_elements::column()
                        .class("listbox")
                        .node_ref(list)
                        .a11y(
                            A11yBinding::new(Role::ListBox)
                                .label("Fruit")
                                .popup_for(move || a11y_id(trigger)),
                        )
                        .child(
                            zgui_elements::control()
                                .class("option")
                                .node_ref(first)
                                .a11y(A11yBinding::new(Role::ListBoxOption).label("Apple")),
                        )
                        .child(
                            zgui_elements::control()
                                .class("option")
                                .node_ref(second)
                                .a11y(A11yBinding::new(Role::ListBoxOption).label("Pear")),
                        ),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    // Moving the highlight moves the relation and nothing else: focus stays where it is, which is
    // the entire reason `active_descendant` exists.
    active.set(1);
    harness.settle(8);

    let update = published(&harness);
    assert_every_update_resolves(&harness);
    assert!(
        zgui_a11y::dump(&update).contains("active_descendant="),
        "the relation the combobox pattern rests on left the update:\n{}",
        zgui_a11y::dump(&update)
    );
    zgui_testkit_scene::dump::golden::assert_matches(
        &golden("combobox_active_descendant.txt"),
        &zgui_a11y::dump(&update),
    );
}

#[test]
fn a_dialog_is_named_by_its_title_and_owned_by_the_control_that_opened_it() {
    let trigger = NodeRef::new();
    let dialog = NodeRef::new();
    let title = NodeRef::new();
    let body = NodeRef::new();

    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::control().node_ref(trigger).a11y(
                        A11yBinding::new(Role::Button)
                            .label("Delete")
                            .has_popup(HasPopup::Dialog)
                            .expanded(true)
                            .owns(move || a11y_id(dialog)),
                    ),
                )
                .child(
                    zgui_elements::column()
                        .class("dialog")
                        .node_ref(dialog)
                        .a11y(
                            A11yBinding::new(Role::Dialog)
                                .modal(true)
                                .labelled_by(move || a11y_id(title))
                                .described_by(move || a11y_id(body))
                                .popup_for(move || a11y_id(trigger)),
                        )
                        .child(zgui_elements::label().node_ref(title).child("Delete file?"))
                        .child(
                            zgui_elements::label()
                                .node_ref(body)
                                .child("This cannot be undone."),
                        ),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    let update = published(&harness);
    assert_every_update_resolves(&harness);
    zgui_testkit_scene::dump::golden::assert_matches(
        &golden("dialog_labelled_by.txt"),
        &zgui_a11y::dump(&update),
    );
}

#[test]
fn an_inbound_click_reaches_the_listener_a_pointer_would_have_reached() {
    // The claim under test is that no component writes accessibility activation logic. The control
    // below has one `on:click` and nothing else; if activation went down a second path this
    // counter would not move.
    let count = RwSignal::new_local(0);
    let button = NodeRef::new();
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::control()
                        .node_ref(button)
                        .on(events::CLICK, move |_| count.update(|n| *n += 1))
                        .a11y(A11yBinding::new(Role::Button).label("Increment")),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    assert_eq!(count.get_untracked(), 0);

    let target = bound_id(button);
    let update = published(&harness);
    let advertised = update
        .nodes
        .iter()
        .find(|(id, _)| *id == target)
        .map(|(_, node)| node.supports_action(Action::Click));
    assert_eq!(
        advertised,
        Some(true),
        "a control whose action an assistive technology cannot see is one it will never offer"
    );

    send(&mut harness, Action::Click, target);

    assert_eq!(
        count.get_untracked(),
        1,
        "the inbound action reached no listener, so every component would have to implement \
         activation a second time"
    );
}

#[test]
fn focus_moved_by_an_inbound_action_is_reported_back_on_the_next_update() {
    let button = NodeRef::new();
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::control()
                        .node_ref(button)
                        .a11y(A11yBinding::new(Role::Button).label("Save")),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    let target = bound_id(button);
    assert_ne!(published(&harness).focus, target);

    send(&mut harness, Action::Focus, target);

    let update = published(&harness);
    assert_eq!(
        update.focus, target,
        "focus rides on every update, so a frame that only moved focus still owes one"
    );
    assert!(
        update.nodes.is_empty(),
        "moving focus is not a document change, so the update that reports it carries no nodes \
         at all:\n{}",
        zgui_a11y::dump(&update)
    );
}

#[test]
fn a_control_is_reported_where_layout_actually_put_it() {
    // Geometry is the half of the projection a transcript cannot show and a screen magnifier
    // depends on entirely: a tree with the right names and no bounds highlights the wrong place
    // on the screen, or nowhere at all.
    let button = NodeRef::new();
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::control()
                        .node_ref(button)
                        .a11y(A11yBinding::new(Role::Button).label("Save")),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    let target = bound_id(button);
    let update = published(&harness);
    let bounds = update
        .nodes
        .iter()
        .find(|(id, _)| *id == target)
        .and_then(|(_, node)| node.bounds())
        .expect("the button reached the tree with a rectangle");

    // The stylesheet gives the control 120 by 24 CSS pixels, and the surface is at a scale of one.
    assert_eq!(bounds.width(), 120.0, "{bounds:?}");
    assert_eq!(bounds.height(), 24.0, "{bounds:?}");

    // And the root carries the scale, which is what makes every other rectangle a CSS one.
    let root = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Window)
        .and_then(|(_, node)| node.transform().copied())
        .expect("the root carries a transform");
    assert_eq!(root.as_coeffs(), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
}

#[test]
fn a_widget_that_moved_is_re_announced_and_a_plain_box_that_moved_is_not() {
    // The producer a name-only projection misses entirely. Nothing about this change reaches the
    // style engine or the document's own edits: the control's declaration is untouched and only
    // its rectangle moved, so without the fragment pass marking it the tree keeps pointing a
    // magnifier at where the control used to be.
    let tall = RwSignal::new_local(false);
    let button = NodeRef::new();
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::r#box().style_property("height", move || {
                    Some(if tall.get() { "80px" } else { "10px" }.to_owned())
                }))
                .child(
                    zgui_elements::control()
                        .node_ref(button)
                        .a11y(A11yBinding::new(Role::Button).label("Save")),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    let target = bound_id(button);
    let before = published(&harness)
        .nodes
        .iter()
        .find(|(id, _)| *id == target)
        .and_then(|(_, node)| node.bounds())
        .expect("the button reached the tree");

    tall.set(true);
    harness.settle(8);

    let update = published(&harness);
    let after = update
        .nodes
        .iter()
        .find(|(id, _)| *id == target)
        .and_then(|(_, node)| node.bounds())
        .unwrap_or_else(|| {
            panic!(
                "the control moved seventy pixels down the page and was never re-announced:\n{}",
                zgui_a11y::dump(&update)
            )
        });
    assert_eq!(after.y0 - before.y0, 70.0, "{before:?} -> {after:?}");
    assert_every_update_resolves(&harness);
}

/// How far the moved control is carried sideways, and how far down.
const SHIFTED: (f64, f64) = (60.0, 20.0);

/// A control under a transform, and one that slides across the window for a second.
const MOVED_CSS: &str = "
root { display: block; width: 400px; height: 300px }
control { display: block; width: 120px; height: 24px }
.shifted { transform: translate(60px, 20px) }
.slides { transform: translateX(0px); animation: slide 1000ms linear }
@keyframes slide {
    from { transform: translateX(0px) }
    to { transform: translateX(240px) }
}
";

#[test]
fn a_control_under_a_transform_is_reported_where_the_transform_puts_it() {
    // A fragment keeps its rectangle in its own space, which is where the control would be if
    // nothing above it had moved. That is the rectangle a screen magnifier draws its highlight
    // around, so reporting it puts the highlight on empty page — every glyph of the control's own
    // name correct, and the box pointing somewhere nothing is drawn.
    let plain = NodeRef::new();
    let shifted = NodeRef::new();
    let mut harness = support::app_with_text(MOVED_CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::control()
                        .node_ref(plain)
                        .a11y(A11yBinding::new(Role::Button).label("Still")),
                )
                .child(
                    zgui_elements::control()
                        .class("shifted")
                        .node_ref(shifted)
                        .a11y(A11yBinding::new(Role::Button).label("Moved")),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    let update = published(&harness);
    let bounds_of = |target: NodeId| {
        update
            .nodes
            .iter()
            .find(|(id, _)| *id == target)
            .and_then(|(_, node)| node.bounds())
            .expect("the control reached the tree with a rectangle")
    };
    // The control against which the moved one is measured. Without it the assertion below holds
    // just as well for a projection that has applied the transform twice, or to the wrong axis.
    let still = bounds_of(bound_id(plain));
    let moved = bounds_of(bound_id(shifted));

    assert_eq!(still.x0, 0.0, "{still:?}");
    assert_eq!(
        (moved.x0 - still.x0, moved.y0 - still.y0 - still.height()),
        SHIFTED,
        "the second control is drawn {SHIFTED:?} from where it was laid out, and that is where a \
         consumer outside this process has to be told it is: {still:?} -> {moved:?}"
    );
    assert_eq!(
        (moved.width(), moved.height()),
        (still.width(), still.height()),
        "a translation moves a control without resizing it"
    );
    assert_every_update_resolves(&harness);
}

#[test]
fn a_control_whose_coordinate_system_moves_is_re_announced_although_nothing_walked_it() {
    // The failure this whole step exists to remove, and the one no other gate can see. An animated
    // transform writes the matrix under a name and leaves every rectangle measured through it
    // exactly where it was: the fragment did not move, the primitive is drawn through the same
    // name and the pixels are right. Nothing in the fragment pass has anything to report, so an
    // obligation that lives in that pass is never raised — and a screen reader is left holding the
    // rectangle the control started from for the whole animation.
    let sliding = NodeRef::new();
    let mut harness = support::app_with_text(MOVED_CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::control()
                        .class("slides")
                        .node_ref(sliding)
                        .a11y(A11yBinding::new(Role::Button).label("Sliding")),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    let target = bound_id(sliding);

    // Sampled through the slide rather than once at the end. A projection that re-announces the
    // control only when something else happens to mark it agrees with a single reading taken after
    // the animation has finished, and disagrees with every one taken during it.
    let mut readings: Vec<f64> = Vec::new();
    for _ in 0..20 {
        harness.advance(core::time::Duration::from_millis(16));
        harness.pump();
        if let Some(bounds) = published(&harness)
            .nodes
            .iter()
            .find(|(id, _)| *id == target)
            .and_then(|(_, node)| node.bounds())
        {
            readings.push(bounds.x0);
        }
    }

    assert!(
        readings.len() > 10,
        "only {} frames of the slide re-announced the control at all; a consumer holding the \
         rectangle it started from is pointing at empty page",
        readings.len()
    );
    let first = readings.first().copied().expect("a reading was taken");
    let last = readings.last().copied().expect("a reading was taken");
    assert!(
        last - first > 40.0,
        "the control slid across the window and was announced between x={first} and x={last}"
    );
    for pair in readings.windows(2) {
        assert!(
            pair[1] >= pair[0],
            "the slide is one way, so a reading may not go backwards: {pair:?}"
        );
    }
    assert_every_update_resolves(&harness);
}

#[test]
fn a_reader_that_connects_late_is_sent_a_whole_tree_although_nothing_is_dirty() {
    // The one place the pipeline produces work with nothing changed. A consumer that has just
    // attached holds nothing, and there is no dirty check anywhere that can notice that: what is
    // missing is on the other side of the connection.
    let mut harness = support::app_with_text(CSS, |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::control().a11y(A11yBinding::new(Role::Button).label("Save")))
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    let first = published(&harness);
    assert!(
        first.tree.is_some(),
        "the first update carries the tree data"
    );

    // Idle: with nothing changing, nothing is published.
    harness.settle(8);
    let quiet = published(&harness);
    assert_eq!(quiet.nodes.len(), first.nodes.len());

    let surface = zgui_platform::Surface::id(
        harness
            .platform()
            .offscreens()
            .first()
            .expect("a surface")
            .as_ref(),
    );
    harness
        .platform()
        .waker()
        .wake(WakeReason::A11yTreeRequested(surface));
    harness.settle(8);

    let again = published(&harness);
    assert!(
        again.tree.is_some() && again.nodes.len() == first.nodes.len(),
        "a request for the initial tree must answer with the whole of it:\n{}",
        zgui_a11y::dump(&again)
    );
    assert!(
        zgui_a11y::dump(&again).contains("Button label=\"Save\""),
        "{}",
        zgui_a11y::dump(&again)
    );
}

#[test]
fn a_layout_only_box_carries_the_role_a_consumer_drops() {
    // The default matters more than it looks: a document is mostly containers, and a default a
    // consumer keeps would make a screen reader announce every nesting level of the layout.
    let mut harness = support::app_with_text(CSS, |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::column().child(zgui_elements::label().child("Hello")))
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    let update = published(&harness);
    let generic = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::GenericContainer)
        .count();
    assert!(generic >= 2, "{}", zgui_a11y::dump(&update));
}

#[test]
fn a_control_css_took_off_the_screen_is_not_announced() {
    // `display: none` removes a subtree from the rendering, and CSS says it is removed from what is
    // read aloud with it. A screen-reader user offered a button that is not on the screen, that no
    // pointer can reach and that does nothing when activated is worse off than one offered no
    // button at all: nothing about it sounds wrong.
    let mut harness = support::app_with_text(
        &format!("{CSS}\n.gone {{ display: none }}\n.wrapper {{ display: contents }}"),
        |cx: &mut zgui_view::BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::control()
                            .class("gone")
                            .a11y(A11yBinding::new(Role::Button).label("Not on screen")),
                    )
                    .child(
                        zgui_elements::column().class("wrapper").child(
                            zgui_elements::control()
                                .a11y(A11yBinding::new(Role::Button).label("Through a wrapper")),
                        ),
                    )
                    .child(
                        zgui_elements::control()
                            .a11y(A11yBinding::new(Role::Button).label("On screen")),
                    )
                    .into_view()
                    .build(cx),
            )
        },
    );
    harness.settle(8);

    let update = published(&harness);
    let hidden = |label: &str| {
        update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(label))
            .map(|(_, node)| node.is_hidden())
    };
    assert_eq!(
        hidden("Not on screen"),
        Some(true),
        "{}",
        zgui_a11y::dump(&update)
    );
    assert_eq!(
        hidden("On screen"),
        Some(false),
        "{}",
        zgui_a11y::dump(&update)
    );
    // A box that generates none of its own but puts its children in its parent's place is present:
    // the rule is "nothing below it renders", not "it renders".
    assert_eq!(
        hidden("Through a wrapper"),
        Some(false),
        "{}",
        zgui_a11y::dump(&update)
    );
    let wrapper = update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.children().len() == 1
                && node.role() == Role::GenericContainer
                && node.label().is_none()
                && update.nodes.iter().any(|(id, child)| {
                    node.children().contains(id) && child.label() == Some("Through a wrapper")
                })
        })
        .map(|(_, node)| node.is_hidden());
    assert_eq!(wrapper, Some(false), "{}", zgui_a11y::dump(&update));
}

#[test]
fn an_inbound_scroll_offset_moves_the_container_it_names() {
    // The half of the action surface the plan says the framework answers for every node, so that no
    // component implements scrolling for an assistive technology a second time. An advertised
    // action nothing carries out is a scrollbar a screen reader drags with nothing moving.
    let port = NodeRef::new();
    let mut harness = support::app_with_text(
        "root { display: block; width: 400px; height: 300px }
         .port { display: block; width: 400px; height: 100px; overflow: scroll }
         .row { display: block; width: 400px; height: 40px }",
        move |cx: &mut zgui_view::BuildCx<'_>| {
            let mut list = zgui_elements::column().class("port").node_ref(port);
            for _ in 0..10 {
                list = list.child(zgui_elements::column().class("row"));
            }
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(list)
                    .into_view()
                    .build(cx),
            )
        },
    );
    harness.settle(8);

    let key = port.get_untracked().expect("the port is bound");
    // Read off the scrolling system rather than off the document: what an inbound action has to
    // have changed is the offset the frame composes fragments with.
    let offset = |harness: &Harness<Runtime>| {
        let window = harness.app().windows().first().expect("a window");
        let node = zgui_view_dom::id::to_document(key).expect("a live node");
        window.scroll().borrow().offset_of(node).y.0
    };
    assert_eq!(offset(&harness), 0.0);

    harness
        .platform()
        .waker()
        .wake(WakeReason::A11yAction(ActionRequest {
            action: Action::SetScrollOffset,
            target_tree: TreeId::ROOT,
            target_node: NodeId(key.as_u64()),
            data: Some(zgui_a11y::ActionData::SetScrollOffset(
                zgui_a11y::Point::new(0.0, 120.0),
            )),
        }));
    harness.settle(8);

    assert_eq!(
        offset(&harness),
        120.0,
        "the container never moved, so the action was advertised and dropped"
    );
}
