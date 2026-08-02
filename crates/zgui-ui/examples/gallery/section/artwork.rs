//! A colour illustration beside a monochrome icon, so the difference between them is one picture.

use zgui::prelude::*;
use zgui::{component, css, view};

use crate::section::asset::{colour, mono};
use crate::shell::{PanelProps, RowProps};

/// What this section's own sheet is installed under.
const SHEET: &str = "gallery-artwork";

/// The sheet these panels are drawn by.
///
/// Nothing here says anything about the illustration's colours, because there is nothing to say:
/// the `color` on each swatch is what the icon beside it follows, and the illustration ignores it.
const CSS: &str = css!(
    ".scene-sm { width: 64px; height: 64px; }
    .scene-lg { width: 112px; height: 112px; }
    .icon-lg { width: 64px; height: 64px; }

    .swatch { align-items: center; gap: var(--zui-space-xs); }
    .pane {
        padding: var(--zui-space-md);
        border-radius: var(--zui-radius-md);
        gap: var(--zui-space-md);
        align-items: center;
    }
    .swatch-plain { color: var(--zui-color-foreground); background-color: var(--zui-color-muted); }
    .swatch-rose { color: #be123c; background-color: #ffe4e6; }
    .swatch-teal { color: #0f766e; background-color: #ccfbf1; }
    .swatch-night { color: #fde68a; background-color: #1e1b4b; }

    .caption { font-size: var(--zui-type-size-xs); color: var(--zui-color-muted-foreground); }"
);

/// The colour-illustration panels.
#[component]
pub(crate) fn Artwork() -> impl IntoView {
    install_stylesheet(SHEET, CSS);

    view! {
        Panel(
            title = "A palette of its own",
            wide = true,
            note = "an illustration and an icon side by side, under two different colours"
        ) {
            Row(label = "the pair, twice") {
                column(class = "swatch") {
                    row(class = "pane swatch-rose") {
                        vector(
                            class = "scene-lg",
                            prop:svg = colour::COTTAGE,
                            a11y:label = "A cottage above a river"
                        )
                        vector(class = "icon-lg", prop:svg = mono::STAR, a11y:label = "Star")
                    }
                    text(class = "caption") {"pair on rose"}
                }
                column(class = "swatch") {
                    row(class = "pane swatch-teal") {
                        vector(
                            class = "scene-lg",
                            prop:svg = colour::COTTAGE,
                            a11y:label = "A cottage above a river"
                        )
                        vector(class = "icon-lg", prop:svg = mono::STAR, a11y:label = "Star")
                    }
                    text(class = "caption") {"pair on teal"}
                }
            }
        }

        Panel(title = "Not tinted by its context", note = "four backgrounds, one unchanged picture") {
            Row(label = "scenes") {
                column(class = "swatch") {
                    row(class = "pane swatch-plain") {
                        vector(class = "scene-sm", prop:svg = colour::COTTAGE, a11y:label = "Scene")
                    }
                    text(class = "caption") {"scene on plain"}
                }
                column(class = "swatch") {
                    row(class = "pane swatch-rose") {
                        vector(class = "scene-sm", prop:svg = colour::COTTAGE, a11y:label = "Scene")
                    }
                    text(class = "caption") {"scene on rose"}
                }
                column(class = "swatch") {
                    row(class = "pane swatch-teal") {
                        vector(class = "scene-sm", prop:svg = colour::COTTAGE, a11y:label = "Scene")
                    }
                    text(class = "caption") {"scene on teal"}
                }
                column(class = "swatch") {
                    row(class = "pane swatch-night") {
                        vector(class = "scene-sm", prop:svg = colour::COTTAGE, a11y:label = "Scene")
                    }
                    text(class = "caption") {"scene on ink"}
                }
            }
        }
    }
}
