//! Where a grid item asks to be placed.

use taffy::{GridPlacement, Line};
use zgui_css::values::grid::GridLineValue;
use zgui_interned::Ident;

/// One end of a `grid-row` or `grid-column`.
///
/// Line zero is not a grid line — CSS numbers them from one, in both directions — so a value that
/// names it is invalid and becomes `auto`, which is what an unplaced end means.
pub fn placement(line: &GridLineValue) -> GridPlacement<Ident> {
    let named = (!line.ident.0.is_empty()).then(|| Ident::new(line.ident.0.as_ref()));
    match (line.is_span, named) {
        (true, None) => match u16::try_from(line.line_num) {
            Ok(0) | Err(_) => GridPlacement::Span(1),
            Ok(span) => GridPlacement::Span(span),
        },
        (true, Some(name)) => {
            let span = u16::try_from(line.line_num).unwrap_or(1).max(1);
            GridPlacement::NamedSpan(name, span)
        }
        (false, None) => match i16::try_from(line.line_num) {
            Ok(0) | Err(_) => GridPlacement::Auto,
            Ok(index) => GridPlacement::Line(index.into()),
        },
        (false, Some(name)) => {
            GridPlacement::NamedLine(name, i16::try_from(line.line_num).unwrap_or(1))
        }
    }
}

/// Both ends of one axis's placement.
pub fn line(start: &GridLineValue, end: &GridLineValue) -> Line<GridPlacement<Ident>> {
    Line {
        start: placement(start),
        end: placement(end),
    }
}

#[cfg(test)]
mod tests {
    use taffy::GridPlacement;
    use zgui_css::values::grid::{CustomIdent, GridLineValue};
    use zgui_interned::Ident;

    use super::placement;

    fn value(name: &str, line_num: i32, is_span: bool) -> GridLineValue {
        GridLineValue {
            ident: CustomIdent(zgui_css::ident_to_atom(Ident::new(name))),
            line_num,
            is_span,
        }
    }

    #[test]
    fn an_unwritten_placement_is_automatic() {
        assert_eq!(placement(&value("", 0, false)), GridPlacement::Auto);
    }

    #[test]
    fn a_numbered_line_survives_in_both_directions() {
        assert_eq!(
            placement(&value("", 3, false)),
            GridPlacement::Line(3_i16.into())
        );
        assert_eq!(
            placement(&value("", -2, false)),
            GridPlacement::Line((-2_i16).into())
        );
    }

    #[test]
    fn line_zero_is_not_a_grid_line_and_becomes_automatic() {
        // Written as `grid-row-start: 0`, which is invalid CSS; carrying it would place the item
        // one track off in a direction that depends on the implicit grid.
        assert_eq!(placement(&value("", 0, false)), GridPlacement::Auto);
    }

    #[test]
    fn a_span_and_a_named_span_are_told_apart() {
        assert_eq!(placement(&value("", 2, true)), GridPlacement::Span(2));
        assert_eq!(
            placement(&value("gutter", 2, true)),
            GridPlacement::NamedSpan(Ident::new("gutter"), 2)
        );
    }

    #[test]
    fn a_named_line_keeps_its_name_and_its_number() {
        assert_eq!(
            placement(&value("main", 2, false)),
            GridPlacement::NamedLine(Ident::new("main"), 2)
        );
    }
}
