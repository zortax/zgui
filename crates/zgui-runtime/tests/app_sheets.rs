//! Several application stylesheets: each call adds one, and they cascade in call order.

mod support;

use zgui_platform_headless::Harness;
use zgui_runtime::App;
use zgui_view::{BuildCx, IntoView, View};

/// The width and height of every laid-out box in the first window.
fn sizes(harness: &Harness<zgui_runtime::Runtime>) -> Vec<(f32, f32)> {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    layout
        .keys()
        .into_iter()
        .filter_map(|key| layout.layout_of(key))
        .map(|resolved| {
            let size = resolved.border_box().size;
            (size.width.0, size.height.0)
        })
        .collect()
}

#[test]
fn every_application_sheet_applies_and_the_later_one_wins() {
    let handler = App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(
            "root { display: block } .panel { display: block; height: 60px; width: 120px }",
        )
        .with_stylesheet(".panel { width: 250px }")
        .with_compositor(support::compositor())
        .with_renderer(Box::new(support::screen))
        .into_handler(|cx: &mut BuildCx<'_>| {
            let view = zgui_elements::r#box().class("panel").into_view();
            Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(8);

    let laid_out = sizes(&harness);
    // The height comes from the first sheet only; the width of the second sheet wins.
    assert!(
        laid_out.contains(&(250.0, 60.0)),
        "no box is 250x60, so a sheet did not apply in call order: {laid_out:?}"
    );
}
