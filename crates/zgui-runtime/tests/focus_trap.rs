//! Tabbing around a modal surface in a real window.
//!
//! A dialog that can be tabbed out of is not modal, and nothing on the screen says so: focus lands
//! on a control behind the backdrop, the keyboard operates something invisible, and every
//! screenshot of it looks right. The script below is therefore the whole of the behaviour — tab
//! forwards past the end, tab backwards past the beginning, and dismiss — driven by real key events
//! through the real dispatcher over a real document.
//!
//! Every node is named by the [`NodeRef`](zgui_view::NodeRef) the view bound to it rather than by a
//! selector, so what the assertions compare is the identity the framework itself handed back.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_platform::SurfaceEvent;
use zgui_reactive::prelude::GetUntracked;
use zgui_view::{FocusTrap, FocusTrapOptions, NodeId, NodeRef, ViewHost};
use zgui_vocab::{Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp};

/// A trigger outside the dialog and three controls inside it, all focusable.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
control { display: block; width: 100px; height: 24px }
dialog { display: block; width: 300px; height: 200px }
";

/// What one scripted run holds on to.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The control that opens the dialog.
    trigger: NodeRef,
    /// The dialog itself, which is what the trap is installed over.
    dialog: NodeRef,
    /// The three controls inside it, in document order.
    inside: [NodeRef; 3],
    /// The installed trap, shared with the handler that dismisses it.
    guard: Rc<RefCell<Option<FocusTrap>>>,
}

impl Script {
    /// Presses one key and lets the frames it produced settle.
    fn press(&mut self, key: NamedKey, code: KeyCode, modifiers: Modifiers) {
        self.harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(key, PhysicalKey::Code(code)),
            modifiers,
            timestamp: Timestamp::ORIGIN,
        });
        self.harness.settle(8);
    }

    /// Which node holds focus, as the framework reports it to a view.
    fn focused(&self) -> Option<NodeId> {
        self.harness
            .app()
            .windows()
            .first()
            .expect("a window")
            .host()
            .focused()
            .get_untracked()
    }

    /// Which of the named nodes holds focus, for a legible assertion.
    fn where_focus_is(&self) -> &'static str {
        let at = self.focused();
        if at == self.trigger.get() {
            "trigger"
        } else if at == self.inside[0].get() {
            "first"
        } else if at == self.inside[1].get() {
            "second"
        } else if at == self.inside[2].get() {
            "third"
        } else if at.is_none() {
            "nothing"
        } else {
            "somewhere else"
        }
    }
}

/// A window with a trigger and a dialog whose controls a trap can be installed over.
fn scripted() -> Script {
    let trigger = NodeRef::new();
    let dialog = NodeRef::new();
    let inside = [NodeRef::new(), NodeRef::new(), NodeRef::new()];
    let guard: Rc<RefCell<Option<FocusTrap>>> = Rc::new(RefCell::new(None));

    let dismisser = Rc::clone(&guard);
    let harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let dismisser = Rc::clone(&dismisser);
        Box::new(
            zgui_elements::column()
                .class("root")
                .on(
                    zgui_view::events::KEY_DOWN,
                    move |cx: &mut zgui_view::EventCx<'_, _>| {
                        if cx.key == Key::Named(NamedKey::Escape) {
                            // Dropping the guard is the dismissal a dismissable layer performs.
                            // Putting focus back where it came from is the framework's half of it,
                            // and is what the assertion below is about.
                            dismisser.borrow_mut().take();
                        }
                    },
                )
                .child(zgui_elements::control().node_ref(trigger))
                .child(
                    zgui_elements::column()
                        .node_ref(dialog)
                        .child(zgui_elements::control().node_ref(inside[0]))
                        .child(zgui_elements::control().node_ref(inside[1]))
                        .child(zgui_elements::control().node_ref(inside[2])),
                )
                .into_view()
                .build(cx),
        )
    });

    Script {
        harness,
        trigger,
        dialog,
        inside,
        guard,
    }
}

#[test]
fn tab_never_leaves_the_trap_shift_tab_wraps_backwards_and_escape_restores_focus() {
    let mut script = scripted();
    script.harness.settle(8);

    // Tab into the window, exactly as a person would. The trigger is the first focusable there is.
    script.press(NamedKey::Tab, KeyCode::Tab, Modifiers::NONE);
    assert_eq!(script.where_focus_is(), "trigger");

    // Opening the dialog is one call, and it is the one `NodeRef::trap_focus` makes.
    *script.guard.borrow_mut() = script.dialog.trap_focus(FocusTrapOptions::MODAL);
    script.harness.settle(8);
    assert_eq!(
        script.where_focus_is(),
        "first",
        "a modal trap focuses the first control in its subtree as it is installed"
    );

    // Tab past the end, several times over. Every landing is inside the dialog.
    let mut visited = Vec::new();
    for _ in 0..9 {
        script.press(NamedKey::Tab, KeyCode::Tab, Modifiers::NONE);
        let at = script.where_focus_is();
        assert!(
            ["first", "second", "third"].contains(&at),
            "tabbing left the trap and landed on {at}"
        );
        visited.push(at);
    }
    assert_eq!(
        visited,
        [
            "second", "third", "first", "second", "third", "first", "second", "third", "first"
        ],
        "the traversal did not cycle through the trap's own controls in order"
    );

    // And backwards, past the beginning.
    script.press(NamedKey::Tab, KeyCode::Tab, Modifiers::SHIFT);
    assert_eq!(
        script.where_focus_is(),
        "third",
        "shift-tab from the first control must wrap to the last, not leave the dialog"
    );

    script.press(NamedKey::Escape, KeyCode::Escape, Modifiers::NONE);
    assert_eq!(
        script.where_focus_is(),
        "trigger",
        "dismissing the dialog dropped focus instead of giving it back to what opened it"
    );

    // With the trap gone, traversal reaches the rest of the document again.
    script.press(NamedKey::Tab, KeyCode::Tab, Modifiers::NONE);
    assert_eq!(
        script.where_focus_is(),
        "first",
        "with nothing installed the whole document is reachable again, in document order"
    );
    script.press(NamedKey::Tab, KeyCode::Tab, Modifiers::SHIFT);
    assert_eq!(
        script.where_focus_is(),
        "trigger",
        "and traversal crosses back out of the dialog, which a live trap would have refused"
    );
}
