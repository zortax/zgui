//! A vector icon set, drawn through the path renderer, sized and coloured by CSS.
//!
//! An icon here is a `const`: a name, a square, and one outline written in path notation. There is
//! no image to decode, no XML to parse and no file to ship beside the binary — an icon is the
//! string a `<vector>` element already carries a drawing in, so drawing one costs the same as
//! setting an attribute.
//!
//! ```
//! use zgui::prelude::*;
//! use zgui::{component, view};
//! use zgui_ui_icons::prelude::*;
//! use zgui_ui_icons::set::{chevron::CHEVRON_RIGHT, mark::CHECK};
//!
//! /// A row with a tick at one end and a chevron at the other.
//! #[component]
//! fn Item() -> impl IntoView {
//!     view! {
//!         row {
//!             Icon(icon = CHECK, size = IconSize::Sm)
//!             text {"Two-factor authentication"}
//!             spacer()
//!             Icon(icon = CHEVRON_RIGHT, label = "Open")
//!         }
//!     }
//! }
//! ```
//!
//! # Only what is named is linked
//!
//! Every icon is a separate `const` rather than an entry in a table, which is what makes the set
//! pay for itself: a table would be one value that every program references whole, and a program
//! that draws three icons would carry all of them. A `const` nothing names contributes nothing —
//! not a string, not a symbol, not a byte.
//!
//! # Colour, and why there is no colour prop
//!
//! An outline with no fill of its own is filled with the element's own computed `color`. That is
//! the whole colour story: an icon inside a destructive button is red because that button's text
//! is red, an icon in a disabled control is faded because the control is, and neither needed a
//! prop or a variant. Setting `--zgui-fill` overrides it for a drawing that has to differ from the
//! text around it.
//!
//! # Where the outlines are written
//!
//! In a square [`IconData::view_box`] units on a side — 24 for every icon here. A counter is a
//! subpath wound the other way inside the same outline, so a ring is genuinely a ring: filling it
//! with the non-zero rule leaves the middle empty rather than covering it with a second shape.
//!
//! ```
//! use zgui::elements::kurbo::{Point, Shape};
//! use zgui_ui_icons::set::status::INFO;
//!
//! let path = INFO.path();
//! // Inside the ring's stroke: filled.
//! assert_ne!(path.winding(Point::new(12.0, 4.0)), 0);
//! // Between the ring and the glyph: a hole.
//! assert_eq!(path.winding(Point::new(6.5, 12.0)), 0);
//! ```
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`icon`] | [`IconData`], [`IconSize`] and [`IconVariants`] |
//! | [`set`] | the icons |
//! | [`view`](mod@view) | [`Icon`], the component, and [`IconStyle`], its sheet |
//! | [`prelude`] | all of the above, in one import |

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod icon;
pub mod prelude;
pub mod set;
pub mod view;

pub use crate::icon::{IconData, IconSize, IconVariants};
pub use crate::view::{Icon, IconProps, IconStyle};
