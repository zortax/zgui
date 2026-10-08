//! A toast's action carries its tone where a style sheet selects on it.

mod harness;

use zgui::prelude::*;
use zgui::view;
use zgui_ui::prelude::*;

use crate::harness::Harness;

/// Announces two messages once the toaster stands: one with an ordinary action, one with a
/// destructive one.
#[component]
fn Announce() -> impl IntoView {
    if let Some(toasts) = use_toaster() {
        toasts.push(Toast::new("Scaled").persistent().action("Undo", || {}));
        toasts.push(
            Toast::new("Two pods stay behind")
                .persistent()
                .destructive_action("Delete them", || {}),
        );
    }
    view! { box {} }
}

#[test]
fn a_toast_action_says_whether_it_destroys_something() {
    let harness = Harness::open();
    harness.mount(|| view! { Toaster { Announce() } });
    let mut tones: Vec<Option<String>> = harness
        .find_all("zui-toast__action")
        .into_iter()
        .map(|node| harness.attribute(node, "data-tone"))
        .collect();
    tones.sort();
    assert_eq!(
        tones,
        [Some("default".to_owned()), Some("destructive".to_owned())]
    );
}
