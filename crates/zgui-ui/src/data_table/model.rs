//! Sorting, filtering, selection and pagination, without a single element.
//!
//! Everything a data table *does* to its rows lives here, as ordinary values over ordinary
//! signals. A table of ten thousand rows sorts, filters and pages the same way whether thirty rows
//! or none of them are mounted, so the answer cannot come from the tree — and a model with no view
//! is a model a test can drive a million rows through in a millisecond.

use std::collections::BTreeSet;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, StoredValue};

use crate::checkbox::Checked;
use crate::data_table::column::{CellText, Column};
use crate::table::ColumnSort;

/// Which column a table is sorted by, and which way.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SortState {
    /// The column's key, when the table is sorted at all.
    pub key: Option<String>,
    /// Which way it is sorted.
    pub direction: ColumnSort,
}

impl SortState {
    /// How `key`'s own header should describe itself.
    ///
    /// A column that is not the sorted one is unsorted, however the sorted one is sorted: two
    /// headers claiming a direction at once is a table a reader cannot make sense of.
    #[must_use]
    pub fn of(&self, key: &str) -> ColumnSort {
        if self.key.as_deref() == Some(key) {
            self.direction
        } else {
            ColumnSort::None
        }
    }

    /// The state pressing `key`'s header produces.
    ///
    /// Pressing a different column sorts by it ascending; pressing the sorted one reverses it.
    #[must_use]
    pub fn pressed(&self, key: &str) -> Self {
        Self {
            key: Some(key.to_owned()),
            direction: self.of(key).next(),
        }
    }
}

/// How many rows a page holds, and which page is showing.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Page {
    /// Which page, counting from zero.
    pub index: usize,
    /// How many rows a page holds. Zero means the table is not paginated.
    pub size: usize,
}

impl Page {
    /// One page holding everything.
    pub const ALL: Self = Self { index: 0, size: 0 };

    /// A table showing `size` rows at a time, starting at the first page.
    #[must_use]
    pub const fn of(size: usize) -> Self {
        Self { index: 0, size }
    }

    /// How many pages `rows` rows make.
    ///
    /// One, when there are no rows: a table showing "page 1 of 0" is a table nobody can read.
    #[must_use]
    pub const fn count(self, rows: usize) -> usize {
        if self.size == 0 {
            return 1;
        }
        let pages = rows.div_ceil(self.size);
        if pages == 0 { 1 } else { pages }
    }

    /// The rows of `rows` this page shows, as a half-open range.
    #[must_use]
    pub fn range(self, rows: usize) -> core::ops::Range<usize> {
        if self.size == 0 {
            return 0..rows;
        }
        let start = (self.index * self.size).min(rows);
        let end = (start + self.size).min(rows);
        start..end
    }

    /// The same page, clamped to a table of `rows` rows.
    ///
    /// What filtering calls: a filter that removes nine pages of rows while page nine is showing
    /// must not leave the reader looking at nothing.
    #[must_use]
    pub fn clamped(self, rows: usize) -> Self {
        Self {
            index: self.index.min(self.count(rows) - 1),
            ..self
        }
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::ALL
    }
}

/// How a row is matched against what somebody typed.
pub type RowMatch<T> = Rc<dyn Fn(&T, &str) -> bool>;

/// What identifies a row, which is what a selection is kept in terms of.
///
/// A name rather than a position: a table that kept its selection by index would lose it the
/// moment anything was sorted, filtered or paged.
pub type RowId<T> = CellText<T>;

/// The rows of a data table, ordered, filtered, selected and paged.
///
/// `Copy`, so it can be read from any number of closures without ceremony, and reactive
/// throughout: writing the sort re-derives the order, which re-derives the page, which is what the
/// body builds from. Nothing is cached in a second place, so nothing can disagree with anything.
///
/// The three closures it holds live in the reactive arena rather than behind a reference count of
/// their own, which is what keeps the whole handle `Copy` — a model that had to be cloned into
/// every event handler would be one every caller cloned twice and moved once.
pub struct DataModel<T: Clone + 'static> {
    /// Every row there is, in the order the caller supplied them.
    rows: Signal<Vec<T>, LocalStorage>,
    /// What identifies a row, which is what selection is kept in terms of.
    id: StoredValue<RowId<T>, LocalStorage>,
    /// The columns, for the comparison the sort needs.
    columns: StoredValue<Rc<Vec<Column<T>>>, LocalStorage>,
    /// Which column is sorted, and which way.
    sort: RwSignal<SortState, LocalStorage>,
    /// What was typed to narrow the table.
    filter: RwSignal<String, LocalStorage>,
    /// How a row is matched against it.
    matches: StoredValue<RowMatch<T>, LocalStorage>,
    /// Which rows are chosen, by identity rather than by position.
    selected: RwSignal<BTreeSet<String>, LocalStorage>,
    /// Which page is showing.
    page: RwSignal<Page, LocalStorage>,
}

impl<T: Clone + 'static> DataModel<T> {
    /// A model over `rows`, identified by `id`, with `columns` to sort by.
    ///
    /// `matches` decides what the filter text means. The default — [`DataModel::matches_any_cell`]
    /// — asks every column for its text and compares without regard to ASCII case, which is what a
    /// search box over a table of strings wants and is stated rather than assumed.
    pub fn new(
        rows: Signal<Vec<T>, LocalStorage>,
        id: impl Fn(&T) -> String + 'static,
        columns: Rc<Vec<Column<T>>>,
        matches: Option<RowMatch<T>>,
        page: Page,
    ) -> Self {
        let by_cell = Rc::clone(&columns);
        let matches = matches.unwrap_or_else(move || {
            let columns = Rc::clone(&by_cell);
            Rc::new(move |row: &T, needle: &str| Self::matches_any_cell(&columns, row, needle))
        });
        let id: RowId<T> = Rc::new(id);
        Self {
            rows,
            id: StoredValue::new_local(id),
            columns: StoredValue::new_local(columns),
            sort: RwSignal::new_local(SortState::default()),
            filter: RwSignal::new_local(String::new()),
            matches: StoredValue::new_local(matches),
            selected: RwSignal::new_local(BTreeSet::new()),
            page: RwSignal::new_local(page),
        }
    }

    /// Whether any column's text for `row` contains `needle`, ignoring ASCII case.
    ///
    /// The collation is ASCII case folding, which is this library's floor everywhere: it is right
    /// for ASCII and wrong for a language whose case mapping is not one-to-one. A table that needs
    /// more supplies its own `matches`.
    #[must_use]
    pub fn matches_any_cell(columns: &[Column<T>], row: &T, needle: &str) -> bool {
        if needle.is_empty() {
            return true;
        }
        let needle = needle.to_ascii_lowercase();
        columns
            .iter()
            .any(|column| column.text(row).to_ascii_lowercase().contains(&needle))
    }

    /// What identifies `row`.
    #[must_use]
    pub fn id_of(&self, row: &T) -> String {
        (self.id.get_value())(row)
    }

    /// The columns.
    #[must_use]
    pub fn columns(&self) -> Rc<Vec<Column<T>>> {
        self.columns.get_value()
    }

    // ---- sorting -------------------------------------------------------------------------------

    /// Which column is sorted, and which way.
    #[must_use]
    pub fn sort(&self) -> SortState {
        self.sort.get()
    }

    /// Sorts by `key`, or reverses it when it is already the sorted column.
    ///
    /// A column with no comparison is left alone, so a header that cannot sort does nothing rather
    /// than claiming a direction it will not honour.
    pub fn press_header(&self, key: &str) {
        if !self
            .columns()
            .iter()
            .any(|column| column.key() == key && column.is_sortable())
        {
            return;
        }
        self.sort.update(|sort| *sort = sort.pressed(key));
    }

    // ---- filtering -----------------------------------------------------------------------------

    /// What was typed to narrow the table.
    #[must_use]
    pub fn filter(&self) -> String {
        self.filter.get()
    }

    /// Narrows the table, and goes back to the first page.
    ///
    /// The page reset is the whole reason this is a method: filtering while on page nine of a table
    /// that now has two leaves a reader looking at nothing, and every table that forgets it has the
    /// same bug.
    pub fn set_filter(&self, text: impl Into<String>) {
        self.filter.set(text.into());
        self.page.update(|page| page.index = 0);
    }

    // ---- the rows themselves -------------------------------------------------------------------

    /// Every row that survives the filter, in the sorted order.
    ///
    /// The whole table, not one page of it: this is what the page is taken out of and what the row
    /// count a reader is told comes from.
    #[must_use]
    pub fn ordered(&self) -> Vec<T> {
        let needle = self.filter.get();
        let matches = self.matches.get_value();
        let mut rows: Vec<T> = self
            .rows
            .get()
            .into_iter()
            .filter(|row| matches(row, &needle))
            .collect();

        let sort = self.sort.get();
        let columns = self.columns();
        if let Some(order) = sort.key.as_deref().and_then(|key| {
            columns
                .iter()
                .find(|column| column.key() == key)
                .and_then(Column::order)
        }) {
            {
                // Stable, so rows that compare equal keep the order they arrived in — which is what
                // makes sorting by one column and then another do what a reader expects.
                rows.sort_by(|left, right| order(left, right));
                if sort.direction == ColumnSort::Descending {
                    rows.reverse();
                }
            }
        }
        rows
    }

    /// How many rows survive the filter.
    #[must_use]
    pub fn total(&self) -> usize {
        let needle = self.filter.get();
        let matches = self.matches.get_value();
        self.rows
            .get()
            .iter()
            .filter(|row| matches(row, &needle))
            .count()
    }

    /// The rows of the page that is showing.
    #[must_use]
    pub fn page_rows(&self) -> Vec<T> {
        let rows = self.ordered();
        let range = self.page.get().clamped(rows.len()).range(rows.len());
        rows[range].to_vec()
    }

    // ---- selection -----------------------------------------------------------------------------

    /// Whether `row` is chosen.
    #[must_use]
    pub fn is_selected(&self, row: &T) -> bool {
        self.selected.get().contains(&self.id_of(row))
    }

    /// Chooses `row`, or unchooses it.
    pub fn set_selected(&self, row: &T, chosen: bool) {
        let id = self.id_of(row);
        self.selected.update(|set| {
            if chosen {
                set.insert(id);
            } else {
                set.remove(&id);
            }
        });
    }

    /// Flips whether `row` is chosen.
    pub fn toggle_selected(&self, row: &T) {
        let chosen = self.is_selected_untracked(row);
        self.set_selected(row, !chosen);
    }

    /// Whether `row` is chosen, without subscribing.
    #[must_use]
    pub fn is_selected_untracked(&self, row: &T) -> bool {
        self.selected.get_untracked().contains(&self.id_of(row))
    }

    /// How many rows are chosen.
    ///
    /// Counted over the identities held rather than over the rows on screen, so a selection made on
    /// page one is still a selection on page nine — which is the only behaviour that makes "select
    /// all, then act" mean anything in a paginated table.
    #[must_use]
    pub fn selected_count(&self) -> usize {
        self.selected.get().len()
    }

    /// The identities of every chosen row.
    #[must_use]
    pub fn selection(&self) -> Vec<String> {
        self.selected.get().iter().cloned().collect()
    }

    /// Chooses every row that survives the filter, or unchooses all of them.
    ///
    /// Every filtered row rather than every row on the page: "select all" in a narrowed table means
    /// what the reader can see, and a button that also chose the rows a filter had hidden would be
    /// one nobody could use safely.
    pub fn set_all_selected(&self, chosen: bool) {
        let ids: Vec<String> = self.ordered().iter().map(|row| self.id_of(row)).collect();
        self.selected.update(|set| {
            for id in ids {
                if chosen {
                    set.insert(id);
                } else {
                    set.remove(&id);
                }
            }
        });
    }

    /// Whether every filtered row is chosen, some of them are, or none.
    ///
    /// The three-state answer a header checkbox needs: a table with some of its rows chosen is
    /// neither ticked nor unticked, and a header that rounded it to one of the two would lie about
    /// what pressing it does.
    #[must_use]
    pub fn all_selected(&self) -> Checked {
        let rows = self.ordered();
        if rows.is_empty() {
            return Checked::No;
        }
        let chosen = self.selected.get();
        let count = rows
            .iter()
            .filter(|row| chosen.contains(&self.id_of(row)))
            .count();
        if count == 0 {
            Checked::No
        } else if count == rows.len() {
            Checked::Yes
        } else {
            Checked::Mixed
        }
    }

    // ---- pagination ----------------------------------------------------------------------------

    /// Which page is showing, clamped to what there is.
    #[must_use]
    pub fn page(&self) -> Page {
        self.page.get().clamped(self.total())
    }

    /// How many pages there are.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.page.get().count(self.total())
    }

    /// Shows page `index`, clamped to what there is.
    pub fn go_to_page(&self, index: usize) {
        let last = self.page_count().saturating_sub(1);
        self.page.update(|page| page.index = index.min(last));
    }

    /// Shows the next page, or stays on the last.
    pub fn next_page(&self) {
        self.go_to_page(self.page.get_untracked().index.saturating_add(1));
    }

    /// Shows the previous page, or stays on the first.
    pub fn previous_page(&self) {
        self.go_to_page(self.page.get_untracked().index.saturating_sub(1));
    }

    /// Changes how many rows a page holds, keeping the reader near where they were.
    pub fn set_page_size(&self, size: usize) {
        let first = self.page.get_untracked().index * self.page.get_untracked().size;
        self.page.update(|page| {
            page.size = size;
            page.index = first.checked_div(size).unwrap_or(0);
        });
    }
}

impl<T: Clone + 'static> Clone for DataModel<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + 'static> Copy for DataModel<T> {}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use zgui::reactive::{Mounted, RwSignal, install};

    use super::{DataModel, Page, SortState};
    use crate::checkbox::Checked;
    use crate::data_table::column::Column;
    use crate::table::ColumnSort;

    /// One row of the fixture.
    #[derive(Clone, PartialEq, Debug)]
    struct Row {
        /// What identifies it.
        id: u32,
        /// A word.
        name: String,
        /// A number.
        size: i64,
    }

    /// `count` rows, named backwards so that sorting has something to do.
    fn rows(count: u32) -> Vec<Row> {
        (0..count)
            .map(|id| Row {
                id,
                name: format!("row-{:04}", count - id - 1),
                size: i64::from(count - id) * 10,
            })
            .collect()
    }

    /// The two columns the fixture sorts by.
    fn columns() -> Rc<Vec<Column<Row>>> {
        Rc::new(vec![
            Column::new("name", "Name", |row: &Row| row.name.clone())
                .sortable_by(|left: &Row, right: &Row| left.name.cmp(&right.name)),
            Column::new("size", "Size", |row: &Row| row.size.to_string())
                .sortable_by(|left: &Row, right: &Row| left.size.cmp(&right.size)),
            Column::new("actions", "", |_: &Row| String::from("…")),
        ])
    }

    /// A model over `count` rows, inside a scope the caller then unmounts.
    fn model(scope: &Mounted, count: u32, page: Page) -> DataModel<Row> {
        scope.with(|| {
            let source = RwSignal::new_local(rows(count));
            DataModel::new(
                source.into(),
                |row: &Row| row.id.to_string(),
                columns(),
                None,
                page,
            )
        })
    }

    #[test]
    fn sorting_orders_by_the_comparison_and_not_by_the_cell_text() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 20, Page::ALL);

        table.press_header("size");
        let ascending = table.ordered();
        assert_eq!(ascending.first().map(|row| row.size), Some(10));
        assert_eq!(ascending.last().map(|row| row.size), Some(200));

        table.press_header("size");
        let descending = table.ordered();
        assert_eq!(descending.first().map(|row| row.size), Some(200));
        assert_eq!(table.sort().direction, ColumnSort::Descending);

        scope.unmount();
    }

    #[test]
    fn a_column_with_no_comparison_refuses_to_be_sorted_by() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 5, Page::ALL);

        table.press_header("actions");
        assert_eq!(table.sort(), SortState::default(), "nothing was claimed");
        assert_eq!(table.ordered(), rows(5), "and nothing moved");

        scope.unmount();
    }

    #[test]
    fn filtering_narrows_the_table_and_takes_the_reader_back_to_the_first_page() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 100, Page::of(10));

        table.go_to_page(9);
        assert_eq!(table.page().index, 9);

        table.set_filter("row-0001");
        assert_eq!(table.total(), 1);
        assert_eq!(
            table.page().index,
            0,
            "page nine of a one-row table is nothing"
        );
        assert_eq!(table.page_rows().len(), 1);

        scope.unmount();
    }

    #[test]
    fn the_filter_ignores_ascii_case_because_a_search_box_that_did_not_would_be_useless() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 10, Page::ALL);
        table.set_filter("ROW-0003");
        assert_eq!(table.total(), 1);
        scope.unmount();
    }

    #[test]
    fn a_selection_is_kept_by_identity_so_sorting_does_not_move_it() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 20, Page::ALL);

        let chosen = table.ordered()[3].clone();
        table.set_selected(&chosen, true);
        assert!(table.is_selected(&chosen));

        table.press_header("size");
        assert!(
            table.is_selected(&chosen),
            "the row moved; the selection followed the row rather than the position",
        );
        assert_eq!(table.selected_count(), 1);
        assert_eq!(table.selection(), vec![chosen.id.to_string()]);

        scope.unmount();
    }

    #[test]
    fn selecting_everything_selects_what_the_filter_left_and_nothing_it_hid() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 100, Page::of(10));

        table.set_filter("row-000");
        assert_eq!(table.total(), 10, "ten rows match");
        table.set_all_selected(true);
        assert_eq!(table.selected_count(), 10);
        assert_eq!(table.all_selected(), Checked::Yes);

        table.set_filter("");
        assert_eq!(table.total(), 100);
        assert_eq!(
            table.all_selected(),
            Checked::Mixed,
            "ten of a hundred is neither all nor none",
        );

        scope.unmount();
    }

    #[test]
    fn a_table_with_no_rows_is_one_page_and_chooses_nothing() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 0, Page::of(25));
        assert_eq!(table.page_count(), 1);
        assert!(table.page_rows().is_empty());
        assert_eq!(table.all_selected(), Checked::No);
        scope.unmount();
    }

    #[test]
    fn paging_covers_every_row_exactly_once() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 97, Page::of(10));
        assert_eq!(table.page_count(), 10, "97 rows in tens is ten pages");

        let mut seen = Vec::new();
        for index in 0..table.page_count() {
            table.go_to_page(index);
            seen.extend(table.page_rows());
        }
        assert_eq!(seen, table.ordered(), "the pages are the table, in order");

        table.next_page();
        assert_eq!(table.page().index, 9, "the last page is the last page");
        table.go_to_page(0);
        table.previous_page();
        assert_eq!(table.page().index, 0);

        scope.unmount();
    }

    #[test]
    fn changing_the_page_size_keeps_the_reader_near_the_same_row() {
        install().ok();
        let scope = Mounted::new();
        let table = model(&scope, 100, Page::of(10));
        table.go_to_page(4);
        let first = table.page_rows()[0].clone();

        table.set_page_size(20);
        assert_eq!(table.page().index, 2, "row 40 is on page 2 in twenties");
        assert!(table.page_rows().contains(&first));

        scope.unmount();
    }
}
