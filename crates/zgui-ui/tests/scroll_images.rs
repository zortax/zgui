//! Album art in a virtual list, watched frame by frame across a scroll glide.
//!
//! The shape a music library scrolls in: hundreds of rows, each with a small picture, sharing a
//! few dozen sources between them. The virtualiser mounts and unmounts rows as the port moves, so
//! a glide exercises every image lifecycle edge at once — attach on mount, detach on unmount,
//! shared tiles, decode-for-display, and re-emission of rows whose replay records recycling
//! dropped.
//!
//! The assertion is about *flicker*: once a row's picture has been decoded and shown, no later
//! frame that draws the row may draw it without its picture. A frame that does is the blink a
//! person sees while the glide is in flight.

mod desktop;
mod device;
mod painted;

use std::path::PathBuf;
use std::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::reactive::RwSignal;
use zgui::view;
use zgui_ui::prelude::*;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #101010 }
    .list { width: 300px; height: 384px; }
    .row { flex-direction: row; align-items: center;
           background-color: #202028; }
    .row__art { width: 26px; height: 26px; border-radius: 3px; margin-left: 8px; }";

/// How many rows the list holds.
const ROWS: usize = 80;
/// How many distinct pictures they share, the way tracks share albums.
const ARTS: usize = 12;
/// The declared row height, matching the sheet.
const ROW: f32 = 32.0;

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// Writes the fixture pictures once and answers where they are.
///
/// Each is 300 texels square — a Spotify-sized source shown at 26 CSS pixels, so the loader's
/// decode-for-display and class selection are on the path, exactly as they are for real art.
fn fixtures() -> PathBuf {
    let dir = std::env::temp_dir().join("zgui-scroll-art-fixtures");
    std::fs::create_dir_all(&dir).expect("a temp dir can be made");
    for art in 0..ARTS {
        let path = dir.join(format!("art{art}.png"));
        if path.exists() {
            continue;
        }
        let tone = (art * 20) as u8;
        let picture = image::RgbaImage::from_fn(300, 300, |x, y| {
            let checker = ((x / 12) + (y / 12)) % 2 == 0;
            let base: u8 = if checker { 200 } else { 90 };
            image::Rgba([base.saturating_add(tone), tone, 255 - tone, 255])
        });
        picture.save(&path).expect("the fixture encodes");
    }
    dir
}

/// The scrollport, in device pixels.
fn port() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(300.0), DevicePx(384.0)),
    )
}

/// How many drawn rows sit fully inside the port, and how many of them carry their picture.
fn rows_and_arts(stage: &Stage) -> (usize, usize) {
    let port = port();
    let rows: Vec<Rect<DevicePx, Device>> = stage
        .quads_in(port)
        .into_iter()
        .map(|quad| quad.bounds)
        .filter(|bounds| {
            // A row background: as wide as the list, one declared row tall, and wholly inside
            // the port. A row straddling the port's edge is mid-arrival — its sliver of picture
            // is an edge-emission question older than this fixture, and not the flicker this
            // fixture exists to catch.
            bounds.size.width.0 > 250.0
                && (bounds.size.height.0 - ROW).abs() < 1.0
                && bounds.origin.y.0 >= -0.5
                && bounds.origin.y.0 + bounds.size.height.0 <= port.size.height.0 + 0.5
        })
        .collect();
    let arts: Vec<Rect<DevicePx, Device>> = stage
        .glyphs_in(port)
        .into_iter()
        .map(|glyph| glyph.bounds)
        .filter(|bounds| bounds.size.width.0 < 30.0)
        .collect();
    let with_art = rows
        .iter()
        .filter(|row| {
            arts.iter().any(|art| {
                art.origin.y.0 >= row.origin.y.0 - 1.0
                    && art.origin.y.0 + art.size.height.0
                        <= row.origin.y.0 + row.size.height.0 + 1.0
            })
        })
        .count();
    (rows.len(), with_art)
}

/// A warm glide never draws a row without the picture it has already shown.
#[test]
fn a_scroll_glide_keeps_every_warm_picture_on_its_row() {
    let dir = fixtures();
    let mut stage = staged!(move || {
        let dir = dir.clone();
        let count = RwSignal::new_local(ROWS);
        view! {
            VirtualList(
                count = count,
                row_size = ROW,
                overscan = 4,
                label = "Tracks",
                class = "list",
                row = move |index: usize| {
                    let src = dir
                        .join(format!("art{}.png", index % ARTS))
                        .to_string_lossy()
                        .into_owned();
                    view! {
                        box(class = "row") {
                            image(class = "row__art", src = Some(src))
                        }
                    }
                }
            )
        }
    });

    // Warm the whole list: glide to the bottom with a settle after each step, so every source is
    // probed, decoded and shown at least once.
    stage.move_to(Point::new(DevicePx(150.0), DevicePx(190.0)));
    for _ in 0..24 {
        stage.wheel(4.0);
    }
    stage.wait(Duration::from_millis(500));
    let (rows, with_art) = rows_and_arts(&stage);
    assert!(rows >= 10, "the port shows a screenful of rows: {rows}");
    assert_eq!(
        with_art, rows,
        "after settling at the bottom, every visible row carries its picture"
    );

    // The glide under test: back to the top, frame by frame, over rows whose pictures are all
    // warm. Record every frame that drew a row without its art.
    let mut flickers: Vec<(usize, usize, usize)> = Vec::new();
    for step in 0..120 {
        stage.wheel_step(-3.0);
        let (rows, with_art) = rows_and_arts(&stage);
        if with_art < rows {
            flickers.push((step, rows, with_art));
        }
    }
    assert!(
        flickers.is_empty(),
        "warm rows were drawn without their pictures mid-glide \
         (step, rows drawn, rows with art): {flickers:?}"
    );
}
