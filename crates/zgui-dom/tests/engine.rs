//! The style engine, driven for real, over this crate's own DOM traits.
//!
//! Everything in this crate that matters is a promise about what the style engine will do when it
//! is handed one of our nodes, and no amount of unit testing reaches it: selector matching, the
//! cascade, snapshots, invalidation and the parallel traversal are all inside the engine, and the
//! only way to find out whether our side of the contract is right is to run it.
//!
//! So this target builds a small engine — feature prefs, a device, a stylesheet parser, a worker
//! pool and the traversal callback — and drives real restyles over real documents. The engine
//! driver that ships lives above this crate; what is here exists to hold this crate's traits to
//! account, and it is deliberately the smallest thing that can do that.
//!
//! | Module | Contents |
//! |---|---|
//! | [`support`] | the engine harness and the documents the cases are written against |
//! | [`cases`] | the questions |

// The engine's own element trait declares its data accessors `unsafe`, because it guarantees that
// one worker owns an element's data for the duration of that element's restyle rather than proving
// it. The traversal callback is the only place that contract is taken up, and it states its reason
// there.
#![allow(unsafe_code)]

mod cases;
mod support;
