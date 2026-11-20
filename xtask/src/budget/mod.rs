//! The standing gate over the caches and what they are allowed to keep.
//!
//! A registered cache that has gone over its soft limit **comes back under it**, within a bounded
//! number of frames after the content that filled it went cold, and without rebuilding what it has
//! just freed. The compositor's atlas is held to the same claim, as the device's memory reports it.
//!
//! Each claim is read off a real window driven against the headless platform. The atlas subject
//! draws on this machine's graphics device and reports itself skipped on a machine without one. It
//! is a step of the definition of done and a subcommand of its own.

mod subject;

use std::path::Path;

use crate::error::Result;
use crate::gate;

/// Runs the gate.
pub(crate) fn run(root: &Path) -> Result<()> {
    gate::run(root, "budget", subject::SUBJECTS)
}
