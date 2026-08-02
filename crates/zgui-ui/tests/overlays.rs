//! The surfaces that float above the window, driven through real frames.
//!
//! Every test here mounts a real component, presses real keys at real elements and asks the tree
//! and the host what changed. Nothing is hand-assembled, so a behaviour that stopped being wired
//! to anything fails here rather than passing quietly.

mod harness;

use core::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui::view::{AttrName, ClassName, Dom};
use zgui::vocab::{
    AnimationEvent, AnimationPhase, DefaultAction, EventKind, FocusCause, FocusEvent, HasPopup,
    Key, NamedKey, Payload, PointerButton, PointerEvent, SemanticFlags, Toggled, UiState,
};
use zgui::{component, view};
use zgui_ui::menu::RESET_AFTER;
use zgui_ui::prelude::*;

use crate::harness::Harness;

// ---- reaching the overlay bands --------------------------------------------------------------

/// Every element on one overlay band, in tree order.
fn overlay(harness: &Harness, layer: OverlayLayer) -> Vec<NodeId> {
    let root = harness.window.dom.overlay_root(harness.window.root, layer);
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        out.push(node);
        let mut children = harness.window.dom.tree().children(node);
        children.reverse();
        stack.extend(children);
    }
    out
}

/// Every element anywhere in the window carrying `class`, in tree order.
///
/// The overlay bands hang off the window's root, exactly as they do in a running window, so one
/// walk from the root reaches a dialog's surface as well as the button that opened it.
fn every(harness: &Harness, class: &str) -> Vec<NodeId> {
    let name = ClassName::new(class);
    harness
        .all()
        .into_iter()
        .filter(|node| harness.window.dom.tree().classes(*node).contains(&name))
        .collect()
}

/// The first element anywhere in the window carrying `class`, if any.
fn find(harness: &Harness, class: &str) -> Option<NodeId> {
    every(harness, class).into_iter().next()
}

/// The first element anywhere in the window carrying `class`.
///
/// # Panics
///
/// Panics naming the class, because every caller is about to assert something about the element
/// and `None` would make that assertion pass by never running.
fn expect(harness: &Harness, class: &str) -> NodeId {
    find(harness, class).unwrap_or_else(|| panic!("nothing in the window carries `{class}`"))
}

/// One attribute of an element.
fn attribute(harness: &Harness, node: NodeId, name: &str) -> Option<String> {
    harness
        .window
        .dom
        .tree()
        .attribute(node, AttrName::new(name))
}

/// Whether an element is the one sequential tab stop of its group.
fn is_tab_stop(harness: &Harness, node: NodeId) -> bool {
    attribute(harness, node, "tabindex").as_deref() == Some("0")
}

/// Runs the frame after the one an interaction caused, which is when a presence unmounts.
fn settle(harness: &Harness) {
    harness.window.advance(Duration::from_millis(1));
}

/// Presses the pointer down at a place in the window, which is what an outside press is.
fn press_at(harness: &Harness, x: f32, y: f32) {
    harness
        .window
        .dispatcher()
        .pointer_at(Point::new(DevicePx(x), DevicePx(y)), EventKind::PointerDown);
    harness.window.frame();
}

/// Sends one pointer event straight at an element, at a place in CSS pixels.
fn point_at(harness: &Harness, node: NodeId, kind: EventKind, x: f32, y: f32) {
    harness.window.dispatcher().send_to(
        node,
        kind,
        Payload::Pointer(PointerEvent::mouse(Point::new(
            zgui::geom::CssPx(x),
            zgui::geom::CssPx(y),
        ))),
    );
    harness.window.frame();
}

/// The identity an accessibility relation names an element by.
fn related(node: NodeId) -> zgui::vocab::NodeId {
    zgui::vocab::NodeId(node.as_u64())
}

// ---- dialog ------------------------------------------------------------------------------------

#[component]
fn ADialog() -> impl IntoView {
    view! {
        Dialog {
            DialogTrigger {"Rename…"}
            DialogContent {
                DialogHeader {
                    DialogTitle {"Rename project"}
                    DialogDescription {"Everyone on the team will see it."}
                }
                DialogFooter {DialogClose {"Cancel"}}
            }
        }
    }
}

#[test]
fn a_dialog_is_absent_until_it_opens_and_gone_again_after_it_closes() {
    // Not hidden, not empty: absent. A closed dialog whose subtree is still mounted is a subtree a
    // reader still meets and a layout still measures.
    let harness = Harness::open();
    harness.mount(|| view! { ADialog() });
    assert!(find(&harness, "zui-dialog").is_none());

    harness.click(harness.find("zui-button"));
    assert!(find(&harness, "zui-dialog").is_some());

    harness.press(expect(&harness, "zui-dialog"), NamedKey::Escape);
    settle(&harness);
    assert!(
        find(&harness, "zui-dialog").is_none(),
        "escape closed it and the exit finished"
    );
}

#[test]
fn a_dialog_is_a_modal_dialog_named_by_its_own_title_and_described_by_its_own_description() {
    let harness = Harness::open();
    harness.mount(|| view! { ADialog() });
    let trigger = harness.find("zui-button");
    harness.click(trigger);

    let surface = expect(&harness, "zui-dialog");
    let semantics = harness.semantics(surface);
    assert_eq!(semantics.role, Role::Dialog);
    assert!(semantics.flags.contains(SemanticFlags::MODAL));

    assert_eq!(
        semantics.relations.labelled_by,
        [related(expect(&harness, "zui-dialog__title"))],
        "the surface points at the title that was actually written"
    );
    assert_eq!(
        semantics.relations.described_by,
        [related(expect(&harness, "zui-dialog__description"))]
    );
    assert_eq!(
        semantics.relations.popup_for,
        Some(related(trigger)),
        "and back at the control it belongs to"
    );
}

#[test]
fn an_open_dialog_traps_focus_and_stops_the_window_scrolling() {
    let harness = Harness::open();
    harness.mount(|| view! { ADialog() });
    assert_eq!(harness.window.host.live_focus_traps(), 0);
    assert!(!harness.window.host.scrolling_frozen());

    harness.click(harness.find("zui-button"));
    assert_eq!(harness.window.host.live_focus_traps(), 1);
    assert!(
        harness.window.host.scrolling_frozen(),
        "the window is frozen while a dialog is open"
    );
    assert!(
        harness.window.host.stylesheet("zui-scroll-lock").is_none(),
        "the window is frozen rather than restyled, because restyling it moves the page"
    );

    harness.press(expect(&harness, "zui-dialog"), NamedKey::Escape);
    settle(&harness);
    assert_eq!(
        harness.window.host.live_focus_traps(),
        0,
        "the trap went with the dialog"
    );
    assert!(
        harness.window.host.stylesheet("zui-scroll-lock").is_none(),
        "and so did the lock"
    );
}

#[test]
fn a_dialog_leaving_waits_for_its_own_animation_rather_than_for_a_number() {
    // The surface a modal keeps mounted has to be the element the sheet animates, which is the
    // surface itself. Wired to anything else — a wrapper, a positioner, nothing at all — the exit
    // would be cut off on the frame it started, and no assertion about *when* it went would notice.
    let harness = Harness::open();
    harness.mount(|| view! { ADialog() });
    harness.click(harness.find("zui-button"));
    let surface = expect(&harness, "zui-dialog");

    harness.window.host.set_running_animations(surface, 1);
    harness.press(surface, NamedKey::Escape);
    settle(&harness);
    assert_eq!(
        attribute(&harness, surface, "data-state").as_deref(),
        Some("closed"),
        "the sheet is told it is leaving"
    );
    // Longer than any exit this library writes, and several times over, without anything ending
    // it: what the surface is waiting for is its own animation rather than a number.
    harness.window.advance(Duration::from_millis(500));
    assert!(
        find(&harness, "zui-dialog").is_some(),
        "and nothing guessed how long that takes"
    );

    harness.window.host.set_running_animations(surface, 0);
    harness.window.dispatcher().send_to(
        surface,
        EventKind::AnimationEnd,
        Payload::Animation(AnimationEvent {
            name: zgui::view::Ident::new("exit"),
            elapsed: Duration::from_millis(180),
            phase: AnimationPhase::Ended,
        }),
    );
    harness.window.frame();
    assert!(
        find(&harness, "zui-dialog").is_none(),
        "the animation ended, and only then was it taken away"
    );
}

#[test]
fn a_press_on_the_dimmed_window_closes_a_dialog_and_does_not_close_an_alert_dialog() {
    // The one behaviour that distinguishes the two, and the reason an alert dialog exists at all:
    // a stray click is not consent to delete something.
    for destructive in [false, true] {
        let harness = Harness::open();
        if destructive {
            harness.mount(|| {
                view! {
                    AlertDialog(default_open = true) {
                        AlertDialogContent {
                            AlertDialogTitle {"Sure?"}
                        }
                    }
                }
            });
        } else {
            harness.mount(|| {
                view! {
                    Dialog(default_open = true) {
                        DialogContent {DialogTitle {"Rename"}}
                    }
                }
            });
        }
        let surface = expect(&harness, "zui-dialog");
        harness.window.place(surface, 300.0, 300.0, 200.0, 100.0);

        press_at(&harness, 10.0, 10.0);
        settle(&harness);
        assert_eq!(
            find(&harness, "zui-dialog").is_none(),
            !destructive,
            "destructive={destructive}"
        );
    }
}

#[test]
fn an_alert_dialog_is_announced_as_one_and_still_answers_escape() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            AlertDialog(default_open = true) {
                AlertDialogContent {
                    AlertDialogTitle {"Delete this project?"}
                    AlertDialogFooter {
                        AlertDialogCancel {"Keep it"}
                        AlertDialogAction {"Delete"}
                    }
                }
            }
        }
    });
    let surface = expect(&harness, "zui-alert-dialog");
    assert_eq!(harness.semantics(surface).role, Role::AlertDialog);

    harness.press(surface, NamedKey::Escape);
    settle(&harness);
    assert!(
        find(&harness, "zui-alert-dialog").is_none(),
        "a keyboard user has to be able to leave what they opened by mistake"
    );
}

#[test]
fn a_control_inside_a_dialog_closes_it_without_anything_being_threaded_down_to_it() {
    let harness = Harness::open();
    harness.mount(|| view! { ADialog() });
    harness.click(harness.find("zui-button"));

    let cancel = every(&harness, "zui-button")
        .into_iter()
        .find(|node| harness.window.dom.tree().text_content(*node) == "Cancel")
        .expect("the footer's close button");
    harness.click(cancel);
    settle(&harness);
    assert!(find(&harness, "zui-dialog").is_none());
}

// ---- sheet and drawer ---------------------------------------------------------------------------

#[test]
fn a_sheet_says_which_edge_it_came_in_from_and_a_drawer_always_says_bottom() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            column {
                Sheet(default_open = true) {
                    SheetContent(side = SheetSide::Left) {
                        SheetTitle {"Navigation"}
                    }
                }
                Drawer(default_open = true) {
                    DrawerContent {DrawerTitle {"Share"}}
                }
            }
        }
    });

    let panels = every(&harness, "zui-sheet");
    assert_eq!(panels.len(), 2);
    let sides: Vec<Option<String>> = panels
        .iter()
        .map(|node| attribute(&harness, *node, "data-side"))
        .collect();
    assert!(sides.contains(&Some("left".to_owned())));
    assert!(sides.contains(&Some("bottom".to_owned())));

    let drawer = expect(&harness, "zui-drawer");
    assert_eq!(harness.semantics(drawer).role, Role::Dialog);
    let handle = expect(&harness, "zui-drawer__handle");
    assert!(
        harness
            .semantics(handle)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "the grab bar is a picture of an affordance, not something to announce"
    );
}

#[test]
fn two_modal_surfaces_hold_the_scroll_lock_between_them() {
    // The count is what stops the inner one closing from unlocking the window the outer one is
    // still open over.
    let harness = Harness::open();
    let inner = harness.window.scope.with(|| RwSignal::new_local(true));
    harness.mount(move || {
        view! {
            Dialog(default_open = true) {
                DialogContent {
                    DialogTitle {"Outer"}
                    Sheet(
                        open = inner,
                        on_open_change = UnsyncCallback::new(move |next: bool| inner.set(next))
                    ) {
                        SheetContent {SheetTitle {"Inner"}}
                    }
                }
            }
        }
    });
    assert!(harness.window.host.scrolling_frozen());

    inner.set(false);
    settle(&harness);
    assert!(
        harness.window.host.scrolling_frozen(),
        "the outer dialog is still open"
    );
}

#[test]
fn two_modal_surfaces_written_side_by_side_hold_the_scroll_lock_between_them_too() {
    // Not the same case as the one above, and the one every application actually has: a dialog and
    // a sheet written next to each other rather than one inside the other. Nesting hides the
    // defect, because the inner surface finds the outer one's count on the way up its own scope
    // chain; siblings find nothing, and two counts writing one declaration mean the first to close
    // takes it away while the second is still open — the window scrolls behind a modal surface.
    let harness = Harness::open();
    let (dialog, sheet) = harness
        .window
        .scope
        .with(|| (RwSignal::new_local(true), RwSignal::new_local(true)));
    harness.mount(move || {
        view! {
            box {
                Dialog(
                    open = dialog,
                    on_open_change = UnsyncCallback::new(move |next: bool| dialog.set(next))
                ) {
                    DialogContent {DialogTitle {"First"}}
                }
                Sheet(
                    open = sheet,
                    on_open_change = UnsyncCallback::new(move |next: bool| sheet.set(next))
                ) {
                    SheetContent {SheetTitle {"Second"}}
                }
            }
        }
    });
    assert!(
        harness.window.host.scrolling_frozen(),
        "two open modal surfaces and the window was never frozen at all"
    );

    dialog.set(false);
    settle(&harness);
    assert!(
        harness.window.host.scrolling_frozen(),
        "the sheet beside it is still open, and the window is scrolling behind it"
    );

    sheet.set(false);
    settle(&harness);
    assert!(
        !harness.window.host.scrolling_frozen(),
        "nothing is open and the window is still frozen"
    );
}

// ---- popover, tooltip, hover card ---------------------------------------------------------------

#[test]
fn a_popover_opens_from_its_trigger_and_a_second_press_on_it_does_not_re_open_it() {
    // The press that dismisses and the click that toggles are one gesture, and a surface that
    // treated them separately would close and re-open in the same breath.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Popover {
                PopoverTrigger {"Size"}
                PopoverContent {text {"Width"}}
            }
        }
    });
    let trigger = harness.find("zui-button");
    harness.window.place(trigger, 0.0, 0.0, 80.0, 24.0);

    harness.click(trigger);
    assert!(find(&harness, "zui-popover").is_some());

    press_at(&harness, 10.0, 10.0);
    harness.click(trigger);
    settle(&harness);
    assert!(
        find(&harness, "zui-popover").is_none(),
        "the trigger's own press did not dismiss it and then re-open it"
    );
}

#[test]
fn a_popover_trigger_says_what_it_opens_and_whether_it_is_showing() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Popover {
                PopoverTrigger {"Size"}
                PopoverContent {text {"Width"}}
            }
        }
    });
    let trigger = harness.find("zui-button");
    assert_eq!(
        attribute(&harness, trigger, "data-state").as_deref(),
        Some("closed")
    );
    assert_eq!(harness.semantics(trigger).expanded, Some(false));

    harness.click(trigger);
    assert_eq!(
        attribute(&harness, trigger, "data-state").as_deref(),
        Some("open")
    );
    let semantics = harness.semantics(trigger);
    assert_eq!(semantics.expanded, Some(true));
    assert_eq!(
        semantics.relations.controls,
        [related(expect(&harness, "zui-popover"))]
    );
}

#[test]
fn a_tooltip_costs_one_timer_and_appears_only_once_the_delay_has_run_out() {
    // The delay is the whole point: without it, dragging a pointer across a toolbar raises and
    // drops one tooltip per button on the way past.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Tooltip(delay = Duration::from_millis(700), close_delay = Duration::ZERO) {
                TooltipTrigger {Button {"B"}}
                TooltipContent {"Bold"}
            }
        }
    });
    let trigger = harness.find("zui-tooltip__trigger");

    assert_eq!(harness.window.host.live_timers(), 0);
    point_at(&harness, trigger, EventKind::PointerEnter, 0.0, 0.0);
    assert_eq!(
        harness.window.host.live_timers(),
        1,
        "one timer, not one per frame"
    );

    harness.window.advance(Duration::from_millis(699));
    assert!(
        find(&harness, "zui-tooltip").is_none(),
        "at 699ms it is still not there"
    );
    harness.window.advance(Duration::from_millis(1));
    assert_eq!(
        harness.semantics(expect(&harness, "zui-tooltip")).role,
        Role::Tooltip
    );
}

#[test]
fn a_pointer_that_leaves_before_the_delay_runs_out_raises_no_tooltip_at_all() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Tooltip(delay = Duration::from_millis(700)) {
                TooltipTrigger {Button {"B"}}
                TooltipContent {"Bold"}
            }
        }
    });
    let trigger = harness.find("zui-tooltip__trigger");

    point_at(&harness, trigger, EventKind::PointerEnter, 0.0, 0.0);
    harness.window.advance(Duration::from_millis(300));
    point_at(&harness, trigger, EventKind::PointerLeave, 0.0, 0.0);
    harness.window.advance(Duration::from_millis(2000));

    assert!(
        find(&harness, "zui-tooltip").is_none(),
        "the pointer was on its way past, and nothing was raised"
    );
}

#[test]
fn a_tooltip_names_the_control_it_describes_without_ever_being_shown() {
    // What a tooltip is *for*, to anyone who cannot see it: the relation exists whether or not the
    // label is on screen.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Tooltip(default_open = true) {
                TooltipTrigger {Button {"B"}}
                TooltipContent {"Bold"}
            }
        }
    });
    let trigger = harness.find("zui-tooltip__trigger");
    assert_eq!(
        harness.semantics(trigger).relations.described_by,
        [related(expect(&harness, "zui-tooltip"))]
    );
}

#[test]
fn focusing_a_tooltip_trigger_shows_it_without_waiting() {
    // A keyboard user asked for this control deliberately, and a delay meant for a pointer
    // travelling past has nothing to say about that.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Tooltip(delay = Duration::from_millis(700)) {
                TooltipTrigger {Button {"B"}}
                TooltipContent {"Bold"}
            }
        }
    });
    let trigger = harness.find("zui-tooltip__trigger");
    harness.window.dispatcher().send_to(
        trigger,
        EventKind::FocusIn,
        Payload::Focus(FocusEvent::new(FocusCause::Keyboard)),
    );
    harness.window.frame();
    assert!(find(&harness, "zui-tooltip").is_some());
}

#[test]
fn a_hover_card_stays_up_while_the_pointer_is_on_it() {
    // Without this a hover card can never be reached: it vanishes in the gap between the trigger
    // and itself, and nothing in it can ever be read.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            HoverCard(delay = Duration::ZERO, close_delay = Duration::from_millis(300)) {
                HoverCardTrigger {text {"@ada"}}
                HoverCardContent {text {"Ada Lovelace"}}
            }
        }
    });
    let trigger = harness.find("zui-hover-card__trigger");
    point_at(&harness, trigger, EventKind::PointerEnter, 0.0, 0.0);
    let card = expect(&harness, "zui-hover-card");

    point_at(&harness, trigger, EventKind::PointerLeave, 0.0, 0.0);
    point_at(&harness, card, EventKind::PointerEnter, 0.0, 0.0);
    harness.window.advance(Duration::from_millis(600));
    assert!(
        find(&harness, "zui-hover-card").is_some(),
        "the pointer reached the card, so the pending close was taken back"
    );
}

// ---- menus --------------------------------------------------------------------------------------

/// A menu whose first item writes down that it was chosen.
#[component]
fn AMenu(
    /// Where the choice is recorded.
    chosen: RwSignal<String, LocalStorage>,
) -> impl IntoView {
    view! {
        DropdownMenu {
            DropdownMenuTrigger {"Actions"}
            DropdownMenuContent {
                MenuLabel {"Invoice"}
                MenuItem(on_select = UnsyncCallback::new(move |()| chosen.set("open".into()))) {
                    "Open"
                }
                MenuItem {"Duplicate"}
                MenuSeparator()
                MenuItem(destructive = true) {"Print"}
            }
        }
    }
}

/// Opens the menu of [`AMenu`], and hands back the harness, the record and the items in order.
fn open_menu() -> (Harness, RwSignal<String, LocalStorage>, Vec<NodeId>) {
    let harness = Harness::open();
    let chosen = harness
        .window
        .scope
        .with(|| RwSignal::new_local(String::new()));
    harness.mount(move || view! { AMenu(chosen = chosen) });
    harness.click(harness.find("zui-button"));
    let items = every(&harness, "zui-menu__item");
    // Tree order is the engine's answer, and a scripted engine answers what a test declares.
    harness.window.host.set_tree_order(items.clone());
    (harness, chosen, items)
}

/// Which item of a menu holds the group's one tab stop.
fn tab_stop(harness: &Harness, items: &[NodeId]) -> Option<usize> {
    items.iter().position(|item| is_tab_stop(harness, *item))
}

#[test]
fn a_menu_is_a_menu_of_menu_items_with_one_tab_stop_between_them() {
    let (harness, _chosen, items) = open_menu();
    assert_eq!(
        harness.semantics(expect(&harness, "zui-menu")).role,
        Role::Menu
    );
    assert_eq!(items.len(), 3);
    for item in &items {
        assert_eq!(harness.semantics(*item).role, Role::MenuItem);
    }
    assert_eq!(
        items
            .iter()
            .filter(|item| is_tab_stop(&harness, **item))
            .count(),
        1,
        "a menu is one thing to tab past, not three"
    );
}

#[test]
fn the_down_arrow_walks_a_menu_and_wraps_at_the_end() {
    let (harness, _chosen, items) = open_menu();
    let list = expect(&harness, "zui-menu__list");
    assert_eq!(tab_stop(&harness, &items), Some(0));

    harness.press(list, NamedKey::ArrowDown);
    assert_eq!(tab_stop(&harness, &items), Some(1));
    harness.press(list, NamedKey::End);
    assert_eq!(tab_stop(&harness, &items), Some(2));
    harness.press(list, NamedKey::ArrowDown);
    assert_eq!(
        tab_stop(&harness, &items),
        Some(0),
        "the end wraps to the beginning"
    );
}

#[test]
fn typing_a_letter_in_a_menu_moves_to_the_item_that_reads_as_beginning_with_it() {
    // The item's text is what it renders, read back from the tree — not a second string declared
    // beside it, which is the copy that drifts the first time either is edited.
    let (harness, _chosen, items) = open_menu();

    // At the item the keyboard is on, which is where a real key press arrives.
    harness.type_char(items[0], 'p');
    assert_eq!(tab_stop(&harness, &items), Some(2), "`p` reached Print");
}

#[test]
fn a_typed_prefix_is_forgotten_after_a_pause() {
    // Without the reset a menu is unusable a moment after the first stray keystroke: every later
    // letter is appended to a prefix nothing begins with, and the menu stops answering the
    // keyboard altogether.
    let (harness, _chosen, items) = open_menu();

    harness.type_char(items[0], 'p');
    assert_eq!(tab_stop(&harness, &items), Some(2), "`p` reached Print");

    harness
        .window
        .advance(RESET_AFTER + Duration::from_millis(200));
    harness.type_char(items[2], 'd');
    assert_eq!(
        tab_stop(&harness, &items),
        Some(1),
        "the pause ended that search, so `d` is a new one and reaches Duplicate"
    );
}

#[test]
fn a_prefix_typed_without_a_pause_keeps_one_search_going() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            DropdownMenu(default_open = true) {
                DropdownMenuContent {
                    MenuItem {"Print"}
                    MenuItem {"Properties"}
                }
            }
        }
    });
    let items = every(&harness, "zui-menu__item");
    harness.window.host.set_tree_order(items.clone());

    for character in "pro".chars() {
        harness.type_char(items[0], character);
    }
    assert_eq!(
        tab_stop(&harness, &items),
        Some(1),
        "`pro` reached Properties rather than stopping at Print"
    );
}

#[test]
fn an_item_that_cannot_be_chosen_is_stepped_over() {
    // Landing on it would strand the keyboard: Enter does nothing there, and the one highlight a
    // menu has would be sitting on a row that refuses to answer.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            DropdownMenu(default_open = true) {
                DropdownMenuContent {
                    MenuItem {"Open"}
                    MenuItem(disabled = true) {"Restore"}
                    MenuItem {"Print"}
                }
            }
        }
    });
    let items = every(&harness, "zui-menu__item");
    harness.window.host.set_tree_order(items.clone());
    let list = expect(&harness, "zui-menu__list");

    harness.press(list, NamedKey::ArrowDown);
    assert_eq!(tab_stop(&harness, &items), Some(2));
    harness.press(list, NamedKey::End);
    harness.press(list, NamedKey::Home);
    assert_eq!(
        tab_stop(&harness, &items),
        Some(0),
        "and the ends are the ends of what can be reached"
    );
}

#[test]
fn the_down_arrow_opens_a_menu_from_its_button() {
    // What everyone reaches for on a control that says it drops something down, and the half of a
    // menu button's keyboard that activation does not cover.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            DropdownMenu {
                DropdownMenuTrigger {"Actions"}
                DropdownMenuContent {MenuItem {"Open"}}
            }
        }
    });
    let trigger = harness.find("zui-button");
    assert_eq!(harness.semantics(trigger).has_popup, Some(HasPopup::Menu));

    harness.press(trigger, NamedKey::ArrowDown);
    assert!(find(&harness, "zui-menu").is_some());
    assert_eq!(harness.semantics(trigger).expanded, Some(true));
}

#[test]
fn enter_chooses_the_item_the_keyboard_is_on() {
    let (harness, chosen, items) = open_menu();
    harness.press(items[0], NamedKey::Enter);
    settle(&harness);
    assert_eq!(chosen.get_untracked(), "open");
    assert!(find(&harness, "zui-menu").is_none());
}

#[test]
fn choosing_an_item_runs_it_and_closes_the_whole_menu() {
    let (harness, chosen, items) = open_menu();
    harness.click(items[0]);
    settle(&harness);

    assert_eq!(chosen.get_untracked(), "open");
    assert!(
        find(&harness, "zui-menu").is_none(),
        "a chosen item takes the menu with it"
    );
}

#[test]
fn a_checkbox_item_reports_its_own_state_and_a_radio_item_reports_the_group_s() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            DropdownMenu(default_open = true) {
                DropdownMenuContent {
                    MenuCheckboxItem(default_checked = true, close_on_select = false) {
                        "Totals"
                    }
                    MenuRadioGroup(default_value = "date") {
                        MenuRadioItem(value = "date", close_on_select = false) {"Date"}
                        MenuRadioItem(value = "amount", close_on_select = false) {"Amount"}
                    }
                }
            }
        }
    });
    let items = every(&harness, "zui-menu__item");
    assert_eq!(items.len(), 3);

    assert_eq!(harness.semantics(items[0]).role, Role::MenuItemCheckBox);
    assert_eq!(harness.semantics(items[0]).toggled, Some(Toggled::True));
    assert_eq!(harness.semantics(items[1]).role, Role::MenuItemRadio);
    assert_eq!(
        attribute(&harness, items[1], "data-state").as_deref(),
        Some("checked")
    );
    assert_eq!(
        attribute(&harness, items[2], "data-state").as_deref(),
        Some("unchecked")
    );

    harness.click(items[2]);
    harness.window.frame();
    assert_eq!(
        attribute(&harness, items[2], "data-state").as_deref(),
        Some("checked"),
        "choosing one moved the group"
    );
    assert_eq!(
        attribute(&harness, items[1], "data-state").as_deref(),
        Some("unchecked")
    );
}

#[test]
fn a_submenu_opens_on_the_right_arrow_and_closes_on_the_left_without_closing_the_menu() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            DropdownMenu(default_open = true) {
                DropdownMenuContent {
                    MenuSub {
                        MenuSubTrigger {"Export as"}
                        MenuSubContent {MenuItem {"PDF"}}
                    }
                }
            }
        }
    });
    let trigger = every(&harness, "zui-menu__item")
        .into_iter()
        .find(|node| {
            harness
                .window
                .dom
                .tree()
                .text_content(*node)
                .starts_with("Export as")
        })
        .expect("the submenu's trigger");
    assert_eq!(harness.semantics(trigger).has_popup, Some(HasPopup::Menu));

    harness.press(trigger, NamedKey::ArrowRight);
    let sub = expect(&harness, "zui-menu--sub");
    assert_eq!(harness.semantics(trigger).expanded, Some(true));

    harness.press(sub, NamedKey::ArrowLeft);
    settle(&harness);
    assert!(
        find(&harness, "zui-menu--sub").is_none(),
        "the left arrow backed out of the branch"
    );
    assert!(
        find(&harness, "zui-menu").is_some(),
        "and left the menu it came from open"
    );
}

#[test]
fn a_pointer_cutting_the_corner_toward_a_submenu_does_not_close_it() {
    // The safe corridor, driven rather than computed: the pointer leaves the trigger heading for
    // the submenu, and the submenu is still there afterwards.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            DropdownMenu(default_open = true) {
                DropdownMenuContent {
                    MenuSub(default_open = true) {
                        MenuSubTrigger {"Export as"}
                        MenuSubContent {MenuItem {"PDF"}}
                    }
                }
            }
        }
    });
    // By what it says, not by "the first item with this class": the submenu's own items carry it
    // too, and picking the wrong one is a test that passes by pressing on nothing.
    let trigger = every(&harness, "zui-menu__item")
        .into_iter()
        .find(|node| {
            harness
                .window
                .dom
                .tree()
                .text_content(*node)
                .starts_with("Export as")
        })
        .expect("the submenu's trigger");
    let sub = expect(&harness, "zui-menu--sub");
    harness.window.place(trigger, 100.0, 100.0, 100.0, 24.0);
    harness.window.place(sub, 200.0, 100.0, 160.0, 200.0);

    // Where the pointer was, then out of the trigger on the diagonal a hand actually draws.
    point_at(&harness, trigger, EventKind::PointerMove, 150.0, 110.0);
    point_at(&harness, trigger, EventKind::PointerLeave, 210.0, 160.0);
    harness.window.advance(Duration::from_millis(500));
    assert!(
        find(&harness, "zui-menu--sub").is_some(),
        "the pointer was on its way there"
    );

    // And straight down the parent menu instead, which is a user who has given up on it. The
    // submenu is re-opened first, so what is being asserted is that this path closes it rather
    // than that it was already closed.
    harness.press(trigger, NamedKey::ArrowRight);
    let sub = expect(&harness, "zui-menu--sub");
    harness.window.place(sub, 200.0, 100.0, 160.0, 200.0);
    point_at(&harness, trigger, EventKind::PointerMove, 150.0, 110.0);
    point_at(&harness, trigger, EventKind::PointerLeave, 150.0, 500.0);
    harness.window.advance(Duration::from_millis(500));
    settle(&harness);
    assert!(
        find(&harness, "zui-menu--sub").is_none(),
        "and this time it was not"
    );
}

#[test]
fn a_context_menu_opens_where_the_pointer_asked_rather_than_where_the_region_is() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            ContextMenu {
                ContextMenuTrigger {text {"A row"}}
                ContextMenuContent {MenuItem {"Copy"}}
            }
        }
    });
    let area = harness.find("zui-context-menu__area");
    harness.window.place(area, 40.0, 20.0, 400.0, 200.0);
    assert!(find(&harness, "zui-menu").is_none());

    point_at(&harness, area, EventKind::ContextMenu, 140.0, 90.0);
    assert!(find(&harness, "zui-menu").is_some());

    let anchor = harness.find("zui-context-menu__anchor");
    let tree = harness.window.dom.tree();
    assert_eq!(
        tree.style_property(anchor, "left").as_deref(),
        Some("100px"),
        "the anchor sits where the pointer was, in the region's own coordinates"
    );
    assert_eq!(tree.style_property(anchor, "top").as_deref(), Some("70px"));
}

#[test]
fn a_press_of_the_secondary_button_asks_for_a_context_menu_too() {
    // A long press produces a request event and a right-click on this platform does not, so a
    // region that only answered the request would have no context menu on a mouse at all.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            ContextMenu {
                ContextMenuTrigger {text {"A row"}}
                ContextMenuContent {MenuItem {"Copy"}}
            }
        }
    });
    let area = harness.find("zui-context-menu__area");
    harness.window.place(area, 0.0, 0.0, 400.0, 200.0);

    harness.window.dispatcher().send_to(
        area,
        EventKind::PointerDown,
        Payload::Pointer(
            PointerEvent::mouse(Point::new(zgui::geom::CssPx(10.0), zgui::geom::CssPx(10.0)))
                .with_button(PointerButton::Secondary),
        ),
    );
    harness.window.frame();
    assert!(find(&harness, "zui-menu").is_some());
}

// ---- select, combobox, command --------------------------------------------------------------------

/// A select over three currencies, one of which cannot be chosen.
#[component]
fn ASelect(
    /// Which value is chosen.
    value: RwSignal<String, LocalStorage>,
) -> impl IntoView {
    view! {
        Select(value = value, on_change = UnsyncCallback::new(move |next: String| value.set(next))) {
            SelectTrigger(label = "Currency") {SelectValue(placeholder = "Choose one")}
            SelectContent {
                SelectItem(value = "gbp") {"Pound sterling"}
                SelectItem(value = "eur") {"Euro"}
                SelectItem(value = "usd", disabled = true) {"US dollar"}
            }
        }
    }
}

/// Mounts a select, opens it, and hands back the harness, its value, its trigger and its options.
fn open_select() -> (Harness, RwSignal<String, LocalStorage>, NodeId, Vec<NodeId>) {
    let harness = Harness::open();
    let value = harness
        .window
        .scope
        .with(|| RwSignal::new_local(String::new()));
    harness.mount(move || view! { ASelect(value = value) });
    let trigger = harness.find("zui-select");
    harness.press(trigger, NamedKey::ArrowDown);
    let options = every(&harness, "zui-select__item");
    harness.window.host.set_tree_order(options.clone());
    (harness, value, trigger, options)
}

#[test]
fn a_select_is_a_combobox_over_a_listbox_and_keeps_the_caret_on_its_trigger() {
    let (harness, _value, trigger, options) = open_select();
    assert_eq!(harness.semantics(trigger).role, Role::ComboBox);
    assert_eq!(
        harness.semantics(trigger).has_popup,
        Some(HasPopup::Listbox)
    );
    assert_eq!(
        harness.semantics(expect(&harness, "zui-select__list")).role,
        Role::ListBox
    );

    for option in &options {
        assert_eq!(harness.semantics(*option).role, Role::ListBoxOption);
        assert_eq!(
            attribute(&harness, *option, "tabindex"),
            None,
            "an option the caret never visits is not a tab stop"
        );
    }
}

#[test]
fn the_arrow_keys_walk_a_select_and_the_trigger_says_which_option_they_are_on() {
    // `active_descendant` is the whole of how a listbox is operable without moving focus, and a
    // select that did not publish it would be a select a reader cannot follow.
    let (harness, _value, trigger, options) = open_select();
    assert_eq!(
        harness.semantics(trigger).relations.active_descendant,
        Some(related(options[0]))
    );

    harness.press(trigger, NamedKey::ArrowDown);
    assert_eq!(
        harness.semantics(trigger).relations.active_descendant,
        Some(related(options[1]))
    );
    assert_eq!(
        attribute(&harness, options[1], "data-active").as_deref(),
        Some("true")
    );

    harness.press(trigger, NamedKey::ArrowDown);
    assert_eq!(
        harness.semantics(trigger).relations.active_descendant,
        Some(related(options[0])),
        "the option that cannot be chosen is stepped over, and the end wraps"
    );
}

#[test]
fn enter_chooses_what_a_select_is_walking_and_the_trigger_then_shows_its_text() {
    let (harness, value, trigger, _options) = open_select();
    harness.press(trigger, NamedKey::ArrowDown);
    harness.press(trigger, NamedKey::Enter);
    settle(&harness);

    assert_eq!(value.get_untracked(), "eur");
    assert!(
        find(&harness, "zui-select__list").is_none(),
        "choosing closed the list"
    );

    // Read while it is **closed**, which is the only state a select spends any time in. Its
    // options are its list and its list is gone, so a control that asked them what the value reads
    // as would show its placeholder over a choice the user has already made.
    assert_eq!(
        harness.window.dom.tree().text_content(trigger),
        "Euro",
        "the control and the list cannot say different things about one value"
    );
}

#[test]
fn a_select_shows_its_value_on_a_trigger_whose_list_has_never_been_opened() {
    // Never opened, and that is the whole of the fixture. The text a value reads as belongs to the
    // option that declares it, and a closed select has not mounted its options — so a control that
    // asked only what is on the list would show its placeholder over a choice the caller has
    // already made, for the whole of the time anybody looks at it. A fixture that opened the list
    // first to read the trigger would be a fixture that removed the defect before measuring it.
    let harness = Harness::open();
    let value = harness
        .window
        .scope
        .with(|| RwSignal::new_local("gbp".to_owned()));
    harness.mount(move || view! { ASelect(value = value) });
    let trigger = harness.find("zui-select");

    assert!(
        find(&harness, "zui-select__list").is_none(),
        "nothing has opened the list"
    );
    assert_eq!(
        harness.window.dom.tree().text_content(trigger),
        "Pound sterling",
        "a closed select shows what it is set to, not what it would say if it were empty"
    );
    assert!(
        !harness.state(trigger).contains(UiState::PLACEHOLDER_SHOWN),
        "and does not claim to be showing a placeholder"
    );
}

#[test]
fn a_select_jumps_to_the_ends_and_leaves_without_choosing_on_escape() {
    let (harness, value, trigger, options) = open_select();

    harness.press(trigger, NamedKey::End);
    assert_eq!(
        harness.semantics(trigger).relations.active_descendant,
        Some(related(options[1])),
        "the end of what can be chosen, not the end of the list"
    );
    harness.press(trigger, NamedKey::Home);
    assert_eq!(
        harness.semantics(trigger).relations.active_descendant,
        Some(related(options[0]))
    );

    harness.press(trigger, NamedKey::Escape);
    settle(&harness);
    assert!(
        find(&harness, "zui-select__list").is_none(),
        "escape closed"
    );
    assert_eq!(
        value.get_untracked(),
        "",
        "and left the value where it found it"
    );
}

#[test]
fn a_select_leaves_tab_alone_and_a_closed_one_leaves_enter_alone() {
    // A select in a form that swallowed either would be a form nobody can submit or leave.
    let harness = Harness::open();
    let value = harness
        .window
        .scope
        .with(|| RwSignal::new_local(String::new()));
    harness.mount(move || view! { ASelect(value = value) });
    let trigger = harness.find("zui-select");

    for key in [NamedKey::Tab, NamedKey::Enter] {
        let delivered = harness.window.dispatcher().key(trigger, Key::Named(key));
        assert_eq!(
            delivered.default,
            DefaultAction::Allowed,
            "{key:?} was left to whatever is around the control"
        );
    }
}

#[test]
fn typing_in_a_combobox_narrows_the_list_to_what_is_left_and_nothing_else_is_mounted() {
    // Not hidden with a style rule: an option that survives is mounted and one that does not is
    // absent, so a reader meets exactly what is on the screen.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Combobox {
                ComboboxInput(placeholder = "Search", label = "Country")
                ComboboxContent {
                    ComboboxItem(value = "gb", text = "United Kingdom") {"United Kingdom"}
                    ComboboxItem(value = "ie", text = "Ireland") {"Ireland"}
                    ComboboxItem(value = "fr", text = "France") {"France"}
                    ComboboxEmpty {"No country by that name."}
                }
            }
        }
    });
    let field = harness.find("zui-combobox__input");

    harness.type_char(field, 'i');
    assert_eq!(
        every(&harness, "zui-select__item").len(),
        2,
        "United Kingdom and Ireland both contain an `i`"
    );

    for character in "rela".chars() {
        harness.type_char(field, character);
    }
    let left = every(&harness, "zui-select__item");
    assert_eq!(left.len(), 1);
    assert_eq!(harness.window.dom.tree().text_content(left[0]), "Ireland");

    harness.type_char(field, 'z');
    assert!(every(&harness, "zui-select__item").is_empty());
    assert!(
        find(&harness, "zui-combobox__empty").is_some(),
        "and it says so"
    );
}

#[test]
fn a_combobox_field_points_at_the_option_the_arrow_keys_are_on() {
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Combobox {
                ComboboxInput(label = "Country")
                ComboboxContent {
                    ComboboxItem(value = "gb", text = "United Kingdom") {"United Kingdom"}
                    ComboboxItem(value = "ie", text = "Ireland") {"Ireland"}
                }
            }
        }
    });
    let field = harness.find("zui-combobox__input");
    assert_eq!(harness.semantics(field).role, Role::ComboBox);

    harness.press(field, NamedKey::ArrowDown);
    let options = every(&harness, "zui-select__item");
    harness.window.host.set_tree_order(options.clone());
    assert_eq!(
        harness.semantics(field).relations.active_descendant,
        Some(related(options[0]))
    );
}

#[test]
fn a_command_palette_runs_what_enter_lands_on_and_stays_where_it_is() {
    // Its list is not a popup, so choosing must not close anything — and the next arrow key must
    // move rather than re-open something that was never open.
    let harness = Harness::open();
    let ran = harness
        .window
        .scope
        .with(|| RwSignal::new_local(String::new()));
    harness.mount(move || {
        view! {
            Command {
                CommandInput(placeholder = "Type a command…", label = "Command")
                CommandList(label = "Commands") {
                    CommandGroup(label = "Invoices") {
                        CommandItem(
                            value = "invoice.new",
                            on_select = UnsyncCallback::new(move |()| ran.set("new".into()))
                        ) {
                            "New invoice"
                        }
                        CommandItem(value = "invoice.export") {"Export invoices"}
                    }
                }
            }
        }
    });
    let field = harness.find("zui-combobox__input");
    let options = every(&harness, "zui-select__item");
    assert_eq!(options.len(), 2);
    harness.window.host.set_tree_order(options.clone());

    // The highlight starts on the first command, so Enter runs it without an arrow key first.
    harness.press(field, NamedKey::Enter);
    assert_eq!(ran.get_untracked(), "new");

    harness.press(field, NamedKey::ArrowDown);
    assert_eq!(
        harness.semantics(field).relations.active_descendant,
        Some(related(options[1])),
        "the list is still there and the arrow keys still walk it"
    );
}

#[test]
fn a_command_dialog_is_a_labelled_dialog_with_no_heading_on_the_surface() {
    // A palette's field is its heading, so the name is written rather than shown — and a dialog
    // with neither would be announced as an unlabelled dialog.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            CommandDialog(default_open = true, title = "Commands") {
                CommandInput(placeholder = "Type a command…")
                CommandList {CommandItem(value = "new") {"New invoice"}}
            }
        }
    });
    let surface = expect(&harness, "zui-dialog");
    let title = expect(&harness, "zui-dialog__title");
    assert_eq!(
        harness.semantics(surface).relations.labelled_by,
        [related(title)]
    );
    assert!(
        harness
            .semantics(title)
            .flags
            .contains(SemanticFlags::HIDDEN),
        "it names the dialog without being a heading on the surface"
    );
}

// ---- layering ---------------------------------------------------------------------------------------

#[test]
fn a_popover_inside_a_dialog_is_dismissed_on_its_own_and_the_dialog_stays() {
    // The case the layer stack exists for, written the way a user meets it.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            Dialog(default_open = true) {
                DialogContent {
                    DialogTitle {"Settings"}
                    Popover(default_open = true) {
                        PopoverTrigger {"Size"}
                        PopoverContent {text {"Width"}}
                    }
                }
            }
        }
    });
    let popover = expect(&harness, "zui-popover");

    harness.press(popover, NamedKey::Escape);
    settle(&harness);
    assert!(find(&harness, "zui-popover").is_none());
    assert!(
        find(&harness, "zui-dialog").is_some(),
        "one press closed one surface"
    );

    harness.press(expect(&harness, "zui-dialog"), NamedKey::Escape);
    settle(&harness);
    assert!(find(&harness, "zui-dialog").is_none());
}

#[test]
fn two_surfaces_written_side_by_side_answer_one_escape_between_them() {
    // Nesting is not the only way two surfaces are open at once, and siblings are the commoner
    // way: a popover left open on the page and a dialog raised over it. Exactly one of them
    // answers the press, and it is the one on top.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            column {
                Popover(default_open = true) {
                    PopoverTrigger {"Size"}
                    PopoverContent {text {"Width"}}
                }
                Dialog(default_open = true) {
                    DialogContent {DialogTitle {"Rename"}}
                }
            }
        }
    });

    harness.press(expect(&harness, "zui-dialog"), NamedKey::Escape);
    settle(&harness);
    assert!(find(&harness, "zui-dialog").is_none(), "the dialog went");
    assert!(
        find(&harness, "zui-popover").is_some(),
        "and the popover, which is not the one on top, stayed"
    );

    harness.press(expect(&harness, "zui-popover"), NamedKey::Escape);
    settle(&harness);
    assert!(
        find(&harness, "zui-popover").is_none(),
        "the next press reached it"
    );
}

#[test]
fn a_modal_surface_goes_on_the_modal_band_and_a_popover_on_the_popover_band() {
    // The band is what decides what is above what, not the order they were written in.
    let harness = Harness::open();
    harness.mount(|| {
        view! {
            column {
                Popover(default_open = true) {
                    PopoverTrigger {"Size"}
                    PopoverContent {text {"Width"}}
                }
                Dialog(default_open = true) {
                    DialogContent {DialogTitle {"Rename"}}
                }
            }
        }
    });
    let on = |layer: OverlayLayer, class: &str| {
        let name = ClassName::new(class);
        overlay(&harness, layer)
            .into_iter()
            .any(|node| harness.window.dom.tree().classes(node).contains(&name))
    };
    assert!(on(OverlayLayer::Popover, "zui-popover"));
    assert!(on(OverlayLayer::Modal, "zui-dialog"));
    assert!(!on(OverlayLayer::Popover, "zui-dialog"));
}

// ---- the corridor, as geometry ------------------------------------------------------------------------

#[test]
fn the_safe_corridor_is_a_pure_function_of_two_points_and_a_rectangle() {
    let at = |x: f32, y: f32| Point::new(DevicePx(x), DevicePx(y));
    let submenu: Rect<DevicePx, Device> = Rect::new(
        at(200.0, 100.0),
        Size::new(DevicePx(160.0), DevicePx(200.0)),
    );

    assert!(zgui_ui::menu::heading_toward(
        at(100.0, 110.0),
        at(160.0, 180.0),
        submenu
    ));
    assert!(!zgui_ui::menu::heading_toward(
        at(100.0, 110.0),
        at(100.0, 420.0),
        submenu
    ));
}
