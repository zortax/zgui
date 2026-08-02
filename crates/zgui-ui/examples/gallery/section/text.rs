//! Text a glyph atlas cannot serve, and text that is turned and still takes a caret.
//!
//! Everything here is an ordinary element with ordinary declarations on it. A run leaves the atlas
//! and is drawn as filled curves when its transform is not a translation, when it is larger than
//! any cache should hold, or when its brush is a ramp rather than one colour — none of which is a
//! keyword anybody writes. The last panel is the one worth reading: the text in it is rotated, and
//! clicking it still puts the caret in front of the letter under the pointer.

use zgui::prelude::*;
use zgui::{component, css, view};
use zgui_ui::prelude::*;

use crate::shell::{PanelProps, RowProps};

/// What this section's own sheet is installed under.
const SHEET: &str = "gallery-styled-text";

/// The sheet these panels are drawn by.
///
/// A transform applies to a box, and `text` is an inline element, so every turned run here is
/// `display: inline-block` first. Without that the declaration is not wrong — it simply does not
/// apply, and the panel would show upright text under a rule that reads as though it should not.
const CSS: &str = css!(
    /* No rule around a cell: what is worth measuring inside one is the ink of the run, and a
    border would be ink as well. */
    ".turn-item { align-items: center; gap: var(--zui-space-xs); }
    .turn-label { font-size: var(--zui-type-size-xs); color: var(--zui-color-muted-foreground); }
    .turn-cell {
        width: 104px;
        height: 104px;
        align-items: center;
        justify-content: center;
    }
    .turned {
        display: inline-block;
        font-size: 30px;
        font-weight: 700;
    }
    .turn-0 { transform: rotate(0deg); }
    .turn-30 { transform: rotate(30deg); }
    .turn-60 { transform: rotate(60deg); }
    .turn-90 { transform: rotate(90deg); }
    .turn-135 { transform: rotate(135deg); }

    .slanted { transform: skewX(-28deg); }
    .stretched-x { transform: scale(2.2, 1); }
    .stretched-y { transform: scale(1, 2.2); }

    .tilt-frame {
        height: 230px;
        width: 100%;
        align-items: center;
        justify-content: center;
    }
    .tilt-card { transform: rotate(-9deg); width: 280px; }

    .display {
        display: inline-block;
        font-size: 116px;
        font-weight: 800;
        line-height: 1.25;
    }
    /* The ramp is declared on the element that holds the letters, because `background-image` does
       not inherit — a ramp on the box around it would have nothing to hand over. The custom
       property does inherit, and it is what moves the ramp off the box and onto the text. */
    .ramp-heading {
        display: inline-block;
        --zgui-text-fill: background;
        background-image: linear-gradient(90deg, #f43f5e, #f59e0b, #22c55e, #6366f1);
        font-size: 54px;
        font-weight: 800;
    }

    .tracked-tight { letter-spacing: -0.06em; }
    .tracked-wide { letter-spacing: 0.32em; }
    .worded { word-spacing: 1.4em; }
    .rule-solid { text-decoration: underline; }
    .rule-double { text-decoration: underline double; }
    .rule-wavy { text-decoration: underline wavy #f43f5e; }
    .rule-through { text-decoration: line-through; }
    .rule-over { text-decoration: overline; }
    .rule-both { text-decoration: underline line-through; }
    .spaced-row { font-size: 20px; }

    .caret-frame {
        height: 190px;
        width: 100%;
        align-items: center;
        justify-content: center;
    }
    .caret-text {
        width: 300px;
        padding: var(--zui-space-sm);
        border: 1px solid var(--zui-color-border);
        border-radius: var(--zui-radius-md);
        background-color: var(--zui-color-background);
        font-size: 18px;
        line-height: 1.6;
    }
    .caret-turned { transform: rotate(-14deg); }
    .caret-upright { transform: rotate(0deg); }"
);

/// Turned, skewed, stretched, oversized, ramped, spaced and underlined text.
#[component]
pub(crate) fn StyledText() -> impl IntoView {
    install_stylesheet(SHEET, CSS);

    view! {
        Panel(title = "Turned type", note = "one run at five angles, and skewed and stretched") {
            Row(label = "rotation") {
                column(class = "turn-item") {
                    row(class = "turn-cell") {text(class = "turned turn-0") {"Hl"}}
                    text(class = "turn-label") {"turn 0"}
                }
                column(class = "turn-item") {
                    row(class = "turn-cell") {text(class = "turned turn-30") {"Hl"}}
                    text(class = "turn-label") {"turn 30"}
                }
                column(class = "turn-item") {
                    row(class = "turn-cell") {text(class = "turned turn-60") {"Hl"}}
                    text(class = "turn-label") {"turn 60"}
                }
                column(class = "turn-item") {
                    row(class = "turn-cell") {text(class = "turned turn-90") {"Hl"}}
                    text(class = "turn-label") {"turn 90"}
                }
                column(class = "turn-item") {
                    row(class = "turn-cell") {text(class = "turned turn-135") {"Hl"}}
                    text(class = "turn-label") {"turn 135"}
                }
            }
            Row(label = "skew and non-uniform scale") {
                column(class = "turn-item") {
                    row(class = "turn-cell") {text(class = "turned slanted") {"Ag"}}
                    text(class = "turn-label") {"skew"}
                }
                column(class = "turn-item") {
                    row(class = "turn-cell") {text(class = "turned stretched-x") {"Ag"}}
                    text(class = "turn-label") {"scale wide"}
                }
                column(class = "turn-item") {
                    row(class = "turn-cell") {text(class = "turned stretched-y") {"Ag"}}
                    text(class = "turn-label") {"scale tall"}
                }
            }
        }

        Panel(title = "Text on a turned card", note = "a whole component rotated, text and all") {
            row(class = "tilt-frame") {
                Card(class = "tilt-card") {
                    CardHeader {
                        CardTitle {"Nine degrees over"}
                        CardDescription {
                            "Every run inside a rotated box is turned with it."
                        }
                    }
                    CardContent {
                        row(class = "pair") {
                            Badge {"Outlined"}
                            Badge(variant = BadgeVariant::Secondary) {"Not tiled"}
                        }
                    }
                }
            }
        }

        Panel(title = "Display and gradient", note = "too large for a cache, and filled with a ramp") {
            Row(label = "116 px") {
                text(class = "display") {"Ag"}
            }
            Row(label = "ramp") {
                text(class = "ramp-heading") {"Ramped"}
            }
        }

        Panel(title = "Spacing", note = "letter-spacing and word-spacing, against the same run") {
            Row(label = "normal") {
                text(class = "spaced-row") {"Tracking and words"}
            }
            Row(label = "letter-spacing -0.06em") {
                text(class = "spaced-row tracked-tight") {"Tracking and words"}
            }
            Row(label = "letter-spacing 0.32em") {
                text(class = "spaced-row tracked-wide") {"Tracking and words"}
            }
            Row(label = "word-spacing 1.4em") {
                text(class = "spaced-row worded") {"Tracking and words"}
            }
        }

        Panel(title = "Decoration", note = "solid, double, wavy, through, over, and two at once") {
            Row(label = "underline") {
                text(class = "spaced-row rule-solid") {"Solid"}
                text(class = "spaced-row rule-double") {"Double"}
                text(class = "spaced-row rule-wavy") {"Wavy"}
            }
            Row(label = "through and over") {
                text(class = "spaced-row rule-through") {"Line through"}
                text(class = "spaced-row rule-over") {"Overline"}
                text(class = "spaced-row rule-both") {"Both at once"}
            }
        }

        TurnedField()
    }
}

/// Selectable text, turned, beside the same text upright.
///
/// Both are editable elements, so the framework places the caret where the pointer landed and
/// drags a selection out of the run. The turned one is the interesting half: the rectangle layout
/// recorded for it is the upright one, and a pointer arrives in the window's coordinates, so
/// answering the click means running the pointer backwards through the rotation first. Getting that
/// wrong is invisible — the text draws correctly, reports correctly and reads correctly, and the
/// caret simply lands in front of the wrong letter.
#[component]
fn TurnedField() -> impl IntoView {
    view! {
        Panel(
            title = "Turned and still selectable",
            wide = true,
            note = "click either one: the caret goes in front of the character under the pointer"
        ) {
            row(class = "caret-frame") {
                editor(class = "caret-text caret-upright", tabindex = {Focus::Sequential}) {
                    "Upright, and you can put the caret anywhere in this sentence."
                }
                editor(class = "caret-text caret-turned", tabindex = {Focus::Sequential}) {
                    "Turned fourteen degrees, and the caret still lands where you clicked."
                }
            }
        }
    }
}
