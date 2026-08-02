//! The kind of control an element is.

/// What kind of control an element is: a button, a tab, a menu item, a table cell.
///
/// The role is the single most important thing an element declares about itself. It decides how
/// an assistive technology announces the element, which keyboard conventions apply to it, and
/// which of the other properties mean anything at all — a checked state means something on a
/// checkbox and nothing on a heading.
///
/// Two roles have framework-wide meaning and are worth knowing before any other:
///
/// * `Role::GenericContainer` is the role of a box that exists for layout only. Consumers filter
///   it out of the tree they present, so a deep visual nesting does not become a deep spoken one.
///   It is the right default for any element a view creates without saying what it is.
/// * `Role::TextRun` is the role of a run of text inside an editable field, and is likewise
///   filtered out of the presented tree.
///
/// This is a re-export rather than a parallel enumeration, and deliberately so: the enumeration
/// has roughly two hundred members and is the interchange vocabulary the platform layer speaks, so
/// a second copy of it would have to be kept in step by hand and would convert on every node of
/// every update.
pub use accesskit::Role;
