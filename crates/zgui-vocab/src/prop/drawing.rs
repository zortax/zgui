//! The three properties an element's outlines are carried in.
//!
//! They are named here rather than beside the element that sets them because three layers read
//! them and only one writes them: a view writes the outlines, the box tree asks whether an element
//! draws any in order to decide what kind of piece it produces, and the paint stage reads them to
//! produce geometry. A name repeated in three crates is a name that can be misspelled in two of
//! them, and a misspelling here is an element that draws nothing with no error anywhere.

/// The property one element's outlines are carried in.
///
/// The value is text: one outline per line, each written the way a path is written in a vector
/// image. A list of lines rather than one string with several subpaths in it, because a backend
/// with real path nodes sets one node per line, and because a counter drawn as a reversed subpath
/// has to stay in the same line as the outline it punches a hole in.
pub const PATHS: &str = "d";

/// The property naming the space those outlines are written in.
///
/// The value is four numbers separated by whitespace or commas: the minimum x, the minimum y, the
/// width and the height of the rectangle the outlines are drawn inside.
///
/// An element that sets one is drawing in a space of its own, and its outlines are scaled to fit
/// its content box — which is what lets one icon constant be drawn at any size. An element that
/// does not set one is drawing in CSS pixels measured from its own content box's top left corner,
/// which is what a chart mark wants: its geometry is already in the coordinates its box was placed
/// with, and fitting it to the box would scale it twice.
pub const VIEW_BOX: &str = "viewBox";

/// The property a whole vector document is carried in.
///
/// The value is the source of an SVG document. An element carrying one draws that document instead
/// of [`PATHS`] — a document brings its own outlines, its own paints and its own space with it, so
/// a list of outlines beside it would be a second opinion about what the element draws.
///
/// It is deliberately not a file name or a URL. What crosses this seam is the document itself, so
/// a view that computed one, read one from disk or embedded one at compile time all reach the paint
/// stage the same way, and nothing below the view layer ever performs I/O.
pub const DOCUMENT: &str = "svg";

/// Whether a change to `name` can change what an element draws.
///
/// The paint stage reads exactly these three properties, so a write to any of them has to be
/// reported as a repaint and a write to anything else must not be — a field whose value changes on
/// every keystroke is a property too, and repainting for it would repaint on every keystroke for
/// nothing.
pub fn paints(name: &str) -> bool {
    name == PATHS || name == VIEW_BOX || name == DOCUMENT
}

/// Reads the four numbers of a view box, or nothing if they are not four numbers.
///
/// Returned as minimum x, minimum y, width and height. A view box with a zero or negative extent is
/// rejected rather than clamped: it names no area to fit anything into, and a fit against it would
/// divide by zero.
///
/// ```
/// use zgui_vocab::prop::drawing::view_box;
///
/// assert_eq!(view_box("0 0 24 24"), Some([0.0, 0.0, 24.0, 24.0]));
/// assert_eq!(view_box("-2, -2, 8, 4"), Some([-2.0, -2.0, 8.0, 4.0]));
/// assert_eq!(view_box("0 0 24"), None);
/// assert_eq!(view_box("0 0 0 24"), None);
/// ```
pub fn view_box(text: &str) -> Option<[f32; 4]> {
    let mut numbers = text
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|piece| !piece.is_empty())
        .map(str::parse::<f32>);
    let mut read = [0.0f32; 4];
    for slot in &mut read {
        *slot = numbers.next()?.ok()?;
    }
    if numbers.next().is_some() {
        return None;
    }
    let usable = read[2] > 0.0 && read[3] > 0.0 && read.iter().all(|value| value.is_finite());
    usable.then_some(read)
}

#[cfg(test)]
mod tests {
    use super::{DOCUMENT, PATHS, VIEW_BOX, paints, view_box};

    #[test]
    fn only_the_drawing_properties_are_reported_as_painting() {
        assert!(paints(PATHS));
        assert!(paints(VIEW_BOX));
        assert!(paints(DOCUMENT));
        assert!(
            !paints("value"),
            "a field's text changes on every keystroke and paints nothing by itself"
        );
    }

    #[test]
    fn a_view_box_is_four_finite_numbers_and_nothing_else() {
        assert_eq!(view_box("0 0 24 24"), Some([0.0, 0.0, 24.0, 24.0]));
        assert_eq!(view_box("  1,2 , 3 , 4  "), Some([1.0, 2.0, 3.0, 4.0]));
        assert_eq!(view_box("0 0 24 24 24"), None, "five numbers is not a box");
        assert_eq!(view_box("0 0 nope 24"), None);
        assert_eq!(view_box(""), None);
        assert_eq!(
            view_box("0 0 -4 4"),
            None,
            "a negative extent names no area"
        );
    }
}
