//! A stack of sections of which one, or several, may be open at a time.

mod content;
mod item;
mod style;
mod trigger;

pub use crate::accordion::content::{AccordionContent, AccordionContentProps};
pub use crate::accordion::item::{AccordionItem, AccordionItemProps};
pub use crate::accordion::style::AccordionStyle;
pub use crate::accordion::trigger::{AccordionTrigger, AccordionTriggerProps};

use std::collections::BTreeSet;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::Orientation;
use zgui_ui_primitives::prelude::*;

/// What the accordion's rules are installed under.
pub(crate) const SHEET: &str = "zui-accordion";

/// How many of an accordion's sections may be open at once.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum AccordionSelection {
    /// One, and opening another closes the first.
    #[default]
    Single,
    /// Any number of them.
    Multiple,
}

/// What an item reads to know whether it is open, and how to change that.
#[derive(Copy, Clone)]
pub struct AccordionContext {
    /// Which sections are open, owned by whoever asked to own them.
    values: Controllable<Vec<String>>,
    /// How many may be open at once.
    selection: AccordionSelection,
    /// Whether a single-selection accordion may end up with nothing open.
    collapsible: bool,
    /// Whether any of it can be operated.
    disabled: Signal<bool, LocalStorage>,
}

impl AccordionContext {
    /// The accordion the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Everything that is open right now.
    #[must_use]
    pub fn open(self) -> BTreeSet<String> {
        self.values.get().into_iter().collect()
    }

    /// Whether the section called `value` is open.
    #[must_use]
    pub fn is_open(self, value: &str) -> bool {
        self.open().contains(value)
    }

    /// Whether the whole accordion is out of action.
    #[must_use]
    pub fn is_disabled(self) -> bool {
        self.disabled.get()
    }

    /// How many sections may be open at once.
    #[must_use]
    pub fn selection(self) -> AccordionSelection {
        self.selection
    }

    /// Opens or closes the section called `value`.
    ///
    /// In a single-selection accordion opening one closes every other, and closing the only open
    /// one is refused unless the accordion was declared `collapsible` — which is the difference
    /// between a set of sections and a set of alternatives that always has an answer.
    pub fn set_open(self, value: &str, open: bool) {
        let mut next = self.open();
        if open {
            if self.selection == AccordionSelection::Single {
                next.clear();
            }
            next.insert(value.to_owned());
        } else {
            if self.selection == AccordionSelection::Single && !self.collapsible {
                return;
            }
            next.remove(value);
        }
        self.values.set(next.into_iter().collect());
    }
}

/// A stack of sections, each with a heading that opens it.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Three questions, one answer showing at a time.
/// #[component]
/// fn Questions() -> impl IntoView {
///     view! {
///         Accordion(default_value = vec!["shipping".to_owned()]) {
///             AccordionItem(value = "shipping") {
///                 AccordionTrigger {"When does it ship?"}
///                 AccordionContent {text {"Within two working days."}}
///             }
///             AccordionItem(value = "returns") {
///                 AccordionTrigger {"Can I send it back?"}
///                 AccordionContent {text {"For thirty days, unopened."}}
///             }
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// One tab stop for the whole accordion, exactly as the authoring practices ask: <kbd>↓</kbd> and
/// <kbd>↑</kbd> move between the headings, <kbd>Home</kbd> and <kbd>End</kbd> go to the first and
/// last, and <kbd>Enter</kbd> or <kbd>Space</kbd> opens whichever heading the arrows landed on.
/// Arrowing moves without opening anything, so a reader can survey the headings before choosing.
///
/// # What each section is built out of
///
/// A [`CollapsibleContext`](crate::CollapsibleContext), the same one [`Collapsible`](crate::Collapsible)
/// publishes — so the sliding, the measurement and the `--zui-collapsible-height` it is animated
/// with are one implementation rather than two that drift.
#[component]
pub fn Accordion(
    /// Which sections are open, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<Vec<String>>,
    /// Which start open, when the accordion owns that itself.
    #[prop(optional)]
    default_value: Option<Vec<String>>,
    /// Told whenever the set changes.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<Vec<String>>>,
    /// How many may be open at once.
    #[prop(default = AccordionSelection::Single)]
    selection: AccordionSelection,
    /// Whether a single-selection accordion may end up with nothing open.
    #[prop(default = true)]
    collapsible: bool,
    /// Whether any of it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Classes merged after the accordion's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The sections.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AccordionStyle::CSS);
    let context = AccordionContext {
        values: Controllable::new(value, default_value.unwrap_or_default(), on_change),
        selection,
        collapsible,
        disabled,
    };
    provide_local_context(context);

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-accordion"), true)
        .class_toggle(zgui::view::ClassName::new(AccordionStyle::CLASS), true);

    view! {
        RovingFocus(orientation = Orientation::Vertical, class = class, {..own}, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
