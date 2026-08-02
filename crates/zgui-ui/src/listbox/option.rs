//! One option, and what a listbox knows about it.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui_ui_primitives::ItemId;

/// What a listbox holds about one of its options.
///
/// The value is what choosing it reports; the text is what it reads as, which is what a typed
/// letter and a filter are matched against and what a closed trigger shows. They are two fields
/// because they are two things: an option worth `"gb"` reads as *United Kingdom*, and a listbox
/// that conflated them would either report a label or show a code.
#[derive(Clone)]
pub struct ListboxOption {
    /// The element the option rendered.
    node: NodeRef,
    /// What choosing it reports.
    value: String,
    /// What it reads as.
    text: String,
    /// Whether it can be chosen.
    disabled: Signal<bool, LocalStorage>,
    /// What choosing it does, beyond reporting the value.
    on_select: Option<UnsyncCallback<()>>,
}

impl ListboxOption {
    /// Describes one option.
    #[must_use]
    pub fn new(
        node: NodeRef,
        value: impl Into<String>,
        text: impl Into<String>,
        disabled: Signal<bool, LocalStorage>,
    ) -> Self {
        Self {
            node,
            value: value.into(),
            text: text.into(),
            disabled,
            on_select: None,
        }
    }

    /// Runs `on_select` whenever this option is chosen, however it was chosen.
    ///
    /// What a command palette needs and a select does not: a select reports a value and the
    /// caller acts on it, whereas an item labelled *Export as CSV* **is** the action, and there
    /// is no value anyone wants afterwards.
    #[must_use]
    pub fn on_select(mut self, on_select: UnsyncCallback<()>) -> Self {
        self.on_select = Some(on_select);
        self
    }

    /// Runs whatever choosing this option does.
    pub fn select(&self) {
        if let Some(on_select) = &self.on_select {
            on_select.run(());
        }
    }

    /// The element it rendered.
    #[must_use]
    pub fn node(&self) -> NodeRef {
        self.node
    }

    /// What choosing it reports.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// What it reads as.
    ///
    /// The text it was given, or — when it was given none — the text it actually renders, read
    /// back from the element. That fallback is what lets an option written as a plain string be
    /// shown on a closed trigger without saying its own label twice.
    #[must_use]
    pub fn text(&self) -> String {
        if self.text.is_empty() {
            self.node.text_content()
        } else {
            self.text.clone()
        }
    }

    /// Whether it can be chosen.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled.get()
    }
}

/// One option, paired with the name its listbox knows it by.
#[derive(Clone)]
pub struct ListboxEntry {
    /// What the listbox calls it.
    id: ItemId,
    /// What it is.
    option: ListboxOption,
}

impl ListboxEntry {
    /// Pairs a name with an option.
    #[must_use]
    pub fn new(id: ItemId, option: ListboxOption) -> Self {
        Self { id, option }
    }

    /// What the listbox calls it.
    #[must_use]
    pub fn id(&self) -> ItemId {
        self.id
    }

    /// What it is.
    #[must_use]
    pub fn option(&self) -> &ListboxOption {
        &self.option
    }
}
