//! A section with a trigger that shows and hides it.

mod content;
mod style;
mod trigger;

pub use crate::collapsible::content::{CollapsibleContent, CollapsibleContentProps};
pub use crate::collapsible::style::CollapsibleStyle;
pub use crate::collapsible::trigger::{CollapsibleTrigger, CollapsibleTriggerProps};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::{Binding, Controllable};

/// What the collapsible's rules are installed under.
pub(crate) const SHEET: &str = "zui-collapsible";

/// What a trigger and its content read to know whether they are showing.
///
/// Published by [`Collapsible`], and by anything else that is a disclosure underneath —
/// [`AccordionItem`](crate::AccordionItem) publishes one of these, which is why the accordion's
/// content is the collapsible's content with another class on it rather than a second
/// implementation of the same measurement.
///
/// `Copy`, so it can be captured by as many closures as a part has.
#[derive(Copy, Clone)]
pub struct CollapsibleContext {
    /// Whether the content is showing.
    open: Controllable<bool>,
    /// Whether the trigger can be operated.
    disabled: Signal<bool, LocalStorage>,
    /// The element that opens and closes it.
    trigger: NodeRef,
    /// The element that is shown and hidden.
    content: NodeRef,
}

impl CollapsibleContext {
    /// Wires up a disclosure from the three props every one of them takes.
    ///
    /// The two element handles are minted here rather than taken, because the only thing anybody
    /// does with them is relate the trigger to the content and back — and a caller that had to
    /// supply them could supply one of them twice.
    #[must_use]
    pub fn new(open: Controllable<bool>, disabled: Signal<bool, LocalStorage>) -> Self {
        Self {
            open,
            disabled,
            trigger: NodeRef::new(),
            content: NodeRef::new(),
        }
    }

    /// Publishes this to every scope below the current one, and hands it back.
    #[must_use]
    pub fn provide(self) -> Self {
        provide_local_context(self);
        self
    }

    /// The disclosure the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether the content is showing.
    #[must_use]
    pub fn is_open(self) -> bool {
        self.open.get()
    }

    /// Whether the trigger can be operated.
    #[must_use]
    pub fn is_disabled(self) -> bool {
        self.disabled.get()
    }

    /// How the state is written as an attribute value, which is what a style sheet selects on.
    #[must_use]
    pub fn state_name(self) -> &'static str {
        if self.is_open() { "open" } else { "closed" }
    }

    /// The element that opens and closes the content.
    #[must_use]
    pub fn trigger(self) -> NodeRef {
        self.trigger
    }

    /// The element that is shown and hidden.
    #[must_use]
    pub fn content(self) -> NodeRef {
        self.content
    }

    /// Shows the content if it was hidden, and hides it if it was showing.
    ///
    /// Does nothing while the disclosure is disabled, so a trigger does not have to ask twice.
    pub fn toggle(self) {
        if self.disabled.get_untracked() {
            return;
        }
        self.open.toggle();
    }

    /// Shows or hides the content outright.
    pub fn set_open(self, open: bool) {
        if self.disabled.get_untracked() {
            return;
        }
        self.open.set(open);
    }
}

/// A section with a trigger that shows and hides it.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// The details of a delivery, folded away until they are asked for.
/// #[component]
/// fn Delivery() -> impl IntoView {
///     let showing = RwSignal::new_local(false);
///     view! {
///         Collapsible(open = showing) {
///             CollapsibleTrigger {"Delivery details"}
///             CollapsibleContent {
///                 text {"Arrives Thursday, signed for."}
///             }
///         }
///     }
/// }
/// ```
///
/// # The height nobody wrote down
///
/// A section that slides open has to slide to *some* height, and a number in a style sheet is a
/// number that is wrong the moment the content changes. So the content measures itself through the
/// observation channel and publishes what it found as `--zui-collapsible-height`, and the sheet
/// animates to that. Nothing in Rust knows how tall anything is, and nothing in CSS is guessed —
/// see [`CollapsibleContent`].
///
/// # Keyboard
///
/// <kbd>Enter</kbd> and <kbd>Space</kbd> on the trigger, which is the framework's own activation
/// of whatever has focus rather than anything this component listens for.
#[component]
pub fn Collapsible(
    /// Whether the content is showing, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Whether it starts showing, when the collapsible owns that itself.
    #[prop(default = false)]
    default_open: bool,
    /// Told whenever it opens or closes, whoever owns it.
    #[prop(optional)]
    on_open_change: Option<UnsyncCallback<bool>>,
    /// Whether the trigger can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the collapsible's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The trigger and the content.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, CollapsibleStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let context = CollapsibleContext::new(
        Controllable::new(open, default_open, on_open_change),
        disabled,
    )
    .provide();

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-collapsible"), true)
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(context.state_name().to_owned())
        });

    view! {
        column(node_ref = element, class = CollapsibleStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
