//! What an ordinary interface asks the renderer for, stated as what it does not ask for.
//!
//! A path rasteriser is built by the first frame whose display list carries a vector item, it costs
//! roughly a third of a second and a hundred and seventy megabytes to build, and it is then held for
//! the life of the device. Whether that ever happens is not a property any one component declares:
//! it falls out of a size, a transform or a paint somewhere, and the frame it falls out of is
//! usually a frame in the middle of an interaction.
//!
//! So it is asserted here, over the whole library at once, on the harness with no graphics device —
//! because a claim about what a component library costs has to hold on every machine that builds
//! it, not only on the ones with a device to skip the test on.

mod desktop;

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_RIGHT;
use zgui_ui_icons::set::mark::CHECK;

use crate::desktop::renderer::{forget_vectors, vectors_drawn};
use crate::desktop::stage::Stage;

/// Room to lay things out in, and nothing that could hide anything.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

/// One of most things: text, a control, an icon, a turned grip and a panel that animates open.
#[component]
fn Everything() -> impl IntoView {
    view! {
        column(class = "page") {
            text {"The quick brown fox jumps over the lazy dog."}
            Button {"Continue"}
            Checkbox(default_checked = true)
            Icon(icon = CHECK, size = IconSize::Md)
            Icon(icon = CHEVRON_RIGHT, size = IconSize::Md)
            NavigationMenu(label = "Main") {
                NavigationMenuList {
                    NavigationMenuItem(value = "products") {
                        NavigationMenuTrigger {"Products"}
                        NavigationMenuContent {
                            NavigationMenuLink {"Editor"}
                        }
                    }
                }
            }
            ResizablePanelGroup(label = "Split") {
                ResizablePanel(default_size = 50.0, min_size = 20.0, label = "List") {
                    text {"Inbox"}
                }
                ResizableHandle(label = "Resize", step = 10.0)
                ResizablePanel(default_size = 50.0, min_size = 20.0, label = "Reading") {
                    text {"Message"}
                }
            }
        }
    }
}

/// The whole claim, in one fixture.
///
/// The chevron a menu turns over, the grip a splitter lays on its side and the panel that grows
/// into place were each, on their own, enough to build the rasteriser — and each of them is a shape
/// the atlas draws. Opening the menu is part of the fixture because the frames worth watching are
/// the ones in the middle of an interaction.
#[test]
fn a_stock_interface_never_asks_for_a_path_rasteriser() {
    forget_vectors();
    let mut stage = Stage::open(SHEET, || view! { Everything() });
    stage.settle();
    stage.click_saying("Products");
    stage.settle();
    stage.click_saying("Products");
    stage.settle();

    let (items, passes) = vectors_drawn();
    assert_eq!(
        items, 0,
        "an ordinary interface put {items} vector items in the display list; the first of them \
         builds a path rasteriser, at about a third of a second and a hundred and seventy megabytes"
    );
    assert_eq!(passes, 0);
}
