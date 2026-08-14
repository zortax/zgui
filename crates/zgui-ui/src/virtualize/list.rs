//! A long list, of which only the visible part exists.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::CustomPropertyName;
use zgui::{component, view};

use crate::virtualize::Virtualize;
use crate::virtualize::style::VirtualListStyle;

/// What the virtualised list's rules are installed under.
const SHEET: &str = "zui-virtual-list";

/// A list of any length that costs the same as a list the size of its scrollport.
///
/// The rows are built by the `row` closure, which is handed an index and returns a view. Only the
/// indices in view — plus `overscan` rows of slack at each edge — are ever asked for, so a list of
/// a hundred thousand rows builds a couple of dozen elements and replaces the rest with two lengths
/// of padding. Scrolling within one row rebuilds nothing at all; scrolling past one destroys one
/// row and builds one row.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Every line of a long log file.
/// #[component]
/// fn Log() -> impl IntoView {
///     let lines = RwSignal::new_local(100_000_usize);
///     view! {
///         VirtualList(
///             count = lines,
///             row_size = 22.0,
///             label = "Log",
///             row = move |index: usize| view! { text {{move || format!("line {index}")}} }
///         )
///     }
/// }
/// ```
///
/// # What a reader is told
///
/// A list whose rows are a window onto a longer one has to say so, or an assistive technology
/// announces "row 3 of 25" in a list of a hundred thousand. Each row therefore carries its true
/// position and the true length of the list, and the container is a list rather than a box.
///
/// # What a row is keyed by
///
/// Its position. Row 4 200 is row 4 200 whatever the list now holds, which is what makes a scroll
/// of one row cost one row and nothing else. A `row` closure that captures the data it draws
/// therefore reaches the rows on screen through a rebuild of the list, which is what a keyed list
/// builds every row again for. A `row` closure that reads a signal updates without one.
///
/// # Keyboard
///
/// None of its own. A virtualised list is a scroll container, and scrolling is the framework's;
/// rows that are operable are operable because the views `row` returns are, and each of them is an
/// ordinary tab stop while it exists. A list whose rows are focusable and whose window is thousands
/// of rows wide is a list where <kbd>Tab</kbd> cannot reach every row — which is a property of
/// virtualisation itself rather than of this component, and is why an operable virtualised list
/// wants a roving tab stop of its own.
#[component]
pub fn VirtualList<V, F>(
    /// How many rows there are in total.
    #[prop(into)]
    count: Signal<usize, LocalStorage>,
    /// How tall one row is, in CSS pixels.
    ///
    /// Declared rather than measured: the window is decided before its rows are built, so a height
    /// taken from the rows would mean building all of them to find out which to build. The list
    /// writes the declaration into `--zui-virtual-row`, which is what makes every row that tall, so
    /// a signal here moves the rows and the scroll extent together.
    #[prop(into, default = Signal::stored_local(32.0))]
    row_size: Signal<f32, LocalStorage>,
    /// How many rows beyond each edge of the port to build, so a fast scroll shows rows rather
    /// than a gap.
    #[prop(default = 4)]
    overscan: usize,
    /// What one row looks like.
    row: F,
    /// What the list is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record the scroll container, for a caller that wants to scroll it.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the list's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView
where
    V: IntoView + 'static,
    F: Fn(usize) -> V + 'static,
{
    install_stylesheet(SHEET, VirtualListStyle::CSS);
    let viewport = node_ref.unwrap_or_default();
    let seen = Virtualize::new(viewport, count, row_size, overscan);
    let row = Rc::new(row);

    let mut semantics = A11yBinding::new(Role::List);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-virtual-list"), true)
        .custom_property(CustomPropertyName::new("zui-virtual-row"), move || {
            Some(format!("{}px", row_size.get()))
        })
        .a11y_from(semantics);

    // Read once per frame into one value, so the two paddings and the row set are three reads of
    // one memo rather than three independent recomputations of the same window.
    let window = Signal::derive_local(move || seen.window());

    view! {
        scroll(node_ref = viewport, class = VirtualListStyle::CLASS, {..own}, {..attrs}, class = class) {
            box(
                class = "zui-virtual-list__pane",
                var:--zui-virtual-lead = move || Some(px(window.get().lead)),
                var:--zui-virtual-trail = move || Some(px(window.get().trail))
            ) {
                for index in move || window.get().indices(), key = |index: &usize| *index {
                    Row(index = index, total = count, row = Rc::clone(&row), {..Attrs::new()})
                }
            }
        }
    }
}

/// One row of a [`VirtualList`], carrying where it really sits.
///
/// It takes a forwarded bundle like every component here that renders an element of its own, and
/// the list hands it an empty one: what a *caller* wrote lands on the scroll container, which is
/// the element they can name, rather than on one of a window's worth of rows that come and go.
#[component]
fn Row<V, F>(
    /// Which row this is, counting from zero, in the whole list rather than in the window.
    index: usize,
    /// How long the whole list is.
    #[prop(into)]
    total: Signal<usize, LocalStorage>,
    /// What builds the row's content.
    row: Rc<F>,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView
where
    V: IntoView + 'static,
    F: Fn(usize) -> V + 'static,
{
    let semantics = A11yBinding::new(Role::ListItem)
        .step(move |a11y| a11y.set_position(index + 1, total.get().max(index + 1)));
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-virtual-list__row"), true)
        .attribute(
            zgui::view::AttrName::new("data-index"),
            Some(index.to_string()),
        )
        .a11y_from(semantics);

    view! { box({..own}, {..attrs}) {{row(index)}} }
}

/// A length as CSS text.
fn px(value: f32) -> String {
    format!("{value}px")
}
