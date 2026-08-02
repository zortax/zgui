//! A table of data that sorts, filters, chooses, resizes and pages — and builds only what shows.

mod column;
mod model;
mod resize;
mod style;

pub use crate::data_table::column::{CellOrder, CellText, Column, track_list};
pub use crate::data_table::model::{DataModel, Page, RowId, RowMatch, SortState};
pub use crate::data_table::resize::{ColumnResizer, ColumnResizerProps, MIN_WIDTH};
pub use crate::data_table::style::DataTableStyle;

use std::collections::BTreeMap;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal, UnsyncCallback};
use zgui::{component, view};

use crate::checkbox::{CheckboxProps, Checked};
use crate::input::InputProps;
use crate::table::{
    CellAlign, ColumnSort, TableBodyProps, TableCellProps, TableHeadProps, TableHeaderProps,
    TableProps, TableRowProps,
};
use crate::virtualize::{VirtualWindow, Virtualize};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::arrow::{ARROW_DOWN, ARROW_UP};
use zgui_ui_icons::set::chevron::{CHEVRON_LEFT, CHEVRON_RIGHT};
use zgui_ui_primitives::Binding;

/// What the data table's rules are installed under.
const SHEET: &str = "zui-data-table";

/// The width of the column that holds the row checkboxes.
const SELECT_TRACK: &str = "40px";

/// A table of rows a reader can sort, narrow, choose from, resize and page through.
///
/// ```
/// use std::rc::Rc;
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// One line of an invoice.
/// #[derive(Clone)]
/// struct Line {
///     /// What identifies it.
///     id: u32,
///     /// What was bought.
///     item: String,
///     /// What it cost, in pence.
///     pence: i64,
/// }
///
/// /// Every line, sortable and searchable.
/// #[component]
/// fn Lines() -> impl IntoView {
///     let lines = RwSignal::new_local(vec![Line { id: 1, item: "Paper".into(), pence: 250 }]);
///     let columns = vec![
///         Column::new("item", "Item", |line: &Line| line.item.clone())
///             .sortable_by(|a: &Line, b: &Line| a.item.cmp(&b.item)),
///         Column::new("cost", "Cost", |line: &Line| format!("{}p", line.pence))
///             .aligned(CellAlign::End)
///             .sized("120px")
///             .sortable_by(|a: &Line, b: &Line| a.pence.cmp(&b.pence)),
///     ];
///     view! {
///         DataTable(
///             rows = lines,
///             columns = columns,
///             row_id = |line: &Line| line.id.to_string(),
///             label = "Invoice lines",
///             selectable = true,
///             filterable = true,
///             page_size = 25_usize
///         )
///     }
/// }
/// ```
///
/// # What it costs to show a million rows
///
/// `virtualized=true` replaces the body's rows with the handful that are on screen and two boxes
/// standing in for the rest, so the number of elements is a function of how tall the table is
/// rather than of how much data there is. Scrolling within one row rebuilds nothing; scrolling past
/// one builds one row and destroys one. It needs `row_size` to be true of the sheet, because the
/// window is decided before the rows in it exist.
///
/// Pagination and virtualisation are alternatives rather than layers: a virtualised table shows
/// every row that survives the filter, and setting both makes the table virtualised.
///
/// # Keyboard
///
/// Every header that can sort is a button, so <kbd>Enter</kbd> and <kbd>Space</kbd> sort by it.
/// Every column's grip is a separator, so <kbd>←</kbd> and <kbd>→</kbd> resize it. The row
/// checkboxes, the search box and the pager are ordinary controls. Nothing here claims
/// <kbd>Tab</kbd>.
///
/// # What a reader is told
///
/// A grid when the rows can be chosen and a table when they are only read. Its true shape — the
/// number of rows that survive the filter, and the number of columns — is stated on the table
/// itself, so a virtualised body of thirty elements standing for ten thousand rows is announced as
/// ten thousand. Each row carries its true index and each header the direction its column is sorted
/// in.
#[component]
pub fn DataTable<T, I>(
    /// Every row there is.
    #[prop(into)]
    rows: Signal<Vec<T>, LocalStorage>,
    /// The columns, in the order they are shown.
    columns: Vec<Column<T>>,
    /// What identifies a row, which is what a selection is kept in terms of.
    row_id: I,
    /// Whether the rows can be chosen.
    #[prop(default = false)]
    selectable: bool,
    /// Whether the table has a search box of its own.
    #[prop(default = false)]
    filterable: bool,
    /// How a row is matched against the search text. Every column's text, by default.
    #[prop(optional)]
    row_match: Option<RowMatch<T>>,
    /// How many rows a page holds. Zero, the default, shows every row at once.
    #[prop(default = 0)]
    page_size: usize,
    /// Whether only the rows on screen are built.
    #[prop(default = false)]
    virtualized: bool,
    /// How tall one row is in CSS pixels, which virtualisation is decided from.
    #[prop(default = 40.0)]
    row_size: f32,
    /// How tall the scrolling body is, in CSS pixels.
    #[prop(optional)]
    body_height: Option<f32>,
    /// Whether a column's width can be dragged.
    #[prop(default = true)]
    resizable: bool,
    /// What is shown when no row survives the filter.
    #[prop(into, default = String::from("No results."))]
    empty: String,
    /// What the table is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Handed the model as the table is built, for a caller that drives it from outside.
    ///
    /// Everything the toolbar and the pager do is a method on [`DataModel`], so a table with a
    /// search box elsewhere on the page, or a "select all" in a menu, needs no new props — it needs
    /// the model, and this is how it gets it.
    #[prop(optional)]
    on_model: Option<UnsyncCallback<DataModel<T>>>,
    /// Where to record the table's own scrolling element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the table's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView
where
    T: Clone + 'static,
    I: Fn(&T) -> String + 'static,
{
    install_stylesheet(SHEET, DataTableStyle::CSS);
    let grid = node_ref.unwrap_or_default();
    let columns = Rc::new(columns);
    let column_count = columns.len() + usize::from(selectable);

    let model = DataModel::new(
        rows,
        row_id,
        Rc::clone(&columns),
        row_match,
        Page::of(page_size),
    );
    if let Some(told) = on_model {
        told.run(model);
    }

    // One width per column key, in CSS pixels, written by the grips and read back into the track
    // list. Kept by key rather than by position so that a table whose columns are reordered keeps
    // each column's width with the column.
    let widths: RwSignal<BTreeMap<String, f32>, LocalStorage> =
        RwSignal::new_local(BTreeMap::new());
    let track_columns = Rc::clone(&columns);
    let tracks = Signal::derive_local(move || {
        let mut list = Vec::with_capacity(column_count);
        if selectable {
            list.push(SELECT_TRACK.to_owned());
        }
        list.push(track_list(&track_columns, &widths.get()));
        list.join(" ")
    });

    // The filtered, sorted rows, held rather than derived. A derived signal is recomputed on every
    // read, and a virtualised body reads its row set on every scroll frame — so a table of ten
    // thousand rows would sort ten thousand rows for every pixel of scroll, which is a great deal
    // slower than never virtualising at all. This is written when the data, the sort or the filter
    // changes, and read as a reference count.
    let ordered: RwSignal<Rc<Vec<T>>, LocalStorage> = RwSignal::new_local(Rc::new(Vec::new()));
    let sorting = RenderEffect::new(move |_| ordered.set(Rc::new(model.ordered())));
    on_cleanup_local(move || drop(sorting));

    let total = Signal::derive_local(move || ordered.get().len());

    let header_columns = Rc::clone(&columns);
    // Only a virtualised table watches its own scroll position. Observation is refcounted per node
    // and paid for every frame, so a table that shows every row it has would be paying for an
    // answer nothing reads.
    let window: Signal<VirtualWindow, LocalStorage> = if virtualized {
        let seen = Virtualize::new(grid, total, row_size, 4);
        Signal::derive_local(move || seen.window())
    } else {
        Signal::stored_local(VirtualWindow::default())
    };

    view! {
        column(class = DataTableStyle::CLASS, class = "zui-data-table", {..attrs}, class = class) {
            // An empty bundle: what the caller wrote lands on the table's own outer element, and
            // these two are parts of it rather than elements a caller names.
            Toolbar(
                model = model,
                filterable = filterable,
                selectable = selectable,
                label = {label.clone().unwrap_or_else(|| String::from("Data"))},
                {..Attrs::new()}
            )
            Table(
                node_ref = grid,
                columns = tracks,
                interactive = selectable,
                sticky_header = true,
                rows = total,
                columns_count = column_count,
                class = "zui-data-table__grid",
                attr:data-virtualized = {Some(virtualized.to_string())},
                var:--zui-data-table-height = move || body_height.map(|height| format!("{height}px"))
            ) {
                TableHeader {
                    TableRow {
                        {selectable.then(|| {
                                                AnyView::new(view! {
                                TableHead(index = 0_usize, align = CellAlign::Center) {
                                    Checkbox(
                                        checked = Binding::controlled(
                                            Signal::derive_local(move || model.all_selected()),
                                            move |state: Checked| {
                                                model.set_all_selected(state == Checked::Yes);
                                            },
                                        ),
                                        a11y:label = "Select every row"
                                    )
                                }
                            })
                        })}
                        {header_columns
                            .iter()
                            .enumerate()
                            .map(|(position, column)| {
                                AnyView::new(header_cell(
                                    model,
                                    column.clone(),
                                    position + usize::from(selectable),
                                    resizable,
                                    widths,
                                ))
                            })
                            .collect::<Vec<AnyView>>()}
                    }
                }
                TableBody {
                    Spacer(size = Signal::derive_local(move || window.get().lead), attr:data-edge = "lead")
                    // Sliced, never walked: a window a thousand rows down a virtualised table
                    // must not cost a thousand steps to reach, or scrolling gets slower the
                    // further it goes.
                    for entry in move || {
                        let rows = ordered.get();
                        let shown = if virtualized {
                            let seen = window.get();
                            let end = seen.end().min(rows.len());
                            seen.first.min(end)..end
                        } else {
                            model.page().range(rows.len())
                        };
                        let first = shown.start;
                        rows[shown]
                            .iter()
                            .cloned()
                            .enumerate()
                            .map(|(index, row)| (first + index, row))
                            .collect::<Vec<_>>()
                    }, key = move |(_, row): &(usize, T)| model.id_of(row) {
                        {AnyView::new(body_row(
                            model,
                            Rc::clone(&columns),
                            entry.1,
                            entry.0,
                            selectable,
                        ))}
                    }
                    Spacer(size = Signal::derive_local(move || window.get().trail), attr:data-edge = "trail")
                    if move || total.get() == 0 {
                        box(class = "zui-data-table__empty") {{empty.clone()}}
                    } else {}
                }
            }
            Pager(model = model, shown = {!virtualized && page_size > 0}, {..Attrs::new()})
        }
    }
}

/// The box that stands in for the rows a virtualised body did not build.
#[component]
fn Spacer(
    /// How tall it is, in CSS pixels.
    #[prop(into)]
    size: Signal<f32, LocalStorage>,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    view! {
        box(
            class = "zui-data-table__spacer",
            a11y:hidden = true,
            var:--zui-data-table-spacer = move || Some(format!("{}px", size.get())),
            {..attrs}
        )
    }
}

/// One column's header, with its sort button and its grip.
fn header_cell<T: Clone + 'static>(
    model: DataModel<T>,
    column: Column<T>,
    index: usize,
    resizable: bool,
    widths: RwSignal<BTreeMap<String, f32>, LocalStorage>,
) -> impl IntoView {
    let cell = NodeRef::new();
    let key = column.key().to_owned();
    let heading = column.header().to_owned();
    let sortable = column.is_sortable();
    let align = column.align();

    let sort_key = key.clone();
    let sort_model = model;
    let sort = Signal::derive_local(move || sort_model.sort().of(&sort_key));

    let press_key = key.clone();

    let resize_key = key.clone();
    let on_resize = UnsyncCallback::new(move |width: f32| {
        widths.update(|widths| {
            widths.insert(resize_key.clone(), width);
        });
    });

    view! {
        TableHead(
            node_ref = cell,
            index = index,
            align = align,
            sort = sort,
            attr:data-column-key = {Some(key.clone())}
        ) {
            {if sortable {
                AnyView::new(view! {
                    control(
                        class = "zui-data-table__sort",
                        tabindex = Focus::Sequential,
                        on:click = move |_| model.press_header(&press_key)
                    ) {
                        {heading.clone()}
                        // Only when it is sorted. An arrow on every heading is an arrow that says
                        // nothing, and the one that means something is then harder to find.
                        {move || match sort.get() {
                            ColumnSort::Ascending => Some(AnyView::new(view! {
                                Icon(
                                    icon = ARROW_UP,
                                    size = {IconSize::Sm},
                                    class = "zui-data-table__arrow"
                                )
                            })),
                            ColumnSort::Descending => Some(AnyView::new(view! {
                                Icon(
                                    icon = ARROW_DOWN,
                                    size = {IconSize::Sm},
                                    class = "zui-data-table__arrow"
                                )
                            })),
                            ColumnSort::None => None,
                        }}
                    }
                })
            } else {
                AnyView::new(heading.clone())
            }}
            {resizable.then(|| {
                AnyView::new(
                    view! { ColumnResizer(header = cell, label = heading.clone(), on_resize = on_resize) },
                )
            })}
        }
    }
}

/// One row of the body.
fn body_row<T: Clone + 'static>(
    model: DataModel<T>,
    columns: Rc<Vec<Column<T>>>,
    row: T,
    index: usize,
    selectable: bool,
) -> impl IntoView {
    let chosen_model = model;
    let chosen_row = row.clone();
    let selected = Signal::derive_local(move || chosen_model.is_selected(&chosen_row));

    let toggle_model = model;
    let toggle_row = row.clone();
    let cells: Vec<AnyView> = columns
        .iter()
        .enumerate()
        .map(|(position, column)| {
            let text = column.text(&row);
            AnyView::new(view! {
                TableCell(
                    index = position + usize::from(selectable),
                    align = column.align(),
                    header = position == 0 && !selectable
                ) {
                    {text}
                }
            })
        })
        .collect();

    view! {
        TableRow(index = index, selected = selected) {
            {selectable.then(|| {
                AnyView::new(view! {
                    TableCell(index = 0_usize, align = CellAlign::Center) {
                        Checkbox(
                            checked = Binding::controlled(
                                Signal::derive_local(move || {
                                    if selected.get() { Checked::Yes } else { Checked::No }
                                }),
                                move |_: Checked| toggle_model.toggle_selected(&toggle_row),
                            ),
                            a11y:label = "Select row"
                        )
                    }
                })
            })}
            {cells}
        }
    }
}

/// The search box and the count above the table.
#[component]
fn Toolbar<T: Clone + 'static>(
    /// The rows this toolbar acts on.
    model: DataModel<T>,
    /// Whether to show the search box.
    filterable: bool,
    /// Whether to report how many rows are chosen.
    selectable: bool,
    /// What the table is called, which names the search box.
    #[prop(optional)]
    label: Option<String>,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    let filter_model = model;
    let typing = zgui::reactive::StoredValue::new_local(model);
    // The table owns the filter, so the search box reads it from there and writes it back there.
    let text = Binding::controlled(
        Signal::derive_local(move || filter_model.filter()),
        move |next: String| typing.with_value(|model| model.set_filter(next)),
    );
    let counted = model;
    // Held, because the surrounding `Show` rebuilds its children and a bundle moved out of on the
    // first build is a bundle the second one does not have.
    let attrs = zgui::reactive::StoredValue::new_local(attrs);
    let placeholder = match &label {
        Some(name) => format!("Search {name}"),
        None => String::from("Search"),
    };

    view! {
        if move || filterable || selectable {
            row(class = "zui-data-table__toolbar", {..attrs.get_value()}) {
                {filterable.then(|| {
                    AnyView::new(view! {
                        Input(
                            class = "zui-data-table__search",
                            value = text,
                            placeholder = placeholder.clone(),
                            label = placeholder.clone()
                        )
                    })
                })}
                {selectable.then(|| {
                    AnyView::new(view! {
                        text(class = "zui-data-table__count", a11y:live = zgui::vocab::Live::Polite) {
                            {move || format!(
                                "{} of {} row(s) selected.",
                                counted.selected_count(),
                                counted.total(),
                            )}
                        }
                    })
                })}
            }
        } else {}
    }
}

/// The controls that move between pages.
#[component]
fn Pager<T: Clone + 'static>(
    /// The rows being paged.
    model: DataModel<T>,
    /// Whether the table is paginated at all.
    shown: bool,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    // Held, for the same reason the toolbar holds its own.
    let attrs = zgui::reactive::StoredValue::new_local(attrs);
    let reading = model;
    let back = model;
    let on = model;

    view! {
        if move || shown {
            row(
                class = "zui-data-table__pager",
                a11y:role = Role::Group,
                a11y:label = "Pages",
                {..attrs.get_value()}
            ) {
                text(class = "zui-data-table__page", a11y:live = zgui::vocab::Live::Polite) {
                    {move || format!(
                        "Page {} of {}",
                        reading.page().index + 1,
                        reading.page_count(),
                    )}
                }
                control(
                    class = "zui-data-table__step",
                    tabindex = Focus::Sequential,
                    a11y:label = "Previous page",
                    a11y:disabled = move || reading.page().index == 0,
                    attr:data-disabled = move || Some((reading.page().index == 0).to_string()),
                    on:click = move |_| back.previous_page()
                ) {
                    Icon(icon = CHEVRON_LEFT, size = {IconSize::Sm})
                    "Previous"
                }
                control(
                    class = "zui-data-table__step",
                    tabindex = Focus::Sequential,
                    a11y:label = "Next page",
                    a11y:disabled = move || reading.page().index + 1 >= reading.page_count(),
                    attr:data-disabled = move || {
                        Some((reading.page().index + 1 >= reading.page_count()).to_string())
                    },
                    on:click = move |_| on.next_page()
                ) {
                    "Next"
                    Icon(icon = CHEVRON_RIGHT, size = {IconSize::Sm})
                }
            }
        } else {}
    }
}
