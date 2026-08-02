//! Rows and columns, as one grid.

mod parts;
mod style;

pub use crate::table::parts::{
    CellAlign, ColumnSort, TableBody, TableBodyProps, TableCaption, TableCaptionProps, TableCell,
    TableCellProps, TableFooter, TableFooterProps, TableHead, TableHeadProps, TableHeader,
    TableHeaderProps, TableRow, TableRowProps,
};
pub use crate::table::style::TableStyle;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::CustomPropertyName;
use zgui::{component, view};

/// What the table's rules are installed under.
pub(crate) const SHEET: &str = "zui-table";

/// A table, laid out as one CSS grid.
///
/// `columns` is a grid track list — `"2fr 1fr 120px"` — and it is the whole of the column model:
/// the sections and rows inside are boxless, so every cell of every row is an item of this one grid
/// and column three is the same width all the way down without anything measuring anything.
///
/// The table is wrapped in a scroller of its own, because a table is the one piece of content whose
/// width is decided by what is in it: a table too wide for the page scrolls sideways rather than
/// making the page do it.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Three invoices.
/// #[component]
/// fn Invoices() -> impl IntoView {
///     view! {
///         Table(columns = "1fr 120px", label = "Invoices") {
///             TableCaption {"Last three months"}
///             TableHeader {
///                 TableRow {
///                     TableHead(index = 0_usize) {"Invoice"}
///                     TableHead(index = 1_usize, align = CellAlign::End) {"Amount"}
///                 }
///             }
///             TableBody {
///                 TableRow(index = 0_usize) {
///                     TableCell(index = 0_usize, header = true) {"INV-001"}
///                     TableCell(index = 1_usize, align = CellAlign::End) {"£250.00"}
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// # What a reader is told
///
/// A table by default and a grid when `interactive` says so. The difference is not cosmetic: a
/// table is content laid out in columns and is read as text, while a grid is a set of cells a
/// reader navigates with the arrow keys and expects to be operable. A component that claimed to be
/// a grid without being operable would be one that promises keys nothing implements.
///
/// `rows` and `columns_count` state the table's true shape, and are worth setting exactly when the
/// rows in the tree are a window onto more of them — which is what
/// [`DataTable`](crate::data_table::DataTable) does with a virtualised body.
#[component]
pub fn Table(
    /// The column tracks, written as a CSS grid track list.
    ///
    /// A signal rather than a string, because a table whose columns can be dragged has a track list
    /// that changes — and a component that took the tracks by value would need a second mechanism
    /// for the case that actually happens.
    #[prop(into, default = Signal::stored_local(String::from("1fr")))]
    columns: Signal<String, LocalStorage>,
    /// Whether the cells are operated rather than read, which decides whether this is a grid.
    #[prop(default = false)]
    interactive: bool,
    /// Whether the column headers stay put while the body scrolls.
    #[prop(default = false)]
    sticky_header: bool,
    /// How many rows the whole table has, when that is more than the tree holds.
    #[prop(into, optional)]
    rows: Option<Signal<usize, LocalStorage>>,
    /// How many columns the whole table has, when that is more than the tree holds.
    #[prop(into, optional)]
    columns_count: Option<Signal<usize, LocalStorage>>,
    /// What the table is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// The element whose text names this one.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Where to record the table's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the table's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The caption, the sections and their rows.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, TableStyle::CSS);
    let element = node_ref.unwrap_or_default();

    let mut semantics = A11yBinding::new(if interactive { Role::Grid } else { Role::Table });
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    if let Some(target) = labelled_by {
        semantics = semantics.labelled_by(target);
    }
    // Stated together or not at all: half a shape is a table whose width a reader still has to
    // count for itself, and counting a virtualised table's elements gives the wrong answer.
    if let (Some(rows), Some(columns_count)) = (rows, columns_count) {
        semantics = semantics.table_size(move || rows.get(), move || columns_count.get());
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-table"), true)
        .class_toggle(zgui::view::ClassName::new(TableStyle::CLASS), true)
        .attribute(
            zgui::view::AttrName::new("data-sticky-header"),
            Some(sticky_header.to_string()),
        )
        .custom_property(CustomPropertyName::new("zui-table-columns"), move || {
            Some(columns.get())
        })
        .a11y_from(semantics);

    view! {
        box(class = "zui-table-container") {
            box(node_ref = element, {..own}, {..attrs}, class = class) {
                {children.into_view_once()}
            }
        }
    }
}
