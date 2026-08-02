//! Converting a reference suite into documents this framework can lay out, and running it.
//!
//! A reference test is two documents that must lay out identically by different means. That is a
//! statement about *layout*, so it is settled by comparing fragment trees rather than pixels: no
//! graphics device, no anti-aliasing tolerance, microseconds instead of milliseconds, and a failure
//! that names the box and the edge that moved rather than a count of differing pixels.
//!
//! A test outside the convertible subset is neither passed nor failed — it is reported
//! unconvertible, with the reason, and counted. Counting it is what stops a suite from looking
//! healthy because most of it was quietly skipped.
//!
//! ```no_run
//! use zgui_conformance::wpt::suite;
//!
//! let results = suite::run_all().expect("the corpus is readable");
//! assert!(results.iter().all(|suite| suite.tests > 0));
//! ```

pub mod markup;
pub mod suite;

pub use crate::wpt::markup::{Converted, Unconvertible, convert};
pub use crate::wpt::suite::{Outcome, SuiteResult, TestResult};
