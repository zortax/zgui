//! Rasterising vector content without compute shaders, for devices that have none.
//!
//! # What this is for
//!
//! It exists so that a capability check has somewhere to fall back *to*. A device with no compute
//! shaders, or with no writable storage textures, cannot run the path renderer everything else here
//! is measured against; without this it would draw no icons at all, which is what a component
//! library is made of. So this exists alongside the check that selects it, rather than being a
//! fallback promised and never written.
//!
//! # The downgrade, stated rather than hidden
//!
//! Switching to this is **visible**, not transparent:
//!
//! * **No blend or compose set.** Everything composites source-over; a path asking for anything else
//!   gets source-over.
//! * **Multisampled coverage rather than analytic.** Every pixel is decided by sixteen samples, so an
//!   edge lands on one of seventeen levels rather than on a continuum. Interiors are exact; edges
//!   differ from the analytic answer by up to about one sixteenth.
//! * **Cost grows with area times outline complexity.** Each pixel of an item's box tests every
//!   sample against every segment of that item's outline. That is entirely reasonable for icons and
//!   entirely unreasonable for a map.
//! * **Ramps are filled flat.** A gradient fill or stroke is drawn in the ramp's mean colour rather
//!   than as a ramp — a gradient-filled icon, or a heading whose brush is a background, looks flat
//!   here and is still there. A paint that samples an image has no such stand-in and is not drawn.
//!
//! Everything else is the same contract: the same plan, executed rather than re-derived; the same
//! residual clips absorbed rather than paid for with a pass; straight — un-premultiplied — colour in
//! the scratch, with the composite premultiplying as it reads.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod raster;

pub use crate::raster::CoverageRaster;
