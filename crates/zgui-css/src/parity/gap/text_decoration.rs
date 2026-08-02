//! The three text-decoration metrics, none of which exists in this build.
//!
//! `text-decoration-line`, `-style` and `-color` are all here and read. The three that say *where*
//! the line goes and *how thick* it is are not: the engine's sources define them for another
//! target only, so the parser does not know the names and a declaration using one is dropped
//! without a word.
//!
//! What that costs is authoring control over an underline's weight and its distance from the
//! baseline. The decoration itself is drawn, from the face's own metrics, which is why nothing in
//! this framework is blocked on them — a component that wants a thicker rule under a heading draws
//! a border instead.

use crate::parity::support::{AbsentReason, Support};

crate::register_properties! {
    text_decoration_thickness => Support::Absent(AbsentReason::GeckoOnly),
    text_underline_offset     => Support::Absent(AbsentReason::GeckoOnly),
    text_underline_position   => Support::Absent(AbsentReason::GeckoOnly),
}

#[cfg(test)]
mod tests {
    use super::REGISTERED;

    #[test]
    fn the_group_is_the_three_metrics_and_not_the_decoration_itself() {
        let names: Vec<String> = REGISTERED.iter().map(|row| row.css_name()).collect();
        assert_eq!(
            names,
            [
                "text-decoration-thickness",
                "text-underline-offset",
                "text-underline-position"
            ]
        );
        assert!(
            !names.iter().any(|name| name == "text-decoration-line"),
            "the line itself is generated and read; only its metrics are missing"
        );
    }
}
