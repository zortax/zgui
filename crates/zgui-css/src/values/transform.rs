//! The four properties that move a box without moving anything around it.

/// The computed value of `backface-visibility`.
pub use style::computed_values::backface_visibility::T as BackfaceVisibilityValue;
/// The computed value of `perspective`.
pub use style::values::computed::Perspective as PerspectiveValue;
/// The computed value of `rotate`.
pub use style::values::computed::Rotate as RotateValue;
/// The computed value of `scale`.
pub use style::values::computed::Scale as ScaleValue;
/// The computed value of `transform`: an ordered list of operations.
///
/// The list is a pipeline and not a set — the operations compose right to left, so reordering two
/// of them is a different transform.
pub use style::values::computed::Transform as TransformValue;
/// One entry of a [`TransformValue`].
pub use style::values::computed::TransformOperation as TransformOperationValue;
/// The computed value of `transform-origin`.
pub use style::values::computed::TransformOrigin as TransformOriginValue;
/// The computed value of `transform-style`, which decides whether children share a 3D context.
pub use style::values::computed::TransformStyle as TransformStyleValue;
/// The computed value of `translate`.
pub use style::values::computed::Translate as TranslateValue;
