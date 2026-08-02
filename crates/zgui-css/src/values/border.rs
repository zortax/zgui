//! Borders and outlines: the four sides, the four corners and the widths between them.

/// The computed value of one `border-*-radius` corner: a horizontal and a vertical radius, which
/// may differ, so a corner is an ellipse quadrant rather than an arc.
pub use style::values::computed::BorderCornerRadius as BorderCornerRadiusValue;
/// The computed value of `border-image-repeat`.
pub use style::values::computed::BorderImageRepeat as BorderImageRepeatValue;
/// The computed value of `border-image-slice`.
pub use style::values::computed::BorderImageSlice as BorderImageSliceValue;
/// The computed value of `border-image-width`.
pub use style::values::computed::BorderImageWidth as BorderImageWidthValue;
/// The computed value of one border or outline width.
pub use style::values::computed::BorderSideWidth as BorderSideWidthValue;
/// The computed value of one `border-*-style`.
pub use style::values::computed::BorderStyle as BorderStyleValue;
/// The computed value of `outline-style`, which has one value a border style does not: `auto`.
pub use style::values::computed::OutlineStyle as OutlineStyleValue;
