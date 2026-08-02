//! SVG documents as drawable content: read once, coloured by context, drawn by either rasteriser.
//!
//! # What this crate is
//!
//! A reader. It turns an SVG document into a flat list of outlines with paints, stroke styles and
//! clips attached, expressed in `kurbo` and `peniko` and this framework's own colour type. It
//! draws nothing, it holds no device, and it names no rasteriser — which is exactly why a document
//! read here can be drawn by the compute-shader path renderer *and* by the fallback that needs no
//! compute shaders. A reader that produced one rasteriser's own scene type would have bound every
//! SVG asset in an application to that rasteriser.
//!
//! # The colour rule
//!
//! There are two kinds of document and they want opposite treatment.
//!
//! One is an icon: it writes `currentColor` and expects to be the colour of whatever it is next
//! to, so the same file is black in a paragraph, white on a dark button and red when the button is
//! in an error state. The other is a logo: it writes its own colours and expects to keep them,
//! whatever the text colour around it happens to be.
//!
//! Both arrive through one element and are told apart by what the document itself says. A paint
//! that asked for `currentColor` becomes [`Ink::Inherited`] and takes the drawing element's colour
//! at the moment it is drawn; a paint that named a colour becomes [`Ink::Solid`] and keeps it. A
//! colour change therefore re-colours an icon and leaves a logo alone, and neither re-reads a byte
//! of the document.
//!
//! ```
//! use zgui_svg::parse;
//!
//! let icon = parse(
//!     r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
//!          <path d="M0 0 H10 V10 H0 Z" fill="currentColor"/>
//!        </svg>"##,
//! )
//! .expect("a readable document");
//! assert!(icon.is_inherited());
//!
//! let logo = parse(
//!     r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
//!          <path d="M0 0 H10 V10 H0 Z" fill="#3366cc"/>
//!        </svg>"##,
//! )
//! .expect("a readable document");
//! assert!(!logo.is_inherited());
//! ```
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`document`] | what a read document is: its box, its outlines and their paint |
//! | [`document::place`] | moving a document into the coordinates it is drawn in |
//! | [`parse`](mod@parse) | the one place the SVG parser is named |

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod document;
pub mod parse;

pub use crate::document::gradient::{Gradient, GradientKind, Stop};
pub use crate::document::ink::Ink;
pub use crate::document::shape::{Clip, Fill, Paint, Shape, Stroke};
pub use crate::document::{Document, Unsupported};
pub use crate::parse::{Error, parse};
