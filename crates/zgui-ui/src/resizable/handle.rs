//! The divider between two panels.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::ui::ELLIPSIS;
use zgui_ui_primitives::Orientation;

use crate::resizable::style::ResizableStyle;
use crate::resizable::{ResizableContext, SHEET};

/// The divider between two panels of a [`ResizablePanelGroup`](crate::ResizablePanelGroup).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ResizablePanelGroup {
///         ResizablePanel(default_size = 50.0) {text {"Left"}}
///         ResizableHandle(label = "Resize the panels", step = 5.0, with_handle = true)
///         ResizablePanel(default_size = 50.0) {text {"Right"}}
///     }
/// }
/// # }
/// ```
///
/// # Dragging
///
/// The press captures the pointer, so the drag carries on when the pointer leaves the divider —
/// which it will, because a divider is nine pixels wide and a hand is not that steady. The
/// distance moved is measured against the group's own box and turned into percentage points, so
/// the same drag means the same thing whatever the window is doing, and the pointer's CSS pixels
/// are divided by the window's scale on the way, because the group's box is in device pixels.
///
/// # Keyboard
///
/// The arrow keys for the group's axis move the divider one `step`; <kbd>Home</kbd> and
/// <kbd>End</kbd> take the panel before it to its smallest and largest; <kbd>Enter</kbd> folds
/// that panel away and brings it back at the size it had.
#[component]
pub fn ResizableHandle(
    /// How far one key press moves the divider, in percentage points.
    #[prop(default = 5.0)]
    step: f64,
    /// Whether to draw a grip straddling the rule.
    #[prop(default = false)]
    with_handle: bool,
    /// What the divider is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the divider's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, ResizableStyle::CSS);
    let context = ResizableContext::current();
    let id = context.map(ResizableContext::register_handle);
    let direction = context.map_or(Orientation::Horizontal, ResizableContext::direction);
    let vertical = matches!(direction, Orientation::Vertical);

    let travel = move || {
        let (context, id) = (context?, id?);
        context.travel_of(id)
    };
    let move_by = move |delta: f64| {
        if let (Some(context), Some(id)) = (context, id) {
            context.drag_by(id, delta);
        }
    };
    let move_to = move |size: f64| {
        if let (Some(context), Some(id)) = (context, id) {
            context.set_before(id, size);
        }
    };

    // Where the pointer was when the drag started, and what the panel's share was then, so that
    // the drag is measured from where it began rather than accumulating a rounding error per frame.
    let from = RwSignal::new_local(None::<(f32, f64)>);
    // What the panel's share was before it was folded away, so it can come back to it.
    let folded = RwSignal::new_local(None::<f64>);

    let group_length = move || {
        let box_ = context?.group().bounds()?;
        let length = if vertical {
            box_.size.height.0
        } else {
            box_.size.width.0
        };
        (length > 0.0).then_some(length)
    };
    let scale = move || {
        let scale = context.map_or(1.0, |context| context.group().scale());
        if scale > 0.0 { scale } else { 1.0 }
    };

    let semantics = A11yBinding::new(Role::Splitter)
        .orientation(if vertical {
            zgui::vocab::Orientation::Vertical
        } else {
            zgui::vocab::Orientation::Horizontal
        })
        .numeric_value(move || travel().map_or(0.0, |(now, _, _)| now))
        .step(move |a11y| {
            let (_, min, max) = travel().unwrap_or((0.0, 0.0, 100.0));
            a11y.numeric_range(min, max).numeric_step(step)
        });
    let semantics = match label {
        Some(text) => semantics.label(text),
        None => semantics,
    };

    let own = Attrs::new()
        .attribute(
            zgui::view::AttrName::new("data-direction"),
            direction.name(),
        )
        .attribute(zgui::view::AttrName::new("data-dragging"), move || {
            from.get().is_some().then(|| "true".to_owned())
        })
        .a11y_from(semantics);

    // Hidden from a reader: the divider it sits on is already announced as a splitter with a
    // position and a range, and the grip is the drawing that says a rule can be dragged.
    let grip = with_handle.then(|| {
        AnyView::new(view! {
            box(
                class = "zui-resizable__grip",
                {..Attrs::new().a11y_from(A11yBinding::unspecified().hidden(true))}
            ) {
                Icon(icon = ELLIPSIS)
            }
        })
    });

    view! {
        control(
            class = "zui-resizable__handle",
            tabindex = {Focus::Sequential},
            on:key_down = move |ev| {
                let forwards = if vertical { NamedKey::ArrowDown } else { NamedKey::ArrowRight };
                let backwards = if vertical { NamedKey::ArrowUp } else { NamedKey::ArrowLeft };
                let (_, min, max) = travel().unwrap_or((0.0, 0.0, 100.0));
                match &ev.key {
                    Key::Named(key) if *key == forwards => move_by(step),
                    Key::Named(key) if *key == backwards => move_by(-step),
                    Key::Named(NamedKey::Home) => move_to(min),
                    Key::Named(NamedKey::End) => move_to(max),
                    Key::Named(NamedKey::Enter) => match folded.get_untracked() {
                        Some(size) => {
                            folded.set(None);
                            move_to(size);
                        }
                        None => {
                            folded.set(travel().map(|(now, _, _)| now));
                            move_to(min);
                        }
                    },
                    _ => return,
                }
                // Claimed, so the same arrow key does not also move whatever contains the group.
                ev.prevent_default();
                ev.stop_propagation();
            },
            on:pointer_down = move |ev| {
                let Some((now, _, _)) = travel() else { return };
                let at = if vertical { ev.position.y.0 } else { ev.position.x.0 };
                ev.capture_pointer();
                from.set(Some((at, now)));
            },
            on:pointer_move = move |ev| {
                let (Some((start, was)), Some(length)) = (from.get_untracked(), group_length())
                else {
                    return;
                };
                let at = if vertical { ev.position.y.0 } else { ev.position.x.0 };
                let moved = f64::from((at - start) * scale() / length) * 100.0;
                move_to(was + moved);
            },
            on:pointer_up = move |ev| {
                from.set(None);
                ev.release_pointer();
            },
            on:pointer_cancel = move |ev| {
                from.set(None);
                ev.release_pointer();
            },
            {..own},
            {..attrs},
            class = class
        ) {
            // The pixel someone sees. The element around it is nine pixels of catchment, so the
            // line is drawn rather than being the element itself — see the sheet.
            box(
                class = "zui-resizable__line",
                {..Attrs::new().a11y_from(A11yBinding::unspecified().hidden(true))}
            ) {}
            {grip}
        }
    }
}
