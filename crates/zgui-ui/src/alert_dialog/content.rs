//! The surface an alert dialog puts its question on.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::ClassName;
use zgui::{component, view};

use zgui::view::AttrName;

use crate::alert_dialog::style::AlertDialogStyle;
use crate::alert_dialog::{AlertDialogSize, SHEET as ALERT_SHEET};
use crate::dialog::{DialogStyle, SHEET};
use crate::overlay::{ModalSurfaceProps, OverlayState, SurfaceLabels};

/// The alert dialog itself: a surface over a dimmed window that a press past it does not answer.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     AlertDialog {
///         AlertDialogTrigger {"Delete"}
///         AlertDialogContent {
///             AlertDialogTitle {"Delete this?"}
///             AlertDialogFooter {
///                 AlertDialogCancel {"No"}
///                 AlertDialogAction {"Yes"}
///             }
///         }
///     }
/// }
/// # }
/// ```
#[component]
pub fn AlertDialogContent(
    /// Whether <kbd>Escape</kbd> closes it.
    ///
    /// It does unless this says otherwise. A surface a keyboard user cannot leave is a trap, and
    /// the case for taking Escape away — that the answer must be deliberate — is answered by the
    /// press outside not counting, which is what distinguishes this from a dialog.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_escape: Signal<bool, LocalStorage>,
    /// How much room it takes, which follows from how much there is to read in it.
    #[prop(default = AlertDialogSize::Default)]
    size: AlertDialogSize,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the surface.
    #[prop(attrs)]
    attrs: Attrs,
    /// The question and the answers.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, DialogStyle::CSS);
    install_stylesheet(ALERT_SHEET, AlertDialogStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let labels = SurfaceLabels::current().unwrap_or_default();

    let own = Attrs::new()
        .class_toggle(ClassName::new(DialogStyle::CLASS), true)
        .class_toggle(ClassName::new(AlertDialogStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-dialog"), true)
        .class_toggle(ClassName::new("zui-alert-dialog"), true)
        .attribute(AttrName::new("data-size"), size.name())
        .a11y_from(
            A11yBinding::unspecified()
                .labelled_by(labels.title())
                .described_by(labels.description()),
        );

    view! {
        ModalSurface(
            state = state,
            role = {Role::AlertDialog},
            dismiss_on_outside_press = {false},
            dismiss_on_escape = dismiss_on_escape,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.view()}
        }
    }
}
