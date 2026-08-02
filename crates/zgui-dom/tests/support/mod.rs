//! The engine harness: everything needed to run a real restyle over a real document.
//!
//! | Module | Contents |
//! |---|---|
//! | [`prefs`] | the feature flags a sheet has to be parsed with |
//! | [`device`] | the viewport and font metrics length units resolve against |
//! | [`sheets`] | stylesheet parsing, and hearing about what the parser dropped |
//! | [`pool`] | worker pools, and marking a thread as one |
//! | [`traversal`] | what each worker does with each element it is handed |
//! | [`damage`] | turning the engine's own per-element damage into obligations |
//! | [`engine`] | the stylist, the restyle, and what one pass did |
//! | [`edit`] | the four steps a change to a styled document is only correct with |
//! | [`filter`] | a real answer to "which state bits can matter here", from the rule set |
//! | [`fixture`] | the documents the cases are written against |
//! | [`rows`] | a list of rows, and the from-scratch oracle structural cases are judged against |
//! | [`read`] | reading computed values back off the tree |

// The harness is compiled into two test targets and each uses a different part of it: the budget
// target reads counters and drives one document, the engine target drives every acceptance case. A
// helper unused by one of them is not dead.
#![allow(dead_code)]

pub(crate) mod damage;
pub(crate) mod device;
pub(crate) mod edit;
pub(crate) mod engine;
pub(crate) mod filter;
pub(crate) mod fixture;
pub(crate) mod pool;
pub(crate) mod prefs;
pub(crate) mod read;
pub(crate) mod rows;
pub(crate) mod sheets;
pub(crate) mod traversal;
