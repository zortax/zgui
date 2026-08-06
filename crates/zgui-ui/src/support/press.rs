//! Controls that answer the press rather than the release.

use zgui::prelude::*;
use zgui::vocab::PointerButton;

/// A `pointer_down` handler that clicks the element the moment the button goes down.
///
/// The framework activates a control on the *release*, which is what lets somebody press a button,
/// think better of it and slide off before letting go. A menu, a select, a switch has no such
/// second thought to offer: the answer is wanted while the finger is still going down, and waiting
/// for the release makes every one of them feel a frame behind the hand.
///
/// This is the whole of that, and it is deliberately not a second copy of the control's behaviour.
/// It clicks the element early through the ordinary path, so the control's own `on:click` is still
/// the one place that says what activating it means — and <kbd>Enter</kbd>, <kbd>Space</kbd> and an
/// assistive technology's default action all reach that same handler, because each of those is a
/// click and none of them is a press.
///
/// Only the primary button. A secondary press is somebody asking for a context menu, and a control
/// that flipped itself on the way to one would be a control nobody can right-click.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::support::activate_on_press;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     control(
///         tabindex = {Focus::Sequential},
///         on:pointer_down = activate_on_press(),
///         on:click = move |_| { /* the one place the behaviour lives */ }
///     ) {"Flip"}
/// }
/// # }
/// ```
pub fn activate_on_press() -> impl Fn(&mut EventCx<'_, events::PointerDown>) + Copy + 'static {
    handler(
        events::POINTER_DOWN,
        move |ev: &mut EventCx<'_, events::PointerDown>| {
            if ev.button == Some(PointerButton::Primary) {
                ev.activate_now();
            }
        },
    )
}
