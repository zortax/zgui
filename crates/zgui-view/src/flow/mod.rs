//! Control flow: showing one thing or another, a list, a delay, a failure, somewhere else.
//!
//! Every one of these is built out of the same two pieces as any other view — an effect and a
//! [`Hole`](crate::view::Hole) — and none of them is special to the framework. They are here
//! because getting each of them right involves one decision that is easy to get wrong, and making
//! that decision once is the point.
//!
//! [`Show`], [`For`] and [`Portal`] are also written as tags — `<Show when=…>`,
//! `<For each=… key=… let:…>` and `<Portal layer=…>` — through the props builders beside them,
//! [`ShowProps`], [`ForProps`] and [`PortalProps`]. The rest are written as values in a block,
//! because their arguments are not attributes.

mod branch;
mod dynamic;
mod each;
mod error;
pub mod given;
mod portal;
mod show;
mod suspense;

pub use crate::flow::dynamic::Dynamic;
pub use crate::flow::each::{EachState, For, ForProps, ForPropsBuilder};
pub use crate::flow::error::{
    ErrorBoundary, ErrorBoundaryState, ResultState, ViewError, report_error,
};
pub use crate::flow::portal::{Portal, PortalProps, PortalPropsBuilder, PortalState};
pub use crate::flow::show::{Show, ShowProps, ShowPropsBuilder, ShowState};
pub use crate::flow::suspense::{Await, Suspense, SuspenseContext, SuspenseState, Transition};
