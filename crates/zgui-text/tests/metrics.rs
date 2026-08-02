//! The fixed-metrics source: the same answer everywhere, every time.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::thread;

use zgui_geom::CssPx;
use zgui_text::{FaceQuery, FixedMetrics, FontMetricsSource};
use zgui_text_style::{FontSlant, GenericFamily, TextStyle};

/// A thousand calls from eight threads give one answer.
///
/// Determinism is the whole reason this source exists — a cascade that saw two answers for one
/// query would produce two different computed styles for one element — so the test is on the *set*
/// of answers rather than on a sample of them.
#[test]
fn a_thousand_calls_from_eight_threads_agree() {
    let source = Arc::new(FixedMetrics::new());
    let mut answers = BTreeSet::new();

    let collected: Vec<Vec<[u32; 7]>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let source = Arc::clone(&source);
                scope.spawn(move || {
                    let style = TextStyle::initial();
                    let query = FaceQuery::of(&style);
                    (0..125)
                        .map(|_| bits(&source.face_metrics(&query, CssPx(16.0), false)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("no worker panics"))
            .collect()
    });

    let mut calls = 0;
    for batch in collected {
        calls += batch.len();
        answers.extend(batch);
    }
    assert_eq!(calls, 1_000, "a thousand calls were actually made");
    assert_eq!(answers.len(), 1, "and every one of them agreed");
}

/// Two threads asking about different sizes still each get the answer for the size they asked for.
///
/// The test above would pass for a source that ignored its arguments entirely, which is exactly the
/// vacuous case; this one is what rules it out.
#[test]
fn different_sizes_give_different_answers() {
    let source = FixedMetrics::new();
    let style = TextStyle::initial();
    let query = FaceQuery::of(&style);

    let small = source.face_metrics(&query, CssPx(10.0), false);
    let large = source.face_metrics(&query, CssPx(40.0), false);

    assert_eq!(small.x_height, Some(CssPx(5.0)));
    assert_eq!(large.x_height, Some(CssPx(20.0)));
    assert_ne!(small.ascent, large.ascent);
}

/// Every metric is present, so a document styled against this source takes the *present* branch of
/// every font-relative unit rather than the fallback branch.
#[test]
fn every_metric_is_present() {
    let source = FixedMetrics::new();
    let style = TextStyle::initial();
    let metrics = source.face_metrics(&FaceQuery::of(&style), CssPx(16.0), false);

    assert!(metrics.x_height.is_some());
    assert!(metrics.zero_advance.is_some());
    assert!(metrics.cap_height.is_some());
    assert!(metrics.ic_width.is_some());
    assert!(metrics.script_percent.is_some());
    assert!(metrics.script_script_percent.is_some());

    // And the fallbacks are therefore never taken.
    assert_eq!(
        metrics.x_height_or_fallback(CssPx(16.0)),
        metrics.x_height.expect("present"),
    );
}

/// Monospace starts from a smaller size than a proportional family, which is what environments do.
#[test]
fn the_base_size_depends_on_the_generic_family() {
    let source = FixedMetrics::new();
    assert_eq!(source.base_size(GenericFamily::Serif), CssPx(16.0));
    assert_eq!(source.base_size(GenericFamily::SansSerif), CssPx(16.0));
    assert!(source.base_size(GenericFamily::Monospace).0 < 16.0);
}

/// The query is built from the style and carries what selects a face, and nothing else.
#[test]
fn a_query_carries_the_face_selecting_properties() {
    let mut style = TextStyle::initial();
    style.weight = 700.0;
    style.slant = FontSlant::Italic;
    style.width = 0.75;

    let query = FaceQuery::of(&style);
    assert_eq!(query.weight, 700.0);
    assert_eq!(query.slant, FontSlant::Italic);
    assert_eq!(query.width, 0.75);
    assert!(std::ptr::eq(query.family, &style.family));
}

/// The seven fields as bits, so that two answers can be compared exactly.
fn bits(metrics: &zgui_text::FaceMetrics) -> [u32; 7] {
    let optional = |value: Option<CssPx>| value.map_or(u32::MAX, |length| length.0.to_bits());
    [
        optional(metrics.x_height),
        optional(metrics.zero_advance),
        optional(metrics.cap_height),
        optional(metrics.ic_width),
        metrics.ascent.0.to_bits(),
        metrics.script_percent.map_or(u32::MAX, f32::to_bits),
        metrics.script_script_percent.map_or(u32::MAX, f32::to_bits),
    ]
}
