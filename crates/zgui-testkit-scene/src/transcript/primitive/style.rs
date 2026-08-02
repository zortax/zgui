//! The two style discriminants a primitive carries as a packed integer.
//!
//! They are rendered by name rather than by number: `style=1` and `style=dashed` are the same fact,
//! and only one of them survives a reviewer reading a diff quickly.

/// A border style, from the discriminant packed into a quad's low byte.
///
/// The dash phase lives above that byte and is appended only when it is non-zero, because it moves
/// where the dashes start and a golden must see it change.
pub fn border(style: u32) -> String {
    let phase = style >> 8;
    let name = match style & 0xff {
        0 => "solid",
        1 => "dashed",
        2 => "dotted",
        _ => "<unknown>",
    };
    if phase == 0 {
        name.to_owned()
    } else {
        format!("{name}+{phase}")
    }
}

/// A decoration style, from its discriminant.
pub fn decoration(style: u32) -> &'static str {
    match style {
        0 => "solid",
        1 => "wavy",
        2 => "dashed",
        3 => "dotted",
        4 => "double",
        _ => "<unknown>",
    }
}

#[cfg(test)]
mod tests {
    use zgui_scene::prim::{BorderStyle, DecorationStyle};

    use super::{border, decoration};

    #[test]
    fn every_border_discriminant_has_a_name() {
        for style in [BorderStyle::Solid, BorderStyle::Dashed, BorderStyle::Dotted] {
            assert_ne!(border(style as u32), "<unknown>");
        }
        assert_eq!(border(BorderStyle::Dashed as u32), "dashed");
    }

    #[test]
    fn a_dash_phase_is_visible_and_a_zero_one_is_not() {
        assert_eq!(border(1), "dashed");
        assert_eq!(border(1 | (3 << 8)), "dashed+3");
    }

    #[test]
    fn every_decoration_discriminant_has_a_name() {
        for style in [
            DecorationStyle::Solid,
            DecorationStyle::Wavy,
            DecorationStyle::Dashed,
            DecorationStyle::Dotted,
            DecorationStyle::Double,
        ] {
            assert_ne!(decoration(style as u32), "<unknown>");
        }
        assert_eq!(decoration(99), "<unknown>");
    }
}
