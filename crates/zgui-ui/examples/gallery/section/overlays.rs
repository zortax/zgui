//! The surfaces that float above the page or take it over.

use core::time::Duration;

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::status::ALERT_TRIANGLE;
use zgui_ui_primitives::{Align, Placement, Side};

use crate::shell::{PanelProps, RowProps};

/// Dialogs, sheets, drawers and the small floating surfaces.
#[component]
pub(crate) fn Overlays() -> impl IntoView {
    view! {
        Panel(title = "Dialog", note = "a modal, with a dropdown inside it that Escape closes first") {
            Row(label = "modal") {
                NestedDialog()
            }
            Row(label = "destructive") {
                AlertDialog {
                    AlertDialogTrigger(variant = ButtonVariant::Destructive) {
                        "Delete"
                    }
                    AlertDialogContent(size = AlertDialogSize::Sm) {
                        AlertDialogMedia {Icon(icon = ALERT_TRIANGLE, label = "")}
                        AlertDialogHeader {
                            AlertDialogTitle {"Delete this project?"}
                            AlertDialogDescription {
                                "Its history goes with it, and none of it can be recovered."
                            }
                        }
                        AlertDialogFooter {
                            AlertDialogCancel {"Keep it"}
                            AlertDialogAction(variant = ButtonVariant::Destructive) {
                                "Delete"
                            }
                        }
                    }
                }
            }
        }

        Panel(title = "Sheet and drawer", note = "in from an edge, and up from the bottom") {
            Row(label = "sheet") {
                Sheet {
                    SheetTrigger(variant = ButtonVariant::Outline, attr:data-testid = "sheet-trigger") {
                        "Details"
                    }
                    SheetContent(side = SheetSide::Right) {
                        SheetHeader {
                            SheetTitle {"Invoice 4471"}
                            SheetDescription {"Issued 3 March, due 17 March."}
                        }
                        text {"One seat, one month."}
                        SheetFooter {SheetClose {"Close"}}
                        SheetDismiss()
                    }
                }
            }
            Row(label = "drawer") {
                Drawer {
                    DrawerTrigger(variant = ButtonVariant::Outline) {"Share"}
                    DrawerContent {
                        DrawerHandle()
                        DrawerHeader {
                            DrawerTitle {"Share this invoice"}
                            DrawerDescription {
                                "Anyone with the link can read it."
                            }
                        }
                        DrawerFooter {DrawerClose {"Done"}}
                    }
                }
            }
        }

        Panel(title = "Popover, tooltip, hover card", note = "beside a control rather than over the page") {
            Row(label = "popover") {
                Popover {
                    PopoverTrigger(variant = ButtonVariant::Outline, attr:data-testid = "popover-trigger") {
                        "Size"
                    }
                    PopoverContent(placement = Placement::new(Side::Bottom, Align::Start)) {
                        PopoverHeader {
                            PopoverTitle {"Dimensions"}
                            PopoverDescription {"How large the frame is drawn."}
                        }
                        // The label and its field share a row the way shadcn's popover demo lays
                        // them out, with the field taking the larger share of it.
                        row(class = "pair") {
                            Label {"Width"}
                            Input(placeholder = "100%", label = "Width", style:width = "55%")
                        }
                        PopoverClose {"Done"}
                    }
                }
            }
            Row(label = "tooltip") {
                TooltipProvider(
                    delay = Duration::from_millis(300),
                    close_delay = Duration::from_millis(150),
                ) {
                    Tooltip {
                        TooltipTrigger {
                            Button(size = ButtonSize::Icon, a11y:label = "Bold") {"B"}
                        }
                        TooltipContent {"Bold"}
                    }
                    Tooltip {
                        TooltipTrigger {
                            Button(size = ButtonSize::Icon, a11y:label = "Italic") {"I"}
                        }
                        TooltipContent {"Italic" TooltipArrow()}
                    }
                }
            }
            Row(label = "hover card") {
                HoverCard {
                    HoverCardTrigger {text {"@ada"}}
                    HoverCardContent {
                        row(class = "pair") {
                            Avatar(label = "Ada Lovelace") {"AL"}
                            text {"Ada Lovelace"}
                        }
                        text {"Joined December 1842"}
                    }
                }
            }
        }
    }
}

/// A dialog with a select and a menu inside it.
///
/// This is the layering case worth looking at: Escape while the list is open closes the list and
/// leaves the dialog standing, and Escape again closes the dialog. The two surfaces are on
/// different bands of the same stack, so neither has to know about the other.
#[component]
fn NestedDialog() -> impl IntoView {
    let currency = RwSignal::new_local("gbp".to_owned());

    view! {
        Dialog {
            DialogTrigger(attr:data-testid = "dialog-trigger") {"Rename…"}
            DialogContent(attr:data-testid = "dialog-content") {
                DialogHeader {
                    DialogTitle {"Rename project"}
                    DialogDescription {
                        "Everyone on the team will see the new name."
                    }
                }
                Input(placeholder = "Project name", label = "Project name")
                Select(value = currency) {
                    SelectTrigger(attr:data-testid = "dialog-select", a11y:label = "Currency") {
                        SelectValue(placeholder = "Choose one")
                    }
                    SelectContent {
                        SelectItem(value = "gbp") {"Pound sterling"}
                        SelectItem(value = "eur") {"Euro"}
                        SelectItem(value = "usd") {"US dollar"}
                    }
                }
                DialogFooter {
                    DialogClose(variant = ButtonVariant::Outline) {"Cancel"}
                    Button {"Rename"}
                }
            }
        }
    }
}
