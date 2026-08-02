//! What a list item marks itself with, and where the mark goes.
//!
//! All three properties are inherited and CSS applies them to the *list item*, not to a
//! pseudo-element of it, so a reader looks them up on the item's own style.

/// The computed value of `list-style-position`.
pub use style::computed_values::list_style_position::T as ListStylePositionValue;
/// The computed value of `list-style-image`.
pub use style::values::computed::Image as ListStyleImageValue;
/// The computed value of `list-style-type`.
pub use style::values::computed::ListStyleType as ListStyleTypeValue;
/// The computed value of `quotes`.
pub use style::values::computed::Quotes as QuotesValue;
