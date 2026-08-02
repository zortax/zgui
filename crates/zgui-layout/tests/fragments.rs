//! The fragment tree: absolute geometry, clips, stacking, sticky and the two unwind folds.
//!
//! | Module | Contents |
//! |---|---|
//! | `place` | where a fragment ends up, and in what order it is painted |
//! | `folds` | what an unwind folds up the tree |
//! | `hits` | what answers at a point |
//! | `bounds` | where an element is, across every piece it was painted as |
//! | `probe` | finding the box and the fragment a fixture element produced |

mod support;

#[path = "fragments/bounds.rs"]
mod bounds;
#[path = "fragments/folds.rs"]
mod folds;
#[path = "fragments/hits.rs"]
mod hits;
#[path = "fragments/place.rs"]
mod place;
#[path = "fragments/probe.rs"]
mod probe;
