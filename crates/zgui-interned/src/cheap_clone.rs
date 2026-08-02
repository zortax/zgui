//! The bound set a cheaply cloned string has to satisfy.

use core::fmt::Debug;

/// A string that costs nothing to clone, compare or default-construct.
///
/// A layout engine handed a custom-identifier type — named grid lines, named grid areas — clones
/// those names freely while it resolves a track list, so the type it is given has to be one whose
/// clone is a copy rather than an allocation. Engines state that requirement as exactly this set
/// of bounds and provide a blanket implementation over it, so a type that satisfies them is
/// accepted without this crate having to name the engine at all.
///
/// It is implemented for every type meeting the bounds, so it is a statement of a contract rather
/// than a thing to implement by hand. Every interned name in this crate satisfies it:
///
/// ```
/// use zgui_interned::{CheapCloneStr, Ident};
///
/// fn takes_any_cheap_string<S: CheapCloneStr>(name: S) -> String {
///     name.as_ref().to_owned()
/// }
///
/// assert_eq!(takes_any_cheap_string(Ident::new("sidebar")), "sidebar");
/// ```
pub trait CheapCloneStr:
    AsRef<str>
    + for<'a> From<&'a str>
    + From<String>
    + PartialEq
    + Eq
    + Clone
    + Default
    + Debug
    + 'static
{
}

impl<T> CheapCloneStr for T where
    T: AsRef<str>
        + for<'a> From<&'a str>
        + From<String>
        + PartialEq
        + Eq
        + Clone
        + Default
        + Debug
        + 'static
{
}
