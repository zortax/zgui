//! What an icon is: a name, a square it is drawn in, and one outline.

mod size;

pub use crate::icon::size::{IconSize, IconVariants};

use zgui::elements::kurbo::BezPath;

/// One icon's geometry, as a compile-time constant.
///
/// An icon is a single outline written in path notation, inside a square user space of
/// [`IconData::view_box`] units on a side. One outline rather than a list of them, because a
/// counter is drawn as a subpath wound the other way: a ring is an outer circle and an inner
/// circle in the same string, and filling it with the non-zero rule leaves the middle empty. Two
/// separate outlines would be two filled shapes and the second would cover the hole in the first.
///
/// Every icon in [`set`](crate::set) is a `const` of this type, so a program that names three of
/// them links three path strings and nothing else — an icon nothing references contributes no
/// bytes at all.
///
/// ```
/// use zgui_ui_icons::set::mark::CHECK;
///
/// assert_eq!(CHECK.name(), "check");
/// assert_eq!(CHECK.view_box(), 24.0);
/// // The geometry is a real path, and it is inside the square it declares.
/// let path = CHECK.path();
/// let bounds = zgui::elements::kurbo::Shape::bounding_box(&path);
/// assert!(bounds.x0 >= 0.0 && bounds.x1 <= 24.0);
/// ```
///
/// # Drawing one
///
/// [`Icon`](crate::Icon) is the component. It renders a `<vector>` whose outline is this and whose
/// size and colour come from CSS, so nothing here decides how large an icon is or what colour it
/// takes.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct IconData {
    /// What the icon is called, in kebab case.
    name: &'static str,
    /// The side of the square its geometry is written in.
    view_box: f64,
    /// The outline, in path notation, in that square.
    path_data: &'static str,
}

impl IconData {
    /// Declares an icon drawn in a square `view_box` units on a side.
    ///
    /// ```
    /// use zgui_ui_icons::IconData;
    ///
    /// /// A square with a square hole in it.
    /// const FRAME: IconData = IconData::new(
    ///     "frame",
    ///     24.0,
    ///     "M2 2 L22 2 L22 22 L2 22 Z M6 6 L6 18 L18 18 L18 6 Z",
    /// );
    /// assert_eq!(FRAME.name(), "frame");
    /// ```
    #[must_use]
    pub const fn new(name: &'static str, view_box: f64, path_data: &'static str) -> Self {
        Self {
            name,
            view_box,
            path_data,
        }
    }

    /// What the icon is called.
    ///
    /// Written to `data-icon` by [`Icon`](crate::Icon), so a style sheet can select one icon out of
    /// a set and a transcript of a frame says which icon was drawn.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The side of the square the geometry is written in.
    #[must_use]
    pub const fn view_box(&self) -> f64 {
        self.view_box
    }

    /// The outline, in path notation.
    ///
    /// This is the form `<vector>` carries a drawing in, so handing it straight to the element
    /// costs no parse and no re-serialisation.
    #[must_use]
    pub const fn path_data(&self) -> &'static str {
        self.path_data
    }

    /// The outline as geometry, for measuring it or transforming it.
    ///
    /// Drawing an icon does not go through this: the component hands the path notation to the
    /// element unchanged. This is for a caller doing something with the shape itself — hit
    /// testing it, fitting it, or checking in a test that a counter really is a hole.
    ///
    /// # Panics
    ///
    /// Panics if the path notation does not parse, which for a `const` declared here is a defect
    /// in this crate rather than something a caller can provoke.
    #[must_use]
    pub fn path(&self) -> BezPath {
        BezPath::from_svg(self.path_data)
            .unwrap_or_else(|error| panic!("the `{}` icon does not parse: {error}", self.name))
    }
}
