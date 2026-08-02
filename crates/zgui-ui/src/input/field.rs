//! What an [`Input`](crate::Input) and a [`Textarea`](crate::Textarea) share.
//!
//! # A field owns no text
//!
//! The text a person types lives in the document, in the text nodes under the editable element,
//! and the caret, the selection, the undo stack and any composition live in the framework's
//! editing model over them. A component here holds none of those. It declares the element, says
//! what the element means, hands the caller's value *into* the model with
//! [`NodeRef::set_value`](zgui::view::NodeRef::set_value), and hears what the user did back out of
//! it as an input event.
//!
//! That is not a stylistic preference. A second copy of the text is a second answer to *what does
//! this field say*, and the two are not written by the same code: the framework paints the caret
//! and the glyphs from the document while a private copy feeds the view. A field built that way
//! shows its starting value for ever, because every keystroke lands in the model nothing is drawn
//! from — and it paints two carets, one from each.

mod placeholder;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, StoredValue, UnsyncCallback};
use zgui::view::EventCx;
use zgui::vocab::UiState;
use zgui_ui_primitives::Binding;

/// Everything a field is built from.
pub(crate) struct Setup {
    /// The field's own element, which is what a value is written into.
    pub element: NodeRef,
    /// What the caller tied the text to, when the caller tied it to anything.
    pub value: Binding<String>,
    /// What it starts as, when the field owns it.
    pub default_value: Option<String>,
    /// What to show while it is empty.
    pub placeholder: Option<String>,
    /// Whether it can be typed into.
    pub disabled: Signal<bool, LocalStorage>,
    /// Whether its value can be changed.
    pub read_only: Signal<bool, LocalStorage>,
    /// Whether it must have a value.
    pub required: Signal<bool, LocalStorage>,
    /// Whether what it holds is wrong.
    pub invalid: Signal<bool, LocalStorage>,
    /// What it is called, for a reader.
    pub label: Option<String>,
    /// The element whose text names it.
    pub labelled_by: Option<NodeRef>,
    /// What it is, for a reader.
    pub role: Role,
}

/// The two things a field's view needs, once everything else has been wired up.
pub(crate) struct Wired {
    /// Everything that goes on the field's own element.
    pub own: Attrs,
    /// The text the element is built holding, which is the whole of what the view writes.
    ///
    /// Written once, as an ordinary child. Everything after it belongs to the editing model,
    /// because a view that rewrote this node would be replacing the very text the model is typing
    /// into.
    pub initial: String,
}

impl Setup {
    /// Declares the element and binds the caller's value to it.
    pub(crate) fn wire(self) -> Wired {
        let initial = self
            .value
            .get_untracked()
            .or_else(|| self.default_value.clone())
            .unwrap_or_default();
        self.follow_the_caller();
        Wired {
            own: self.attrs(),
            initial,
        }
    }

    /// Keeps a bound field's element in step with what the caller holds.
    ///
    /// Nothing at all for an unbound one: a field with no owner is driven by the person typing
    /// into it, and an effect writing its own last value back would be a second writer.
    fn follow_the_caller(&self) {
        let value = self.value;
        if !value.is_bound() {
            return;
        }
        let element = self.element;
        // Text the element already holds does nothing, so the ordinary loop — a keystroke
        // announced, the caller's value moving, the value arriving back here — leaves the caret
        // exactly where the person is typing.
        let follow = RenderEffect::new(move |_| {
            if let Some(text) = value.get() {
                element.set_value(&text);
            }
        });
        // Held for as long as the field is: an effect whose handle is dropped stops running, and a
        // field bound to a dropped one follows its signal precisely once.
        zgui::reactive::on_cleanup_local(move || drop(follow));
    }

    /// Everything the field's element carries.
    fn attrs(&self) -> Attrs {
        let Self {
            disabled,
            read_only,
            required,
            invalid,
            role,
            ..
        } = *self;

        let mut semantics = A11yBinding::new(role)
            .disabled(move || disabled.get())
            .read_only(move || read_only.get())
            .required(move || required.get())
            // Only set when it is: an accessibility tree says nothing about a control whose value
            // is fine, and reporting `valid` on every field is noise a reader has to hear past.
            .step(move |a11y| {
                if invalid.get() {
                    a11y.invalid(zgui::vocab::Invalid::True)
                } else {
                    a11y
                }
            });
        // No value is declared here. A field's value is its own text, the accessibility projection
        // reads it from there, and declaring one would be this component answering a question it
        // deliberately does not hold the answer to.
        if let Some(text) = &self.placeholder {
            semantics = semantics.placeholder(text.clone());
        }
        if let Some(text) = &self.label {
            semantics = semantics.label(text.clone());
        }
        if let Some(target) = self.labelled_by {
            semantics = semantics.labelled_by(target);
        }

        let own = Attrs::new()
            .class_toggle(zgui::view::ClassName::new("zui-field"), true)
            .state(UiState::DISABLED, move || disabled.get())
            .state(UiState::READ_ONLY, move || read_only.get())
            .state(UiState::REQUIRED, move || required.get())
            .state(UiState::INVALID, move || invalid.get())
            .a11y_from(semantics);
        placeholder::declared(own, self.placeholder.as_deref())
    }
}

/// The handler that carries the framework's own announcement back to whoever holds the text.
///
/// Every change to the text arrives here, whoever caused it and whichever way — a key, a paste, an
/// input method's provisional text — because the framework announces the value it wrote. A field
/// that watched keys instead would miss all but the first.
///
/// The binding is written first and `on_change` told after, so a caller who bound a writable
/// signal finds it holding what the field says without writing a callback to copy it across.
pub(crate) fn reporting(
    value: Binding<String>,
    on_change: Option<UnsyncCallback<String>>,
) -> impl Fn(&mut EventCx<'_, events::Input>) + 'static {
    let held = StoredValue::new_local(on_change);
    move |cx: &mut EventCx<'_, events::Input>| {
        let Some(payload) = cx.payload().as_value() else {
            return;
        };
        let text = payload.value.to_string();
        if value.get_untracked().is_none_or(|held| held != text) {
            value.write(text.clone());
        }
        if let Some(Some(on_change)) = held.try_get_value() {
            on_change.run(text);
        }
    }
}
