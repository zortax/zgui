//! Where the selectable units of a shaped line sit.
//!
//! A caret is placed between clusters and a hit test answers with the cluster it landed in, so
//! both need the same thing: for one line, in the order the eye reads it, which bytes each cluster
//! covers and how far along the line it sits. Neither is derivable from the string — a ligature is
//! one cluster of several characters, a mark is none at all, and a bidirectional line puts the
//! bytes on the screen in an order the string does not have — so it comes from the shaper, which
//! is the only thing that knows.
//!
//! [`ClusterGeometry`](crate::ClusterGeometry) is one such unit. This module is what a line's worth
//! of them is grouped into: one [`ClusterRun`] per stretch of uniform direction, which is the
//! grouping both consumers need — an accessibility tree reports a direction per run, and a caret
//! at a direction boundary has two places it can be drawn.

pub mod run;
pub mod shaped;

pub use crate::cluster::run::ClusterRun;
pub use crate::cluster::shaped::ShapedClusters;
