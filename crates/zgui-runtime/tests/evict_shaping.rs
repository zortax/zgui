//! What the shaping budget costs a frame in a document whose live text is over its limit.
//!
//! The live text pins every entry, so the only entries eviction can take are old versions of the
//! paragraph that changed. The assertions are counts: a frame that changes nothing examines no
//! paragraph, and a frame that changes one examines a few, however many the document holds.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

mod support;

use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_runtime::budget::CacheId;
use zgui_view::{BuildCx, IntoView, View};

const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   text { display: block }";

/// How many fixed rows the document holds, each a paragraph of its own.
const ROWS: usize = 400;

/// A level far under what the rows alone hold.
const LIMIT: usize = 16;

/// The most paragraphs a one-paragraph change may examine.
///
/// The new key, the old key and the eviction of the old key, with room for one more.
const EXAMINED_PER_CHANGE: u64 = 4;

/// What the counters did across `act`.
fn moved(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    act: impl FnOnce(&mut zgui_platform_headless::Harness<zgui_runtime::Runtime>),
) -> zgui_profile::Counters {
    let before = zgui_profile::counter::snapshot();
    act(harness);
    harness.settle(16);
    before.delta(&zgui_profile::counter::snapshot())
}

#[test]
fn eviction_work_follows_what_changed() {
    let _turn = zgui_profile::counter::exclusive();
    let typed = RwSignal::new(String::from("typed 0"));
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let mut page =
            zgui_elements::column().child(zgui_elements::text().child(move || typed.get()));
        for row in 0..ROWS {
            page = page.child(zgui_elements::text().child(format!("row {row}")));
        }
        Box::new(page.into_view().build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(16);
    let window = &mut harness.app_mut().windows_mut()[0];
    let limits = window.cache_limits();
    window.set_cache_limits(zgui_runtime::budget::CacheLimits {
        shaped_paragraphs: LIMIT,
        ..limits
    });
    window.request_frame();
    harness.settle(16);

    let held = |harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>| {
        harness.app_mut().windows_mut()[0]
            .budget_report()
            .line(CacheId::ParagraphShaping)
            .report
    };
    let opening = held(&mut harness);
    assert!(
        opening.resident > ROWS as u64 && opening.resident == opening.pinned,
        "the rows are meant to pin everything held, over the limit: {opening:?}"
    );

    // A frame that changes nothing.
    let epoch = harness.app().windows()[0].budget_epoch();
    let idle = moved(&mut harness, |harness| {
        harness.app_mut().windows_mut()[0].request_frame();
    });

    // Frames that each change one paragraph.
    const CHANGES: u64 = 5;
    let typing = moved(&mut harness, |harness| {
        for turn in 1..=CHANGES {
            typed.set(format!("typed {turn}"));
            harness.settle(4);
        }
    });

    assert!(
        harness.app().windows()[0].budget_epoch() > epoch,
        "the idle frame enforced no budget"
    );
    if !zgui_profile::COUNTERS_ENABLED {
        return;
    }
    assert_eq!(
        idle.paragraphs_examined, 0,
        "a frame that changed nothing examined held paragraphs"
    );
    assert_eq!(idle.paragraphs_evicted, 0);

    assert!(typing.text_shaped >= CHANGES, "the changes shaped nothing");
    assert_eq!(
        typing.paragraphs_evicted, CHANGES,
        "each change leaves one old version, and the budget takes it"
    );
    assert!(
        typing.paragraphs_examined <= CHANGES * EXAMINED_PER_CHANGE,
        "{} paragraphs examined for {CHANGES} one-paragraph changes in a document of {}: eviction \
         walks the cache",
        typing.paragraphs_examined,
        opening.resident
    );
    let closing = held(&mut harness);
    assert_eq!(closing.resident, opening.resident);
    assert_eq!(closing.pinned, opening.pinned);
}
