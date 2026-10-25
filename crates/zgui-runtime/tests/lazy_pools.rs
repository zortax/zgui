//! When the style and layout pools start their workers.
//!
//! An application holds both pools from the start, but a pool starts its workers on first use
//! only. A small document never uses either pool, so its process holds no pool thread. A large
//! document starts both on its first frame.
//!
//! The pools are counted by their workers' thread names in `/proc/self/task`, so this file holds
//! one test: a second test in the same process could start a pool while the first counts.

mod support;

use zgui_platform_headless::Harness;
use zgui_runtime::{App, Runtime};
use zgui_view::{BuildCx, IntoView, View};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .row { display: block; height: 12px }";

/// How many running threads have a name that starts with `prefix`.
fn threads_named(prefix: &str) -> usize {
    std::fs::read_dir("/proc/self/task")
        .expect("the process lists its threads")
        .filter_map(|task| std::fs::read_to_string(task.ok()?.path().join("comm")).ok())
        .filter(|name| name.starts_with(prefix))
        .count()
}

/// A window over `rows` rows of distinct text, with both pools four workers wide.
fn window(rows: usize) -> Harness<Runtime> {
    let handler = App::new()
        .with_title("pools")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_style_threads(4)
        .with_layout_threads(4)
        .with_renderer(Box::new(support::capture))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| {
            std::sync::Arc::new(zgui_testkit_scene::MonoRaster::new())
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let mut list = zgui_elements::column().class("list");
            for row in 0..rows {
                list = list.child(
                    zgui_elements::column()
                        .class("row")
                        .child(format!("row {row}")),
                );
            }
            Box::new(list.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(64);
    harness
}

#[test]
fn a_pool_starts_on_the_first_document_large_enough_to_use_it() {
    if std::env::var_os("ZGUI_STYLE_THREADS").is_some()
        || std::env::var_os("ZGUI_LAYOUT_THREADS").is_some()
    {
        eprintln!("skipped: the environment sets the pool widths");
        return;
    }

    let small = window(3);
    assert!(
        small.app().windows()[0].document().borrow().len() < 64,
        "the small document is below the cascade pool's floor"
    );
    assert_eq!(
        threads_named("zgui-style-"),
        0,
        "a small document started the style pool"
    );
    assert_eq!(
        threads_named("zgui-layout-"),
        0,
        "a small document started the layout pool"
    );
    drop(small);

    let large = window(300);
    assert_eq!(
        threads_named("zgui-style-"),
        4,
        "the first broad restyle started the style pool"
    );
    assert_eq!(
        threads_named("zgui-layout-"),
        4,
        "the first pre-shape of 300 distinct paragraphs started the layout pool"
    );
    drop(large);
}
