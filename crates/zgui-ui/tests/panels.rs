//! Scrolling, resizing, announcing and submitting, driven through real frames.

mod harness;

use core::time::Duration;

use zgui::prelude::*;
use zgui::reactive::{RwSignal, UnsyncCallback};
use zgui::view;
use zgui::vocab::{NamedKey, SemanticFlags, SharedString};
use zgui_ui::form::{Validator, use_form_field};
use zgui_ui::prelude::*;
use zgui_ui::toast::{Toast, ToastKind, use_toaster};

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

/// A pointer event of `kind` at `(x, y)` CSS pixels, aimed straight at `node`.
fn pointer(harness: &Harness, node: NodeId, kind: zgui::vocab::EventKind, x: f32, y: f32) {
    harness.window.dispatcher().send_to(
        node,
        kind,
        zgui::vocab::Payload::Pointer(zgui::vocab::PointerEvent::mouse(zgui::geom::Point::new(
            zgui::geom::CssPx(x),
            zgui::geom::CssPx(y),
        ))),
    );
    harness.window.frame();
}

/// Tells `node` that focus left it, exactly as a window does when the keyboard moves on.
fn focus_out(harness: &Harness, node: NodeId) {
    harness.window.dispatcher().send_to(
        node,
        zgui::vocab::EventKind::FocusOut,
        zgui::vocab::Payload::Focus(zgui::vocab::FocusEvent::new(
            zgui::vocab::FocusCause::Keyboard,
        )),
    );
    harness.window.frame();
}

// ---- scroll area ------------------------------------------------------------------------------

/// A scroll area whose viewport has `content` of content in a `viewport`-tall scrollport, scrolled
/// to `offset`. Answers the bar.
fn scrolling(harness: &Harness, content: f32, viewport: f32, offset: f32) -> NodeId {
    let bar = harness.find("zui-scroll-area__bar");
    let scroller = harness.find("zui-scroll-area__viewport");

    // The strip the bar lies over, both as a one-shot answer and as the observation it watches.
    harness.window.place(bar, 0.0, 0.0, 15.0, viewport);
    harness.window.dom.deliver(
        bar,
        zgui::view::ObservedValue::BorderBox(zgui::geom::Rect::new(
            zgui::geom::Point::new(zgui::geom::DevicePx(0.0), zgui::geom::DevicePx(0.0)),
            zgui::geom::Size::new(zgui::geom::DevicePx(15.0), zgui::geom::DevicePx(viewport)),
        )),
    );
    harness.window.dom.deliver(
        scroller,
        zgui::view::ObservedValue::ScrollPosition(ScrollPosition {
            offset: zgui::geom::Point::new(zgui::geom::DevicePx(0.0), zgui::geom::DevicePx(offset)),
            content_size: zgui::geom::Size::new(
                zgui::geom::DevicePx(200.0),
                zgui::geom::DevicePx(content),
            ),
            scrollport: zgui::geom::Size::new(
                zgui::geom::DevicePx(200.0),
                zgui::geom::DevicePx(viewport),
            ),
        }),
    );
    harness.window.frame();
    bar
}

#[test]
fn the_bar_draws_nothing_and_answers_no_pointer() {
    // The decision this component was rebuilt around. Every scrolling box reserves a gutter and the
    // engine composes a track and a thumb into it; a second thumb drawn here would be a second
    // answer to one question, sampled at a different moment, and the frame in which the two
    // disagree is the frame somebody is looking at. So this element draws no thumb, and a press on
    // it asks for nothing — it falls through to the engine's own bar underneath.
    let harness = Harness::open();
    harness.mount(|| view! { ScrollArea(label = "Names") {text {"a long list"}} });
    let bar = scrolling(&harness, 400.0, 200.0, 0.0);

    let thumbs = zgui::view::ClassName::new("zui-scroll-area__thumb");
    assert!(
        harness.all().into_iter().all(|node| !harness
            .window
            .dom
            .tree()
            .classes(node)
            .contains(&thumbs)),
        "the component draws no thumb of its own"
    );

    pointer(
        &harness,
        bar,
        zgui::vocab::EventKind::PointerDown,
        5.0,
        50.0,
    );
    pointer(
        &harness,
        bar,
        zgui::vocab::EventKind::PointerMove,
        5.0,
        100.0,
    );
    assert!(
        harness.window.host.scroll_commands().is_empty(),
        "a press and a drag on the bar are the engine's, and this component asked for neither"
    );
}

#[test]
fn a_bar_with_nothing_to_scroll_says_so_and_stays_measurable() {
    // Built only when it is needed, a bar would take its own tab stop and its own announcement
    // away the moment the content shrank, and would have to invent them again when it grew.
    let harness = Harness::open();
    harness.mount(|| view! { ScrollArea(label = "Names") {text {"a short list"}} });
    let bar = scrolling(&harness, 100.0, 200.0, 0.0);

    assert_eq!(
        harness.attribute(bar, "data-scrollable").as_deref(),
        Some("false")
    );

    scrolling(&harness, 400.0, 200.0, 0.0);
    assert_eq!(
        harness.attribute(bar, "data-scrollable").as_deref(),
        Some("true")
    );
}

#[test]
fn a_scrollbar_can_be_operated_without_looking_at_it() {
    let harness = Harness::open();
    harness.mount(|| view! { ScrollArea(label = "Names") {text {"a long list"}} });
    let bar = scrolling(&harness, 800.0, 200.0, 0.0);

    assert_eq!(harness.semantics(bar).role, Role::ScrollBar);
    assert_eq!(
        harness.semantics(bar).orientation,
        Some(zgui::vocab::Orientation::Vertical)
    );
    assert_eq!(harness.semantics(bar).numeric.max, Some(600.0));

    harness.press(bar, NamedKey::PageDown);
    assert_eq!(
        harness.window.host.scroll_commands().last().map(|c| c.1),
        Some(ScrollTarget::Offset(zgui::geom::Point::new(
            zgui::geom::DevicePx(0.0),
            zgui::geom::DevicePx(200.0)
        )))
    );

    harness.press(bar, NamedKey::End);
    assert_eq!(
        harness.window.host.scroll_commands().last().map(|c| c.1),
        Some(ScrollTarget::Offset(zgui::geom::Point::new(
            zgui::geom::DevicePx(0.0),
            zgui::geom::DevicePx(600.0)
        )))
    );
}

#[test]
fn a_bar_with_nothing_to_scroll_is_not_a_tab_stop_either() {
    // The bar is always built, because it is what a reader is told about the region and what a
    // keyboard operates it through. A fixed tab stop would leave a keyboard user landing on a bar
    // whose keys move nothing.
    let harness = Harness::open();
    harness.mount(|| view! { ScrollArea(label = "Names") {text {"a short list"}} });
    let bar = scrolling(&harness, 100.0, 200.0, 0.0);
    assert_eq!(
        harness.attribute(bar, "tabindex").as_deref(),
        Some("-1"),
        "a bar with nothing to scroll is still reached by tabbing"
    );

    scrolling(&harness, 400.0, 200.0, 0.0);
    assert_eq!(
        harness.attribute(bar, "tabindex").as_deref(),
        Some("0"),
        "and one with something to scroll has to be reachable"
    );
}

// ---- resizable --------------------------------------------------------------------------------

/// Two panels with a divider, in a group 400 CSS pixels wide.
fn split(harness: &Harness) -> (NodeId, Vec<NodeId>) {
    harness.mount(|| {
        view! {
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
    });
    let group = harness.only_child();
    harness.window.place(group, 0.0, 0.0, 400.0, 300.0);
    let handle = harness.find("zui-resizable__handle");
    let panels = all_with(harness, "zui-resizable__panel");
    assert_eq!(panels.len(), 2);
    (handle, panels)
}

#[test]
fn two_panels_share_the_group_out_between_them() {
    let harness = Harness::open();
    let (_, panels) = split(&harness);

    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("50.0000%")
    );
    assert_eq!(
        custom(&harness, panels[1], "zui-panel-size").as_deref(),
        Some("50.0000%")
    );
}

#[test]
fn dragging_a_divider_takes_from_one_panel_and_gives_to_the_other() {
    let harness = Harness::open();
    let (handle, panels) = split(&harness);

    pointer(
        &harness,
        handle,
        zgui::vocab::EventKind::PointerDown,
        200.0,
        0.0,
    );
    pointer(
        &harness,
        handle,
        zgui::vocab::EventKind::PointerMove,
        280.0,
        0.0,
    );

    // 80 of 400 CSS pixels is a fifth of the group.
    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("70.0000%")
    );
    assert_eq!(
        custom(&harness, panels[1], "zui-panel-size").as_deref(),
        Some("30.0000%"),
        "the group is no longer accounted for"
    );
}

#[test]
fn a_divider_stops_at_the_minimum_of_the_panel_it_is_squeezing() {
    let harness = Harness::open();
    let (handle, panels) = split(&harness);

    pointer(
        &harness,
        handle,
        zgui::vocab::EventKind::PointerDown,
        200.0,
        0.0,
    );
    pointer(
        &harness,
        handle,
        zgui::vocab::EventKind::PointerMove,
        900.0,
        0.0,
    );

    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("80.0000%")
    );
    assert_eq!(
        custom(&harness, panels[1], "zui-panel-size").as_deref(),
        Some("20.0000%"),
        "the panel beside it was squeezed past its declared minimum"
    );
}

#[test]
fn a_divider_can_be_moved_from_the_keyboard_and_says_where_it_is() {
    let harness = Harness::open();
    let (handle, panels) = split(&harness);

    assert_eq!(harness.semantics(handle).role, Role::Splitter);
    assert_eq!(harness.semantics(handle).numeric.value, Some(50.0));
    assert_eq!(harness.semantics(handle).numeric.min, Some(20.0));

    harness.press(handle, NamedKey::ArrowRight);
    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("60.0000%")
    );
    assert_eq!(harness.semantics(handle).numeric.value, Some(60.0));

    harness.press(handle, NamedKey::Home);
    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("20.0000%")
    );

    harness.press(handle, NamedKey::End);
    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("80.0000%")
    );
}

#[test]
fn enter_folds_the_panel_before_the_divider_and_brings_it_back_where_it_was() {
    let harness = Harness::open();
    let (handle, panels) = split(&harness);

    harness.press(handle, NamedKey::ArrowRight);
    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("60.0000%")
    );

    harness.press(handle, NamedKey::Enter);
    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("20.0000%")
    );

    harness.press(handle, NamedKey::Enter);
    assert_eq!(
        custom(&harness, panels[0], "zui-panel-size").as_deref(),
        Some("60.0000%"),
        "it came back at some other size than the one it had"
    );
}

// ---- toast ------------------------------------------------------------------------------------

/// A toaster with a button that announces something.
#[component]
fn Announcer(
    /// What the button announces.
    toast: Toast,
) -> impl IntoView {
    let toasts = use_toaster();
    let held = StoredValue::new_local(toast);
    view! {
        Button(on:click = move |_| {
            if let Some(toasts) = toasts {
                toasts.push(held.get_value());
            }
        }) {
            "Announce"
        }
    }
}

/// Mounts a toaster and something that announces `toast` through it, and hands back the button.
fn toaster(harness: &Harness, toast: Toast) -> NodeId {
    harness.mount(move || {
        view! {
            Toaster(limit = 2) {Announcer(toast = toast)}
        }
    });
    harness.find("zui-button")
}

#[test]
fn anything_under_the_toaster_can_announce_something_without_a_prop_between_them() {
    let harness = Harness::open();
    let button = toaster(&harness, Toast::new("Saved").description("On the server."));
    assert!(all_with(&harness, "zui-toast").is_empty());

    harness.click(button);

    let toasts = all_with(&harness, "zui-toast");
    assert_eq!(toasts.len(), 1);
    // The title and the line under it, and nothing else: the control that dismisses it is a drawn
    // mark rather than a letter, so it contributes no text for a reader to trip over.
    assert_eq!(
        harness.window.dom.tree().text_content(toasts[0]),
        "SavedOn the server."
    );
    assert_eq!(harness.semantics(toasts[0]).role, Role::Alert);
    assert_eq!(
        harness.semantics(toasts[0]).label,
        Some(SharedString::from("Saved"))
    );
}

#[test]
fn only_an_error_interrupts_a_reader() {
    let harness = Harness::open();
    let button = toaster(&harness, Toast::new("Wrong").kind(ToastKind::Error));
    harness.click(button);
    let toasts = all_with(&harness, "zui-toast");

    assert_eq!(
        harness.semantics(toasts[0]).live,
        Some(zgui::vocab::Live::Assertive)
    );
    assert_eq!(
        harness.attribute(toasts[0], "data-kind").as_deref(),
        Some("error")
    );
}

#[test]
fn a_toast_goes_away_on_its_own_when_its_time_is_up() {
    let harness = Harness::open();
    let button = toaster(
        &harness,
        Toast::new("Saved").duration(Duration::from_secs(2)),
    );
    harness.click(button);
    assert_eq!(all_with(&harness, "zui-toast").len(), 1);

    harness.window.advance(Duration::from_millis(1_999));
    assert_eq!(all_with(&harness, "zui-toast").len(), 1, "too early");

    harness.window.advance(Duration::from_millis(1));
    let leaving = all_with(&harness, "zui-toast");
    assert_eq!(
        harness.attribute(leaving[0], "data-state").as_deref(),
        Some("closed"),
        "its time being up starts it leaving rather than deleting it, so the sheet has something \
         to animate"
    );

    // And it goes, once there is no animation left to wait for — after the grace the departure
    // gives a live window's cascade to start one, which is what stops an exit being cut short by
    // an unrelated animation ending in the same batch as the dismissal.
    harness.window.advance(Duration::from_millis(60));
    assert!(all_with(&harness, "zui-toast").is_empty());
}

#[test]
fn a_toast_being_read_does_not_disappear_from_under_the_pointer() {
    let harness = Harness::open();
    let button = toaster(
        &harness,
        Toast::new("Saved").duration(Duration::from_secs(2)),
    );
    harness.click(button);
    let toast = harness.find("zui-toast");

    pointer(
        &harness,
        toast,
        zgui::vocab::EventKind::PointerEnter,
        0.0,
        0.0,
    );
    harness.window.advance(Duration::from_secs(10));
    assert_eq!(
        all_with(&harness, "zui-toast").len(),
        1,
        "a message that vanished while it was being read might as well not have been shown"
    );

    pointer(
        &harness,
        toast,
        zgui::vocab::EventKind::PointerLeave,
        0.0,
        0.0,
    );
    harness.window.advance(Duration::from_secs(2));
    harness.window.advance(Duration::from_millis(1));
    assert!(all_with(&harness, "zui-toast").is_empty());
}

#[test]
fn a_toast_pushed_far_enough_is_dismissed_and_one_pushed_a_little_comes_back() {
    let harness = Harness::open();
    let button = toaster(&harness, Toast::new("Saved").persistent());
    harness.click(button);
    let toast = harness.find("zui-toast");

    pointer(
        &harness,
        toast,
        zgui::vocab::EventKind::PointerDown,
        0.0,
        0.0,
    );
    pointer(
        &harness,
        toast,
        zgui::vocab::EventKind::PointerMove,
        20.0,
        0.0,
    );
    let slot = harness.find("zui-toast__slot");
    assert_eq!(
        custom(&harness, slot, "zui-toast-swipe").as_deref(),
        Some("20px"),
        "the toast has to follow the finger while the finger is on it"
    );
    pointer(
        &harness,
        toast,
        zgui::vocab::EventKind::PointerUp,
        20.0,
        0.0,
    );
    assert_eq!(
        all_with(&harness, "zui-toast").len(),
        1,
        "20px is not a swipe"
    );
    assert_eq!(
        custom(&harness, slot, "zui-toast-swipe").as_deref(),
        Some("0px")
    );

    pointer(
        &harness,
        toast,
        zgui::vocab::EventKind::PointerDown,
        0.0,
        0.0,
    );
    pointer(
        &harness,
        toast,
        zgui::vocab::EventKind::PointerMove,
        90.0,
        0.0,
    );
    pointer(
        &harness,
        toast,
        zgui::vocab::EventKind::PointerUp,
        90.0,
        0.0,
    );
    // Past the departure's grace, which is what a window with no animation waits before going.
    harness.window.advance(Duration::from_millis(60));
    assert!(all_with(&harness, "zui-toast").is_empty());
}

#[test]
fn the_stack_stops_at_its_limit_by_asking_the_oldest_to_go() {
    // Asked to go, not deleted. A toast that vanished the instant a third arrived would have no exit
    // for a sheet to animate, and everything above it would move in the same frame.
    let harness = Harness::open();
    let button = toaster(&harness, Toast::new("Saved").persistent());
    for _ in 0..3 {
        harness.click(button);
    }

    let toasts = all_with(&harness, "zui-toast");
    assert_eq!(toasts.len(), 3, "the third is on its way out, not gone");
    let states: Vec<Option<String>> = toasts
        .iter()
        .map(|toast| harness.attribute(*toast, "data-state"))
        .collect();
    assert_eq!(
        states,
        [
            Some("open".to_owned()),
            Some("open".to_owned()),
            Some("closed".to_owned())
        ],
        "the newest two stay and the oldest leaves"
    );

    // Past the departure's grace, which is what a window with no animation waits before going.
    harness.window.advance(Duration::from_millis(60));
    assert_eq!(
        all_with(&harness, "zui-toast").len(),
        2,
        "a stack that grew would cover the interface"
    );
}

#[test]
fn a_toast_is_placed_clear_of_whatever_the_layout_measured_below_it() {
    // The step from one toast to the next is a measurement rather than a number: a toast with a
    // description is taller than one without, and a stack stepped by some fixed amount puts one
    // toast through another the first time a caller writes a longer message.
    let harness = Harness::open();
    let button = toaster(&harness, Toast::new("Saved").persistent());
    harness.click(button);
    harness.click(button);

    let slots = all_with(&harness, "zui-toast__slot");
    assert_eq!(slots.len(), 2, "newest first");
    assert_eq!(
        custom(&harness, slots[0], "zui-toast-offset").as_deref(),
        Some("0px"),
        "the newest is against the corner"
    );

    // What the newest turned out to be, reported the way the layout reports it: the slot's content
    // — the toast alone, in the layout's own space, which no transform touches — with the gap the
    // slot carries as padding added back by the item from the same constant the region uses. A
    // 48-pixel toast under a 14-pixel gap is a 62-pixel step.
    harness.window.dom.deliver(
        slots[0],
        zgui::view::ObservedValue::ContentSize(zgui::geom::Size::new(
            zgui::geom::DevicePx(360.0),
            zgui::geom::DevicePx(48.0),
        )),
    );
    harness.window.frame();

    assert_eq!(
        custom(&harness, slots[1], "zui-toast-offset").as_deref(),
        Some("62px"),
        "and the one above it clears exactly that"
    );
}

// ---- form -------------------------------------------------------------------------------------

/// The control inside a form field, wired to it through the field's own published handles.
#[component]
fn FieldInput(
    /// The value it edits.
    value: RwSignal<String, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    let field = use_form_field();
    let attrs = field.map(FormFieldContext::attrs).unwrap_or_default();
    view! {
        Input(
            value = value,
            node_ref = field.map(FormFieldContext::control).unwrap_or_default(),
            {..attrs}
        )
    }
}

/// A form with one required field, and a way to see whether it was sent.
fn form(
    harness: &Harness,
    name: RwSignal<String, zgui::reactive::LocalStorage>,
) -> std::rc::Rc<std::cell::Cell<usize>> {
    let sent = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = std::rc::Rc::clone(&sent);
    harness.mount(move || {
        let rule = Validator::new(move || {
            name.get()
                .is_empty()
                .then(|| "A name is needed.".to_owned())
        });
        view! {
            Form(on_submit = UnsyncCallback::new(move |()| count.set(count.get() + 1))) {
                FormField(name = "name", validate = rule) {
                    FormItem {
                        FormLabel {"Name"}
                        FieldInput(value = name)
                        FormDescription {"As it appears on the card."}
                        FormMessage()
                    }
                }
            }
        }
    });
    sent
}

#[test]
fn a_field_says_nothing_until_it_has_been_left() {
    // A form that is scarlet before a word has been typed has told the user off for arriving.
    let harness = Harness::open();
    let name = RwSignal::new_local(String::new());
    form(&harness, name);
    let message = harness.find("zui-form__message");
    let control = harness.find(zgui_ui::InputStyle::CLASS);

    assert_eq!(
        harness.attribute(message, "data-state").as_deref(),
        Some("quiet")
    );
    assert_eq!(harness.semantics(control).invalid, None);

    focus_out(&harness, control);

    assert_eq!(
        harness.attribute(message, "data-state").as_deref(),
        Some("error")
    );
    assert_eq!(
        harness.window.dom.tree().text_content(message),
        "A name is needed."
    );
    assert_eq!(
        harness.semantics(control).invalid,
        Some(zgui::vocab::Invalid::True)
    );
}

#[test]
fn the_control_describes_itself_with_the_hint_until_it_describes_itself_with_the_complaint() {
    let harness = Harness::open();
    let name = RwSignal::new_local(String::new());
    form(&harness, name);
    let control = harness.find(zgui_ui::InputStyle::CLASS);
    let description = harness.find("zui-form__description");
    let message = harness.find("zui-form__message");

    assert_eq!(
        harness.semantics(control).relations.described_by,
        [zgui::vocab::NodeId(description.as_u64())]
    );

    focus_out(&harness, control);
    assert_eq!(
        harness.semantics(control).relations.described_by,
        [zgui::vocab::NodeId(message.as_u64())],
        "the hint after the complaint is the wrong way round"
    );

    name.set("Ada".to_owned());
    harness.window.frame();
    assert_eq!(
        harness.semantics(control).relations.described_by,
        [zgui::vocab::NodeId(description.as_u64())]
    );
}

#[test]
fn the_label_names_the_control_without_anybody_wiring_the_two_together() {
    let harness = Harness::open();
    let name = RwSignal::new_local(String::new());
    form(&harness, name);
    let control = harness.find(zgui_ui::InputStyle::CLASS);
    let label = harness.find("zui-label");

    assert_eq!(
        harness.semantics(control).relations.labelled_by,
        [zgui::vocab::NodeId(label.as_u64())],
        "the naming is done from the control's side, and nobody wired the two together by hand"
    );
}

#[test]
fn a_form_that_is_not_ready_is_not_sent_and_the_keyboard_goes_to_what_is_wrong() {
    let harness = Harness::open();
    let name = RwSignal::new_local(String::new());
    let sent = form(&harness, name);
    let control = harness.find(zgui_ui::InputStyle::CLASS);
    let message = harness.find("zui-form__message");
    harness.window.transcript.clear();

    harness.press(control, NamedKey::Enter);

    assert_eq!(sent.get(), 0, "an invalid form was sent");
    assert_eq!(
        harness.attribute(message, "data-state").as_deref(),
        Some("error"),
        "the user was left watching a button do nothing"
    );
    assert!(
        harness
            .window
            .transcript
            .to_string()
            .contains(&format!("focus #{}", control.as_u64())),
        "the keyboard was left where it was rather than taken to what is wrong"
    );
}

#[test]
fn a_form_that_is_ready_is_sent() {
    let harness = Harness::open();
    let name = RwSignal::new_local("Ada".to_owned());
    let sent = form(&harness, name);
    let control = harness.find(zgui_ui::InputStyle::CLASS);

    harness.press(control, NamedKey::Enter);
    assert_eq!(sent.get(), 1);

    let message = harness.find("zui-form__message");
    assert_eq!(
        harness.attribute(message, "data-state").as_deref(),
        Some("quiet")
    );
    assert!(
        !harness
            .semantics(control)
            .flags
            .contains(SemanticFlags::DISABLED)
    );
}
