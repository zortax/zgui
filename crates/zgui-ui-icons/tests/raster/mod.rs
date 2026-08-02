//! Reading back what a graphics device actually drew for an icon.
//!
//! Everything else written about this component stops at the display list: the outline reached the
//! element, the box was the right size, a primitive was pushed. A rasteriser that writes nothing at
//! all satisfies every one of those, and a renderer with no rasteriser attached satisfies them
//! while composing an unwritten scratch — which is a blank space where an icon should be, and the
//! state a component gallery's icon card was found in.
//!
//! So these fixtures drive the component the way an application does — through the umbrella, over
//! the headless platform — and put a *real* graphics device at the bottom of it, offscreen so that
//! the pixels can be copied back and asserted on.

#![allow(
    dead_code,
    unreachable_pub,
    reason = "one support module serves several groups of assertions, none of which uses all of it"
)]

pub mod device;
pub mod measure;
pub mod page;
pub mod script;
