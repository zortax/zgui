//! The standing gate over the caches and what they are allowed to keep.
//!
//! Two claims run together, because neither is safe without the other:
//!
//! * a registered cache that has gone over its soft limit **comes back under it**, within a
//!   bounded number of frames after the content that filled it went cold, and without rebuilding
//!   what it has just freed;
//! * a cached range that is replayed rather than re-encoded **never names a raster the cache
//!   behind it has since freed** — which is what a cache that evicts freely and a stage that draws
//!   without looking anything up produce together, and which no assertion about either alone can
//!   see.
//!
//! Both are read off real windows driven against the headless platform, so the gate needs no
//! display and no graphics device. It is a step of the definition of done and a subcommand of its
//! own.

mod subject;

use std::path::Path;

use crate::error::Result;
use crate::gate;

/// Runs the gate.
pub(crate) fn run(root: &Path) -> Result<()> {
    gate::run(root, "budget", subject::SUBJECTS)
}
