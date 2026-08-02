//! Face metrics, the memo in front of them, and the lock they would otherwise take.

mod support;

use std::sync::Arc;
use std::thread;

use zgui_geom::CssPx;
use zgui_text::{FaceQuery, FontMetricsSource, FontSource};
use zgui_text_parley::{BASE_SIZE, FontSystem, FontSystemOptions, MONOSPACE_BASE_SIZE};
use zgui_text_style::{GenericFamily, TextStyle};

/// How many sizes the load below asks about.
const SIZES: [f32; 8] = [12.0, 13.0, 14.0, 16.0, 18.0, 20.0, 24.0, 32.0];

/// A hundred thousand queries from eight threads reach the collection fewer than a hundred times.
///
/// The cascade asks this question once per element per restyle, from every worker thread at once,
/// and the collection it would otherwise ask needs exclusive access even to answer. What makes
/// that survivable is the memo: the lock is taken once per *distinct* query, so the count tracks
/// the eight sizes and the eight threads racing for them, not the hundred thousand calls.
#[test]
fn the_metrics_memo_keeps_the_lock_cold() {
    let fonts = support::fonts();
    let before = fonts.lock_acquisitions();

    thread::scope(|scope| {
        for _ in 0..8 {
            let fonts = Arc::clone(&fonts);
            scope.spawn(move || {
                let style = support::style();
                let query = FaceQuery::of(&style);
                for index in 0..12_500 {
                    let size = CssPx(SIZES[index % SIZES.len()]);
                    let metrics = fonts.face_metrics(&query, size, false);
                    assert!(metrics.x_height.is_some(), "the shipped face declares one");
                }
            });
        }
    });

    let taken = fonts.lock_acquisitions() - before;
    assert!(
        taken < 100,
        "100 000 queries over {} distinct keys took the collection's lock {taken} times",
        SIZES.len()
    );
    assert_eq!(
        fonts.metrics_memo_len(),
        SIZES.len(),
        "and the memo holds one answer per distinct key"
    );
}

/// The same query gives the same answer however many threads ask it.
#[test]
fn the_answers_are_the_same_from_every_thread() {
    let fonts = support::fonts();
    let style = support::style();
    let expected = fonts.face_metrics(&FaceQuery::of(&style), CssPx(16.0), false);

    thread::scope(|scope| {
        for _ in 0..8 {
            let fonts = Arc::clone(&fonts);
            scope.spawn(move || {
                let style = support::style();
                let query = FaceQuery::of(&style);
                for _ in 0..1_000 {
                    assert_eq!(fonts.face_metrics(&query, CssPx(16.0), false), expected);
                }
            });
        }
    });
}

/// The metrics read are the face's own, not a fraction of the size.
#[test]
fn the_metrics_come_from_the_face() {
    let fonts = support::fonts();
    let style = support::style();
    let metrics = fonts.face_metrics(&FaceQuery::of(&style), CssPx(100.0), false);

    let x_height = metrics.x_height.expect("declared").0;
    let cap_height = metrics.cap_height.expect("declared").0;
    let zero = metrics.zero_advance.expect("the face has a digit zero").0;
    assert!(
        x_height > 0.0 && x_height < cap_height && cap_height < metrics.ascent.0,
        "x-height {x_height}, cap height {cap_height} and ascent {} must nest",
        metrics.ascent.0
    );
    assert!(zero > 0.0 && zero < 100.0, "a digit zero advance of {zero}");
    assert!(
        (x_height - 50.0).abs() > 0.001,
        "an x-height of exactly half the size is the fixed-metrics answer, not a face's"
    );
}

/// A query no registered family answers still cascades, with every optional metric absent.
#[test]
fn an_unmatched_query_reports_no_metrics_rather_than_failing() {
    let empty = FontSystem::new(FontSystemOptions::registered_only());
    let style = TextStyle::initial();
    let metrics = empty.face_metrics(&FaceQuery::of(&style), CssPx(16.0), false);
    assert_eq!(metrics.x_height, None);
    assert_eq!(metrics.ascent, CssPx(0.0));
    assert_eq!(
        metrics.x_height_or_fallback(CssPx(16.0)),
        CssPx(8.0),
        "and each dependent unit takes its documented fallback"
    );
}

/// The generic roles carry the sizes an environment configures them with.
#[test]
fn generic_families_have_their_own_base_sizes() {
    let fonts = support::fonts();
    assert_eq!(
        fonts.base_size(GenericFamily::Monospace),
        MONOSPACE_BASE_SIZE
    );
    assert_eq!(fonts.base_size(GenericFamily::SansSerif), BASE_SIZE);
    assert_eq!(fonts.base_size(GenericFamily::Serif), BASE_SIZE);
}

/// Registering a family clears the memo, so a later query cannot be served an answer from before
/// the face that now wins the match existed.
#[test]
fn registering_a_face_drops_stale_answers() {
    let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    let style = support::style();
    let cold = fonts.face_metrics(&FaceQuery::of(&style), CssPx(16.0), false);
    assert_eq!(cold.ascent, CssPx(0.0), "nothing is registered yet");

    fonts
        .register(support::face("NotoSans-Regular.ttf"), None)
        .expect("registers");

    let warm = fonts.face_metrics(&FaceQuery::of(&style), CssPx(16.0), false);
    assert!(
        warm.ascent.0 > 0.0,
        "the answer must come from the face registered since"
    );
}
