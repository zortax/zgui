//! What one column of a data table is.

use std::rc::Rc;

use crate::table::CellAlign;

/// How a column reads a row.
///
/// One closure rather than a field name and a reflection mechanism: the row type is the caller's,
/// the cell text is a function of it, and a column that renders a currency, joins two fields or
/// counts a nested collection is the same one line as a column that shows a string.
pub type CellText<T> = Rc<dyn Fn(&T) -> String>;

/// How a column compares two rows.
///
/// `None` makes the column unsortable, which is the honest answer for a column of buttons.
pub type CellOrder<T> = Rc<dyn Fn(&T, &T) -> core::cmp::Ordering>;

/// One column of a [`DataTable`](crate::data_table::DataTable).
///
/// ```
/// use zgui_ui::data_table::Column;
/// use zgui_ui::table::CellAlign;
///
/// /// One line of an invoice.
/// #[derive(Clone)]
/// struct Line {
///     /// What was bought.
///     item: String,
///     /// What it cost, in pence.
///     pence: i64,
/// }
///
/// let columns = vec![
///     Column::new("item", "Item", |line: &Line| line.item.clone())
///         .sortable_by(|left: &Line, right: &Line| left.item.cmp(&right.item)),
///     Column::new("cost", "Cost", |line: &Line| format!("£{:.2}", line.pence as f64 / 100.0))
///         .aligned(CellAlign::End)
///         .sized("120px")
///         .sortable_by(|left: &Line, right: &Line| left.pence.cmp(&right.pence)),
/// ];
///
/// assert_eq!(columns[0].key(), "item");
/// assert!(columns[1].is_sortable());
/// assert_eq!(columns[1].track(), "120px");
/// ```
pub struct Column<T> {
    /// What names this column in the sort state and in `data-column-key`.
    key: String,
    /// What the column is called, in the header.
    header: String,
    /// The grid track this column occupies.
    track: String,
    /// How the cell content sits.
    align: CellAlign,
    /// How a row becomes this column's cell.
    cell: CellText<T>,
    /// How two rows compare in this column, when they can.
    order: Option<CellOrder<T>>,
}

impl<T: 'static> Column<T> {
    /// A column named `key`, headed `header`, whose cells are `cell`.
    ///
    /// Unsortable and one fraction wide until told otherwise.
    pub fn new(
        key: impl Into<String>,
        header: impl Into<String>,
        cell: impl Fn(&T) -> String + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            header: header.into(),
            track: String::from("1fr"),
            align: CellAlign::Start,
            cell: Rc::new(cell),
            order: None,
        }
    }

    /// The same column, sorted by `order`.
    ///
    /// Sorting is by a comparison of the rows rather than of the cell text, so a column of dates
    /// sorts as dates and a column of money sorts as numbers — sorting the text would put £1,000
    /// before £2.
    #[must_use]
    pub fn sortable_by(mut self, order: impl Fn(&T, &T) -> core::cmp::Ordering + 'static) -> Self {
        self.order = Some(Rc::new(order));
        self
    }

    /// The same column, `track` wide, written as one CSS grid track.
    #[must_use]
    pub fn sized(mut self, track: impl Into<String>) -> Self {
        self.track = track.into();
        self
    }

    /// The same column, with its content aligned.
    #[must_use]
    pub const fn aligned(mut self, align: CellAlign) -> Self {
        self.align = align;
        self
    }

    /// What names this column.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// What the column is called.
    #[must_use]
    pub fn header(&self) -> &str {
        &self.header
    }

    /// The grid track this column occupies.
    #[must_use]
    pub fn track(&self) -> &str {
        &self.track
    }

    /// How the content sits.
    #[must_use]
    pub const fn align(&self) -> CellAlign {
        self.align
    }

    /// This column's cell for `row`.
    #[must_use]
    pub fn text(&self, row: &T) -> String {
        (self.cell)(row)
    }

    /// Whether this column can be sorted at all.
    #[must_use]
    pub const fn is_sortable(&self) -> bool {
        self.order.is_some()
    }

    /// How two rows compare in this column, when they can.
    #[must_use]
    pub fn order(&self) -> Option<&CellOrder<T>> {
        self.order.as_ref()
    }
}

impl<T> Clone for Column<T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            header: self.header.clone(),
            track: self.track.clone(),
            align: self.align,
            cell: Rc::clone(&self.cell),
            order: self.order.clone(),
        }
    }
}

impl<T: 'static> core::fmt::Debug for Column<T> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Column")
            .field("key", &self.key)
            .field("header", &self.header)
            .field("track", &self.track)
            .field("sortable", &self.is_sortable())
            .finish_non_exhaustive()
    }
}

/// The grid track list a set of columns lays out as, with any dragged widths applied.
///
/// `widths` is keyed by [`Column::key`] and holds CSS pixels. A column nobody has dragged keeps the
/// track it declared, which is what lets a table mix a fixed column, a fractional one and one the
/// reader has resized without any of the three knowing about the others.
///
/// ```
/// use std::collections::BTreeMap;
/// use zgui_ui::data_table::{Column, track_list};
///
/// let columns = vec![
///     Column::new("a", "A", |row: &u8| row.to_string()),
///     Column::new("b", "B", |row: &u8| row.to_string()).sized("80px"),
/// ];
/// assert_eq!(track_list(&columns, &BTreeMap::new()), "1fr 80px");
///
/// let dragged = BTreeMap::from([(String::from("a"), 220.0)]);
/// assert_eq!(track_list(&columns, &dragged), "220px 80px");
/// ```
#[must_use]
pub fn track_list<T: 'static>(
    columns: &[Column<T>],
    widths: &std::collections::BTreeMap<String, f32>,
) -> String {
    columns
        .iter()
        .map(|column| match widths.get(column.key()) {
            Some(width) => format!("{width}px"),
            None => column.track().to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}
