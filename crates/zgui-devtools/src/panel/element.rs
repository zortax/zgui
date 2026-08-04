//! The picked element: what it is, what box it was given, and what the cascade computed for it.
//!
//! The box model is drawn as four nested boxes rather than as four rows of numbers, because the
//! question it answers is a spatial one — *where did this width come from* — and a reader who has to
//! subtract the padding from the border box in their head has been given a table, not a diagram.

use zgui::prelude::*;
use zgui::{component, view};

use crate::state::DevTools;

/// The element tab.
#[component]
pub(crate) fn ElementPanel(
    /// Where the picked element is published.
    tools: DevTools,
) -> impl IntoView {
    let element = tools.element;
    let picking = tools.picking;
    view! {
        column(
            class = "zgui-devtools__body zgui-devtools__split-detail",
            style:height = move || Some(format!("{:.0}px", tools.detail.get()))
        ) {
            if move || element.get().is_some() {
                text(class = "zgui-devtools__head") {
                    {move || element.get().map_or_else(String::new, |it| selector(&it))}
                }
                row(class = "zgui-devtools__row") {
                    text(class = "zgui-devtools__key") {"fragments"}
                    text(class = "zgui-devtools__value") {
                        {move || element.get().map_or(0, |it| it.fragments).to_string()}
                    }
                }
                text(class = "zgui-devtools__head") {"box model, device pixels"}
                column(class = "zgui-devtools__box zgui-devtools__box-border") {
                        text(class = "zgui-devtools__note zgui-devtools__note-border") {
                            {move || element.get().map_or_else(String::new, |it| extent("border", it.boxes.border))}
                        }
                    column(class = "zgui-devtools__box zgui-devtools__box-padding") {
                            text(class = "zgui-devtools__note zgui-devtools__note-padding") {
                                {move || element.get().map_or_else(String::new, |it| extent("padding", it.boxes.padding))}
                            }
                        column(class = "zgui-devtools__box zgui-devtools__box-content") {
                                text(class = "zgui-devtools__note zgui-devtools__note-content") {
                                    {move || element.get().map_or_else(String::new, |it| extent("content", it.boxes.content))}
                                }
                        }
                    }
                }
                text(class = "zgui-devtools__head") {"computed style"}
                text(class = "zgui-devtools__note") {
                    "the layout properties always, then everything that is not its initial value"
                }
                for row in move || element.get().map(|it| it.style).unwrap_or_default(),
                    key = |row: &crate::sample::Declaration| row.property.clone()
                {
                    row(class = "zgui-devtools__row") {
                        text(class = "zgui-devtools__key") {{row.property.clone()}}
                        // A colour is the one computed value a serialisation genuinely cannot
                        // convey: `rgb(122, 162, 247)` is four tokens and a number nobody pictures.
                        // Painted beside it, the same row answers "which blue" at a glance.
                        Show(when = {let has = row.swatch.is_some(); move || has}) {
                            box(
                                class = "zgui-devtools__swatch",
                                style:background-color = {row.swatch.clone()}
                            )
                        }
                        text(
                            class = "zgui-devtools__value",
                            class:zgui-devtools__value-quiet = !row.authored
                        ) {
                            {row.value.clone()}
                        }
                    }
                }
            } else {
                column(class = "zgui-devtools__row") {
                    text(class = "zgui-devtools__value-quiet") {
                        "Nothing picked. Press pick, or Ctrl+Shift+C, and move the pointer."
                    }
                }
            }
            if move || picking.get() {
                text(class = "zgui-devtools__note") {
                    "Picking. Move the pointer to aim, click to keep it, Escape to stop."
                }
            }
        }
    }
}

/// The element written the way a selector would name it.
fn selector(element: &crate::sample::Element) -> String {
    let mut out = element.name.clone();
    if let Some(id) = &element.id {
        out.push('#');
        out.push_str(id);
    }
    for class in &element.classes {
        out.push('.');
        out.push_str(class);
    }
    out
}

/// One box of the box model, as a line.
fn extent(name: &str, rect: zgui::geom::Rect<zgui::geom::DevicePx, zgui::geom::Device>) -> String {
    format!(
        "{name} {:.1} x {:.1} at {:.1}, {:.1}",
        rect.size.width.0, rect.size.height.0, rect.origin.x.0, rect.origin.y.0
    )
}
