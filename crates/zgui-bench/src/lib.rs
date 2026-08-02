//! The instruments the reference workloads are built out of.
//!
//! The binaries in `src/bin` are the workloads themselves; this is what they have in common. It
//! exists as a library rather than as a module of the harness because each workload is its own
//! process — a document mounted on a thread that has already mounted another is a document whose
//! caches are somebody else's — and separate processes cannot share a module of `main.rs`.
//!
//! Everything here serves one rule: **a criterion is a slope or a same-run ratio, never an
//! absolute microsecond count.** [`reference::fit`] turns a handful of (size, cost) points into a
//! slope; [`reference::verdict`] states a gate as a dimensionless number and refuses to pass when
//! the measurement behind it broke; [`reference::sample`] takes the medians the points are made of;
//! [`reference::watch`] records what a frame damaged, which is a fraction and therefore already
//! dimensionless.

#![forbid(unsafe_code)]

pub mod reference;
