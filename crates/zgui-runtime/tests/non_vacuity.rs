//! The proof that each counter of avoided work can move, and can be left alone.
//!
//! Every counter here records work a stage *did not do*. Such a counter reads zero when the stage
//! is skipping perfectly, zero when the stage has stopped skipping, and zero when nobody ever
//! incremented it — so an upper bound written against one is green from the day it is written
//! whatever happens underneath. What separates the three cases is a pair of situations: one the
//! skip exists for, in which the counter must move, and one in which reusing an answer would be
//! wrong, in which it must not.
//!
//! Each case below is one such pair, driven over the real pipeline —
//! [`assert_non_vacuous`](zgui_profile::counter::non_vacuity::assert_non_vacuous) takes the
//! counter block for the length of both halves, so both documents are opened *inside* the
//! scenarios rather than beside them.
//!
//! `cargo xtask skips` requires one of these for every counter the table declares a skip, and
//! fails the build naming any that has none.

mod support;

use zgui_profile::Counter;
use zgui_profile::counter::non_vacuity::{Scenario, assert_non_vacuous};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_view::{BuildCx, ClassName, IntoView, View};

/// A scrollport with rows in it, one of which can be lit.
const CSS: &str = "root { display: block; width: 400px; height: 300px; overflow: scroll }
                   .row { display: block; width: 400px; height: 20px; background-color: #202020 }
                   .lit { background-color: #f0f0f0 }";

/// How many rows a document has to hold before it reaches past the bottom of the scrollport.
///
/// The port is three hundred pixels and a row is twenty, so this is five times what fits: enough
/// that a frame emitting only what can be seen is doing visibly less than a frame emitting the
/// document.
const OVERFLOWING: usize = 60;

/// The one row whose class the interaction changes.
const LIT_ROW: usize = 3;

/// A document of `rows` rows, the fourth of which carries `lit`.
fn document(
    rows: usize,
    lit: RwSignal<bool>,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let mut root = zgui_elements::column().class("root");
        for index in 0..rows {
            let mut row = zgui_elements::column().class("row");
            if index == LIT_ROW {
                row = row.class_toggle(ClassName::new("lit"), move || lit.get());
            }
            root = root.child(row.child(zgui_elements::text().child(format!("row {index}"))));
        }
        Box::new(root.into_view().build(cx))
    })
}

/// Opens a document of `rows` rows and settles it, which is a first paint of everything in it.
fn opened(rows: usize) {
    let lit = RwSignal::new(false);
    let mut harness = document(rows, lit);
    harness.settle(64);
}

/// Opens an overflowing document, settles it, and then changes the class on one row.
///
/// The open is inside the measurement rather than beside it because the counter block is
/// process-wide and the assertion holds it: work done outside the lock is work another test is
/// reading. It costs the fired half nothing — what is asserted there is that the counter moved at
/// all, and the open can only add to it.
fn opened_then_one_row_changed() {
    let lit = RwSignal::new(false);
    let mut harness = document(OVERFLOWING, lit);
    harness.settle(64);
    lit.set(true);
    harness.settle(64);
}

#[test]
fn a_layout_pass_is_held_when_nothing_moved_and_never_on_a_document_being_opened() {
    assert_non_vacuous(
        Counter::LayoutsHeld,
        Scenario::new(
            "a class change that alters no geometry, on a document already laid out",
            opened_then_one_row_changed,
        ),
        Scenario::new("opening a document that has never been laid out", || {
            opened(OVERFLOWING);
        }),
    );
}

#[test]
fn a_cached_range_is_replayed_on_a_repaint_and_never_on_a_first_paint() {
    assert_non_vacuous(
        Counter::ChunksTranslated,
        Scenario::new(
            "a repaint of a document whose ranges were encoded by the frame before",
            opened_then_one_row_changed,
        ),
        Scenario::new(
            "the first paint of a document, which has no ranges yet",
            || {
                opened(OVERFLOWING);
            },
        ),
    );
}

#[test]
fn primitives_are_culled_when_the_document_reaches_past_the_port_and_never_when_it_fits() {
    // The pair the other two cannot stand in for: culling is decided against what can be seen
    // rather than against what has changed, so the silent half is a *smaller document* and not an
    // earlier frame. A document that fits inside its scrollport has nothing to refuse.
    assert_non_vacuous(
        Counter::PrimitivesCulled,
        Scenario::new(
            "a document five times the height of the scrollport holding it",
            opened_then_one_row_changed,
        ),
        Scenario::new("a document that fits inside its scrollport whole", || {
            opened(6);
        }),
    );
}
