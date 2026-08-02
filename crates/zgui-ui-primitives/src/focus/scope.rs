//! Confining focus to a subtree, and putting it back afterwards.

use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect};
use zgui::{component, view};

use crate::diag::note;

/// Confines keyboard navigation to its own subtree while `trapped` is on.
///
/// A modal surface that does not trap focus is a modal surface a keyboard user tabs straight out
/// of, into controls they cannot see and which are announced as though nothing had opened. The
/// trap is what makes a dialog a dialog.
///
/// Three behaviours come with it, and each is a field of [`FocusTrapOptions`]:
///
/// * **wrap** — tabbing past the last control goes back to the first, so there is no way out;
/// * **auto_focus** — focus moves inside as the trap goes up, so the first key press lands here;
/// * **restore** — focus goes back to whatever held it when the trap went up, so closing a dialog
///   returns the caret to the button that opened it.
///
/// [`FocusTrapOptions::MODAL`] is all three, which is what a dialog wants.
/// [`FocusTrapOptions::CONFINE_ONLY`] is only the first, which is what a menu opened from a
/// toolbar wants: it confines the arrow keys without taking the toolbar's focus away.
///
/// Traps stack, and the innermost wins — so a dialog opened from a dialog behaves, and closing the
/// inner one hands navigation back to the outer one rather than to the document.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui_primitives::prelude::*;
///
/// #[component]
/// fn Dialog(open: Signal<bool, zgui::reactive::LocalStorage>) -> impl IntoView {
///     view! {
///         Show(when = move || open.get(), fallback = || view! { box() }) {
///             FocusScope {
///                 control(tabindex = Focus::Sequential) {"Close"}
///             }
///         }
///     }
/// }
/// ```
#[component]
pub fn FocusScope(
    /// Whether the trap is in force. Turning it off releases the trap without unmounting anything.
    #[prop(into, default = Signal::stored_local(true))]
    trapped: Signal<bool, LocalStorage>,
    /// How the trap behaves while it is installed.
    #[prop(default = FocusTrapOptions::MODAL)]
    options: FocusTrapOptions,
    /// Where to record the element the trap is installed on.
    #[prop(optional)]
    element_ref: Option<NodeRef>,
    /// Extra classes on the scope's own element.
    #[prop(into, optional)]
    class: Classes,
    /// What is inside the trap.
    children: Children,
) -> impl IntoView {
    let root = element_ref.unwrap_or_default();
    let who = crate::diag::next_id();

    // The guard is what holds the trap; dropping it releases it and, when the trap asked to
    // restore, puts focus back. It is held across effect runs rather than re-created on each,
    // because installing a second trap over the first is a stack two deep for one dialog.
    let held: Rc<RefCell<Option<FocusTrap>>> = Rc::new(RefCell::new(None));
    let installed = {
        let held = Rc::clone(&held);
        RenderEffect::new(move |_| {
            // The handle binds as the element is built, so the first run of this effect may find
            // nothing. Reading it here is what brings the effect back when it does bind.
            let node = root.get();
            let bound = node.is_some();
            if trapped.get() && bound {
                if held.borrow().is_none() {
                    *held.borrow_mut() = root.trap_focus(options);
                    note!(
                        "focus.trap",
                        "who={who} node={node:?} installed={}",
                        held.borrow().is_some()
                    );
                }
            } else {
                // Dropped outside the borrow: releasing a trap can move focus, and moving focus
                // can re-enter anything holding this.
                let released = held.borrow_mut().take();
                if released.is_some() {
                    note!("focus.release", "who={who} node={node:?} by=effect");
                }
                drop(released);
            }
        })
    };
    on_cleanup_local(move || {
        note!("focus.release", "who={who} by=cleanup");
        drop(installed);
    });

    view! {
        box(class = class, node_ref = root, attr:data-focus-scope = "") {
            {children.into_view_once()}
        }
    }
}
