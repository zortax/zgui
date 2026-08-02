//! Lines of one repeated letter, for measuring where glyphs actually land.
//!
//! Repeating a single glyph is what makes a readback measurable without a shaper: every glyph has
//! the same outline and therefore the same ink offset from its own origin, so the distance between
//! two glyphs' ink is exactly the advance between them. A run of them at a size whose advance is
//! not a whole number of pixels is the case a per-glyph rounding gets wrong, and it gets it wrong
//! visibly: the positions stop being a straight line in the index.
//!
//! Several sizes, because an advance's fractional part is what the phase has to carry and one size
//! samples one fraction.

use zgui::prelude::*;

/// How many letters each line holds.
const RUN: usize = 40;

/// One line of `RUN` repetitions of `letter` at `size` device-independent pixels.
#[component]
pub(crate) fn Ruler(
    /// The class carrying the size.
    #[prop(into)]
    class: String,
    /// The letter to repeat.
    #[prop(into)]
    letter: String,
) -> impl IntoView {
    let line: String = letter.repeat(RUN);
    view! { text(class = format!("ruler {class}")) {{line}} }
}

/// Five rulers, each at a different size.
#[component]
pub(crate) fn Rulers() -> impl IntoView {
    view! {
        column(class = "page") {
            Ruler(class = "s21", letter = "H")
            Ruler(class = "s27", letter = "H")
            Ruler(class = "s33", letter = "H")
            Ruler(class = "s40", letter = "H")
            Ruler(class = "s47", letter = "H")
            Ruler(class = "s40", letter = "l")
            text(class = "ruler s40") {"The quick brown fox jumps over the lazy dog"}
        }
    }
}

/// White letters on black, with nothing else on the surface.
pub(crate) const SHEET: &str = css!(
    ":root {
        background-color: #000000;
        color: #ffffff;
        font-family: sans-serif;
    }

    .page { gap: 18px; padding: 24px; }

    .ruler {
        display: block;
        line-height: 1.6;
        letter-spacing: 0;
    }

    .s21 { font-size: 21px; }
    .s27 { font-size: 27px; }
    .s33 { font-size: 33px; }
    .s40 { font-size: 40px; }
    .s47 { font-size: 47px; }"
);
