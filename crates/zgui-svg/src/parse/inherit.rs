//! How a document that wrote `currentColor` is told apart from one that wrote a colour.
//!
//! # The problem
//!
//! The parser resolves `currentColor` while it builds its tree: it looks up the inherited `color`
//! and hands back a colour, with no record that the document asked for the inherited one. Reading
//! its output alone, `fill="currentColor"` and `fill="black"` are the same document.
//!
//! Keeping that difference matters more than it looks. If it is lost, either every document is
//! re-parsed whenever the element's colour changes — a parse per hover — or a document's own
//! colours get overwritten by the element's, which silently repaints a multi-colour logo in the
//! text colour of the paragraph around it.
//!
//! # The scheme
//!
//! The document is parsed twice, with two different colours injected as the root's `color`. Any
//! paint that came out as the first colour in the first parse *and* the second colour in the
//! second parse asked for `currentColor`; anything else is a colour the document wrote down. A
//! literal fill that happens to equal one of the two injected colours comes out as itself in the
//! other parse, so it is kept as a colour — which one parse against one sentinel could not do.
//!
//! Both parses see identical bytes apart from the injected declaration, so they produce identical
//! structure and the two results are merged shape by shape. If they ever did not, the first parse
//! is taken unchanged: the failure mode is a `currentColor` document that stops following its
//! element, never a document whose own colours are thrown away.

use zgui_color::Color;

use crate::document::gradient::{Gradient, Stop};
use crate::document::ink::Ink;
use crate::document::shape::{Fill, Paint, Shape, Stroke};

/// The colour injected as the root's `color` in the first parse.
const FIRST: (u8, u8, u8) = (0x1f, 0x0d, 0x3b);

/// The colour injected in the second.
const SECOND: (u8, u8, u8) = (0xc4, 0xf2, 0x0e);

/// The style sheet that names the root's colour for one parse.
///
/// A rule rather than a parser option, because there is no option for it: the inherited colour is
/// an ordinary presentation attribute and a style sheet is the only supported way in.
fn declare((red, green, blue): (u8, u8, u8)) -> String {
    format!("svg {{ color: #{red:02x}{green:02x}{blue:02x}; }}")
}

/// The style sheet for the first parse.
pub(crate) fn first_sheet() -> String {
    declare(FIRST)
}

/// The style sheet for the second.
pub(crate) fn second_sheet() -> String {
    declare(SECOND)
}

/// Whether a pair of resolved colours is the mark of `currentColor`.
fn is_pair(first: Color, second: Color) -> bool {
    let expected = |(red, green, blue): (u8, u8, u8)| Color::srgb_u8(red, green, blue, 255);
    first.components() == expected(FIRST).components()
        && second.components() == expected(SECOND).components()
}

/// One colour of the first parse, merged with the same colour of the second.
fn ink(first: Ink, second: Ink) -> Ink {
    match (first, second) {
        (Ink::Solid(one), Ink::Solid(two)) if is_pair(one, two) => {
            Ink::Inherited { alpha: one.alpha() }
        }
        _ => first,
    }
}

/// One ramp of the first parse, merged with the same ramp of the second.
fn gradient(mut first: Gradient, second: &Gradient) -> Gradient {
    if first.stops.len() != second.stops.len() {
        return first;
    }
    let merged: smallvec::SmallVec<[Stop; 4]> = first
        .stops
        .iter()
        .zip(second.stops.iter())
        .map(|(one, two)| Stop {
            offset: one.offset,
            color: ink(one.color, two.color),
        })
        .collect();
    first.stops = merged;
    first
}

/// One paint of the first parse, merged with the same paint of the second.
fn paint(first: Paint, second: &Paint) -> Paint {
    match (first, second) {
        (Paint::Solid(one), Paint::Solid(two)) => Paint::Solid(ink(one, *two)),
        (Paint::Gradient(one), Paint::Gradient(two)) => Paint::Gradient(gradient(one, two)),
        (first, _) => first,
    }
}

/// Merges the two parses of one document.
///
/// The first parse's geometry is kept in full; only its colours are revisited.
pub(crate) fn merge(first: Vec<Shape>, second: &[Shape]) -> Vec<Shape> {
    if first.len() != second.len() {
        return first;
    }
    first
        .into_iter()
        .zip(second.iter())
        .map(|(mut one, two)| {
            one.fill = match (one.fill, two.fill.as_ref()) {
                (Some(fill), Some(other)) => Some(Fill {
                    paint: paint(fill.paint, &other.paint),
                    rule: fill.rule,
                }),
                (fill, _) => fill,
            };
            one.stroke = match (one.stroke, two.stroke.as_ref()) {
                (Some(stroke), Some(other)) => Some(Stroke {
                    paint: paint(stroke.paint, &other.paint),
                    style: stroke.style,
                }),
                (stroke, _) => stroke,
            };
            one
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use zgui_color::Color;

    use super::{FIRST, SECOND, declare, ink, is_pair};
    use crate::document::ink::Ink;

    fn sentinel((red, green, blue): (u8, u8, u8)) -> Color {
        Color::srgb_u8(red, green, blue, 255)
    }

    #[test]
    fn a_declaration_names_the_colour_the_parser_will_read() {
        assert_eq!(declare((0x01, 0xab, 0xff)), "svg { color: #01abff; }");
    }

    #[test]
    fn the_two_sentinels_together_are_the_mark_and_either_one_alone_is_not() {
        assert!(is_pair(sentinel(FIRST), sentinel(SECOND)));
        assert!(
            !is_pair(sentinel(FIRST), sentinel(FIRST)),
            "a document that literally fills with the first sentinel resolves to it in both \
             parses, and is a colour of its own"
        );
        assert!(!is_pair(sentinel(SECOND), sentinel(SECOND)));
        assert!(!is_pair(Color::BLACK, Color::BLACK));
    }

    #[test]
    fn the_alpha_of_an_inherited_ink_is_the_one_the_document_folded_into_it() {
        let half = |rgb: (u8, u8, u8)| Ink::Solid(sentinel(rgb).with_alpha(0.5));
        assert_eq!(
            ink(half(FIRST), half(SECOND)),
            Ink::Inherited { alpha: 0.5 },
            "a half-opaque currentColor fill is half opaque whatever colour it inherits"
        );
    }

    #[test]
    fn a_colour_that_is_not_the_pair_survives_untouched() {
        let red = Ink::Solid(Color::srgb(1.0, 0.0, 0.0, 1.0));
        assert_eq!(ink(red, red), red);
    }
}
