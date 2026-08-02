//! Whether each surface that floats above the page ever reaches the screen.
//!
//! # Why this is not the overlay fixtures beside it
//!
//! Every other assertion about an overlay in this package asks the *document* a question: is a node
//! mounted on the overlay band, does it carry `data-state="open"`, is it in the accessibility tree.
//! All of those are true of a window in which the surface is nowhere to be seen, and that is not a
//! hypothetical: a gallery driven by a real pointer found a tooltip and a dropdown menu that never
//! appeared at all while every headless assertion about them was green.
//!
//! So each fixture here opens a real window on a real graphics device, drives the gesture a person
//! makes with a pointer that has to be somewhere, moves the clock on the way an output moves it,
//! and then asks the pixels whether the surface's own words were painted. A surface that mounted
//! and was never drawn fails, and it fails saying which of the four stages it stopped at.

mod desktop;
mod device;
mod painted;

use core::time::Duration;

use zgui::geom::DevicePx;
use zgui::view;
use zgui::view::AnyView;
use zgui_ui::prelude::*;
use zgui_ui_primitives::{Align, Placement, Side};
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{SETTLED, Stage};
use crate::painted::words::{aim, assert_absent, assert_painted};

/// The page every fixture is laid out on.
///
/// White, roomy, and with the controls pushed away from the top-left corner, so that a surface
/// asked to go above or to the left of its trigger has somewhere to go and the collision handling
/// is not the thing under test.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 120px; gap: 40px; align-items: flex-start }
                     .region { width: 260px; height: 90px; border: 1px solid #d0d0d0 }
                     .card { padding: 40px; margin-left: 260px; border: 1px solid #d0d0d0 }
                     .row { padding: 24px; gap: 16px }";

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

// ---- the surfaces that open on a press ---------------------------------------------------------

#[test]
fn a_dropdown_menu_opens_on_a_press_and_its_items_are_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                DropdownMenu {
                    DropdownMenuTrigger(variant = ButtonVariant::Outline) {"Account"}
                    DropdownMenuContent {
                        MenuItem {"Settings"}
                        MenuItem {"Billing"}
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "Settings");

    let trigger = aim(&stage, "Account");
    stage.click(trigger);
    stage.wait(SETTLED);
    assert_painted(&stage, "Settings");
}

#[test]
fn a_popover_opens_on_a_press_and_its_contents_are_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Popover {
                    PopoverTrigger(variant = ButtonVariant::Outline) {"Size"}
                    PopoverContent(placement = Placement::new(Side::Bottom, Align::Start)) {
                        text {"Width"}
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "Width");

    stage.click(aim(&stage, "Size"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Width");
}

#[test]
fn a_select_opens_on_a_press_and_its_items_are_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Select {
                    SelectTrigger(a11y:label = "Currency") {
                        SelectValue(placeholder = "Choose one")
                    }
                    SelectContent {
                        SelectItem(value = "gbp") {"Pound sterling"}
                        SelectItem(value = "eur") {"Euro"}
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "Pound sterling");

    stage.click(aim(&stage, "Choose one"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Pound sterling");
}

#[test]
fn a_dialog_opens_on_a_press_and_its_title_is_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Dialog {
                    DialogTrigger {"Rename"}
                    DialogContent {
                        DialogHeader {DialogTitle {"Rename project"}}
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "Rename project");

    stage.click(aim(&stage, "Rename"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Rename project");
}

#[test]
fn an_alert_dialog_opens_on_a_press_and_its_title_is_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                AlertDialog {
                    AlertDialogTrigger(variant = ButtonVariant::Destructive) {"Delete"}
                    AlertDialogContent {
                        AlertDialogHeader {
                            AlertDialogTitle {"Delete this project?"}
                        }
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "Delete this project?");

    stage.click(aim(&stage, "Delete"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Delete this project?");
}

#[test]
fn a_sheet_opens_on_a_press_and_its_title_is_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Sheet {
                    SheetTrigger(variant = ButtonVariant::Outline) {"Details"}
                    SheetContent(side = SheetSide::Right) {
                        SheetHeader {SheetTitle {"Invoice 4471"}}
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "Invoice 4471");

    stage.click(aim(&stage, "Details"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Invoice 4471");
}

#[test]
fn a_drawer_opens_on_a_press_and_its_title_is_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Drawer {
                    DrawerTrigger(variant = ButtonVariant::Outline) {"Share"}
                    DrawerContent {
                        DrawerHeader {DrawerTitle {"Share this invoice"}}
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "Share this invoice");

    stage.click(aim(&stage, "Share"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Share this invoice");
}

#[test]
fn a_menubar_menu_opens_on_a_press_and_its_items_are_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Menubar(label = "Main menu") {
                    MenubarMenu(value = "file") {
                        MenubarTrigger {"File"}
                        MenubarContent {MenubarItem {"New window"}}
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "New window");

    stage.click(aim(&stage, "File"));
    stage.wait(SETTLED);
    assert_painted(&stage, "New window");
}

#[test]
fn a_navigation_menu_opens_on_a_press_and_its_links_are_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
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
            }
        }
    }));
    assert_absent(&stage, "Editor");

    stage.click(aim(&stage, "Products"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Editor");
}

#[test]
fn a_combobox_opens_when_its_field_is_asked_for_the_list_and_its_items_are_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Combobox {
                    ComboboxInput(placeholder = "Search countries", label = "Country")
                    ComboboxContent {
                        ComboboxItem(value = "gb") {"United Kingdom"}
                        ComboboxItem(value = "ie") {"Ireland"}
                    }
                }
            }
        }
    }));
    assert_absent(&stage, "United Kingdom");

    // A field says nothing, so there is no text to aim at. Tab reaches it — it is the only thing
    // on the page that takes focus — and what has focus is an element with a box, so the pointer
    // still goes to the middle of the actual field rather than to a guessed coordinate.
    stage.press_named(zgui::vocab::NamedKey::Tab);
    let field = stage.focused().expect("tab reached the field");
    stage.click(stage.centre_of(field));
    stage.wait(SETTLED);
    assert_absent(&stage, "United Kingdom");

    // A combobox is a field before it is a list: a press puts the caret in it, and what asks for
    // the list is asking for the list. That is the gesture, and it is the one measured here.
    stage.press_named(zgui::vocab::NamedKey::ArrowDown);
    stage.wait(SETTLED);
    assert_painted(&stage, "United Kingdom");
}

// ---- the surface that opens on the other button -------------------------------------------------

#[test]
fn a_context_menu_opens_on_a_right_click_and_its_items_are_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                ContextMenu {
                    ContextMenuTrigger {
                        box(class = "region") {text {"Right-click here."}}
                    }
                    ContextMenuContent {MenuItem {"Paste"}}
                }
            }
        }
    }));
    assert_absent(&stage, "Paste");

    let at = aim(&stage, "Right-click here.");
    stage.right_click(at);
    stage.wait(SETTLED);
    assert_painted(&stage, "Paste");

    // Where the pointer asked, not merely somewhere. A context menu is the one surface whose
    // anchor is a point rather than a control, and the whole of what it means is *here* — a menu
    // that opens in the far corner of the window has answered the wrong question.
    let item = stage
        .census()
        .control("Paste")
        .and_then(|seen| seen.rect)
        .expect("the item is on the window");
    let away = (item.origin.x.0 - at.x.0).hypot(item.origin.y.0 - at.y.0);
    assert!(
        away < 120.0,
        "the menu opened {away} pixels from the point the pointer asked at"
    );
}

// ---- the surfaces that open on a pointer that stays ---------------------------------------------

#[test]
fn a_tooltip_opens_when_the_pointer_stays_and_its_label_is_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Tooltip(delay = Duration::from_millis(300)) {
                    TooltipTrigger {
                        Button(size = ButtonSize::Icon, a11y:label = "Bold") {"B"}
                    }
                    TooltipContent {"Embolden"}
                }
            }
        }
    }));
    assert_absent(&stage, "Embolden");

    stage.move_to(aim(&stage, "B"));
    // Past the delay, and then long enough for the surface to have finished appearing.
    stage.wait(Duration::from_millis(600));
    stage.wait(SETTLED);
    assert_painted(&stage, "Embolden");
}

#[test]
fn a_hover_card_opens_when_the_pointer_stays_and_its_contents_are_painted() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                HoverCard(delay = Duration::from_millis(300)) {
                    HoverCardTrigger {text {"@ada"}}
                    HoverCardContent {text {"Joined December 1842"}}
                }
            }
        }
    }));
    assert_absent(&stage, "Joined December 1842");

    stage.move_to(aim(&stage, "@ada"));
    stage.wait(Duration::from_millis(600));
    stage.wait(SETTLED);
    assert_painted(&stage, "Joined December 1842");
}

/// A trigger several boxes deep, well away from the corner its parents start at.
///
/// The case a one-panel page cannot express, and the one every real application is: the trigger's
/// offset *inside its parent* is a handful of pixels while its place *in the window* is hundreds.
/// Anything that mistakes the first for the second still puts the surface exactly where it belongs
/// on a page with one control on it at the origin, and puts it in the wrong panel here.
#[test]
fn a_popover_whose_trigger_is_deep_in_the_page_opens_beside_that_trigger() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                box(class = "card") {
                    row(class = "row") {
                        text {"width"}
                        Popover {
                            PopoverTrigger(variant = ButtonVariant::Outline) {"Size"}
                            PopoverContent(placement = Placement::new(Side::Bottom, Align::Start)) {
                                text {"Width"}
                            }
                        }
                    }
                }
            }
        }
    }));
    let trigger = stage
        .census()
        .control("Size")
        .and_then(|seen| seen.rect)
        .expect("the trigger is laid out");
    assert!(
        trigger.origin.x.0 > 300.0 && trigger.origin.y.0 > 150.0,
        "the trigger is at {trigger:?}, which is near enough the corner that its place inside its \
         parent and its place in the window could be confused without this noticing"
    );

    stage.click(aim(&stage, "Size"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Width");

    let surface = stage
        .census()
        .control("Width")
        .and_then(|seen| seen.rect)
        .expect("the label is on the window");
    let across = (surface.origin.x.0 - trigger.origin.x.0).abs();
    let down = surface.origin.y.0 - (trigger.origin.y.0 + trigger.size.height.0);
    assert!(
        across < 60.0 && (-4.0..80.0).contains(&down),
        "the popover opened {across} pixels across and {down} pixels below the button that opened \
         it, rather than under it"
    );
}

// ---- the same surfaces, on a page that has been scrolled ---------------------------------------

/// A page taller than the window, with the overlay in the middle of it.
///
/// The shape of every real application and of the gallery, and the shape in which none of these
/// surfaces reached the screen: the overlay bands hang off the window's root, the root is what
/// scrolls, and a surface placed in the window's own pixels was then carried up the page by the
/// scroll along with everything else.
const SCROLLING: &str = ":root { background-color: #ffffff; color: #101010;
                                 font-family: sans-serif; overflow: auto }
                         .page { padding: 40px; gap: 40px; align-items: flex-start }
                         .tall { height: 1400px }";

/// Opens `view` on a page that scrolls, or reports the run skipped.
macro_rules! scrolled {
    ($view:expr) => {
        match Stage::open(SCROLLING, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// Scrolls until the control saying `text` is somewhere in the middle of the window.
///
/// # Panics
///
/// Panics when it never arrives, because a fixture that gave up quietly would go on to click at
/// wherever the control was last seen — which is off the window.
fn scroll_to(stage: &mut Stage, text: &str) {
    stage.move_to(zgui::geom::Point::new(DevicePx(600.0), DevicePx(300.0)));
    for _ in 0..60 {
        let top = stage
            .census()
            .control(text)
            .and_then(|seen| seen.rect)
            .map(|rect| rect.origin.y.0);
        match top {
            Some(top) if (80.0..420.0).contains(&top) => return,
            _ => stage.wheel(1.0),
        }
    }
    panic!("{text:?} never came into view, so there is nothing to aim at");
}

#[test]
fn a_dropdown_menu_on_a_scrolled_page_is_painted_beside_its_trigger() {
    let mut stage = scrolled!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                box(class = "tall") {text {"above"}}
                DropdownMenu {
                    DropdownMenuTrigger(variant = ButtonVariant::Outline) {"Account"}
                    DropdownMenuContent {MenuItem {"Settings"}}
                }
                box(class = "tall") {text {"below"}}
            }
        }
    }));
    scroll_to(&mut stage, "Account");
    let trigger = stage
        .census()
        .control("Account")
        .and_then(|seen| seen.rect)
        .expect("the trigger is on the window");

    stage.click(aim(&stage, "Account"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Settings");

    // Beside the trigger, not merely somewhere. A surface that opened at the top of the document
    // is painted nowhere near what opened it, and "it is on the screen" alone would not say so.
    let surface = stage
        .census()
        .control("Settings")
        .and_then(|seen| seen.rect)
        .expect("the surface is on the window");
    let gap = surface.origin.y.0 - (trigger.origin.y.0 + trigger.size.height.0);
    assert!(
        (-4.0..80.0).contains(&gap),
        "the menu opened {gap} pixels below the bottom of the button that opened it"
    );
}

#[test]
fn a_tooltip_on_a_scrolled_page_is_painted_beside_its_trigger() {
    let mut stage = scrolled!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                box(class = "tall") {text {"above"}}
                Tooltip(delay = Duration::from_millis(300)) {
                    TooltipTrigger {
                        Button(size = ButtonSize::Icon, a11y:label = "Bold") {"B"}
                    }
                    TooltipContent {"Embolden"}
                }
                box(class = "tall") {text {"below"}}
            }
        }
    }));
    scroll_to(&mut stage, "B");
    let trigger = stage
        .census()
        .control("B")
        .and_then(|seen| seen.rect)
        .expect("the trigger is on the window");

    stage.move_to(aim(&stage, "B"));
    stage.wait(Duration::from_millis(600));
    stage.wait(SETTLED);
    assert_painted(&stage, "Embolden");

    let surface = stage
        .census()
        .control("Embolden")
        .and_then(|seen| seen.rect)
        .expect("the label is on the window");
    let gap = trigger.origin.y.0 - (surface.origin.y.0 + surface.size.height.0);
    assert!(
        (-4.0..80.0).contains(&gap),
        "the tooltip was painted {gap} pixels above the top of the control it names"
    );
}

#[test]
fn a_popover_on_a_scrolled_page_is_painted_beside_its_trigger() {
    let mut stage = scrolled!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                box(class = "tall") {text {"above"}}
                Popover {
                    PopoverTrigger(variant = ButtonVariant::Outline) {"Size"}
                    PopoverContent(placement = Placement::new(Side::Bottom, Align::Start)) {
                        text {"Width"}
                    }
                }
                box(class = "tall") {text {"below"}}
            }
        }
    }));
    scroll_to(&mut stage, "Size");
    let trigger = stage
        .census()
        .control("Size")
        .and_then(|seen| seen.rect)
        .expect("the trigger is on the window");

    stage.click(aim(&stage, "Size"));
    stage.wait(SETTLED);
    assert_painted(&stage, "Width");

    let surface = stage
        .census()
        .control("Width")
        .and_then(|seen| seen.rect)
        .expect("the label is on the window");
    let gap = surface.origin.y.0 - (trigger.origin.y.0 + trigger.size.height.0);
    assert!(
        (-4.0..80.0).contains(&gap),
        "the popover opened {gap} pixels below the bottom of the button that opened it"
    );
}

/// A box pinned to the window, on the page that scrolls under it.
///
/// The floor every surface above stands on, asserted on its own so that a failure says *fixed
/// positioning is broken* rather than *the popover is broken*. Read off the pixels rather than off
/// the box, because a box that reports the right rectangle and is composed at the wrong offset is
/// exactly the shape this was.
#[test]
fn a_box_fixed_to_the_window_stays_where_it_is_when_the_page_scrolls_under_it() {
    const PINNED: &str = ":root { background-color: #ffffff; overflow: auto }
                          .tall { height: 2400px }
                          .pinned { position: fixed; left: 20px; top: 20px;
                                    width: 200px; height: 40px; background-color: #000000 }";
    let mut stage = match Stage::open(PINNED, || {
        AnyView::new(view! {
            column {
                box(class = "tall") {text {"page"}}
                box(class = "pinned")
            }
        })
    }) {
        Some(stage) => stage,
        None => {
            eprintln!("skipped: no usable graphics device");
            return;
        }
    };
    let on_it = zgui::geom::Point::new(DevicePx(40.0), DevicePx(30.0));
    stage.wait(SETTLED);
    let pinned = stage.colour_at(on_it);
    assert!(
        pinned.0 < 40 && pinned.1 < 40 && pinned.2 < 40,
        "the pinned box is not painted where it was put, so nothing below asserts anything"
    );

    stage.move_to(zgui::geom::Point::new(DevicePx(600.0), DevicePx(300.0)));
    for _ in 0..6 {
        stage.wheel(3.0);
    }
    stage.wait(SETTLED);
    assert_eq!(
        stage.colour_at(on_it),
        pinned,
        "the page scrolled and took the box fixed to the window with it"
    );
}

// ---- what a surface leaves behind ---------------------------------------------------------------

/// A tooltip is shown, the pointer leaves, and then something else is pressed.
///
/// The sequence a person performs without thinking, and the one nothing could reach before: a
/// tooltip that never opened never left anything behind either. What it leaves is a dismissable
/// layer, and a layer that outlives its surface is invisible, is on top of everything, and answers
/// every press in the window from then on — so the next control pressed does nothing at all, and
/// the window looks dead rather than looking wrong.
#[test]
fn a_tooltip_that_has_been_shown_and_dismissed_leaves_the_next_press_alone() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Tooltip(delay = Duration::from_millis(300)) {
                    TooltipTrigger {
                        Button(size = ButtonSize::Icon, a11y:label = "Bold") {"B"}
                    }
                    TooltipContent {"Embolden"}
                }
                Popover {
                    PopoverTrigger(variant = ButtonVariant::Outline) {"Size"}
                    PopoverContent(placement = Placement::new(Side::Bottom, Align::Start)) {
                        text {"Width"}
                    }
                }
            }
        }
    }));

    stage.move_to(aim(&stage, "B"));
    stage.wait(Duration::from_millis(600));
    assert_painted(&stage, "Embolden");

    // Away, and long enough for the closing delay and the exit to be over.
    stage.move_to(aim(&stage, "Size"));
    stage.wait(Duration::from_millis(900));
    assert_absent(&stage, "Embolden");

    stage.press_release();
    stage.wait(SETTLED);
    assert_painted(&stage, "Width");
}

// ---- the one that is not portalled at all -------------------------------------------------------

#[test]
fn a_command_list_paints_its_items_without_being_opened() {
    let stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Command {
                    CommandInput(placeholder = "Type a command…", label = "Command")
                    CommandList {
                        CommandGroup(label = "Invoices") {
                            CommandItem(value = "invoice.new", text = "New invoice") {
                                "New invoice"
                            }
                        }
                    }
                }
            }
        }
    }));
    assert_painted(&stage, "New invoice");
}
