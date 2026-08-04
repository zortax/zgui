//! The composites, driven: menus opened, trails walked, panels folded and slides stepped through.

mod harness;

use zgui::prelude::*;
use zgui::view;
use zgui::vocab::{AriaCurrent, Key, Modifiers, NamedKey, SemanticFlags, SharedString};
use zgui_ui::prelude::*;

use crate::harness::Harness;

/// Every element under the root carrying `class`, in tree order.
fn all_with(harness: &Harness, class: &str) -> Vec<NodeId> {
    let name = zgui::view::ClassName::new(class);
    harness
        .all()
        .into_iter()
        .filter(|node| harness.window.dom.tree().classes(*node).contains(&name))
        .collect()
}

/// What a node is publishing for one custom property.
fn custom(harness: &Harness, node: NodeId, property: &str) -> Option<String> {
    harness
        .window
        .dom
        .tree()
        .custom_property(node, zgui::view::CustomPropertyName::new(property))
}

/// Tells `node` that focus arrived on it, exactly as a window does after the keyboard moved it.
fn focus_in(harness: &Harness, node: NodeId) {
    harness.window.dispatcher().send_to(
        node,
        zgui::vocab::EventKind::FocusIn,
        zgui::vocab::Payload::Focus(zgui::vocab::FocusEvent::new(
            zgui::vocab::FocusCause::Keyboard,
        )),
    );
    harness.window.frame();
}

/// The focus moves a transcript recorded, without the handler lines around them.
fn focus_moves(harness: &Harness) -> Vec<String> {
    harness
        .window
        .transcript
        .to_string()
        .lines()
        .filter(|line| line.starts_with("focus "))
        .map(str::to_owned)
        .collect()
}

// ---- menubar ----------------------------------------------------------------------------------

/// A bar of two menus, and the handles of its two names.
fn menubar(harness: &Harness) -> Vec<NodeId> {
    harness.mount(|| {
        view! {
            Menubar(label = "Main") {
                MenubarMenu(value = "file") {
                    MenubarTrigger {"File"}
                    MenubarContent {
                        MenubarItem(shortcut = "Ctrl+N") {"New"}
                        MenubarSeparator()
                        MenubarItem {"Quit"}
                    }
                }
                MenubarMenu(value = "edit") {
                    MenubarTrigger {"Edit"}
                    MenubarContent {MenubarItem {"Undo"}}
                }
            }
        }
    });
    harness.window.host.set_tree_order(harness.all());
    let triggers = all_with(harness, "zui-menubar__trigger");
    assert_eq!(triggers.len(), 2);
    triggers
}

#[test]
fn a_menu_name_says_it_opens_a_menu_and_whether_it_is_open() {
    let harness = Harness::open();
    let triggers = menubar(&harness);

    assert_eq!(harness.semantics(triggers[0]).role, Role::MenuItem);
    assert_eq!(
        harness.semantics(triggers[0]).has_popup,
        Some(zgui::vocab::HasPopup::Menu)
    );
    assert_eq!(harness.semantics(triggers[0]).expanded, Some(false));

    harness.click(triggers[0]);
    assert_eq!(harness.semantics(triggers[0]).expanded, Some(true));
    assert_eq!(
        harness.attribute(triggers[0], "data-state").as_deref(),
        Some("open")
    );
}

#[test]
fn the_surface_a_menu_opens_exists_only_while_it_is_open() {
    let harness = Harness::open();
    let triggers = menubar(&harness);
    assert!(
        all_with(&harness, "zui-menubar__content").is_empty(),
        "a shut menu is costing nothing"
    );

    harness.click(triggers[0]);
    let surfaces = all_with(&harness, "zui-menubar__content");
    assert_eq!(surfaces.len(), 1);
    assert_eq!(harness.semantics(surfaces[0]).role, Role::Menu);
    assert_eq!(
        harness.semantics(surfaces[0]).relations.popup_for,
        Some(zgui::vocab::NodeId(triggers[0].as_u64()))
    );
}

#[test]
fn arrowing_along_a_bar_that_already_has_a_menu_open_opens_the_next_one() {
    // And arrowing along a shut bar opens nothing: a survey, not a series of menus flashing past.
    let harness = Harness::open();
    let triggers = menubar(&harness);

    focus_in(&harness, triggers[1]);
    assert_eq!(
        harness.semantics(triggers[1]).expanded,
        Some(false),
        "walking a shut bar opened a menu"
    );

    harness.click(triggers[0]);
    focus_in(&harness, triggers[1]);
    assert_eq!(harness.semantics(triggers[1]).expanded, Some(true));
    assert_eq!(
        harness.semantics(triggers[0]).expanded,
        Some(false),
        "two menus of one bar were open at once"
    );
}

#[test]
fn the_down_arrow_opens_the_menu_the_bar_is_on() {
    let harness = Harness::open();
    let triggers = menubar(&harness);

    harness.press(triggers[0], NamedKey::ArrowDown);
    assert_eq!(harness.semantics(triggers[0]).expanded, Some(true));
}

#[test]
fn choosing_an_item_closes_the_menu_and_brings_the_keyboard_back() {
    let harness = Harness::open();
    let triggers = menubar(&harness);
    harness.click(triggers[0]);
    let items = all_with(&harness, "zui-menubar__item");
    assert_eq!(items.len(), 2);
    harness.window.transcript.clear();

    harness.click(items[0]);

    assert_eq!(harness.semantics(triggers[0]).expanded, Some(false));
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[0].as_u64())],
        "a menu is a detour, and the keyboard has to come back from it"
    );
}

#[test]
fn a_menu_item_announces_the_keystroke_that_does_the_same_thing() {
    let harness = Harness::open();
    let triggers = menubar(&harness);
    harness.click(triggers[0]);
    let items = all_with(&harness, "zui-menubar__item");

    assert_eq!(
        harness.semantics(items[0]).keyboard_shortcut,
        Some(SharedString::from("Ctrl+N"))
    );
    let rule = harness.find("zui-menubar__separator");
    assert!(
        harness
            .semantics(rule)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "a rule between two runs of items is punctuation, not an item"
    );
}

#[test]
fn opening_a_menu_puts_the_keyboard_on_its_first_item() {
    // Without this the menu cannot be walked at all: the caret is still on the bar, so the next
    // arrow key moves along the names and the items below are unreachable from the keyboard.
    let harness = Harness::open();
    let triggers = menubar(&harness);
    harness.window.transcript.clear();

    harness.press(triggers[0], NamedKey::ArrowDown);
    harness.window.host.set_tree_order(harness.all());
    let items = all_with(&harness, "zui-menubar__item");
    assert_eq!(items.len(), 2, "the down arrow opened the menu");
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", items[0].as_u64())]
    );
}

#[test]
fn the_arrows_and_the_ends_walk_an_open_menu_and_escape_leaves_it() {
    let harness = Harness::open();
    let triggers = menubar(&harness);
    harness.click(triggers[0]);
    harness.window.host.set_tree_order(harness.all());
    let items = all_with(&harness, "zui-menubar__item");

    harness.window.transcript.clear();
    harness.press(items[0], NamedKey::End);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", items[1].as_u64())],
        "End did not reach the last item"
    );

    harness.window.transcript.clear();
    harness.press(items[1], NamedKey::ArrowUp);
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", items[0].as_u64())]
    );

    // Escape from *inside* the menu, which is where the keyboard actually is once it is open.
    harness.window.transcript.clear();
    harness.press(items[0], NamedKey::Escape);
    assert_eq!(harness.semantics(triggers[0]).expanded, Some(false));
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", triggers[0].as_u64())],
        "escape closed the menu and left the caret on an element that is gone"
    );
}

#[test]
fn typing_a_letter_in_an_open_menu_moves_to_the_item_beginning_with_it() {
    let harness = Harness::open();
    let triggers = menubar(&harness);
    harness.click(triggers[0]);
    harness.window.host.set_tree_order(harness.all());
    let items = all_with(&harness, "zui-menubar__item");
    harness.window.transcript.clear();

    harness.type_char(items[0], 'q');
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", items[1].as_u64())],
        "a menu of twenty items is unusable without this"
    );
}

// ---- navigation menu --------------------------------------------------------------------------

#[test]
fn a_navigation_section_expands_and_builds_its_panel_only_then() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
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
    });
    harness.window.host.set_tree_order(harness.all());
    let trigger = harness.find("zui-navigation-menu__trigger");
    let panel = harness.find("zui-navigation-menu__content");

    assert_eq!(harness.semantics(trigger).role, Role::Button);
    assert_eq!(harness.semantics(trigger).expanded, Some(false));
    assert_eq!(
        harness.semantics(trigger).relations.controls,
        [zgui::vocab::NodeId(panel.as_u64())]
    );
    assert_eq!(
        harness.window.dom.tree().text_content(panel),
        "",
        "a shut panel is costing nothing"
    );

    harness.click(trigger);
    assert_eq!(harness.semantics(trigger).expanded, Some(true));
    assert_eq!(harness.window.dom.tree().text_content(panel), "Editor");
    assert!(
        !harness
            .semantics(panel)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "an open panel is still hidden from a reader"
    );
}

#[test]
fn escape_shuts_an_open_section_and_puts_the_keyboard_back() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            NavigationMenu {
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
    });
    harness.window.host.set_tree_order(harness.all());
    let trigger = harness.find("zui-navigation-menu__trigger");
    harness.click(trigger);
    harness.window.transcript.clear();

    harness.press(trigger, NamedKey::Escape);
    assert_eq!(harness.semantics(trigger).expanded, Some(false));
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", trigger.as_u64())]
    );
}

#[test]
fn the_link_for_the_page_you_are_on_says_so() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            NavigationMenu {
                NavigationMenuList {
                    NavigationMenuItem(value = "pricing") {
                        NavigationMenuLink(active = true) {"Pricing"}
                    }
                }
            }
        }
    });
    let link = harness.find("zui-navigation-menu__link");

    assert_eq!(harness.semantics(link).current, Some(AriaCurrent::Page));
    assert_eq!(
        harness.attribute(link, "data-active").as_deref(),
        Some("true"),
        "a sheet has to be able to pick the same link out"
    );
}

#[test]
fn escape_from_inside_an_open_panel_shuts_it_too() {
    // Where the keyboard actually is once a section has been opened and tabbed into: on a link in
    // the panel. A listener on the trigger alone never sees that press, because the trigger and the
    // panel are siblings.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            NavigationMenu {
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
    });
    harness.window.host.set_tree_order(harness.all());
    let trigger = harness.find("zui-navigation-menu__trigger");
    harness.click(trigger);
    let link = harness.find("zui-navigation-menu__link");
    harness.window.transcript.clear();

    harness.press(link, NamedKey::Escape);
    assert_eq!(harness.semantics(trigger).expanded, Some(false));
    assert_eq!(
        focus_moves(&harness),
        [format!("focus #{}", trigger.as_u64())],
        "the panel the caret was in has gone, so the caret has to come back with it"
    );
}

// ---- breadcrumb -------------------------------------------------------------------------------

#[test]
fn a_trail_says_where_you_are_and_not_only_where_you_could_go() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Breadcrumb {
                BreadcrumbList {
                    BreadcrumbItem {BreadcrumbLink {"Home"}}
                    BreadcrumbSeparator()
                    BreadcrumbItem {BreadcrumbPage {"Billing"}}
                }
            }
        }
    });
    let trail = harness.only_child();
    let link = harness.find("zui-breadcrumb__link");
    let page = harness.find("zui-breadcrumb__page");
    let rule = harness.find("zui-breadcrumb__separator");

    assert_eq!(harness.semantics(trail).role, Role::Navigation);
    assert_eq!(
        harness.semantics(trail).label,
        Some(SharedString::from("Breadcrumb"))
    );
    assert_eq!(harness.semantics(link).current, None);
    assert_eq!(
        harness.semantics(page).current,
        Some(AriaCurrent::Page),
        "without this every crumb is announced the same way"
    );
    assert!(
        harness
            .semantics(page)
            .flags
            .contains(SemanticFlags::DISABLED),
        "a link to the page you are on is a link that does nothing"
    );
    assert!(
        harness
            .semantics(rule)
            .flags
            .contains(SemanticFlags::HIDDEN)
    );
}

// ---- pagination -------------------------------------------------------------------------------

#[test]
fn the_page_being_shown_is_the_one_the_pager_calls_current() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Pagination {
                PaginationContent {
                    PaginationItem {PaginationPrevious(disabled = true)}
                    PaginationItem {PaginationLink(page = 1, current = true) {"1"}}
                    PaginationItem {PaginationLink(page = 2) {"2"}}
                    PaginationItem {PaginationEllipsis()}
                    PaginationItem {PaginationNext()}
                }
            }
        }
    });
    let links = all_with(&harness, "zui-pagination__link");
    // Previous, 1, 2, next.
    assert_eq!(links.len(), 4);

    assert_eq!(harness.semantics(links[1]).current, Some(AriaCurrent::Page));
    assert_eq!(
        harness.semantics(links[1]).label,
        Some(SharedString::from("Page 1"))
    );
    assert_eq!(
        harness.semantics(links[2]).current,
        Some(AriaCurrent::False)
    );
    assert!(
        harness
            .semantics(links[0])
            .flags
            .contains(SemanticFlags::DISABLED),
        "the way back from the first page is disabled rather than taken away"
    );
    assert!(
        !harness
            .semantics(links[3])
            .flags
            .contains(SemanticFlags::DISABLED)
    );
}

#[test]
fn the_gap_in_a_pager_is_announced_rather_than_hidden() {
    // Pages were left out, and a reader who is not told hears a pager that skips from 2 to 17.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Pagination {PaginationContent {PaginationItem {
                PaginationEllipsis()
            }}}
        }
    });
    let gap = harness.find("zui-pagination__ellipsis");
    let icon = harness.children(gap)[0];
    assert_eq!(
        harness.semantics(icon).label,
        Some(SharedString::from("More pages"))
    );
    assert!(
        !harness
            .semantics(icon)
            .flags
            .contains(SemanticFlags::HIDDEN)
    );
}

// ---- sidebar ----------------------------------------------------------------------------------

#[test]
fn the_control_folds_the_panel_and_says_what_it_controls() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SidebarProvider {
                Sidebar(label = "Project") {SidebarContent {text {"Files"}}}
                SidebarInset {SidebarTrigger()}
            }
        }
    });
    let frame = harness.only_child();
    let panel = harness.find("zui-sidebar");
    let trigger = harness.find("zui-sidebar__trigger");

    assert_eq!(harness.semantics(panel).role, Role::Complementary);
    assert_eq!(
        harness.semantics(trigger).relations.controls,
        [zgui::vocab::NodeId(panel.as_u64())]
    );
    assert_eq!(harness.semantics(trigger).expanded, Some(true));
    assert_eq!(
        harness.attribute(frame, "data-state").as_deref(),
        Some("expanded")
    );

    harness.click(trigger);
    assert_eq!(
        harness.attribute(frame, "data-state").as_deref(),
        Some("collapsed")
    );
    assert_eq!(harness.semantics(trigger).expanded, Some(false));
    assert!(
        !harness
            .semantics(panel)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "a panel folded to icons still holds every entry, so it is still read out"
    );
}

#[test]
fn a_panel_folded_clean_off_the_page_is_taken_out_of_the_tree() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SidebarProvider(collapse = SidebarCollapse::Offcanvas) {
                Sidebar(label = "Project") {SidebarContent {text {"Files"}}}
                SidebarInset {SidebarTrigger()}
            }
        }
    });
    let panel = harness.find("zui-sidebar");
    let trigger = harness.find("zui-sidebar__trigger");

    assert!(
        !harness
            .semantics(panel)
            .flags
            .contains(SemanticFlags::HIDDEN)
    );
    harness.click(trigger);
    assert!(
        harness
            .semantics(panel)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "a panel that is no longer on the page is still read out"
    );
}

#[test]
fn the_rail_folds_the_panel_the_way_the_control_does() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SidebarProvider {
                Sidebar {SidebarContent {text {"Files"}} SidebarRail()}
                SidebarInset {text {"The document"}}
            }
        }
    });
    let frame = harness.only_child();
    let rail = harness.find("zui-sidebar__rail");

    harness.click(rail);
    assert_eq!(
        harness.attribute(frame, "data-state").as_deref(),
        Some("collapsed")
    );
    harness.click(rail);
    assert_eq!(
        harness.attribute(frame, "data-state").as_deref(),
        Some("expanded")
    );
}

#[test]
fn the_panel_tells_the_frame_what_shape_it_took() {
    // The frame's rules reach every part of the sidebar, so the shape has to be an attribute of
    // the frame however the caller happened to spell it.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SidebarProvider {
                Sidebar(side = SidebarSide::Right, variant = SidebarVariant::Floating) {
                    SidebarContent {text {"Files"}}
                }
                SidebarInset {text {"The document"}}
            }
        }
    });
    let frame = harness.only_child();

    assert_eq!(
        harness.attribute(frame, "data-side").as_deref(),
        Some("right")
    );
    assert_eq!(
        harness.attribute(frame, "data-variant").as_deref(),
        Some("floating")
    );
}

#[test]
fn an_entry_carrying_a_count_leaves_room_for_it() {
    // The count is placed against the entry rather than laid out in it, so nothing but the entry
    // itself can know that the label has to stop short.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SidebarProvider {Sidebar {SidebarContent {SidebarGroup {SidebarGroupContent {
                SidebarMenu {
                    SidebarMenuItem {SidebarMenuButton {"Drafts"}}
                    SidebarMenuItem {
                        SidebarMenuButton(size = SidebarMenuSize::Lg) {"Inbox"}
                        SidebarMenuBadge {"24"}
                    }
                }
            }}}}}
        }
    });
    let entries = all_with(&harness, "zui-sidebar__menu-item");
    let badge = harness.find("zui-sidebar__menu-badge");

    assert_eq!(harness.attribute(entries[0], "data-action"), None);
    assert_eq!(
        harness.attribute(entries[1], "data-action").as_deref(),
        Some("true")
    );
    assert_eq!(harness.attribute(badge, "data-size").as_deref(), Some("lg"));
}

#[test]
fn the_shortcut_folds_the_panel_from_anywhere_in_the_window() {
    // The listener is on the window's own root, not on anything the sidebar renders: a shortcut
    // that only worked while the sidebar had focus is a shortcut nobody can use to get to it.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SidebarProvider {
                Sidebar()
                SidebarInset {Button {"Somewhere else entirely"}}
            }
        }
    });
    let frame = harness.only_child();
    let elsewhere = harness.find("zui-button");

    harness
        .window
        .dispatcher()
        .with_modifiers(Modifiers::CONTROL)
        .key(elsewhere, Key::Character(SharedString::from("b")));
    harness.window.frame();

    assert_eq!(
        harness.attribute(frame, "data-state").as_deref(),
        Some("collapsed")
    );

    harness
        .window
        .dispatcher()
        .with_modifiers(Modifiers::CONTROL)
        .key(elsewhere, Key::Character(SharedString::from("b")));
    harness.window.frame();
    assert_eq!(
        harness.attribute(frame, "data-state").as_deref(),
        Some("expanded")
    );
}

#[test]
fn a_plain_b_is_left_alone_so_a_field_can_still_be_typed_into() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SidebarProvider {
                Sidebar()
                SidebarInset {Button {"Elsewhere"}}
            }
        }
    });
    let frame = harness.only_child();
    let elsewhere = harness.find("zui-button");

    harness
        .window
        .dispatcher()
        .key(elsewhere, Key::Character(SharedString::from("b")));
    harness.window.frame();

    assert_eq!(
        harness.attribute(frame, "data-state").as_deref(),
        Some("expanded")
    );
}

#[test]
fn the_place_the_sidebar_is_showing_is_the_current_one() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            SidebarProvider {Sidebar {SidebarContent {SidebarGroup {
                SidebarGroupLabel {"Views"}
                SidebarMenu {
                    SidebarMenuItem {SidebarMenuButton(active = true) {"Files"}}
                    SidebarMenuItem {SidebarMenuButton {"Search"}}
                }
            }}}}
        }
    });
    let buttons = all_with(&harness, "zui-sidebar__menu-button");
    let group = harness.find("zui-sidebar__group");
    let heading = harness.find("zui-sidebar__group-label");

    assert_eq!(
        harness.semantics(buttons[0]).current,
        Some(AriaCurrent::Page)
    );
    assert_eq!(
        harness.semantics(buttons[1]).current,
        Some(AriaCurrent::False)
    );
    assert_eq!(
        harness.semantics(group).relations.labelled_by,
        [zgui::vocab::NodeId(heading.as_u64())],
        "a group with a heading is named by it rather than being an anonymous box"
    );
}

// ---- carousel ---------------------------------------------------------------------------------

/// A carousel of three slides, and its two arrows.
fn carousel(harness: &Harness, wrap: bool) {
    harness.mount(move || {
        view! {
            Carousel(label = "Photographs", wrap = wrap) {
                CarouselContent {
                    CarouselItem {text {"One"}}
                    CarouselItem {text {"Two"}}
                    CarouselItem {text {"Three"}}
                }
                CarouselPrevious()
                CarouselNext()
            }
        }
    });
}

#[test]
fn stepping_a_carousel_moves_it_on_by_one_slide() {
    // What the *document* says, which is which slide is showing and no more than that. How far the
    // track actually moved is a question about geometry that no window without a layout can answer,
    // and asserting this and calling it a working carousel is exactly how an empty viewport once
    // shipped green — see `showing.rs`, which reads the slides off the screen.
    let harness = Harness::open();
    carousel(&harness, false);
    let track = harness.find("zui-carousel__track");
    let next = harness.find("zui-carousel__arrow--next");
    let slides = all_with(&harness, "zui-carousel__item");

    assert_eq!(
        custom(&harness, track, "zui-carousel-index").as_deref(),
        Some("0")
    );
    assert_eq!(
        harness.attribute(slides[0], "data-state").as_deref(),
        Some("active")
    );

    harness.click(next);
    assert_eq!(
        custom(&harness, track, "zui-carousel-index").as_deref(),
        Some("1")
    );
    assert_eq!(
        harness.attribute(slides[1], "data-state").as_deref(),
        Some("active"),
        "one press moves it on by one slide and not by more"
    );
    assert_eq!(
        harness.attribute(slides[0], "data-state").as_deref(),
        Some("inactive")
    );
}

#[test]
fn a_slide_says_which_of_how_many_it_is() {
    let harness = Harness::open();
    carousel(&harness, false);
    let slides = all_with(&harness, "zui-carousel__item");
    assert_eq!(slides.len(), 3);

    assert_eq!(
        harness.semantics(slides[1]).position.position_in_set,
        Some(2)
    );
    assert_eq!(harness.semantics(slides[1]).position.size_of_set, Some(3));
    assert_eq!(
        harness.semantics(slides[0]).role_description,
        Some(SharedString::from("slide"))
    );
}

#[test]
fn an_arrow_with_nowhere_to_go_says_so() {
    let harness = Harness::open();
    carousel(&harness, false);
    let previous = harness.find("zui-carousel__arrow--previous");
    let next = harness.find("zui-carousel__arrow--next");

    assert!(
        harness
            .semantics(previous)
            .flags
            .contains(SemanticFlags::DISABLED),
        "the first slide has nothing behind it"
    );
    assert!(
        !harness
            .semantics(next)
            .flags
            .contains(SemanticFlags::DISABLED)
    );

    harness.click(next);
    harness.click(next);
    assert!(
        harness
            .semantics(next)
            .flags
            .contains(SemanticFlags::DISABLED),
        "the last slide has nothing after it"
    );
    assert!(
        !harness
            .semantics(previous)
            .flags
            .contains(SemanticFlags::DISABLED)
    );
}

#[test]
fn a_wrapping_carousel_never_runs_out_of_either_end() {
    let harness = Harness::open();
    carousel(&harness, true);
    let track = harness.find("zui-carousel__track");
    let previous = harness.find("zui-carousel__arrow--previous");

    assert!(
        !harness
            .semantics(previous)
            .flags
            .contains(SemanticFlags::DISABLED)
    );
    harness.click(previous);
    assert_eq!(
        custom(&harness, track, "zui-carousel-index").as_deref(),
        Some("2")
    );
}

#[test]
fn the_arrow_keys_step_a_carousel_along_its_own_axis_only() {
    let harness = Harness::open();
    carousel(&harness, false);
    let root = harness.only_child();
    let track = harness.find("zui-carousel__track");

    harness.press(root, NamedKey::ArrowDown);
    assert_eq!(
        custom(&harness, track, "zui-carousel-index").as_deref(),
        Some("0"),
        "a horizontal carousel inside a scrolling page must leave the page's keys alone"
    );

    harness.press(root, NamedKey::ArrowRight);
    assert_eq!(
        custom(&harness, track, "zui-carousel-index").as_deref(),
        Some("1")
    );
}
