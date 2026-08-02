//! Whether the check that primitives still own their coordinate systems is wired to anything.
//!
//! The check itself is proven where it lives: `zgui-scene`'s own cases push a primitive under a
//! name, hand the slot to a stranger, replay the range and watch the frame loop refuse it. What
//! those cases cannot say is whether a *window* records anything to check. The recording is a word
//! per primitive that a scene keeps only when asked, the asking was an environment variable no test
//! sets, and a check with nothing recorded reports no faults — which is indistinguishable, from the
//! outside, from a check that passed.
//!
//! So the two cases here are about the wiring and not about the rule: that a real window's frames
//! keep a name for the primitives they push, that the check runs over them, and that a document
//! which gives coordinate systems back and takes the slots again — the situation the whole
//! occupancy counter exists for — comes through it intact.
//!
//! # What is still not exercised, and why it is not written as a pass
//!
//! No *document* here produces a fault. Producing one needs a record that survives a frame while
//! the coordinate system it names is released and its slot reissued, and nothing a window can be
//! driven to do in this phase arranges that: a subtree that goes away takes its records with it,
//! and a subtree that stays keeps a name nothing released. The guardrail is there for what a later
//! phase introduces — records retained across frames independently of the boxes that made them.
//!
//! What has been watched failing here is the *path*: inverting the comparison in `spatial_faults`
//! makes both cases below panic out of the frame loop, naming the real quads of a real frame. So
//! the call site executes, over names a real window recorded, and a fault would stop the frame
//! rather than reach a renderer. What is untested is only whether this phase can produce one.

mod support;

use zgui_scene::SpatialId;
use zgui_view::{IntoView, View};

/// Two subtrees that each establish a coordinate system, shown one at a time.
///
/// Two rather than one, and toggled in antiphase, because the case is about a slot coming *back*:
/// one subtree alone released and re-created would be handed its own slot again, which is the
/// situation nothing can go wrong in. The two carry the same transform, so a slot that changed
/// hands resolves to the same matrix and nothing downstream of the name differs.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.card { display: block; width: 80px; height: 40px;
        background-color: rgb(20, 120, 220);
        transform: translateX(30px) }
.pip { display: block; width: 10px; height: 10px;
       background-color: rgb(250, 250, 250) }
";

/// A window holding whichever of the two cards a signal selects.
fn cards(
    second: zgui_reactive::RwSignal<bool, zgui_reactive::reexport::LocalStorage>,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    use zgui_reactive::prelude::Get;
    use zgui_view::{AnyView, ShowProps};

    support::app(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    ShowProps::builder()
                        .when(move || !second.get())
                        .children(|| {
                            AnyView::new(
                                zgui_elements::column()
                                    .class("card")
                                    .child(zgui_elements::column().class("pip")),
                            )
                        })
                        .fallback(|| {
                            AnyView::new(
                                zgui_elements::column()
                                    .class("card")
                                    .child(zgui_elements::column().class("pip")),
                            )
                        })
                        .build()
                        .render(),
                )
                .into_view()
                .build(cx),
        )
    })
}

/// Every coordinate system the window's tree holds, slot by slot.
fn slots(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> Vec<Option<SpatialId>> {
    harness.app().windows()[0].scene().spatial.slots().collect()
}

/// Turns the check on before the window's first frame and settles it.
fn watched(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, check: bool) {
    harness.app_mut().windows_mut()[0].set_check_spatial_dependencies(check);
    harness.settle(8);
}

#[test]
fn a_window_asked_to_keep_the_names_keeps_one_for_the_primitives_it_pushes() {
    // The non-vacuity control for everything below, and for the standing runs under
    // `ZGUI_INVARIANTS=1`. `check_spatial_dependencies` is silent about a frame that recorded
    // nothing, so "no faults" is worth exactly as much as the number this asserts.
    let second = zgui_reactive::RwSignal::new_local(false);
    let mut harness = cards(second);
    watched(&mut harness, true);
    let recorded = harness.app().windows()[0].spatial_dependencies_recorded();
    assert!(
        recorded > 0,
        "a frame that drew a card and a pip recorded {recorded} names, so the check it feeds \
         passes over nothing",
    );

    // And the other side of the switch: a window that was not asked pays nothing, which is what
    // makes the check affordable to leave wired in.
    let mut plain = cards(zgui_reactive::RwSignal::new_local(false));
    watched(&mut plain, false);
    assert_eq!(
        plain.app().windows()[0].spatial_dependencies_recorded(),
        0,
        "a window nobody asked is storing a word per primitive per frame",
    );
}

#[test]
fn a_window_that_gives_a_coordinate_system_back_and_reissues_its_slot_draws_an_intact_frame() {
    // The situation the occupancy counter exists for, reached by driving a real window rather than
    // by building a scene by hand: a box establishing a coordinate system leaves the document, its
    // node is given back, the slot returns to the allocator, and the next box to want one is handed
    // it. Every frame of that is checked, and each of them records something to check.
    use zgui_reactive::prelude::Set;

    let second = zgui_reactive::RwSignal::new_local(false);
    let mut harness = cards(second);
    watched(&mut harness, true);
    let before = slots(&harness);

    let mut reissued = false;
    let mut recorded = 0;
    for turn in 0..6 {
        second.set(turn % 2 == 1);
        harness.settle(8);
        // The most any one frame of the turn kept. Settling runs frames past the one that redrew,
        // and a frame with nothing in its damage pushes no primitives and so records no names —
        // which is not the check being off, and must not read as it.
        recorded = recorded.max(harness.app().windows()[0].spatial_dependencies_recorded());
        // A slot whose occupant has a later generation than the one it held is a slot that came
        // back and was handed to somebody else — the premise the check is about. Without this the
        // case would hold just as well for a document that never released anything.
        reissued |= slots(&harness)
            .iter()
            .zip(before.iter())
            .any(|(now, then)| {
                matches!(
                    (now, then),
                    (Some(now), Some(then))
                        if now.index() == then.index() && now.generation() != then.generation()
                )
            });
    }
    assert!(
        reissued,
        "no coordinate system's slot ever changed hands, so this drove the safe case six times",
    );
    assert!(
        recorded > 0,
        "no frame of the six turns recorded a name, so the frame loop's check passed over nothing",
    );
}
