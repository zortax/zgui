//! The pieces a table is written out of.
//!
//! Six components, each one element, each saying what it is. They are separate components rather
//! than props on [`Table`](crate::table::Table) because a table's *shape* is written by nesting —
//! a header holds rows, a row holds cells — and a shape written as data is a shape a caller cannot
//! put anything unexpected in.
//!
//! # Why none of them generates a box
//!
//! A section and a row are `display: contents`. The whole table is one grid, and its tracks are the
//! columns; a row that generated a box would be a single grid item containing every cell of that
//! row, and each row would then decide its own column widths in isolation. Boxless rows put every
//! cell of every row into the same grid, which is what makes column three the same width in row one
//! and row nine hundred.
//!
//! A boxless element still matches selectors and still holds semantics, so
//! `.zui-table__row:hover .zui-table__cell` highlights a row and a reader still meets a row between
//! the table and its cells.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::SortDirection;
use zgui::{component, view};

/// How a cell's content sits in its track.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum CellAlign {
    /// Against the start edge, which is what text wants.
    #[default]
    Start,
    /// Centred.
    Center,
    /// Against the end edge, which is what a number wants so that the digits line up.
    End,
}

impl CellAlign {
    /// The value written to `data-align`, which is what the sheet selects on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
        }
    }
}

/// Which way a column is sorted, when it is.
///
/// The framework's own [`SortDirection`] plus the state a sortable column spends most of its life
/// in — unsorted — which the framework's enumeration deliberately does not have, because "no
/// direction" is the absence of the property rather than a value of it.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum ColumnSort {
    /// Not sorted by this column.
    #[default]
    None,
    /// Smallest first.
    Ascending,
    /// Largest first.
    Descending,
}

impl ColumnSort {
    /// The value written to `data-sort`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }

    /// What a reader is told, when there is anything to tell it.
    #[must_use]
    pub const fn direction(self) -> Option<SortDirection> {
        match self {
            Self::None => None,
            Self::Ascending => Some(SortDirection::Ascending),
            Self::Descending => Some(SortDirection::Descending),
        }
    }

    /// The direction pressing this header next would sort in.
    ///
    /// Unsorted and descending both become ascending, so a second press on a column reverses it and
    /// a third reverses it back — the cycle every table has, without a third press that unsorts and
    /// leaves the reader wondering what happened.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::None | Self::Descending => Self::Ascending,
        }
    }
}

/// A table's caption, which is what names it.
#[component]
pub fn TableCaption(
    /// Classes merged after the caption's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The caption's text.
    children: Children,
) -> impl IntoView {
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-table__caption"), true)
        .a11y_from(A11yBinding::new(Role::Caption));
    view! { box({..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// The section of a table holding its column headers.
#[component]
pub fn TableHeader(
    /// Where to record the section's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the section's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The header rows.
    children: Children,
) -> impl IntoView {
    section("zui-table__header", node_ref, class, attrs, children)
}

/// The section of a table holding its data rows.
#[component]
pub fn TableBody(
    /// Where to record the section's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the section's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The rows.
    children: Children,
) -> impl IntoView {
    section("zui-table__body", node_ref, class, attrs, children)
}

/// The section of a table holding its summary rows.
#[component]
pub fn TableFooter(
    /// Where to record the section's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the section's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The summary rows.
    children: Children,
) -> impl IntoView {
    section("zui-table__footer", node_ref, class, attrs, children)
}

/// One boxless row group, which is what all three sections are.
fn section(
    name: &'static str,
    node_ref: Option<NodeRef>,
    class: Classes,
    attrs: Attrs,
    children: Children,
) -> impl IntoView {
    let element = node_ref.unwrap_or_default();
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-table__section"), true)
        .class_toggle(zgui::view::ClassName::new(name), true)
        .a11y_from(A11yBinding::new(Role::RowGroup));
    view! {
        box(node_ref = element, {..own}, {..attrs}, class = class) {{children.into_view_once()}}
    }
}

/// One row of a table.
#[component]
pub fn TableRow(
    /// Which row of the whole table this is, counting from zero.
    ///
    /// Worth setting exactly when the rows in the tree are a window onto more of them: a reader
    /// counting elements would otherwise announce row 3 of a thousand-row table as row 3 of thirty.
    #[prop(into, optional)]
    index: Option<Signal<usize, LocalStorage>>,
    /// Whether this row is one of the chosen ones.
    #[prop(into, optional)]
    selected: Option<Signal<bool, LocalStorage>>,
    /// Where to record the row's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the row's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The cells.
    children: Children,
) -> impl IntoView {
    let element = node_ref.unwrap_or_default();
    let mut semantics = A11yBinding::new(Role::Row);
    if let Some(index) = index {
        semantics = semantics.row_index(move || index.get());
    }
    if let Some(selected) = selected {
        semantics = semantics.selected(move || selected.get());
    }

    let mut own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-table__row"), true)
        .a11y_from(semantics);
    if let Some(selected) = selected {
        own = own.attribute(zgui::view::AttrName::new("data-selected"), move || {
            Some(selected.get().to_string())
        });
    }
    if let Some(index) = index {
        own = own.attribute(zgui::view::AttrName::new("data-index"), move || {
            Some(index.get().to_string())
        });
    }

    view! { box(node_ref = element, {..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// One column header.
#[component]
pub fn TableHead(
    /// Which column this is, counting from zero.
    #[prop(into, optional)]
    index: Option<Signal<usize, LocalStorage>>,
    /// How the header's content sits in its track.
    #[prop(default = CellAlign::Start)]
    align: CellAlign,
    /// Which way this column is sorted, when it is sorted at all.
    #[prop(into, default = Signal::stored_local(ColumnSort::None))]
    sort: Signal<ColumnSort, LocalStorage>,
    /// Where to record the header's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the header's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the column is called.
    children: Children,
) -> impl IntoView {
    let element = node_ref.unwrap_or_default();
    let mut semantics =
        A11yBinding::new(Role::ColumnHeader).step(move |a11y| match sort.get().direction() {
            Some(direction) => a11y.sort_direction(direction),
            None => a11y,
        });
    if let Some(index) = index {
        semantics = semantics.column_index(move || index.get());
    }

    let mut own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-table__head"), true)
        .attribute(zgui::view::AttrName::new("data-align"), align.name())
        .attribute(zgui::view::AttrName::new("data-sort"), move || {
            Some(sort.get().name().to_owned())
        })
        .a11y_from(semantics);
    if let Some(index) = index {
        own = own.attribute(zgui::view::AttrName::new("data-column"), move || {
            Some(index.get().to_string())
        });
    }

    view! { box(node_ref = element, {..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

/// One data cell.
#[component]
pub fn TableCell(
    /// Which column this is, counting from zero.
    #[prop(into, optional)]
    index: Option<Signal<usize, LocalStorage>>,
    /// How the content sits in its track.
    #[prop(default = CellAlign::Start)]
    align: CellAlign,
    /// Whether this cell names its row rather than holding a value of it.
    #[prop(default = false)]
    header: bool,
    /// Where to record the cell's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the cell's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is in the cell.
    children: Children,
) -> impl IntoView {
    let element = node_ref.unwrap_or_default();
    let role = if header { Role::RowHeader } else { Role::Cell };
    let mut semantics = A11yBinding::new(role);
    if let Some(index) = index {
        semantics = semantics.column_index(move || index.get());
    }

    let mut own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-table__cell"), true)
        .attribute(zgui::view::AttrName::new("data-align"), align.name())
        .a11y_from(semantics);
    if let Some(index) = index {
        own = own.attribute(zgui::view::AttrName::new("data-column"), move || {
            Some(index.get().to_string())
        });
    }

    view! { box(node_ref = element, {..own}, {..attrs}, class = class) {{children.into_view_once()}} }
}

#[cfg(test)]
mod tests {
    use super::ColumnSort;
    use zgui::vocab::SortDirection;

    #[test]
    fn pressing_a_sorted_column_reverses_it_and_never_unsorts_it() {
        let mut sort = ColumnSort::None;
        sort = sort.next();
        assert_eq!(sort, ColumnSort::Ascending);
        sort = sort.next();
        assert_eq!(sort, ColumnSort::Descending);
        sort = sort.next();
        assert_eq!(sort, ColumnSort::Ascending, "a third press reverses again");
    }

    #[test]
    fn an_unsorted_column_tells_a_reader_nothing_rather_than_telling_it_ascending() {
        assert_eq!(ColumnSort::None.direction(), None);
        assert_eq!(
            ColumnSort::Descending.direction(),
            Some(SortDirection::Descending)
        );
    }
}
