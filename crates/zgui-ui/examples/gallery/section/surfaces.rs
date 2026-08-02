//! The surfaces that move: scroll areas, resizable panes and the carousel.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui::prelude::*;

use crate::shell::PanelProps;

/// Names for the scroll area to be too short for.
const NAMES: [&str; 24] = [
    "Ada",
    "Grace",
    "Barbara",
    "Katherine",
    "Margaret",
    "Radia",
    "Frances",
    "Jean",
    "Adele",
    "Mary",
    "Evelyn",
    "Betty",
    "Kathleen",
    "Marlyn",
    "Ruth",
    "Joan",
    "Sister",
    "Erna",
    "Klara",
    "Hedy",
    "Dorothy",
    "Annie",
    "Henrietta",
    "Williamina",
];

/// Everything with more content than room to show it.
#[component]
pub(crate) fn Surfaces() -> impl IntoView {
    view! {
        Panel(title = "Scroll area", note = "a long list in a short box, with its own bars") {
            ScrollArea(class = "tall") {
                column(class = "stack") {
                    for name in || NAMES, key = |name: &&str| *name {
                        text {{name}}
                    }
                }
            }
        }

        Panel(title = "Resizable", note = "a handle the pointer drags and the arrows move") {
            box(class = "tall wide") {
                ResizablePanelGroup(label = "Messages and reading pane") {
                    ResizablePanel(default_size = 35.0, min_size = 15.0) {
                        column(class = "stack") {text {"Inbox"}text {"Archive"}}
                    }
                    ResizableHandle(label = "Resize the message list")
                    ResizablePanel(default_size = 65.0) {
                        text {"The message, and what it says."}
                    }
                }
            }
        }

        Panel(title = "Carousel", note = "one slide at a time, stepped with the arrows") {
            // The arrows hang 48px outside the strip, the way shadcn's do, so the carousel needs
            // that much air on either side or the panel's edge cuts them off mid-disc.
            box(class = "carousel-frame") {
                Carousel(label = "Photographs") {
                    CarouselContent {
                        CarouselItem {box(class = "frame") {text {"One"}}}
                        CarouselItem {box(class = "frame") {text {"Two"}}}
                        CarouselItem {box(class = "frame") {text {"Three"}}}
                    }
                    CarouselPrevious()
                    CarouselNext()
                }
            }
        }
    }
}
