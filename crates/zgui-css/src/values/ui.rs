//! What the pointer and the caret interact with.

/// The computed value of `pointer-events`.
///
/// Inherited, so a descendant of an element that takes no pointer events computes to the same
/// value rather than having to be found by a walk up the tree.
pub use style::values::computed::ui::PointerEvents as PointerEventsValue;

/// The computed value of `cursor`: the keyword, and the images that were offered before it.
///
/// Inherited, which is what makes the pointer's cursor a question about the *innermost* element it
/// is over rather than a walk up the tree: an element that says nothing has already computed to
/// whatever its ancestor said.
pub use style::values::computed::ui::Cursor as CursorValue;
/// The keyword half of `cursor`.
pub use style::values::specified::ui::CursorKind;

/// The computed value of `user-select`.
///
/// *Not* inherited, which is the whole difficulty with it: `auto` means "whatever the parent's used
/// value was", so the used value is found by walking up until something says otherwise. A property
/// that merely inherited would be answered by reading the element.
pub use style::values::specified::ui::UserSelect as UserSelectValue;

/// The computed value of `caret-color`, which is `auto` or a colour.
pub use style::values::computed::color::CaretColor as CaretColorValue;
/// The `auto`-or-a-value shape a caret colour takes.
pub use style::values::generics::color::GenericColorOrAuto as ColorOrAuto;
