//! Every update this window publishes, applied and read by the consumer that really reads them.
//!
//! The updates a window publishes are *differences*, and what goes wrong with a difference is not
//! the frame it is built in. A node leaves the tree; a node somewhere else still names it, has not
//! itself changed, and is therefore in nothing this frame marked — so it is never re-sent, and the
//! consumer keeps an identifier that resolves to nothing until a screen reader asks that control
//! for its name.
//!
//! So the sequences here are the shapes that make nodes leave: a subtree that unmounts, a keyed
//! list that reorders and empties, a relation pointing into the subtree being taken away.

mod support;

use support::replay_a11y;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set, Update as _};
use zgui_view::{A11yBinding, AnyView, ForProps, IntoView, NodeRef, ShowProps, View};
use zgui_vocab::Role;

const CSS: &str = "
root { display: block; width: 400px; height: 300px }
control { display: block; width: 120px; height: 24px }
label { display: block; width: 160px; height: 20px }
";

#[test]
fn a_toggled_subtree_replays_into_the_real_consumer() {
    let open = RwSignal::new_local(false);
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    ShowProps::builder()
                        .when(move || open.get())
                        .children(move || {
                            AnyView::new(
                                zgui_elements::column()
                                    .child(zgui_elements::label().child("Delete file?"))
                                    .child(
                                        zgui_elements::control()
                                            .a11y(A11yBinding::new(Role::Button).label("Confirm")),
                                    ),
                            )
                        })
                        .fallback(|| AnyView::new(zgui_elements::label().child("closed")))
                        .build()
                        .render(),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    for _ in 0..4 {
        open.update(|value| *value = !*value);
        harness.settle(8);
    }
    replay_a11y(&harness);
}

#[test]
fn a_reordered_list_replays_into_the_real_consumer() {
    let rows = RwSignal::new_local(vec![1u32, 2, 3, 4]);
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    ForProps::builder()
                        .each(move || rows.get())
                        .key(|row: &u32| *row)
                        .children(move |row: u32| {
                            zgui_elements::control()
                                .a11y(A11yBinding::new(Role::Button).label(row.to_string()))
                        })
                        .build()
                        .render(),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    rows.set(vec![4, 3, 2, 1]);
    harness.settle(8);
    rows.set(vec![4, 9, 1]);
    harness.settle(8);
    rows.set(vec![]);
    harness.settle(8);
    rows.set(vec![7, 8]);
    harness.settle(8);
    replay_a11y(&harness);
}

#[test]
fn a_field_stops_naming_a_label_that_unmounts_underneath_it() {
    // The failure a check over one update at a time cannot see. The field is not touched by the
    // change at all: the hint below it goes away, and the field is left holding an identifier the
    // consumer has dropped. Nothing is wrong with either update in isolation, and a screen reader
    // asking the field for its name is what crashes.
    let open = RwSignal::new_local(true);
    let hint = NodeRef::new();
    let mut harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::control().a11y(
                    A11yBinding::new(Role::TextInput).labelled_by(move || {
                        zgui_a11y::NodeId(hint.get().map_or(0, |id| id.as_u64()))
                    }),
                ))
                .child(
                    ShowProps::builder()
                        .when(move || open.get())
                        .children(move || {
                            AnyView::new(
                                zgui_elements::label()
                                    .node_ref(hint)
                                    .child("As it appears on your passport"),
                            )
                        })
                        .build()
                        .render(),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    let named = zgui_a11y::dump(
        &harness
            .platform()
            .offscreens()
            .first()
            .expect("a surface")
            .last_a11y_update()
            .expect("an update"),
    );
    assert!(
        named.contains("labelled_by="),
        "the relation was never written, so taking its target away proves nothing:\n{named}"
    );

    open.set(false);
    harness.settle(8);
    replay_a11y(&harness);
}
