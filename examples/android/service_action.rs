//! One explicit acceptance row: launch operation, then manually verify result.
use zgui::prelude::*;
use zgui_ui::prelude::*;

#[component]
pub(super) fn ServiceAction(label: &'static str, action: &'static str) -> impl IntoView {
    let state = crate::lab::State::current();
    let checked = Signal::derive_local(move || {
        if state
            .service_checks
            .with(|checks| checks.iter().any(|check| check == action))
        {
            Checked::Yes
        } else {
            Checked::No
        }
    });
    view! {
        row(class = "setting") {
            Checkbox(
                checked = Binding::controlled(checked, move |value: Checked| state.service_checks.update(|checks| {
                    checks.retain(|check| check != action);
                    if value == Checked::Yes { checks.push(action.into()); }
                })),
                a11y:label = format!("Verified: {label}"),
                attr:data-verified = action,
            )
            Button(attr:data-service = action, on:click = move |_| super::invoke(action, state.service_report)) { {label} }
        }
    }
}
