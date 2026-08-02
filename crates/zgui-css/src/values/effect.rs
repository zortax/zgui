//! Filters, shadows and the two properties that force a group to be composited on its own.

/// The computed value of `isolation`.
pub use style::computed_values::isolation::T as IsolationValue;
/// The computed value of `mix-blend-mode`.
pub use style::computed_values::mix_blend_mode::T as MixBlendModeValue;
/// One entry of a `box-shadow` list.
pub use style::values::computed::BoxShadow as BoxShadowValue;
/// One entry of a `filter` or `backdrop-filter` list.
///
/// Both properties hold a list, and the list is ordered: the operations apply in the order written,
/// so it is a pipeline rather than a set.
pub use style::values::computed::Filter as FilterValue;
/// The computed value of `opacity`, between zero and one.
pub use style::values::computed::Opacity as OpacityValue;
/// One entry of a `text-shadow` list, which unlike a box shadow has no spread.
pub use style::values::computed::SimpleShadow as SimpleShadowValue;
/// The computed value of `clip-path`.
pub use style::values::computed::basic_shape::ClipPath as ClipPathValue;
