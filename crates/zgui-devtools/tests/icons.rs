//! The toolbar's icons take up the room they are drawn in.
//!
//! A drawing has no content of its own to size it: what it occupies is what the sheet says and
//! nothing else. So an icon that is allowed to shrink, in a bar that has run out of room, is an
//! icon of no width at all — a button that draws nothing, laid out perfectly, in a panel every
//! other assertion says is correct. That is exactly what happened when the tab strip grew a sixth
//! tab, and it is invisible to every test that asks where the button is rather than how big what is
//! inside it came out.
//!
//! Whether the path data parses is asserted beside the constants themselves, in `panel::icon`.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::DevTools;

use support::{find_box, opened, run};

/// How close two lengths have to be to be the same length.
const CLOSE: f32 = 0.5;

/// Both toolbar icons are laid out at the size the sheet asks for.
#[test]
fn the_toolbar_icons_are_the_size_the_sheet_gives_them() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    run(&mut harness, 8);

    let icon =
        find_box(&harness, "zgui-devtools__icon").expect("the toolbar's icons are in the document");
    assert!(
        (icon.size.width.0 - 16.0).abs() < CLOSE && (icon.size.height.0 - 16.0).abs() < CLOSE,
        "an icon the sheet gives 16x16 came out {}x{}",
        icon.size.width.0,
        icon.size.height.0
    );
}

/// The icons keep their size in a panel dragged as narrow as it goes.
///
/// The narrow end is where the bar runs out of room, which is the case the shrink actually bit in.
#[test]
fn the_icons_survive_the_narrowest_the_panel_goes() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    run(&mut harness, 8);

    // Narrower than the panel is allowed to be, so this lands on the floor whatever it is.
    tools.set_width(0.0);
    run(&mut harness, 8);

    let icon = find_box(&harness, "zgui-devtools__icon")
        .expect("the toolbar's icons are still in the document");
    assert!(
        (icon.size.width.0 - 16.0).abs() < CLOSE,
        "at the panel's narrowest an icon came out {}px wide",
        icon.size.width.0
    );
}
