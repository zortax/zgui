//! Where an element sits in a table or a grid.

/// The coordinates of an element inside a table, and the size of the table it belongs to.
///
/// A table's shape is not always visible from its structure. Cells may span, a header row may live
/// in a different scrolling element from its body, and a virtualised grid renders a window onto a
/// far larger one. Every field here is optional, and a table whose structure does speak for itself
/// sets none of them.
///
/// ```
/// use zgui_vocab::TablePosition;
///
/// let cell = TablePosition {
///     row_index: Some(4),
///     column_index: Some(0),
///     row_span: Some(2),
///     ..TablePosition::default()
/// };
/// assert!(cell.is_set());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TablePosition {
    /// How many rows the whole table has, set on the table itself.
    pub row_count: Option<usize>,
    /// How many columns the whole table has, set on the table itself.
    pub column_count: Option<usize>,
    /// This element's zero-based row.
    pub row_index: Option<usize>,
    /// This element's zero-based column.
    pub column_index: Option<usize>,
    /// How many rows this cell covers.
    pub row_span: Option<usize>,
    /// How many columns this cell covers.
    pub column_span: Option<usize>,
}

impl TablePosition {
    /// Whether any table property is set.
    pub fn is_set(&self) -> bool {
        self.row_count.is_some()
            || self.column_count.is_some()
            || self.row_index.is_some()
            || self.column_index.is_some()
            || self.row_span.is_some()
            || self.column_span.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::TablePosition;

    #[test]
    fn an_unset_position_reports_itself_unset() {
        assert!(!TablePosition::default().is_set());
    }

    #[test]
    fn a_table_that_only_states_its_size_is_still_set() {
        assert!(
            TablePosition {
                row_count: Some(1_000),
                ..TablePosition::default()
            }
            .is_set()
        );
    }
}
