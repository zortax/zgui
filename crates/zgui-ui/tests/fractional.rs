//! What a display whose scale is not a whole number does to a row that fits.
//!
//! A box is very often sized to hold exactly one line — a word between two buttons, an option in a
//! list — and its width *is* that line's max-content width plus its own padding. Handing the line
//! back the space inside the box means adding those insets and subtracting them again, which in
//! binary floating point does not always return what went in. At scale 1.0 and 1.5 the numbers are
//! exact and nothing is lost; at 1.2 they are not, and a shortfall of a millionth of a pixel used to
//! break a row that fits into two lines.
//!
//! Both fixtures are the panels it was reported from, mounted from the gallery's own source, so what
//! is measured cannot drift away from what is shipped. The claim is about heights: a box holding one
//! line is about a line tall, and one that has broken is about two.

#[path = "../examples/gallery/section/mod.rs"]
#[allow(
    dead_code,
    unused_imports,
    reason = "the gallery's sections are one module; these panels are the ones a scale broke"
)]
mod section;
#[path = "../examples/gallery/shell.rs"]
#[allow(
    dead_code,
    reason = "the shell is one module; this uses its sheet and nothing else"
)]
mod shell;

mod desktop;

use zgui::view;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::desktop::stage::Stage;

/// The scales a page is checked at: two that are exact in binary, and the one this machine runs.
const SCALES: [f64; 3] = [1.0, 1.2, 1.5];

/// How tall the line saying exactly `text` is.
///
/// The *smallest* box that says it, which is the text's own: the element around it is a control
/// with padding, and a control is taller than its line whether or not the line has broken.
fn height_of(stage: &Stage, text: &str) -> f32 {
    stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == text)
        .filter_map(|node| node.rect)
        .map(|rect| rect.size.height.0)
        .filter(|height| *height > 0.0)
        .fold(f32::INFINITY, f32::min)
}

/// How tall the *lowest* line saying exactly `text` is.
///
/// An open select says its chosen option twice — once on the control and once in the list under it
/// — and it is the row in the list that is being asked about. Measuring the smallest box would
/// measure the control's copy, which is not in a box that was sized to hold it.
fn lowest_line_of(stage: &Stage, text: &str) -> Option<f32> {
    stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == text)
        .filter_map(|node| node.rect)
        .filter(|rect| rect.size.height.0 > 0.0)
        .max_by(|left, right| left.origin.y.0.total_cmp(&right.origin.y.0))
        .map(|rect| rect.size.height.0)
}

/// One line of the size the library sets these in, in device pixels at `scale`.
fn one_line(scale: f64) -> f32 {
    // The small leading, which is what both of these are set in.
    20.0 * scale as f32
}

#[test]
fn a_word_between_two_buttons_stays_on_one_line_at_every_scale() {
    use crate::section::*;
    let mut stage = Stage::open(crate::shell::SHEET, || {
        view! {
            ThemeProvider {
                Toaster {
                    column(class = "page") { box(class = "grid") { Composites() } }
                }
            }
        }
    });
    stage.settle();

    for scale in SCALES {
        stage.present_at(scale);
        stage.settle();
        let height = height_of(&stage, "12 selected");
        assert!(
            height.is_finite(),
            "the button group's word is not laid out at scale {scale}"
        );
        assert!(
            height < one_line(scale) * 1.6,
            "at scale {scale} the word between two buttons is {height} tall, which is two lines: \
             the box was sized to hold it and then told it did not fit"
        );
    }
}

#[test]
fn an_option_in_an_open_select_stays_on_one_line_at_every_scale() {
    use crate::section::*;
    let mut stage = Stage::open(crate::shell::SHEET, || {
        view! {
            ThemeProvider {
                Toaster {
                    column(class = "page") { box(class = "grid") { Menus() } }
                }
            }
        }
    });
    stage.settle();

    for scale in SCALES {
        stage.present_at(scale);
        stage.settle();
        // The control shows the chosen option's own words, so pressing it opens the list and the
        // same words appear a second time, in the row that is being measured.
        stage.click_saying("Pound sterling");
        stage.settle();
        let height = lowest_line_of(&stage, "Pound sterling")
            .unwrap_or_else(|| panic!("the select's option is not laid out at scale {scale}"));
        assert!(
            height < one_line(scale) * 1.6,
            "at scale {scale} the option in the open list is {height} tall, which is two lines"
        );
        stage.key(zgui::vocab::NamedKey::Escape);
        stage.settle();
    }
}
