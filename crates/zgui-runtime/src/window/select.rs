//! Whether the pointer may select text where it is, which is what `user-select` decides.
//!
//! # Why this is a walk and every other property is a read
//!
//! `user-select` is not an inherited property, and its initial value is `auto` — which means *what
//! my parent's used value was*. So an element that says nothing has no answer of its own and the
//! answer is found above it, and an element that says `none` answers for everything below it that
//! says nothing. That is the shape the specification gives it, and it is the reason this is the one
//! property in the framework read by climbing rather than by looking.
//!
//! The walk is bounded by the tree and is taken once per press, never per frame: what it gates is a
//! pointer gesture, and a pointer gesture is a thing a person does a few times a second at most.

use zgui_css::values::ui::UserSelectValue;
use zgui_dom::NodeKey;

use crate::window::Window;

impl Window {
    /// Whether a pointer press at `node` may place a caret or start a selection.
    ///
    /// `user-select: none` is what a button, a label or a drag handle is written with, and what it
    /// is for is exactly this: a press on one stays a press, with no drag behind it to paint half
    /// the interface blue. Only the pointer default is gated. A keyboard still moves the caret, so a
    /// control that refuses the mouse is still reachable.
    pub(crate) fn selectable_at(&self, node: NodeKey) -> bool {
        !matches!(self.used_user_select(node), UserSelectValue::None)
    }

    /// The used value of `user-select` at one element.
    ///
    /// `auto` on an *editable* element is `text` and stops the walk, which is the specification's
    /// own first case: a field inside a panel that has switched selection off is still a field.
    fn used_user_select(&self, node: NodeKey) -> UserSelectValue {
        let document = self.document.borrow();
        let Some(mut index) = document.store().index_of(node) else {
            return UserSelectValue::Auto;
        };
        loop {
            let key = document.store().key_of(index);
            if let Some(style) = document.node(index).primary_style() {
                match style.get_ui().user_select {
                    UserSelectValue::Auto => {
                        if crate::editing::Editors::is_editable(&document, key) {
                            return UserSelectValue::Text;
                        }
                    }
                    settled => return settled,
                }
            }
            let Some(parent) = document.store().core(index).parent() else {
                return UserSelectValue::Auto;
            };
            index = parent;
        }
    }
}
