//! The shipped component gallery, at every size this harness drives it in.
//!
//! The real gallery — the very source files `crates/zgui-ui/examples/gallery` ships, included
//! through `#[path]` at the crate root, because its own source names `crate::section` and
//! `crate::shell` — plus the one row that is not the gallery's: four
//! swatches whose only job is to be clicked. Nothing else here is a fixture written to be measured.

mod probe;
mod sheet;
mod sizes;

pub(crate) use crate::gallery::probe::*;
pub(crate) use crate::gallery::sheet::*;
pub(crate) use crate::gallery::sizes::*;

use crate::draw::renderer;
use zgui::prelude::*;
use zgui::runtime::Runtime;
use zgui::view::IntoView;
use zgui::view::{Anchor, BuildCx};

/// How wide the window opens, in CSS pixels. The gallery's own.
pub(crate) const WIDTH: f32 = 1600.0;

/// How tall it opens.
pub(crate) const HEIGHT: f32 = 1000.0;

/// Builds the runtime for one document size.
pub(crate) fn runtime(size: &str) -> Runtime {
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let app = zgui::runtime::App::new()
        .with_title("gallery-scale")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(sheet())
        .with_renderer(Box::new(renderer))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()));
    let built = match size {
        "s0" => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Gallery0() }.into_view().build(cx))
        }),
        "s1" => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Gallery1() }.into_view().build(cx))
        }),
        "s2" => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Gallery2() }.into_view().build(cx))
        }),
        "s4" => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Gallery4() }.into_view().build(cx))
        }),
        "s8" => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Gallery8() }.into_view().build(cx))
        }),
        "s13" => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Gallery13() }.into_view().build(cx))
        }),
        "s26" => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Gallery26() }.into_view().build(cx))
        }),
        other => panic!("unknown size {other}; one of {SIZES:?}"),
    };
    built.expect("the reactive runtime installs")
}
