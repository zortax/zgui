//! Whether the components that only hold things put what they hold on the screen.
//!
//! # The report this reproduces
//!
//! A card drew as an empty frame: the border, the fill and the shadow were all there, at the size
//! the tokens give a card with a header, a body and a footer in it, and nothing inside it was on
//! the screen. An alert did the same. What was under it: both were built on the `surface` element,
//! which is replaced content — the box a producer on the graphics device fills — so layout sized
//! each box from a producer that was never given and never reached the children at all.
//!
//! Every assertion below every other fixture in this package passed while that was true. The
//! pieces were in the document, they carried their classes, the accessibility tree read the title
//! and the description, and the view-level harness places boxes itself rather than laying them
//! out. So the reading here is taken through the whole engine and off the pixels: a card whose
//! contents are laid out nowhere reports [`Reached::Unplaced`], and one whose contents are laid
//! out and painted under something else reports [`Reached::Unpainted`].
//!
//! [`Reached::Unplaced`]: crate::painted::words::Reached::Unplaced
//! [`Reached::Unpainted`]: crate::painted::words::Reached::Unpainted

mod desktop;
mod device;
mod painted;

use zgui::view;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{SETTLED, Stage};
use crate::painted::words::assert_painted;

/// The page the fixtures are laid out on.
///
/// The column is given a width because a card takes its parent's, and a page that aligned its
/// items to the start would give each one only as much as its longest line.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 32px; gap: 32px; width: 560px }";

/// Opens the fixture, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(mut stage) => {
                stage.wait(SETTLED);
                stage
            }
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

#[test]
fn a_card_draws_every_piece_it_was_given() {
    let stage = staged!(|| view! {
        ThemeProvider {
            column(class = "page") {
                Card {
                    CardHeader {
                        CardTitle {"March"}
                        CardDescription {"Due on the 28th"}
                    }
                    CardContent {text {"Forty two pounds"}}
                    CardFooter {Button {"Pay"}}
                }
            }
        }
    });

    for words in ["March", "Due on the 28th", "Forty two pounds", "Pay"] {
        assert_painted(&stage, words);
    }
}

#[test]
fn an_alert_draws_its_title_and_its_description() {
    let stage = staged!(|| view! {
        ThemeProvider {
            column(class = "page") {
                Alert(variant = AlertVariant::Destructive) {
                    AlertTitle {"Your card expires this month"}
                    AlertDescription {"Update it before the next invoice."}
                }
            }
        }
    });

    for words in [
        "Your card expires this month",
        "Update it before the next invoice.",
    ] {
        assert_painted(&stage, words);
    }
}
