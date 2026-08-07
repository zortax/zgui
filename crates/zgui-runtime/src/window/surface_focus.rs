//! What a window does when it stops being the one being typed into, and when it becomes it again.
//!
//! Keyboard focus is two things at once and they are not the same thing. Inside the document one
//! element has focus, and that element keeps it: a person who alt-tabs away and back is still in
//! the field they were in, with the caret where they left it. Outside the document the *surface*
//! either is or is not receiving the keyboard, and that is what this is about.
//!
//! Losing it ends three things that no later event will ever come back and end.
//!
//! An input method's composition is the first. The window system takes the input method away with
//! the focus and sends nothing else — no commit, no dismissal — so provisional text left open stays
//! provisional for ever: the field is showing text that is in no undo entry and that every key
//! afterwards is refused for, because a model that believes it is composing must refuse keys.
//!
//! A pending edit is the second. Leaving a field announces the value the user settled on, and it is
//! the only moment a form has to validate on; a window whose field is left by the whole window going
//! away announced nothing, so the field the user typed into and then switched away from is a field
//! whose value was never committed.
//!
//! What the pointer left behind is the third. The button is released over some other window and the
//! cursor leaves without a leave event, so `:hover` and `:active` stay written on elements the
//! pointer is nowhere near — a control lit up under a window that is not even in front.

use zgui_platform::SurfaceEvent;
use zgui_vocab::Timestamp;

use crate::window::Window;

impl Window {
    /// Whether this event is the surface's keyboard focus changing, and what to.
    pub(crate) fn surface_focus_of(event: &SurfaceEvent) -> Option<bool> {
        match event {
            SurfaceEvent::Focused(focused) => Some(*focused),
            _ => None,
        }
    }

    /// Carries out what the surface gaining or losing the keyboard means.
    pub(crate) fn surface_focus_changed(&mut self, focused: bool, timestamp: Timestamp) {
        if focused {
            // Text input is asked for again for whatever still has focus. The window system
            // disabled it when it took the keyboard away, so a field returned to without this is a
            // field an input method will not start a composition in — which is a Japanese keyboard
            // that types nothing at all, in a field that looks exactly as it did before.
            self.report_text_input();
            if self.focus_is_editable() {
                self.carets.restart(self.clock.now());
            }
            return;
        }
        self.settle_focused_field(timestamp);
        self.disable_text_input();
        self.cancel_press();
        // The caret stops rather than merely being hidden. A blink is a deadline the loop parks on,
        // so a window that kept blinking behind whatever is in front of it would wake twice a
        // second, for ever, to draw an insertion point nobody can see.
        self.carets.stop();
    }

    /// Ends a composition and announces the value of whatever field is being typed into.
    ///
    /// In that order: the composition's text is part of the value that settles, and settling first
    /// would announce the text as it stood before the provisional text became real.
    fn settle_focused_field(&mut self, timestamp: Timestamp) {
        let Some(node) = self.router.interaction().focus.focused() else {
            return;
        };
        let edited = {
            let document = self.document.borrow();
            self.editors.end_composition(&document, node)
        };
        if let Some(selection) = edited.selection.clone() {
            self.host.write_selection(node, selection);
        }
        // The text a composition was showing is what the field now holds outright, and a value
        // bound to the field has already been told that text as provisional. Announcing it again
        // as an input event would be announcing a change that nothing can observe; the settled
        // value below is the announcement this moment owes.
        self.report_change(node, timestamp);
    }

    /// Tells the surface that no text is being typed, and forgets any composition it reported.
    ///
    /// Aimed at nothing rather than at what has focus: the element still has focus and will have it
    /// when the window comes back, but the surface has no keyboard to compose with in the meantime.
    fn disable_text_input(&mut self) {
        if let Some(told) = self.ime.focused(None, None) {
            self.surface.set_text_input(match told {
                zgui_input::ime::Told::Enabled(area) => Some(area),
                zgui_input::ime::Told::Disabled => None,
            });
        }
    }

    /// Whether what holds focus is something a caret belongs in.
    fn focus_is_editable(&self) -> bool {
        let Some(node) = self.router.interaction().focus.focused() else {
            return false;
        };
        let document = self.document.borrow();
        crate::editing::Editors::is_editable(&document, node)
    }

    /// Lets go of whatever the pointer is holding down, leaving focus and hover where they are.
    pub(crate) fn cancel_press(&mut self) {
        let filter = self.engine.filter();
        let document = self.document.borrow();
        self.router.cancel_press(&document, &filter);
    }
}
