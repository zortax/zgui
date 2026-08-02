//! The worked applications, and the one thing they prove that a test cannot.
//!
//! Each example is a whole program under `examples/` at the root of this repository, built as a
//! target of this package:
//!
//! | Example | What it is for |
//! |---|---|
//! | `counter` | the smallest application that has state: two buttons and a number |
//! | `todo` | a keyed list, text entered from the keyboard, and state that outlives a row |
//! | `styled` | a gallery of what the style engine draws: flexbox, grid, gradients, shadows, rounded corners, transforms, filters and type |
//!
//! ```text
//! cargo run -p zgui-examples --example counter
//! cargo run -p zgui-examples --example todo
//! cargo run -p zgui-examples --example styled
//! ```
//!
//! # Why they are a package of their own
//!
//! This package depends on `zgui` and on nothing else. An application does, so the examples do,
//! and the arrangement is what makes that claim checkable rather than aspirational: an example
//! living beside the framework's own crates would compile against everything they pull in, and
//! would keep compiling on the day the umbrella stopped being enough on its own.
//!
//! There is nothing in this library. The programs are the deliverable.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
