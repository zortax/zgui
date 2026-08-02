//! The handle that makes a column wider or narrower.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui::{component, view};

/// The narrowest a column may be dragged, in CSS pixels.
///
/// A floor rather than a prop: a column dragged to nothing is a column nobody can find again, and
/// every table that allows it grows a "reset columns" button to undo it.
pub const MIN_WIDTH: f32 = 48.0;

/// A grip that resizes the column it sits in.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{RwSignal, UnsyncCallback};
/// use zgui::{component, view};
/// use zgui_ui::data_table::{ColumnResizer, ColumnResizerProps};
///
/// /// A header cell with a grip on its trailing edge.
/// #[component]
/// fn Header() -> impl IntoView {
///     let cell = NodeRef::new();
///     let width = RwSignal::new_local(160.0_f32);
///     view! {
///         box(node_ref = cell) {
///             "Name"
///             ColumnResizer(
///                 header = cell,
///                 label = "Name",
///                 on_resize = UnsyncCallback::new(move |next: f32| width.set(next))
///             )
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// A separator is a tab stop, and <kbd>←</kbd> and <kbd>→</kbd> move it by `step` pixels. A table
/// whose columns can only be dragged is a table whose columns cannot be resized without a pointer,
/// and the arrow keys cost one handler.
///
/// # Why the width is reported rather than applied
///
/// The grip knows how wide the column has become; it does not know where that width is kept. A
/// table stores one width per column key and rebuilds its track list from all of them, so a grip
/// that wrote a style on its own element would move the grip and leave the column where it was.
#[component]
pub fn ColumnResizer(
    /// The header cell whose width is being changed.
    header: NodeRef,
    /// What the column is called, so a reader knows what the separator moves.
    #[prop(into)]
    label: String,
    /// Told the column's new width in CSS pixels, as it changes.
    on_resize: UnsyncCallback<f32>,
    /// How far one arrow key moves the edge, in CSS pixels.
    #[prop(default = 8.0)]
    step: f32,
    /// Classes merged after the grip's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    // Where the drag started, and how wide the column was then. Held rather than recomputed,
    // because measuring during the drag would measure a column that has already moved.
    let origin: RwSignal<Option<(f32, f32)>, LocalStorage> = RwSignal::new_local(None);

    // A pointer reports where it is in CSS pixels and an element reports where it is in device
    // pixels; the surface's scale is what relates the two.
    let width_now = move || {
        let scale = header.scale();
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        header.bounds().map(|box_| box_.size.width.0 / scale)
    };

    let semantics = A11yBinding::new(Role::Splitter)
        .label(format!("Resize {label}"))
        .orientation(zgui::vocab::Orientation::Vertical);

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-data-table__grip"), true)
        .a11y_from(semantics);

    view! {
        control(
            tabindex = Focus::Sequential,
            on:pointer_down = move |ev| {
                let Some(width) = width_now() else { return };
                ev.capture_pointer();
                origin.set(Some((ev.position.x.0, width)));
                // The press belongs to the grip, not to the header it sits in: without this,
                // gripping a sortable column's edge would also sort by it.
                ev.stop_propagation();
                ev.prevent_default();
            },
            on:pointer_move = move |ev| {
                let Some((from, width)) = origin.get_untracked() else { return };
                on_resize.run((width + (ev.position.x.0 - from)).max(MIN_WIDTH));
            },
            on:pointer_up = move |ev| {
                origin.set(None);
                ev.release_pointer();
            },
            on:pointer_cancel = move |ev| {
                origin.set(None);
                ev.release_pointer();
            },
            on:key_down = move |ev| {
                let by = match &ev.key {
                    zgui::vocab::Key::Named(zgui::vocab::NamedKey::ArrowLeft) => -step,
                    zgui::vocab::Key::Named(zgui::vocab::NamedKey::ArrowRight) => step,
                    _ => return,
                };
                let Some(width) = width_now() else { return };
                on_resize.run((width + by).max(MIN_WIDTH));
                ev.prevent_default();
                ev.stop_propagation();
            },
            {..own},
            {..attrs},
            class = class
        )
    }
}
