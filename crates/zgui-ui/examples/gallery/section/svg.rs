//! Vector documents: one asset in several colours, one asset at several sizes, and one that brings
//! a ramp and a clip of its own.

use zgui::prelude::*;
use zgui::{component, css, view};
use zgui_ui::prelude::*;

use crate::section::asset::mono;
use crate::shell::{PanelProps, RowProps};

/// What this section's own sheet is installed under.
const SHEET: &str = "gallery-svg";

/// The sheet these panels are drawn by.
///
/// A `vector` has no size of its own — a document is a shape, not a number of pixels — so every
/// rule here says how large its box is, and the drawing is fitted into whatever that turns out to
/// be. The tinted boxes set nothing but `color`, which is the whole of how one asset takes three.
const CSS: &str = css!(
    ".mark-sm { width: 28px; height: 28px; }
    .mark-md { width: 48px; height: 48px; }
    .mark-lg { width: 72px; height: 72px; }

    .tint {
        padding: var(--zui-space-sm);
        border-radius: var(--zui-radius-md);
        align-items: center;
        gap: var(--zui-space-sm);
    }
    .tint-plain { color: var(--zui-color-foreground); }
    .tint-rose { color: #be123c; background-color: #ffe4e6; }
    .tint-teal { color: #0f766e; background-color: #ccfbf1; }
    .tint-night {
        color: #fde68a;
        background-color: #1e1b4b;
    }

    .fit-square { width: 96px; height: 96px; }
    .fit-wide { width: 144px; height: 48px; }
    .fit-tall { width: 48px; height: 120px; }
    .fit-frame {
        border: 1px dashed var(--zui-color-border);
        border-radius: var(--zui-radius-md);
    }

    .swatch { align-items: center; gap: var(--zui-space-xs); }
    .caption { font-size: var(--zui-type-size-xs); color: var(--zui-color-muted-foreground); }

    .facet-sm { width: 56px; height: 56px; }
    .facet-lg { width: 112px; height: 112px; }"
);

/// The vector panels.
#[component]
pub(crate) fn Svg() -> impl IntoView {
    install_stylesheet(SHEET, CSS);

    view! {
        Panel(
            title = "One asset, four colours",
            note = "the same source, following the colour of what holds it"
        ) {
            Row(label = "contexts") {
                column(class = "swatch") {
                    row(class = "tint tint-plain") {
                        vector(class = "mark-md", prop:svg = mono::STAR, a11y:label = "Star")
                    }
                    text(class = "caption") {"star on plain"}
                }
                column(class = "swatch") {
                    row(class = "tint tint-rose") {
                        vector(class = "mark-md", prop:svg = mono::STAR, a11y:label = "Star")
                    }
                    text(class = "caption") {"star on rose"}
                }
                column(class = "swatch") {
                    row(class = "tint tint-teal") {
                        vector(class = "mark-md", prop:svg = mono::STAR, a11y:label = "Star")
                    }
                    text(class = "caption") {"star on teal"}
                }
                column(class = "swatch") {
                    row(class = "tint tint-night") {
                        vector(class = "mark-md", prop:svg = mono::STAR, a11y:label = "Star")
                    }
                    text(class = "caption") {"star on ink"}
                }
            }
            Row(label = "inside a control") {
                Button {
                    vector(class = "mark-sm", prop:svg = mono::STAR, a11y:hidden = true)
                    "Favourite"
                }
                Button(variant = ButtonVariant::Outline) {
                    vector(class = "mark-sm", prop:svg = mono::STAR, a11y:hidden = true)
                    "Outline"
                }
            }
            Row(label = "sizes") {
                vector(class = "mark-sm", prop:svg = mono::STAR, a11y:label = "Small star")
                vector(class = "mark-md", prop:svg = mono::STAR, a11y:label = "Medium star")
                vector(class = "mark-lg", prop:svg = mono::STAR, a11y:label = "Large star")
            }
        }

        Panel(title = "Fitting a view box", note = "one wide drawing in three shapes of box") {
            Row(label = "box shapes") {
                column(class = "swatch") {
                    box(class = "fit-frame fit-square") {
                        vector(class = "fit-square", prop:svg = mono::BANNER, a11y:label = "Banner")
                    }
                    text(class = "caption") {"banner in a square"}
                }
                column(class = "swatch") {
                    box(class = "fit-frame fit-wide") {
                        vector(class = "fit-wide", prop:svg = mono::BANNER, a11y:label = "Banner")
                    }
                    text(class = "caption") {"banner in a wide box"}
                }
                column(class = "swatch") {
                    box(class = "fit-frame fit-tall") {
                        vector(class = "fit-tall", prop:svg = mono::BANNER, a11y:label = "Banner")
                    }
                    text(class = "caption") {"banner in a tall box"}
                }
            }
            Row(label = "preserveAspectRatio") {
                column(class = "swatch") {
                    box(class = "fit-frame fit-wide") {
                        vector(
                            class = "fit-wide",
                            prop:svg = mono::framed("xMidYMid meet"),
                            a11y:label = "Meet"
                        )
                    }
                    text(class = "caption") {"aspect xMidYMid meet"}
                }
                column(class = "swatch") {
                    box(class = "fit-frame fit-wide") {
                        vector(class = "fit-wide", prop:svg = mono::framed("none"), a11y:label = "None")
                    }
                    text(class = "caption") {"aspect none"}
                }
                column(class = "swatch") {
                    box(class = "fit-frame fit-wide") {
                        vector(
                            class = "fit-wide",
                            prop:svg = mono::framed("xMinYMid meet"),
                            a11y:label = "Left"
                        )
                    }
                    text(class = "caption") {"aspect xMinYMid meet"}
                }
            }
        }

        Panel(title = "A ramp and a clip", note = "colours the document brings, through an outline") {
            Row(label = "two sizes") {
                vector(class = "facet-sm", prop:svg = mono::FACET, a11y:label = "Facet, small")
                vector(class = "facet-lg", prop:svg = mono::FACET, a11y:label = "Facet, large")
            }
            Row(label = "in a tinted context") {
                column(class = "swatch") {
                    row(class = "tint tint-rose") {
                        vector(class = "facet-sm", prop:svg = mono::FACET, a11y:label = "Facet on rose")
                    }
                    text(class = "caption") {"facet on rose"}
                }
            }
        }
    }
}
