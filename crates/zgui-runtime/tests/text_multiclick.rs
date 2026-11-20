//! Default multi-click selection through real pointer routing, shaping, and editing.
mod support;

use std::time::Duration;
use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{
    KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerButton,
    PointerEvent, Timestamp, UiState,
};

type Harness = zgui_platform_headless::Harness<zgui_runtime::Runtime>;

fn field(text: &'static str, width: usize, cancel: bool) -> (Harness, NodeRef) {
    let node = NodeRef::new();
    let css = format!(
        "root {{display:block;width:400px;height:300px;padding:12px 20px}} editor {{display:block;width:{width}px;height:180px;white-space:pre-wrap}} "
    );
    let mut app = support::app_with_text(&css, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::editor()
                        .node_ref(node)
                        .on(zgui_view::events::POINTER_DOWN, move |event| {
                            if cancel {
                                event.prevent_default();
                            }
                        })
                        .child(text),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);
    (app, node)
}
fn point(
    app: &mut Harness,
    action: PointerAction,
    x: f32,
    y: f32,
    time: u64,
    button: PointerButton,
) {
    let mut event = PointerEvent::mouse(Point::new(CssPx(x), CssPx(y)));
    event.button = Some(button);
    app.deliver_to_first(SurfaceEvent::Pointer {
        action,
        event,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN + Duration::from_millis(time),
    });
    app.settle(8);
}
fn click(app: &mut Harness, x: f32, y: f32, time: u64) {
    point(
        app,
        PointerAction::Pressed,
        x,
        y,
        time,
        PointerButton::Primary,
    );
    point(
        app,
        PointerAction::Released,
        x,
        y,
        time + 10,
        PointerButton::Primary,
    );
}
fn location(app: &Harness, column: f32, line: usize) -> (f32, f32) {
    let mut lines = support::lines_on_device(&app.app().windows()[0]);
    lines.sort_by(|a, b| a.origin.y.0.total_cmp(&b.origin.y.0));
    let rect = lines[line];
    (
        rect.origin.x.0 + column * 8.0,
        rect.origin.y.0 + rect.size.height.0 / 2.0,
    )
}

#[test]
fn double_selects_word_triple_selects_paragraph_and_typing_replaces_selection() {
    let (mut app, node) = field("one two three", 300, false);
    let (x, y) = location(&app, 5.2, 0);
    click(&mut app, x, y, 0);
    assert_eq!(node.selection(), Some(5..5));
    click(&mut app, x, y, 150);
    assert_eq!(node.selection(), Some(4..7));
    // Pointer movement at same hit must not collapse word selection.
    point(
        &mut app,
        PointerAction::Moved,
        x,
        y,
        200,
        PointerButton::Primary,
    );
    assert_eq!(node.selection(), Some(4..7));
    click(&mut app, x, y, 300);
    assert_eq!(node.selection(), Some(0..13));
    app.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::character("x"),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN + Duration::from_millis(350),
    });
    app.settle(8);
    assert_eq!(node.selection(), Some(1..1));
    assert_eq!(node.text_content(), "x");
}

#[test]
fn word_drag_extends_whole_words_in_both_directions() {
    let (mut app, node) = field("one two three", 300, false);
    let (x, y) = location(&app, 5.2, 0);
    click(&mut app, x, y, 0);
    point(
        &mut app,
        PointerAction::Pressed,
        x,
        y,
        150,
        PointerButton::Primary,
    );
    let (right, _) = location(&app, 10.2, 0);
    point(
        &mut app,
        PointerAction::Moved,
        right,
        y,
        180,
        PointerButton::Primary,
    );
    assert_eq!(node.selection(), Some(4..13));
    let (left, _) = location(&app, 1.2, 0);
    point(
        &mut app,
        PointerAction::Moved,
        left,
        y,
        200,
        PointerButton::Primary,
    );
    assert_eq!(node.selection(), Some(0..7));
    point(
        &mut app,
        PointerAction::Released,
        left,
        y,
        220,
        PointerButton::Primary,
    );
    assert_eq!(node.selection(), Some(0..7));
}

#[test]
fn triple_selection_takes_the_paragraph_across_soft_wraps_and_drags_by_paragraphs() {
    // The first paragraph wraps onto three lines at 64px; the second paragraph is 19..33.
    let (mut app, node) = field("one two three four\nfive six seven", 64, false);
    let (x, y) = location(&app, 2.2, 1);
    for t in [0, 150, 300] {
        click(&mut app, x, y, t);
    }
    assert_eq!(node.selection(), Some(0..18), "Whole first paragraph");
    let last = support::lines_on_device(&app.app().windows()[0]).len() - 1;
    let (x, y) = location(&app, 2.2, last);
    click(&mut app, x, y, 1000);
    click(&mut app, x, y, 1150);
    point(
        &mut app,
        PointerAction::Pressed,
        x,
        y,
        1300,
        PointerButton::Primary,
    );
    assert_eq!(node.selection(), Some(19..33), "Whole second paragraph");
    let (left, top) = location(&app, 1.2, 0);
    point(
        &mut app,
        PointerAction::Moved,
        left,
        top,
        1350,
        PointerButton::Primary,
    );
    assert_eq!(node.selection(), Some(0..33));
}

#[test]
fn a_key_pressed_during_a_drag_leaves_the_drag_live() {
    let (mut app, node) = field("one two three", 300, false);
    let (x, y) = location(&app, 0.2, 0);
    point(
        &mut app,
        PointerAction::Pressed,
        x,
        y,
        0,
        PointerButton::Primary,
    );
    let (middle, _) = location(&app, 5.0, 0);
    point(
        &mut app,
        PointerAction::Moved,
        middle,
        y,
        50,
        PointerButton::Primary,
    );
    app.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::named(NamedKey::Shift, PhysicalKey::Code(KeyCode::ShiftLeft)),
        modifiers: Modifiers::SHIFT,
        timestamp: Timestamp::ORIGIN + Duration::from_millis(60),
    });
    app.settle(8);
    let (far, _) = location(&app, 12.0, 0);
    point(
        &mut app,
        PointerAction::Moved,
        far,
        y,
        80,
        PointerButton::Primary,
    );
    assert_eq!(node.selection(), Some(0..12));
}

#[test]
fn a_double_click_in_a_secure_field_selects_the_whole_value() {
    let node = NodeRef::new();
    let css = "root {display:block;width:400px;height:300px;padding:12px 20px} field {display:block;width:300px} ";
    let mut app = support::app_with_text(css, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::field()
                        .node_ref(node)
                        .secure(true)
                        .child("correct horse"),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);
    let (x, y) = location(&app, 2.2, 0);
    click(&mut app, x, y, 0);
    click(&mut app, x, y, 150);
    // A run would show where the value has a space.
    assert_eq!(node.selection(), Some(0..13));
}

#[test]
fn click_timeout_distance_secondary_and_cancellation_do_not_select_words() {
    let (mut app, node) = field("one two three", 300, false);
    let (x, y) = location(&app, 5.2, 0);
    click(&mut app, x, y, 0);
    click(&mut app, x, y, 900);
    assert_eq!(node.selection(), Some(5..5));
    let (far, _) = location(&app, 9.2, 0);
    click(&mut app, far, y, 1000);
    assert_eq!(node.selection(), Some(9..9));
    point(
        &mut app,
        PointerAction::Pressed,
        far,
        y,
        1100,
        PointerButton::Secondary,
    );
    assert_eq!(node.selection(), Some(9..9));
    click(&mut app, far, y, 1200);
    assert_eq!(node.selection(), Some(9..9));
    point(
        &mut app,
        PointerAction::Cancelled,
        far,
        y,
        1250,
        PointerButton::Primary,
    );
    click(&mut app, far, y, 1300);
    assert_eq!(node.selection(), Some(9..9));
    app.deliver_to_first(SurfaceEvent::Focused(false));
    app.deliver_to_first(SurfaceEvent::Focused(true));
    app.settle(8);
    click(&mut app, far, y, 1400);
    assert_eq!(
        node.selection(),
        Some(9..9),
        "Surface focus loss clears click chain"
    );
}

#[test]
fn readonly_fields_select_but_disabled_and_cancelled_defaults_do_not() {
    for state in [UiState::READ_ONLY, UiState::DISABLED] {
        let (mut app, node) = field("one two", 300, false);
        let (x, y) = location(&app, 5.2, 0);
        let window = &app.app().windows()[0];
        let document = window.document().borrow();
        let index = document
            .store()
            .index_of(zgui_view_dom::id::to_document(node.get_untracked().unwrap()).unwrap())
            .unwrap();
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_state(index, state, true)
            })
            .unwrap();
        drop(document);
        app.settle(8);
        click(&mut app, x, y, 0);
        click(&mut app, x, y, 150);
        if state == UiState::READ_ONLY {
            assert_eq!(node.selection(), Some(4..7));
        } else {
            assert_eq!(node.selection(), None);
        }
    }
    let (mut app, node) = field("one two", 300, true);
    let (x, y) = location(&app, 5.2, 0);
    for t in [0, 150, 300] {
        click(&mut app, x, y, t);
    }
    assert!(node.selection().is_none_or(|range| range.is_empty()));
}

#[test]
fn triple_click_handles_empty_text_and_a_fourth_click() {
    let (mut app, node) = field("one two three", 300, false);
    let (x, y) = location(&app, 5.2, 0);
    for t in [0, 150, 300] {
        click(&mut app, x, y, t);
    }
    assert_eq!(node.selection(), Some(0..13));
    click(&mut app, x, y, 450);
    assert_eq!(
        node.selection(),
        Some(5..5),
        "Fourth press starts a new caret gesture"
    );
    let (mut app, node) = field("", 300, false);
    let rect = app.app().windows()[0].box_with_class("root").unwrap();
    for t in [0, 150, 300] {
        click(&mut app, rect.origin.x.0 + 24.0, rect.origin.y.0 + 16.0, t);
    }
    assert!(node.selection().is_none_or(|range| range.is_empty()));
}

#[test]
fn multi_click_selection_uses_css_coordinates_at_fractional_and_retina_scales() {
    use zgui_geom::{DevicePx, Size};
    for scale in [1.0_f64, 1.5, 2.0] {
        let (mut app, node) = field("one two three", 300, false);
        app.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
            scale_factor: scale,
            size: Size::new(
                DevicePx((400.0 * scale) as f32),
                DevicePx((300.0 * scale) as f32),
            ),
        });
        app.settle(8);
        let line = support::first_line_on_device(&app.app().windows()[0]);
        let x = line.origin.x.0 / scale as f32 + 5.2 * 8.0;
        let y = (line.origin.y.0 + line.size.height.0 / 2.0) / scale as f32;
        click(&mut app, x, y, 0);
        // Three CSS pixels stays within click slop even on Retina (six device pixels).
        click(&mut app, x + 3.0, y, 150);
        assert_eq!(node.selection(), Some(4..7), "Word selection at {scale}x");
        click(&mut app, x + 2.0, y, 300);
        assert_eq!(
            node.selection(),
            Some(0..13),
            "Paragraph selection at {scale}x"
        );
    }
}
