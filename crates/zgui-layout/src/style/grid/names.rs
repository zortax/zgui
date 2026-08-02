//! Line names, as an iterator of iterators of borrowed identifiers.
//!
//! The layout algorithms ask for *references* to names, so a name cannot be produced on the fly
//! from the style engine's own identifier table — there would be nothing for the reference to point
//! at. This is why line names are translated once, when the box is built, and only borrowed here.

use zgui_interned::Ident;

/// A set of line names per line, in line order.
///
/// The concrete iterator type matters: the layout engine's own blanket implementation covers
/// exactly this shape, so naming it is what makes the names usable without a wrapper of our own.
pub type LineNamesIter<'a> = core::iter::Map<
    core::slice::Iter<'a, Vec<Ident>>,
    fn(&Vec<Ident>) -> core::slice::Iter<'_, Ident>,
>;

/// One line's names, as an iterator over borrowed identifiers.
///
/// The parameter is a vector rather than a slice because the layout engine's own implementation is
/// written over exactly this function type, and a slice would not match it.
#[allow(clippy::ptr_arg)]
fn borrow(names: &Vec<Ident>) -> core::slice::Iter<'_, Ident> {
    names.iter()
}

/// The names of a sequence of lines.
pub fn line_names(names: &[Vec<Ident>]) -> LineNamesIter<'_> {
    names
        .iter()
        .map(borrow as fn(&Vec<Ident>) -> core::slice::Iter<'_, Ident>)
}

/// No lines, and therefore no names.
pub fn no_line_names<'a>() -> LineNamesIter<'a> {
    line_names(&[])
}

/// The names of a repetition's lines, which are always none.
pub type EmptyLineNames<'a> = LineNamesIter<'a>;

#[cfg(test)]
mod tests {
    use zgui_interned::Ident;

    use super::{line_names, no_line_names};

    #[test]
    fn a_line_can_carry_several_names_and_a_line_can_carry_none() {
        let names = vec![
            vec![Ident::new("start"), Ident::new("header-start")],
            Vec::new(),
            vec![Ident::new("end")],
        ];
        let rendered: Vec<Vec<&str>> = line_names(&names)
            .map(|set| set.map(|name| name.as_str()).collect())
            .collect();
        assert_eq!(
            rendered,
            vec![vec!["start", "header-start"], Vec::new(), vec!["end"]]
        );
    }

    #[test]
    fn the_empty_sequence_yields_nothing_and_still_reports_its_length() {
        let mut empty = no_line_names();
        assert_eq!(empty.len(), 0);
        assert!(empty.next().is_none());
    }
}
