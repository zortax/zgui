//! What colour the glyphs are actually drawn in after every text colour in the window changes.
//!
//! A theme flip is the one change that moves *every* text colour at once, and it is the change a
//! window is least able to check for itself: the cascade settles on the new colours, the elements
//! all report them, and a test that asks the style system what colour an element is will pass while
//! the window on the screen is still half light. The colour a glyph is drawn in does not come from
//! the element — it comes from a slot in the display list's brush table that the glyph named when
//! it was shaped, and a flip that fails to rewrite the slot leaves the old colour on the screen for
//! as long as the shaping survives, which is for ever.
//!
//! So these tests read the colour off **every** sprite in the display list, which is the last thing
//! before the pixels, and they read it in the order the glyphs are stacked down the window, so that
//! one string left behind in the old theme is a different answer rather than a set that still
//! happens to contain the right values.
//!
//! Two flips are covered, because a theme reaches a document by two entirely separate routes and
//! each has its own way of not arriving:
//!
//! * an **attribute** on an element, which re-matches selectors under it;
//! * the **desktop's colour scheme**, which re-matches `prefers-color-scheme` against a device the
//!   window has to be told to rebuild, and which is how a component library's tokens are themed —
//!   custom properties declared on `:root`, read through `var()` by everything below it.

mod support;

use zgui_platform::{ColorScheme, PlatformCx, SurfaceEvent, WakeReason};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_view::{AttrName, BuildCx, IntoView, View};

/// Four elements holding text, three naming their own colour and one inheriting, and a theme
/// attribute that moves all four at once.
const ATTRIBUTE_CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   text { display: block }
                   .page { display: block; color: rgb(10, 10, 10) }
                   .title { display: block; color: rgb(20, 20, 20) }
                   .body { display: block; color: rgb(30, 30, 30) }
                   .muted { display: block }
                   .page[data-theme=\"dark\"] { color: rgb(210, 210, 210) }
                   .page[data-theme=\"dark\"] .title { color: rgb(220, 220, 220) }
                   .page[data-theme=\"dark\"] .body { color: rgb(230, 230, 230) }";

/// The same four elements, coloured the way a token library colours them: three custom properties
/// declared on `:root`, read through `var()`, and redeclared by a `prefers-color-scheme` query.
///
/// Nothing in the document changes when this theme flips. The attribute above at least writes
/// something a node carries; here the only thing that moves is the answer a media query gives, so
/// every element that has to cascade again has to be reached from the device alone.
const SCHEME_CSS: &str = ":root { display: block; width: 400px; height: 300px;
                           --page: rgb(10, 10, 10);
                           --title: rgb(20, 20, 20);
                           --body: rgb(30, 30, 30) }
                   @media (prefers-color-scheme: dark) {
                     :root { --page: rgb(210, 210, 210);
                             --title: rgb(220, 220, 220);
                             --body: rgb(230, 230, 230) }
                   }
                   text { display: block }
                   .page { display: block; color: var(--page) }
                   .title { display: block; color: var(--title) }
                   .body { display: block; color: var(--body) }
                   .muted { display: block }";

/// The same four elements, with one of them generating content that names a colour of its own.
///
/// The generated content is the case an element-shaped assertion cannot reach. A pseudo-element
/// cascades separately from the element it hangs off and therefore holds its own colour — a
/// placeholder is muted while the field around it is not — so its glyphs claim a brush slot of
/// their own. An update remembered against the element alone covers one of the two, and the other
/// keeps the colour it was shaped in.
///
/// The two colours are deliberately far apart *and* different from the element's own, so that
/// generated content left behind cannot be mistaken for generated content that followed.
const PSEUDO_CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   text { display: block }
                   .page { display: block; color: rgb(10, 10, 10) }
                   .title { display: block; color: rgb(20, 20, 20) }
                   .body { display: block; color: rgb(30, 30, 30) }
                   .muted { display: block }
                   .muted::before { content: \"ee\"; color: rgb(50, 50, 50) }
                   .page[data-theme=\"dark\"] { color: rgb(210, 210, 210) }
                   .page[data-theme=\"dark\"] .title { color: rgb(220, 220, 220) }
                   .page[data-theme=\"dark\"] .body { color: rgb(230, 230, 230) }
                   .page[data-theme=\"dark\"] .muted::before { color: rgb(250, 250, 250) }";

/// The same four elements, none of which declares a colour, under a page that declares one in the
/// dark theme and not in the light one.
///
/// This is the ordinary shape of a themed application and it is *not* the shape above. Nothing here
/// names a colour in the light theme, so the page and everything under it share the root's cascade
/// result and every string is drawn through the one slot claimed against it. The dark theme gives
/// the page a result of its own, which the root does not join — so the flip splits a slot rather
/// than rewriting one, and the flip back merges the pieces onto a cascade result that already has a
/// slot of its own and never stopped having one.
const INHERITED_CSS: &str = ":root { display: block; width: 400px; height: 300px;
                             color: rgb(10, 10, 10) }
                   text { display: block }
                   .page { display: block }
                   .title { display: block }
                   .body { display: block }
                   .muted { display: block }
                   .page[data-theme=\"dark\"] { color: rgb(210, 210, 210) }";

/// The eight glyphs of [`INHERITED_CSS`], every one of them inheriting the root's colour.
const LIGHT_INHERITED: [u8; 8] = [10; 8];

/// The same eight, with the page's dark colour inherited by all of them.
const DARK_INHERITED: [u8; 8] = [210; 8];

/// [`INHERITED_CSS`] with one more string, in a layer of its own beside the themed page.
///
/// This is the shape a tooltip, a menu and a dialog all have: content that is *not* inside the
/// themed subtree, drawn in a colour it inherits from the root, and mounted or shaped long after
/// the theme was last flipped. It inherits the root's cascade result — the same result the page and
/// everything under it inherited before the theme gave the page one of its own — so it is the
/// element that finds out whether the way back from that result to a slot still means what it did.
const LAYERED_CSS: &str = ":root { display: block; width: 400px; height: 300px;
                           color: rgb(10, 10, 10) }
                   text { display: block }
                   .page { display: block }
                   .title { display: block }
                   .body { display: block }
                   .muted { display: block }
                   .layer { display: block }
                   .page[data-theme=\"dark\"] { color: rgb(210, 210, 210) }";

/// The ten glyphs of [`LAYERED_CSS`] with the page dark: four strings inside it and one beside it.
///
/// The last two entries are the layer's, and they are the assertion: the layer is outside the page,
/// so the page's theme is not its theme and it stays the colour the root gives it.
const DARK_BESIDE_A_LAYER: [u8; 10] = [210, 210, 210, 210, 210, 210, 210, 210, 10, 10];

/// What the four strings are drawn in under the light theme, one entry per glyph.
///
/// Two glyphs per string, and the strings stack down the window in document order: the title, the
/// body, the muted line that inherits from the page, and the page's own bare string. Every element
/// that holds text is in here, which is the point — a flip that rewrote the colours it could see
/// and left the two inheriting strings behind is a different vector, not a shorter one.
const LIGHT: [u8; 8] = [20, 20, 30, 30, 10, 10, 10, 10];

/// The same eight glyphs under the dark theme.
const DARK: [u8; 8] = [220, 220, 230, 230, 210, 210, 210, 210];

/// The grey every glyph in the window is drawn in, ordered down the window and then across it.
///
/// The red channel alone only because every colour here is a grey, so one channel names it. The
/// *order* is what makes this an assertion about which string is which: sorted by where the sprite
/// lands, it is the document's own reading order, so a window whose title flipped and whose body
/// did not cannot produce the same answer as one where both did.
fn drawn_greys(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u8> {
    let mut placed: Vec<_> = harness.app().windows()[0]
        .scene()
        .primitives
        .mono_sprites
        .iter()
        .map(|sprite| {
            (
                sprite.bounds[1].to_bits(),
                sprite.bounds[0].to_bits(),
                (sprite.color[0] * 255.0).round() as u8,
            )
        })
        .collect();
    placed.sort_unstable();
    placed.into_iter().map(|(_, _, grey)| grey).collect()
}

/// Damages the whole window without moving anything in it, and settles the frame that follows.
///
/// A surface that comes back from being hidden is the one repaint that asks for every pixel again
/// while changing no input to any of them: the extent is what it was, so nothing reflows and no
/// string is shaped again, and every glyph is therefore drawn through the slot it named when it was
/// shaped — before the theme moved. A resize would not do: it changes the thing being laid out, and
/// a flip that only survives because the strings were shaped afresh would pass.
/// Coming back from occlusion damages the whole surface, and the frame that answers it is paced:
/// it is offered once, declined while the reconfiguration it owes could not yet have been seen, and
/// asked for again by the deadline. So the clock is moved past that deadline before the frame the
/// assertion reads is expected to have run.
fn repaint_everything(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    let surface = surface_of(harness);
    harness.deliver(surface, SurfaceEvent::Occluded(true));
    harness.settle(8);
    harness.deliver(surface, SurfaceEvent::Occluded(false));
    harness.settle(8);
    harness.advance(std::time::Duration::from_millis(50));
    harness.settle(8);

    // The repaint has to have happened for anything below to mean anything. A frame that emitted
    // nothing leaves the display list empty, and an empty list is not the old theme — it is no
    // theme, and an assertion about colours would then be an assertion about nothing.
    assert_eq!(
        drawn_greys(harness).len(),
        LIGHT.len(),
        "the surface was damaged in full and the frame that answered it drew no text at all"
    );
}

/// Damages the whole window and settles, without moving or re-shaping anything in it.
///
/// The same act as [`repaint_everything`], without its assertion about how many glyphs a particular
/// fixture has.
fn repaint_all(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    let surface = surface_of(harness);
    harness.deliver(surface, SurfaceEvent::Occluded(true));
    harness.settle(8);
    harness.deliver(surface, SurfaceEvent::Occluded(false));
    harness.settle(8);
    harness.advance(std::time::Duration::from_millis(50));
    harness.settle(8);
}

/// The window's surface, which is what a platform event is addressed to.
fn surface_of(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> zgui_platform::SurfaceId {
    harness
        .platform()
        .offscreens()
        .first()
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window")
}

/// The page and its four strings: a title, a body, a line that inherits, and one bare string.
///
/// The theme attribute is on the page and is written from `theme`, which answers `None` for the
/// light theme. A window driven by the desktop's scheme instead passes a signal nothing ever sets,
/// so both fixtures build the same tree and the same eight glyphs.
fn four_strings(cx: &mut BuildCx<'_>, theme: RwSignal<bool>) -> Box<dyn zgui_view::Anchor> {
    let view = zgui_elements::column()
        .class("page")
        .attribute(AttrName::new("data-theme"), move || {
            theme.get().then(|| "dark".to_owned())
        })
        .child(
            zgui_elements::column()
                .class("title")
                .child(zgui_elements::text().child("aa")),
        )
        .child(
            zgui_elements::column()
                .class("body")
                .child(zgui_elements::text().child("bb")),
        )
        .child(
            zgui_elements::column()
                .class("muted")
                .child(zgui_elements::text().child("cc")),
        )
        .child(zgui_elements::text().child("dd"))
        .into_view();
    Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
}

/// The same page, with one more string beside it rather than inside it.
fn four_strings_and_a_layer(
    cx: &mut BuildCx<'_>,
    theme: RwSignal<bool>,
) -> Box<dyn zgui_view::Anchor> {
    let view = zgui_elements::column()
        .child(
            zgui_elements::column()
                .class("page")
                .attribute(AttrName::new("data-theme"), move || {
                    theme.get().then(|| "dark".to_owned())
                })
                .child(
                    zgui_elements::column()
                        .class("title")
                        .child(zgui_elements::text().child("aa")),
                )
                .child(
                    zgui_elements::column()
                        .class("body")
                        .child(zgui_elements::text().child("bb")),
                )
                .child(
                    zgui_elements::column()
                        .class("muted")
                        .child(zgui_elements::text().child("cc")),
                )
                .child(zgui_elements::text().child("dd")),
        )
        .child(
            zgui_elements::column()
                .class("layer")
                .child(zgui_elements::text().child("ee")),
        )
        .into_view();
    Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
}

/// The four strings, under a page whose `data-theme` follows the signal that is returned.
fn themed(
    css: &str,
) -> (
    RwSignal<bool>,
    zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) {
    let dark = RwSignal::new(false);
    let harness = support::app_with_text(css, move |cx: &mut BuildCx<'_>| four_strings(cx, dark));
    (dark, harness)
}

/// The same four strings, on a desktop that expresses `scheme` before the window is opened.
fn under_scheme(
    scheme: Option<ColorScheme>,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let platform = zgui_platform_headless::Headless::new();
    platform.set_color_scheme(scheme);
    let never = RwSignal::new(false);
    support::app_with_text_on(platform, SCHEME_CSS, move |cx: &mut BuildCx<'_>| {
        four_strings(cx, never)
    })
}

#[test]
fn every_string_in_the_window_is_redrawn_in_the_new_theme_s_colour() {
    let (dark, mut harness) = themed(ATTRIBUTE_CSS);
    harness.settle(8);

    // The control. Without four distinct colours actually reaching the sprites there is no partial
    // rewrite to detect, and the assertion below would pass on an empty window.
    assert_eq!(
        drawn_greys(&harness),
        LIGHT,
        "the light theme was not drawn, so nothing here is being tested"
    );

    dark.set(true);
    harness.settle(8);

    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the theme was flipped and some of the window's text is still drawn in the old colour"
    );

    dark.set(false);
    harness.settle(8);

    assert_eq!(
        drawn_greys(&harness),
        LIGHT,
        "the theme was flipped back and some of the window's text stayed dark"
    );
}

#[test]
fn generated_content_follows_the_flip_its_element_follows() {
    let (dark, mut harness) = themed(PSEUDO_CSS);
    harness.settle(8);

    // The control. Without the generated content actually reaching the display list in a colour of
    // its own there is nothing here that could be left behind.
    assert!(
        drawn_greys(&harness).contains(&50),
        "the generated content was not drawn in its own colour, so nothing here is being tested: \
         {:?}",
        drawn_greys(&harness)
    );

    dark.set(true);
    harness.settle(8);

    let greys = drawn_greys(&harness);
    assert!(
        greys.contains(&250),
        "the theme was flipped and the generated content was not redrawn in the new colour: \
         {greys:?}"
    );
    assert!(
        !greys.contains(&50),
        "the theme was flipped and the generated content is still drawn in the old colour: \
         {greys:?}"
    );

    // And back, because a slot rewritten once may have been rewritten by being abandoned: an
    // element given a fresh slot answers the first flip and no other.
    dark.set(false);
    harness.settle(8);

    let greys = drawn_greys(&harness);
    assert!(
        greys.contains(&50) && !greys.contains(&250),
        "the theme was flipped back and the generated content stayed dark: {greys:?}"
    );
}

#[test]
fn the_new_theme_survives_a_repaint_that_changes_nothing() {
    let (dark, mut harness) = themed(ATTRIBUTE_CSS);
    harness.settle(8);
    dark.set(true);
    harness.settle(8);

    // Asserted before the repaint as well as after it. Without this the test passes on a window
    // whose flip never landed at all and was only ever repaired by the repaint, which is the
    // opposite of what it claims to check.
    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the theme flip itself did not land, so its survival is not what is being tested"
    );

    // A frame that damages everything and reshapes nothing is where a brush table holding one
    // stale slot shows itself: the glyphs are drawn again, and they are drawn through the slot they
    // named when they were shaped.
    repaint_everything(&mut harness);

    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "a repaint after the theme flip brought the old colours back"
    );
}

#[test]
fn every_string_follows_the_desktop_into_dark_and_back() {
    let mut harness = under_scheme(None);
    harness.settle(8);
    let surface = surface_of(&harness);

    assert_eq!(
        drawn_greys(&harness),
        LIGHT,
        "the light theme was not drawn, so nothing here is being tested"
    );

    harness.platform().set_color_scheme(Some(ColorScheme::Dark));
    harness.deliver(surface, SurfaceEvent::ColorSchemeChanged(ColorScheme::Dark));
    harness.settle(8);

    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the desktop switched to dark and the window is still drawn in the light tokens"
    );

    harness
        .platform()
        .set_color_scheme(Some(ColorScheme::Light));
    harness.deliver(
        surface,
        SurfaceEvent::ColorSchemeChanged(ColorScheme::Light),
    );
    harness.settle(8);

    assert_eq!(
        drawn_greys(&harness),
        LIGHT,
        "the desktop switched back to light and the window stayed dark"
    );
}

#[test]
fn the_desktop_s_theme_survives_a_repaint_that_changes_nothing() {
    let mut harness = under_scheme(None);
    harness.settle(8);
    let surface = surface_of(&harness);
    harness.platform().set_color_scheme(Some(ColorScheme::Dark));
    harness.deliver(surface, SurfaceEvent::ColorSchemeChanged(ColorScheme::Dark));
    harness.settle(8);

    // Asserted before the repaint, for the same reason as above and with more force here: the
    // viewport this route rebuilds is also rebuilt by a resize, so a test that only looked after
    // the repaint would pass on a window in which the scheme reached the cascade nowhere else.
    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the desktop's flip itself did not land, so its survival is not what is being tested"
    );

    // The same repaint-with-no-reshaping as above, against the route where nothing in the document
    // changed at all: what the glyphs are drawn through has to still hold the dark tokens.
    repaint_everything(&mut harness);

    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "a repaint after the desktop's theme flip brought the light tokens back"
    );
}

#[test]
fn a_resize_after_the_desktop_s_flip_does_not_re_light_the_window() {
    let mut harness = under_scheme(None);
    harness.settle(8);
    let surface = surface_of(&harness);
    harness.platform().set_color_scheme(Some(ColorScheme::Dark));
    harness.deliver(surface, SurfaceEvent::ColorSchemeChanged(ColorScheme::Dark));
    harness.settle(8);
    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the desktop's flip itself did not land, so nothing here is being tested"
    );

    // The viewport is what carries the scheme to the cascade's device, and a resize builds a new
    // one from the surface's extent. A rebuild that does not carry the scheme across defaults it,
    // and the default is light — so the window silently returns to the light tokens the first time
    // it is dragged by a corner, with the desktop still dark and no event left to correct it.
    harness.deliver(
        surface,
        SurfaceEvent::Resized(zgui_geom::Size::new(
            zgui_geom::DevicePx(360.0),
            zgui_geom::DevicePx(280.0),
        )),
    );
    harness.advance(std::time::Duration::from_millis(50));
    harness.settle(8);

    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "resizing the window re-lit it in the light tokens on a dark desktop"
    );
}

/// The window's device pixel ratio, changed the way dragging it onto another monitor changes it.
///
/// The surface keeps the same number of device pixels, which is what a window physically the same
/// size on a differently-scaled output has: what moves is how many CSS pixels that is.
fn move_to_scale(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, scale: f64) {
    let surface = surface_of(harness);
    harness.deliver(
        surface,
        SurfaceEvent::ScaleFactorChanged {
            scale_factor: scale,
            size: zgui_geom::Size::new(zgui_geom::DevicePx(400.0), zgui_geom::DevicePx(300.0)),
        },
    );
    harness.advance(std::time::Duration::from_millis(50));
    harness.settle(16);
}

/// Dragging the window between two differently-scaled monitors, with the theme already flipped.
///
/// This is the one change that re-shapes every string in the window without restyling a single
/// element: a shaped paragraph is keyed by the ratio it was shaped at, so a new ratio misses and
/// shapes again, while `font-size: 12px` is twelve CSS pixels at every ratio and the cascade is
/// therefore untouched. So every glyph in the window claims its brush slot afresh, from a cascade
/// result that has not moved — and if the way back from that result to a slot has been broken, the
/// re-shaped glyphs pick up a slot holding the colour of the theme the window is no longer in.
///
/// Both directions are walked, and the walk starts at the fractional ratio, because the defect this
/// guards is asymmetric in practice: what breaks it is the *second* claim against a cascade result,
/// which only the second shaping makes.
#[test]
fn dragging_the_window_between_monitors_does_not_re_light_the_dark_theme() {
    let (dark, mut harness) = themed(ATTRIBUTE_CSS);
    harness.settle(8);
    move_to_scale(&mut harness, 1.2);
    dark.set(true);
    harness.settle(8);
    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the flip itself did not land at this ratio, so nothing here is being tested"
    );

    move_to_scale(&mut harness, 1.0);

    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the window was dragged to another monitor and its text came back in the light theme"
    );

    move_to_scale(&mut harness, 1.2);

    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the window was dragged back and its text came back in the light theme"
    );
}

/// The same drag, after the theme has been flipped back and forth rather than once.
///
/// The round trip is the whole of it. A flip that gives the page a colour of its own moves every
/// string off the slot the root's cascade result claimed; the flip back moves them onto that result
/// again — and that result never stopped having a slot, because the root itself is still drawn
/// through it. What a window does with a cascade result that already has a slot is the question,
/// and it has only one answer that survives being asked twice: the slot belongs to the result. A
/// window that instead points the result at whichever slot the element happened to be using leaves
/// the table saying that the root's colour is the page's — and every string re-shaped afterwards
/// reads the answer, which is what dragging the window onto another monitor makes all of them do.
#[test]
fn a_theme_flipped_back_and_forth_survives_being_dragged_between_monitors() {
    let (dark, mut harness) = themed(INHERITED_CSS);
    harness.settle(8);
    assert_eq!(
        drawn_greys(&harness),
        LIGHT_INHERITED,
        "the inherited light theme was not drawn, so nothing here is being tested"
    );

    for _ in 0..2 {
        dark.set(true);
        harness.settle(8);
        assert_eq!(
            drawn_greys(&harness),
            DARK_INHERITED,
            "a flip into the dark theme did not reach every inherited string"
        );
        dark.set(false);
        harness.settle(8);
        assert_eq!(
            drawn_greys(&harness),
            LIGHT_INHERITED,
            "a flip back to the light theme did not reach every inherited string"
        );
    }

    dark.set(true);
    harness.settle(8);
    assert_eq!(
        drawn_greys(&harness),
        DARK_INHERITED,
        "the last flip did not land, so the drag below is not what is being tested"
    );

    // Every string in the window is shaped again here, and every one of them asks the table what
    // slot its cascade result resolves to. Nothing else in the window has changed.
    move_to_scale(&mut harness, 1.2);

    assert_eq!(
        drawn_greys(&harness),
        DARK_INHERITED,
        "the window was dragged to another monitor and its inherited text came back light"
    );
}

/// Content beside the themed subtree keeps the root's colour after the theme has been flipped.
///
/// The layer inherits the root's cascade result, which is the result the page inherited too before
/// the theme gave it one of its own. So the flip is where the two part company, and the way back
/// from that result to a brush slot is the only thing that says so. A window that lets an element
/// re-point a result which already has a slot writes the page's answer over the root's — after
/// which everything resolving through the root is sent to the page's slot and drawn in the page's
/// colour, which on a dark page is light text on whatever the layer's own background is.
///
/// The drag is what makes every string ask again. Nothing else in the window changes.
#[test]
fn a_layer_beside_the_themed_page_is_not_drawn_in_the_page_s_colour() {
    let dark = RwSignal::new(false);
    let mut harness = support::app_with_text(LAYERED_CSS, move |cx: &mut BuildCx<'_>| {
        four_strings_and_a_layer(cx, dark)
    });
    harness.settle(8);
    assert_eq!(
        drawn_greys(&harness),
        [10; 10],
        "the light theme was not drawn, so nothing here is being tested"
    );

    for _ in 0..2 {
        dark.set(true);
        harness.settle(8);
        dark.set(false);
        harness.settle(8);
    }
    dark.set(true);
    harness.settle(8);

    // A flip damages what it changed, and the layer is exactly what it did not change — so the
    // display list after one holds the page's strings and not the layer's. The whole surface is
    // asked for again here so that every string in the window is in the list being read.
    repaint_all(&mut harness);

    assert_eq!(
        drawn_greys(&harness),
        DARK_BESIDE_A_LAYER,
        "the page's theme reached the layer beside it"
    );

    // Every string is shaped again and resolves its brush slot afresh, from cascade results that
    // have not moved. This is a window being dragged onto a differently-scaled monitor.
    move_to_scale(&mut harness, 1.2);

    assert_eq!(
        drawn_greys(&harness),
        DARK_BESIDE_A_LAYER,
        "the window was dragged to another monitor and the layer took the page's colour"
    );
}

#[test]
fn a_desktop_wide_notification_repaints_the_window_in_the_new_scheme() {
    let mut harness = under_scheme(None);
    harness.settle(8);
    assert_eq!(
        drawn_greys(&harness),
        LIGHT,
        "the light theme was not drawn, so nothing here is being tested"
    );

    // The other route, and the one with nothing addressed to a surface in it: a platform that
    // notices the desktop's preference centrally reports it as a wake, and the window has to read
    // the preference rather than merely draw itself again.
    harness.platform().set_color_scheme(Some(ColorScheme::Dark));
    harness
        .platform()
        .waker()
        .wake(WakeReason::ColorSchemeChanged);
    harness.settle(8);

    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "a desktop-wide colour scheme notification redrew the window in the old tokens"
    );
}

#[test]
fn a_window_opened_on_a_dark_desktop_is_dark_in_its_first_frame() {
    let mut harness = under_scheme(Some(ColorScheme::Dark));
    harness.settle(8);

    // Not a repaint: the very first frame. A window that reads the preference only when it is told
    // it moved launches light on a dark desktop, which is a white flash before anything changes it.
    assert_eq!(
        drawn_greys(&harness),
        DARK,
        "the window was opened on a dark desktop and drew its first frame in the light tokens"
    );
}
