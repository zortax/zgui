//! What a drawing is made of, and where its colour comes from.

use kurbo::BezPath;
use zgui_interned::CustomPropertyName;
use zgui_view::{IntoReactiveValue, PropKey, PropValue};

use crate::element::Element;
use crate::tag::Vector;

/// The property a drawing's outlines are carried in.
///
/// One path per line, each written the way a path is written in a vector image, so the list a view
/// passed survives the crossing and a backend with real vector nodes sets one path node per line.
pub use zgui_vocab::prop::drawing::PATHS;

/// The property naming the space a drawing's outlines are written in.
pub use zgui_vocab::prop::drawing::VIEW_BOX;

/// The property a whole vector document is carried in.
pub use zgui_vocab::prop::drawing::DOCUMENT;

/// The property a drawing's hit shape is carried in.
pub const HIT_SHAPE: &str = "hit";

/// The custom property that overrides what a drawing is filled with.
pub const FILL: &str = "zgui-fill";

/// The custom property that says what a drawing is stroked with.
pub const STROKE: &str = "zgui-stroke";

/// The custom property that says how wide that stroke is.
pub const STROKE_WIDTH: &str = "zgui-stroke-width";

/// Writes a list of paths the way [`PATHS`] carries them.
pub fn to_path_data<'a>(paths: impl IntoIterator<Item = &'a BezPath>) -> String {
    let mut data = String::new();
    for path in paths {
        if !data.is_empty() {
            data.push('\n');
        }
        data.push_str(&path.to_svg());
    }
    data
}

/// Reads back what [`to_path_data`] wrote, dropping any line that is not a path.
pub fn from_path_data(data: &str) -> Vec<BezPath> {
    data.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| BezPath::from_svg(line).ok())
        .collect()
}

impl Element<Vector> {
    /// The outlines this element draws, in its own coordinate space.
    ///
    /// ```
    /// use kurbo::{BezPath, Rect, Shape};
    /// use zgui_elements::vector;
    ///
    /// let square = Rect::new(0.0, 0.0, 16.0, 16.0).to_path(0.1);
    /// let drawing = vector().paths([square]);
    /// ```
    #[must_use]
    pub fn paths(self, paths: impl IntoIterator<Item = BezPath>) -> Self {
        let paths: Vec<BezPath> = paths.into_iter().collect();
        self.property(
            PropKey::new(PATHS),
            PropValue::from(to_path_data(paths.iter()).as_str()),
        )
    }

    /// A whole vector document this element draws, as its source.
    ///
    /// The document brings its own outlines, its own paints and its own space, so this replaces
    /// [`Element::paths`] and [`Element::view_box`] rather than adding to them.
    ///
    /// Its colours are its own unless it asked for the inherited one. A document written with
    /// `fill="currentColor"` takes this element's colour, so the same asset is dark on a light
    /// button and light on a dark one with nothing but a `color` declaration between them. A
    /// document written with colours of its own keeps every one of them, whatever colour the text
    /// around it happens to be — the two are told apart by what the document says, not by which
    /// method was called.
    ///
    /// ```
    /// use zgui_elements::vector;
    ///
    /// // An icon that follows the text around it.
    /// let icon = vector().document(
    ///     r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">
    ///          <path d="M2 8 L8 2 L14 8 L8 14 Z" fill="currentColor"/>
    ///        </svg>"#,
    /// );
    /// ```
    #[must_use]
    pub fn document(self, source: &str) -> Self {
        self.property(PropKey::new(DOCUMENT), PropValue::from(source))
    }

    /// The square of user space the outlines are written in, and the size they are drawn at.
    ///
    /// Setting one is what makes a drawing resolution-independent: the outlines are scaled
    /// uniformly to fit this element's content box and centred in whatever is left over, so one
    /// path constant is drawn at every size an icon is asked for.
    ///
    /// Without one, the outlines are already in the coordinates the element's own box was placed
    /// with — CSS pixels from its content box's top left corner — which is what a chart mark
    /// wants, because the same numbers decided where its box goes.
    ///
    /// ```
    /// use zgui_elements::vector;
    ///
    /// // A twenty-four unit square, whatever size the element ends up.
    /// let icon = vector().view_box(0.0, 0.0, 24.0, 24.0);
    /// ```
    #[must_use]
    pub fn view_box(self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.property(
            PropKey::new(VIEW_BOX),
            PropValue::from(format!("{x} {y} {width} {height}").as_str()),
        )
    }

    /// The shape a pointer has to be inside for this drawing to be the thing it hit.
    ///
    /// Without one, the drawing's whole box is its hit area, which is what a small icon wants and
    /// what a thin diagonal mark in a chart does not.
    #[must_use]
    pub fn hit_shape(self, shape: BezPath) -> Self {
        self.property(
            PropKey::new(HIT_SHAPE),
            PropValue::from(shape.to_svg().as_str()),
        )
    }

    /// What the outlines are filled with.
    ///
    /// Without this the fill is the element's own computed `color`, which makes the universal
    /// "an icon takes the colour of the text around it" behaviour the default rather than a
    /// keyword — and means `.icon:hover { color: … }` themes an icon with nothing else added.
    ///
    /// This is deliberately not `fill`. There is no `fill` property in this engine build: the
    /// whole family of vector-paint properties is gated to another engine and every declaration
    /// using one is dropped while the sheet is being parsed, so a drawing painted from `fill`
    /// would be a drawing painted from nothing.
    #[must_use]
    pub fn fill<M>(self, paint: impl IntoReactiveValue<Option<String>, M>) -> Self {
        self.custom_property(CustomPropertyName::new(FILL), paint)
    }

    /// What the outlines are stroked with. Nothing, unless this says otherwise.
    #[must_use]
    pub fn stroke<M>(self, paint: impl IntoReactiveValue<Option<String>, M>) -> Self {
        self.custom_property(CustomPropertyName::new(STROKE), paint)
    }

    /// How wide that stroke is.
    #[must_use]
    pub fn stroke_width<M>(self, width: impl IntoReactiveValue<Option<String>, M>) -> Self {
        self.custom_property(CustomPropertyName::new(STROKE_WIDTH), width)
    }
}

#[cfg(test)]
mod tests {
    use kurbo::{BezPath, Rect, Shape};

    use super::{from_path_data, to_path_data};

    #[test]
    fn a_list_of_paths_survives_the_crossing_as_a_list() {
        let first = Rect::new(0.0, 0.0, 8.0, 8.0).to_path(0.1);
        let second = Rect::new(2.0, 2.0, 4.0, 4.0).to_path(0.1);
        let data = to_path_data([&first, &second]);
        assert_eq!(data.lines().count(), 2);

        let read = from_path_data(&data);
        assert_eq!(read.len(), 2, "two marks, not one path with two subpaths");
        assert_eq!(read[0].bounding_box(), first.bounding_box());
        assert_eq!(read[1].bounding_box(), second.bounding_box());
    }

    #[test]
    fn no_paths_is_no_lines_rather_than_one_empty_one() {
        assert_eq!(to_path_data(Vec::<&BezPath>::new()), "");
        assert!(from_path_data("").is_empty());
        assert!(from_path_data("\n  \n").is_empty());
    }
}
