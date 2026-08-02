//! The questions this target exists to answer.
//!
//! | Module | Question |
//! |---|---|
//! | [`restyle`] | does a real traversal over our traits produce the right computed values |
//! | [`selectors`] | does every selector shape match the elements it should |
//! | [`syntax`] | which selector syntaxes does this build of the engine actually accept |
//! | [`pseudo`] | does an element get a style for `::before`, and only when a rule says so |
//! | [`damage`] | does a freshly inserted subtree get any layout obligation at all |
//! | [`mutate`] | does the batched change API leave the tree and its obligations intact |
//! | [`structure`] | do the siblings of an inserted or removed child re-match |
//! | [`descent`] | does a second mark with no intervening walk still get descended into |
//! | [`state_mask`] | does a cached state mask that outlives a class change lose a restyle |
//! | [`parallel`] | does a parallel traversal agree with a sequential one |
//! | [`seams`] | do the host hooks reach the engine, and does the default answer nothing |

mod damage;
mod descent;
mod mutate;
mod parallel;
mod pseudo;
mod restyle;
mod seams;
mod selectors;
mod state_mask;
mod structure;
mod syntax;
