//! Two windows drawing through the one compositor of their application, each into an output of
//! its own and from one glyph atlas.

mod support;

use zgui_platform::SurfaceAttributes;
use zgui_runtime::WindowSpec;
use zgui_view::{BuildCx, IntoView, View};

/// A line of text, which is what the compositor rasterises into its glyph atlas.
const SHEET: &str = "root { display: block; width: 300px; height: 100px } text { display: block }";

/// A window holding one line of text.
fn line(cx: &mut BuildCx<'_>) -> Box<dyn zgui_view::Anchor> {
    Box::new(
        zgui_elements::column()
            .class("root")
            .child(zgui_elements::text().child("shared"))
            .into_view()
            .build(cx),
    )
}

/// The output `window` draws into, as the first line of what it shows names it.
fn output(window: &zgui_runtime::Window) -> String {
    let drawing = support::drawing(window);
    drawing
        .transcript()
        .lines()
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or_default()
        .to_owned()
}

/// The atlas tile of every glyph `window` shows, in draw order.
fn tiles(window: &zgui_runtime::Window) -> Vec<String> {
    support::drawing(window)
        .glyphs()
        .iter()
        .map(|glyph| glyph.field("tile").unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn two_windows_draw_one_line_from_the_same_glyph_tiles() {
    let mut harness = support::app_with_text(SHEET, line);
    harness.settle(8);
    let mut options = zgui_runtime::WindowContent::default();
    options.window_stylesheets.push(SHEET.to_owned());
    harness.app().window_commands().open(WindowSpec::new(
        SurfaceAttributes::new("second"),
        options,
        Box::new(line),
    ));
    harness.settle(8);

    let windows = harness.app().windows();
    assert_eq!(windows.len(), 2, "the second window opened");
    // Outputs are numbered by the compositor that creates them, so two windows on one compositor
    // draw into two outputs of different numbers.
    assert_ne!(
        output(&windows[0]),
        output(&windows[1]),
        "the windows draw into outputs of one number, so each has a compositor of its own"
    );
    let (first, second) = (tiles(&windows[0]), tiles(&windows[1]));
    assert_eq!(
        first.len(),
        "shared".len(),
        "the first window drew its line"
    );
    assert_eq!(
        first, second,
        "the windows drew one line from different tiles, so they do not share the atlas"
    );
}
