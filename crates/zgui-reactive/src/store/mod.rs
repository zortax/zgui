//! Reactivity at field granularity, for state with a shape.
//!
//! A signal holding a struct wakes every reader when any field changes. A store wakes only the
//! readers of the field that changed, which is what makes a large form or a long list of rows
//! cost what it should.
//!
//! Everything here is re-exported from the crate root.

mod keyed;

pub use keyed::{ArcField, AtKeyed, Field, KeyedSubfield, Patch, Store, StoreField, Subfield};

/// The engine the [`derive@Store`] and [`Patch`] derives generate their paths against.
///
/// A derive macro expands to code in *your* crate, and the code these two expand to names the
/// engine by its own name. A crate that derives a store therefore has to have that name in scope,
/// and a crate that depends on this one rather than on the engine has no way to put it there —
/// so the derives would be re-exported and unusable, which is the worst of both.
///
/// Bring it into the module that derives:
///
/// ```
/// use zgui_reactive::store::reactive_stores;
/// use zgui_reactive::{Patch, Store};
///
/// /// Application state with a known shape.
/// #[derive(Clone, Debug, PartialEq, Store, Patch)]
/// struct Settings {
///     /// What the interface is called.
///     title: String,
///     /// How wide it starts.
///     width: u32,
/// }
///
/// let mut settings = Settings { title: "zgui".to_owned(), width: 800 };
/// settings.width = 900;
/// assert_eq!(settings.width, 900);
/// ```
///
/// Nothing else in it is part of this framework's surface: reach for the names published here
/// rather than through this alias.
pub use ::reactive_stores;
