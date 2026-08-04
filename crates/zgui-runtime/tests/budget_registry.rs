//! What the registry guarantees about a window's caches, asserted over the registry itself.
//!
//! Every assertion here walks [`Window::budget_report`] rather than naming a cache, so a cache
//! registered later is covered by them the moment it is visited — there is no list here to remember
//! to extend. What each cache individually promises is on its own adapter; what this file is about
//! is the promise that holds for all of them at once.

mod support;

use zgui_runtime::budget::CacheId;
use zgui_view::{BuildCx, IntoView, View};

/// A document with text in it and a drawing beside it.
///
/// Both, because the caches the two fill are different and a fixture with only one of them would
/// leave the other empty before the forget as well as after — which is an assertion that cannot
/// fail.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 12px 20px }
                   text { display: block }
                   .mark { display: block; width: 40px; height: 40px }";

/// A triangle, as the notation an element carries.
fn triangle() -> zgui_elements::kurbo::BezPath {
    let mut path = zgui_elements::kurbo::BezPath::new();
    path.move_to((2.0, 2.0));
    path.line_to((22.0, 2.0));
    path.line_to((12.0, 22.0));
    path.close_path();
    path
}

/// A window holding a paragraph and a drawn shape.
fn window() -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child("the quick brown fox"))
                .child(
                    zgui_elements::vector()
                        .class("mark")
                        .view_box(0.0, 0.0, 24.0, 24.0)
                        .paths([triangle()]),
                )
                .into_view()
                .build(cx),
        )
    })
}

/// Which caches the fixture actually manages to fill.
///
/// Read from the window rather than assumed. It is the control on every assertion below: "empty
/// after forget" is satisfied by a window whose caches were empty all along, so what makes the
/// claim mean anything is that these were not.
fn filled(window: &mut zgui_runtime::Window) -> Vec<CacheId> {
    window
        .budget_report()
        .lines()
        .filter(|line| !line.report.is_empty())
        .map(|line| line.id)
        .collect()
}

/// The assertion the registry exists to make possible.
#[test]
fn every_registered_cache_is_empty_after_forget() {
    let mut app = window();
    app.settle(8);

    let before = {
        let window = &mut app.app_mut().windows_mut()[0];
        let filled = filled(window);
        assert!(
            filled.contains(&CacheId::GlyphAtlas)
                && filled.contains(&CacheId::ParagraphShaping)
                && filled.contains(&CacheId::VectorResources),
            "the fixture is meant to fill the glyph atlas, the shaping cache and the vector cache \
             before anything is forgotten, and filled only {filled:?} — an empty cache is emptied \
             by doing nothing at all"
        );
        window.forget_caches();
        filled
    };

    let window = &mut app.app_mut().windows_mut()[0];
    for line in window.budget_report().lines() {
        assert_eq!(
            line.report.resident,
            0,
            "the {} cache held {} {} after every cache was told to forget",
            line.id.name(),
            line.report.resident,
            line.report.unit.name()
        );
    }
    assert!(
        !before.is_empty(),
        "and something was held before, or nothing was asserted"
    );
}

/// Forgetting is a reset and not a wedge: the window produces the same content again.
///
/// This is the half that makes `forget` usable as the oracle's cold side. A window that emptied its
/// caches and then drew nothing — because everything measured from a shaped paragraph was left
/// pointing at a paragraph that no longer exists — would satisfy the assertion above and would be
/// useless for the comparison the registry exists to drive.
#[test]
fn a_window_that_has_forgotten_everything_fills_its_caches_again() {
    let mut app = window();
    app.settle(8);

    let (glyphs, drawings) = {
        let window = &app.app().windows()[0];
        (
            window.scene().primitives.mono_sprites.len(),
            window.scene().primitives.vectors.len(),
        )
    };
    assert!(
        glyphs > 0 && drawings > 0,
        "the fixture drew {glyphs} glyphs and {drawings} vector items before anything was forgotten"
    );

    app.app_mut().windows_mut()[0].forget_caches();
    app.settle(8);

    let window = &app.app().windows()[0];
    assert_eq!(
        window.scene().primitives.mono_sprites.len(),
        glyphs,
        "the same document must draw the same glyphs after its caches were dropped"
    );
    assert_eq!(
        window.scene().primitives.vectors.len(),
        drawings,
        "and the same drawings"
    );
    assert!(
        !filled(&mut app.app_mut().windows_mut()[0]).is_empty(),
        "and the caches must have refilled, or the frames above were served from nothing"
    );
}

/// The registry covers exactly the caches the window has, once each.
#[test]
fn the_report_names_every_registered_cache_exactly_once() {
    let mut app = window();
    app.settle(4);

    let window = &mut app.app_mut().windows_mut()[0];
    let named: Vec<CacheId> = window.budget_report().lines().map(|line| line.id).collect();

    assert_eq!(
        named,
        CacheId::ALL.to_vec(),
        "a line that was never visited would read as an empty cache and would be indistinguishable \
         from one"
    );
}

/// A stated level evicts cold entries while active shaping remains pinned.
///
/// The vector cache can reach zero because its entries are rebuildable. The paragraph currently
/// backing layout cannot: evicting it would turn a soft limit into a full reshape on every frame.
#[test]
fn a_budget_drops_rebuildable_content_but_pins_active_shaping() {
    let mut app = window();
    app.settle(8);

    let held = {
        let window = &mut app.app_mut().windows_mut()[0];
        let report = window.budget_report();
        assert_eq!(
            report.over_limit().count(),
            0,
            "nothing in this fixture comes near the levels a window states by default, and \
             something did"
        );
        (
            report.line(CacheId::ParagraphShaping).report.resident,
            report.line(CacheId::VectorResources).report.resident,
            window.layout().borrow().flattenings(),
        )
    };
    assert!(
        held.0 > 0 && held.1 > 0,
        "the fixture holds {} shaped paragraphs and {} placed drawings, and the levels below have \
         to be under both of those to be reached at all",
        held.0,
        held.1
    );

    // Under what the fixture holds, so the next frame's budget step has an excess to take.
    app.app_mut().windows_mut()[0].set_cache_limits(zgui_runtime::budget::CacheLimits {
        shaped_paragraphs: 0,
        placed_drawings: 0,
    });
    app.app_mut().windows_mut()[0].request_frame();
    app.settle(4);

    let window = &mut app.app_mut().windows_mut()[0];
    let report = window.budget_report();
    let over: Vec<(CacheId, u64)> = report.over_limit().collect();
    assert_eq!(
        report.line(CacheId::VectorResources).report.resident,
        0,
        "the rebuildable vector cache did not reach its zero limit"
    );
    let shaping = report.line(CacheId::ParagraphShaping).report;
    assert_eq!(shaping.resident, held.0);
    assert_eq!(shaping.pinned, held.0);
    assert_eq!(
        over,
        vec![(CacheId::ParagraphShaping, held.0)],
        "only the active shaped paragraph should remain over a zero soft limit"
    );
    assert_eq!(
        window.layout().borrow().flattenings(),
        held.2,
        "enforcing a soft shaping limit rebuilt the live text contexts"
    );
}

/// The one place a cache's second half is named, because the report cannot see it.
///
/// The glyph atlas is registered as one cache and holds two things: the tiles, which the report
/// counts in bytes, and beside them what each glyph key rasterised to — where the pixels go, and
/// whether there were any. The second has no byte figure and so contributes nothing to `resident`,
/// which means the registry-wide claim would go on holding if it were never emptied. It has to be
/// emptied: a remembered placement whose tile has gone names a rectangle something else now
/// occupies, which is the failure the whole eviction work exists to prevent.
#[test]
fn forgetting_the_atlas_forgets_the_glyph_placements_beside_it() {
    let mut app = window();
    app.settle(8);

    let window = &mut app.app_mut().windows_mut()[0];
    assert!(
        window.content().glyphs_held() > 0,
        "the fixture must have rasterised something for this to be an assertion at all"
    );

    window.forget_caches();

    assert_eq!(
        window.content().glyphs_held(),
        0,
        "the placements outlived the tiles they name"
    );
}
