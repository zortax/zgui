//! Whether a dialog's letters land on whole device pixels.
//!
//! A dialog is centred by `left: 50%; top: 50%` and a transform that pulls it back by half of
//! itself. A panel of odd width or height is then pulled back by a half pixel. Glyph tiles are
//! rasterised on the pixel grid and sampled texel for texel, so a tile drawn a half pixel off that
//! grid samples its texels on their boundaries: rows and columns repeat or drop, and the letters
//! come out distorted or with a row cut off. The fixture sizes the panel odd in both directions and
//! asserts that every letter on it is drawn at a whole pixel.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::view;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{HEIGHT, SETTLED, Stage, WIDTH};
use crate::painted::words::{aim, assert_painted};

/// The page, with a panel sized odd in both directions.
const SHEET: &str = ":root {
                         background-color: #ffffff;
                         color: #101010;
                         font-family: sans-serif;
                     }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }
                     box.zui-dialog { width: 401px; max-width: 401px; height: 201px }";

/// What the dialog's trigger says.
const TRIGGER: &str = "Open dialog";

/// What the dialog says.
const TITLE: &str = "Rename project";

/// The whole surface.
fn window() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(WIDTH), DevicePx(HEIGHT)),
    )
}

/// Where the window says the panel is: the largest box whose text starts with the title and that is
/// not the overlay band the size of the window.
fn panel(stage: &Stage) -> Rect<DevicePx, Device> {
    let whole = WIDTH * HEIGHT;
    let census = stage.census();
    let node = census
        .nodes
        .iter()
        .filter(|node| {
            node.text.starts_with(TITLE) && node.area() > 0.0 && node.area() < whole * 0.9
        })
        .max_by(|left, right| left.area().total_cmp(&right.area()))
        .expect("the dialog is in the document");
    node.rect.expect("a node with area has a box")
}

#[test]
fn a_settled_dialog_of_odd_size_draws_its_letters_on_whole_pixels() {
    let Some(mut stage) = Stage::open(SHEET, || {
        view! {
            ThemeProvider {
                column(class = "page") {
                    Dialog {
                        DialogTrigger {{TRIGGER}}
                        DialogContent {
                            DialogTitle {{TITLE}}
                            DialogDescription {"Give it a name the invoices can carry."}
                        }
                    }
                }
            }
        }
    }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };

    let at = aim(&stage, TRIGGER);
    stage.click(at);
    stage.wait(SETTLED);
    stage.repaint();
    assert_painted(&stage, TITLE);

    let panel = panel(&stage);
    assert_eq!(
        (panel.size.width.0, panel.size.height.0),
        (401.0, 201.0),
        "the panel is sized odd in both directions"
    );
    let letters: Vec<_> = stage
        .glyphs_in(window())
        .into_iter()
        .filter(|glyph| panel.contains(glyph.bounds.origin))
        .collect();
    assert!(
        !letters.is_empty(),
        "the panel at {panel:?} drew no letters"
    );
    for glyph in letters {
        let origin = glyph.bounds.origin;
        assert!(
            (origin.x.0 - origin.x.0.round()).abs() < 1e-3
                && (origin.y.0 - origin.y.0.round()).abs() < 1e-3,
            "a letter is drawn at ({}, {}), off the pixel grid",
            origin.x.0,
            origin.y.0
        );
    }
}
