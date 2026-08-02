//! The named lines and named areas of one grid container.
//!
//! Track *sizes* are read straight off the computed style while layout runs, but names cannot be:
//! the layout algorithms ask for an iterator of *references* to identifiers, and the identifiers in
//! a computed style belong to the style engine's own table rather than to this framework's. So the
//! names — and only the names — are translated once when the box is built, and the layout pass
//! borrows the translation.
//!
//! Nothing is stored for the overwhelming majority of grids, which name no line and no area.

use zgui_interned::Ident;

/// One line's set of names, in source order.
pub type LineNames = Vec<Ident>;

/// The named lines and areas of one grid container.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GridNames {
    /// Names for the row lines, one entry per line: one more than the number of tracks.
    pub rows: Vec<LineNames>,
    /// Names for the column lines, on the same rule.
    pub columns: Vec<LineNames>,
    /// The rectangles `grid-template-areas` names, in one-based grid line numbers.
    pub areas: Vec<taffy::GridTemplateArea<Ident>>,
}

impl GridNames {
    /// Whether nothing here would change a layout, in which case it need not be stored.
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
            && self.rows.iter().all(Vec::is_empty)
            && self.columns.iter().all(Vec::is_empty)
    }

    /// The row line names, or nothing if no row line is named.
    pub fn row_lines(&self) -> Option<&[LineNames]> {
        (!self.rows.iter().all(Vec::is_empty)).then_some(&self.rows[..])
    }

    /// The column line names, or nothing if no column line is named.
    pub fn column_lines(&self) -> Option<&[LineNames]> {
        (!self.columns.iter().all(Vec::is_empty)).then_some(&self.columns[..])
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::Ident;

    use super::GridNames;

    #[test]
    fn a_grid_that_names_nothing_stores_nothing() {
        let names = GridNames {
            rows: vec![Vec::new(), Vec::new()],
            columns: vec![Vec::new()],
            areas: Vec::new(),
        };
        assert!(names.is_empty());
        assert_eq!(names.row_lines(), None);
        assert_eq!(names.column_lines(), None);
    }

    #[test]
    fn one_named_line_is_enough_to_be_worth_storing() {
        let names = GridNames {
            rows: vec![vec![Ident::new("header")], Vec::new()],
            columns: vec![Vec::new()],
            areas: Vec::new(),
        };
        assert!(!names.is_empty());
        assert!(names.row_lines().is_some());
        assert_eq!(names.column_lines(), None);
    }
}
