//! `<vector>`: what a drawing carries, where its colour comes from, and what happens to a
//! declaration this engine build has no property for.

mod support;

use std::sync::Arc;

use zgui_css::values::custom;
use zgui_elements::kurbo::{self, Rect, Shape};
use zgui_elements::vector::{self as vector_props};
use zgui_geom::CssPx;
use zgui_scene::Scene;
use zgui_style::{SheetOrigin, SheetSource, StyleEngine, Viewport};
use zgui_text::FixedMetrics;
use zgui_view::{Anchor, IntoView, NodeId, PropKey, View};

use crate::support::Window;

/// One colour's opaque eight-bit sRGB channels, which is what a golden compares.
fn srgb8(color: zgui_color::Color) -> [u8; 3] {
    let [r, g, b, _] = color.to_premultiplied_srgb();
    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ]
}

/// A styled window with one drawing under a themed container.
struct Drawing {
    window: Window,
    engine: StyleEngine,
    container: NodeId,
    mark: NodeId,
    /// Kept alive: dropping the built view would unmount the tree the tests are reading.
    #[allow(dead_code)]
    built: Box<dyn Anchor>,
    /// Kept alive: dropping a sheet's handle removes the sheet.
    sheets: Vec<zgui_style::SheetHandle>,
}

impl Drawing {
    /// Builds `<box><vector paths=… /></box>` and installs `sheet` as the application's.
    fn open(sheet: &str) -> Self {
        let window = Window::open();
        let square = Rect::new(0.0, 0.0, 16.0, 16.0).to_path(0.1);
        let triangle = Rect::new(2.0, 2.0, 6.0, 6.0).to_path(0.1);
        let view = zgui_elements::r#box().class("chart").child(
            zgui_elements::vector()
                .class("mark")
                .paths([square, triangle]),
        );

        let mut built = window
            .window
            .with(|| view.into_view().build(&mut window.cx.cx()));
        built.mount(&window.dom, window.root, None);
        let container = built.node();
        let mark = window.backend.children(container)[0];

        let mut engine = {
            let document = window.document.borrow();
            StyleEngine::new(
                &document,
                Arc::new(FixedMetrics::new()),
                Viewport::new(CssPx(1280.0), CssPx(800.0)),
            )
        };
        let handle = {
            let document = window.document.borrow();
            let (handle, diagnostics) =
                engine.add_sheet(&document, SheetOrigin::Author, SheetSource::Text(sheet));
            assert!(
                diagnostics.is_empty(),
                "this fixture's sheet is meant to parse cleanly: {diagnostics:?}"
            );
            handle
        };
        let mut drawing = Self {
            window,
            engine,
            container,
            mark,
            built: Box::new(built),
            sheets: vec![handle],
        };
        drawing.restyle();
        drawing
    }

    /// Runs a restyle and reports how many elements it styled.
    fn restyle(&mut self) -> usize {
        let mut document = self.window.document.borrow_mut();
        self.engine.restyle(&mut document, None).styled
    }

    /// How the mark is painted, resolved the way the paint stage resolves it.
    fn paint(&self) -> zgui_paint::emit::vector::ShapePaint {
        let index = self.window.backend.index_of(self.mark);
        let document = self.window.document.borrow();
        let style = document
            .node(index)
            .primary_style()
            .expect("the mark was styled");
        zgui_paint::emit::vector::shape_paint(&style, 1.0)
    }

    /// The outlines the view put on the mark, read back the way the paint stage would.
    fn paths(&self) -> Vec<kurbo::BezPath> {
        match self
            .window
            .backend
            .property(self.mark, PropKey::new(vector_props::PATHS))
        {
            zgui_view::PropValue::Text(data) => vector_props::from_path_data(&data),
            other => panic!("a drawing carries its outlines as text, and this is {other:?}"),
        }
    }
}

#[test]
fn a_drawing_with_no_paint_of_its_own_is_filled_with_the_colour_it_inherits() {
    let drawing = Drawing::open(".chart { color: rgb(10, 20, 30) }");
    let paint = drawing.paint();
    assert_eq!(
        srgb8(paint.fill),
        [10, 20, 30],
        "the icon idiom is the default, not a keyword"
    );
    assert_eq!(paint.stroke, None);
    assert_eq!(paint.stroke_width, 1.0);
    drawing.window.window.unmount();
}

#[test]
fn the_fill_custom_property_on_an_ancestor_overrides_the_inherited_colour() {
    let drawing = Drawing::open(".chart { color: rgb(10, 20, 30); --zgui-fill: rgb(200, 0, 0) }");
    assert_eq!(srgb8(drawing.paint().fill), [200, 0, 0]);

    let index = drawing.window.backend.index_of(drawing.mark);
    let document = drawing.window.document.borrow();
    let style = document.node(index).primary_style().expect("styled");
    assert_eq!(custom::text(&style, "zgui-fill"), Some("rgb(200, 0, 0)"));
    drop(document);
    drawing.window.window.unmount();
}

#[test]
fn a_stroke_appears_only_when_a_custom_property_asks_for_one() {
    let drawing = Drawing::open(
        ".mark { --zgui-stroke: rgb(0, 0, 255); --zgui-stroke-width: 3px; color: rgb(1, 2, 3) }",
    );
    let paint = drawing.paint();
    assert_eq!(paint.stroke.map(srgb8), Some([0, 0, 255]));
    assert_eq!(paint.stroke_width, 3.0);
    assert_eq!(srgb8(paint.fill), [1, 2, 3], "the fill is untouched");
    drawing.window.window.unmount();
}

/// Changing nothing but the theme's fill has to reach the paint stage, and the only thing that can
/// tell it to is the identity key. Without the key's custom-property field the mark keeps the
/// colour it had, on screen, with nothing to notice it by.
#[test]
fn changing_only_the_fill_repaints_the_mark() {
    let mut drawing = Drawing::open(".chart { --zgui-fill: rgb(200, 0, 0) }");
    let before = key_of(&drawing);
    assert_eq!(srgb8(drawing.paint().fill), [200, 0, 0]);

    {
        let document = drawing.window.document.borrow();
        let index = drawing.window.backend.index_of(drawing.container);
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_custom_property(
                    index,
                    zgui_interned::CustomPropertyName::new("zgui-fill"),
                    Some("rgb(0, 200, 0)"),
                );
            })
            .expect("not poisoned");
    }
    assert!(drawing.restyle() > 0, "the change reached the traversal");

    assert_eq!(srgb8(drawing.paint().fill), [0, 200, 0]);
    assert_ne!(
        key_of(&drawing),
        before,
        "the mark's painted appearance changed and its identity key did not say so"
    );
    drawing.window.window.unmount();
}

/// The mark's painted-appearance identity, as the frame compares it.
fn key_of(drawing: &Drawing) -> (zgui_css::StructPtr, zgui_css::StructPtr) {
    let index = drawing.window.backend.index_of(drawing.mark);
    let document = drawing.window.document.borrow();
    let style = document.node(index).primary_style().expect("styled");
    let (inherited, non_inherited) = zgui_css::StructPtr::custom_properties(&style);
    (inherited, non_inherited)
}

/// `vector { fill: red }` is not a rule this build can honour, and the report is the only way an
/// author finds out.
#[test]
fn the_svg_paint_properties_are_reported_as_absent_and_change_nothing() {
    let mut drawing = Drawing::open(".chart { color: rgb(9, 9, 9) }");
    let before = drawing.paint();

    let diagnostics = {
        let document = drawing.window.document.borrow();
        let (handle, diagnostics) = drawing.engine.add_sheet(
            &document,
            SheetOrigin::Author,
            SheetSource::Text("vector { fill: red; stroke: blue; stroke-width: 4px }"),
        );
        drawing.sheets.push(handle);
        diagnostics
    };
    assert_eq!(
        diagnostics.len(),
        3,
        "one report per declaration this build has no property for: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|report| report.message.contains("fill") || report.message.contains("stroke")),
        "{diagnostics:?}"
    );

    drawing.restyle();
    let after = drawing.paint();
    assert_eq!(srgb8(after.fill), srgb8(before.fill));
    assert_eq!(after.stroke, None);
    assert_eq!(after.stroke_width, 1.0);
    drawing.window.window.unmount();
}

#[test]
fn the_outlines_a_view_set_reach_a_renderer_as_the_list_it_wrote_in_the_colour_it_inherited() {
    let mut drawing = Drawing::open(".chart { color: rgb(0, 128, 255) }");
    let paths = drawing.paths();
    assert_eq!(paths.len(), 2, "two marks, not one path with two subpaths");

    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(64, 64));
    let paint = drawing.paint();
    let ink = zgui_geom::Rect::new(
        zgui_geom::Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(0.0)),
        zgui_geom::Size::new(zgui_geom::DevicePx(16.0), zgui_geom::DevicePx(16.0)),
    );
    let mut pushed = 0;
    for (index, path) in paths.into_iter().enumerate() {
        pushed += zgui_paint::emit::vector::emit(
            &mut scene,
            zgui_scene::VectorId(index as u32),
            Arc::new(path),
            ink,
            paint,
            zgui_paint::emit::vector::VectorPlacement {
                clip: zgui_scene::ClipId::ROOT,
                transform: zgui_scene::SpatialId::VIEWPORT,
                scale: 1.0,
            },
        );
    }
    assert_eq!(pushed, 2, "one filled item per outline");
    assert_eq!(scene.primitives.vectors.len(), 2);

    // Through a renderer, not read back out of the scene the test just filled: what a rasteriser is
    // handed is the display list, and the colour in it is the one the cascade resolved.
    scene.finish(&zgui_bits::DamageSet::full());
    let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
    zgui_render::Renderer::draw(&mut renderer, &scene, &zgui_bits::DamageSet::full());
    let transcript = renderer
        .transcript()
        .expect("a frame was drawn")
        .to_string();
    let shapes: Vec<&str> = transcript
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("vector order="))
        .collect();
    assert_eq!(
        shapes.len(),
        2,
        "the renderer was handed two shapes:\n{transcript}"
    );
    for shape in &shapes {
        assert!(
            shape.contains("fill=solid srgb(0, 0.502, 1, 1)"),
            "the shapes reached the renderer in the colour the drawing inherited:\n{transcript}"
        );
    }
    assert_ne!(
        shapes[0]
            .split_once(" d=")
            .expect("a shape carries its outline"),
        shapes[1]
            .split_once(" d=")
            .expect("a shape carries its outline"),
        "two marks became one outline drawn twice"
    );

    drawing.restyle();
    drawing.window.window.unmount();
}
