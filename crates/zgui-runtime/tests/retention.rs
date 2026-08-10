//! What the paint cache keeps for fragments the damage stopped reaching.
//!
//! The record of a fragment lives exactly as long as the fragment. A frame that never visits one —
//! culled, outside every damage rectangle, scrolled away — leaves its record standing, so the next
//! frame whose damage reaches its ink replays it instead of encoding it again. The old cache
//! dropped every unvisited record at the end of the frame, which made alternating damage pay a
//! re-encode on every return.
//!
//! The fixture makes the return observable: two lines of text overlap, so changing the first
//! visits the second through its ink. Between two such changes, damage moves to the far end of the
//! document, which is the frame the second line's record has to survive.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

mod support;

use zgui_profile::{COUNTERS_ENABLED, Counter};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_testkit_scene::counters::Recording;

/// A tall window: two overlapping lines at the top, one far line at the bottom.
const CSS: &str = "
root { display: block; width: 400px; height: 300px; background-color: #101010 }
text { display: block; width: 200px }
.over { margin-top: -8px }
.far { margin-top: 200px }
";

/// A window with a changing line, an overlapping still line, and a changing line far below.
fn window(
    near: RwSignal<i32>,
    far: RwSignal<i32>,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child(move || near.get().to_string()))
                .child(zgui_elements::text().class("over").child("88"))
                .child(
                    zgui_elements::text()
                        .class("far")
                        .child(move || far.get().to_string()),
                )
                .into_view()
                .build(cx),
        )
    })
}

/// Whether two line fragments of different paragraphs overlap, which the fixture depends on.
fn lines_overlap(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> bool {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    let mut lines: Vec<(u32, zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>)> = Vec::new();
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            if let zgui_layout::FragmentKind::Line { paragraph, .. } = fragment.kind {
                lines.push((paragraph.index(), fragment.ink));
            }
        }
    }
    lines.iter().enumerate().any(|(at, (paragraph, ink))| {
        lines[at + 1..]
            .iter()
            .any(|(other, other_ink)| other != paragraph && ink.intersects(*other_ink))
    })
}

#[test]
fn a_record_survives_frames_that_never_visit_it_and_replays_on_return() {
    let near = RwSignal::new(0);
    let far = RwSignal::new(0);
    let mut recording = Recording::begin();
    let mut harness = window(near, far);

    // The control: building the document encodes everything, which is what makes the equality at
    // the bottom mean "the return re-encoded nothing extra" rather than "nothing was measured".
    let built = recording.measure(|| {
        harness.settle(8);
    });
    let encode = built.control(Counter::ChunksReencoded);
    assert!(
        lines_overlap(&harness),
        "no two lines overlap, so a change of the first line cannot visit the second and the \
         assertions below would hold of any cache at all"
    );

    // The pattern being pinned, taken once as its own control: the changed line encodes, the
    // overlapped still line is visited through its ink and replays.
    let first = recording.measure(|| {
        near.set(1);
        harness.settle(8);
    });

    // Damage moves to the far end of the document. Nothing visits the two top lines, and the old
    // cache dropped both records here.
    let between = recording.measure(|| {
        far.set(1);
        harness.settle(8);
    });

    // The return: the same change again. The overlapped line was not visited for a whole frame
    // and must still replay, so the return costs exactly what the first change cost.
    let third = recording.measure(|| {
        near.set(2);
        harness.settle(8);
    });

    if !COUNTERS_ENABLED {
        return;
    }

    assert!(
        between.get(Counter::ChunksRetainedUnvisited) > 0,
        "the far change visited the whole document, so nothing was retained and the fixture \
         proves nothing about unvisited records"
    );
    third.assert_at_most(
        Counter::ChunksReencoded,
        first.get(Counter::ChunksReencoded),
        &encode,
    );
    assert!(
        third.get(Counter::ChunksTranslated) >= first.get(Counter::ChunksTranslated),
        "the return replayed less than the first change did ({} against {}), so a record was \
         dropped while its fragment stood",
        third.get(Counter::ChunksTranslated),
        first.get(Counter::ChunksTranslated),
    );
}
