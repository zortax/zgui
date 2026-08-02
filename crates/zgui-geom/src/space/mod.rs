//! The coordinate spaces geometry is measured in.
//!
//! A space is a zero-sized marker carried as a type parameter by [`Point`](crate::Point),
//! [`Size`](crate::Size) and [`Rect`](crate::Rect). It occupies no bytes and generates no code;
//! its only job is to make a value from one space unusable where another is expected.
//!
//! Three spaces exist and no more can be added from outside this crate, because every conversion
//! between them is defined here. They are related as follows:
//!
//! - [`Css`] is the author's coordinate system: the origin of the viewport, y growing downward,
//!   measured in [`CssPx`](crate::CssPx).
//! - [`Device`] is the output surface's pixel grid, measured in [`DevicePx`](crate::DevicePx).
//!   A [`Scale<Css, Device>`](crate::Scale) converts between the two.
//! - [`Layout`] is the space layout arithmetic happens in, measured in [`Au`](crate::Au) so that
//!   repeated addition never accumulates rounding error.

pub mod css;
pub mod device;
pub mod layout;

pub(crate) mod derive;

pub use crate::space::css::Css;
pub use crate::space::device::Device;
pub use crate::space::layout::Layout;

/// A coordinate space that geometry can be measured in.
///
/// This trait is implemented by [`Css`], [`Device`] and [`Layout`] and cannot be implemented
/// outside this crate: the set of spaces is closed because the set of conversions between them
/// is.
pub trait Space: sealed::Sealed + Copy + Send + Sync + 'static {
    /// The space's name, for diagnostics.
    ///
    /// ```
    /// use zgui_geom::{Css, Space};
    ///
    /// assert_eq!(Css::NAME, "css");
    /// ```
    const NAME: &'static str;
}

/// Keeps [`Space`] closed to this crate.
pub(crate) mod sealed {
    /// The supertrait no downstream type can name.
    pub trait Sealed {}
}

/// Declares a space marker: an uninhabited type, so no value of it can ever exist.
macro_rules! space {
    ($name:ident, $text:literal, $($doc:literal),+ $(,)?) => {
        $(#[doc = $doc])+
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {}

        impl $crate::space::sealed::Sealed for $name {}

        impl $crate::space::Space for $name {
            const NAME: &'static str = $text;
        }
    };
}

pub(crate) use space;
