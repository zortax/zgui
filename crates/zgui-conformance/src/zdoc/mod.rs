//! `.zdoc`: an element tree and a style sheet, and nothing else.
//!
//! A layout conformance suite is written in HTML, and this framework ships no HTML — not in the
//! library, and deliberately not in the test harness either, because a parser admitted "just for
//! tests" is how a locked constraint erodes. So a suite is *converted* once, ahead of time, into
//! the format here: a viewport, a style sheet, and a tree of elements with classes, text and
//! natural sizes. That is the whole of what the convertible part of a layout suite uses, and it is
//! small enough to parse in a hundred lines that could never grow into a document engine.
//!
//! ```
//! use zgui_conformance::zdoc::Zdoc;
//!
//! let source = "\
//! @viewport 200 100
//! @css
//! root { display: flex; width: 200px }
//! .grow { flex-grow: 1 }
//! @tree
//! root
//!   div.grow \"one\"
//!   div [40x30]
//! ";
//!
//! let document = Zdoc::parse(source).expect("a well-formed document");
//! assert_eq!(document.root.children.len(), 2);
//! assert_eq!(document.root.children[0].classes, ["grow"]);
//! assert_eq!(document.root.children[1].replaced, Some((40.0, 30.0)));
//! ```

pub mod build;
pub mod parse;
pub mod source;

pub use crate::zdoc::parse::ParseError;
pub use crate::zdoc::source::{Element, Zdoc};
