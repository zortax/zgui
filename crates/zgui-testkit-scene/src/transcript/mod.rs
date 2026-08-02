//! A finished display list, as text a review can read and a golden can hold.
//!
//! The transcript is the primary regression artifact for everything that produces a scene, because
//! it is *diffable*: a reviewer reads what changed instead of squinting at two images, and it does
//! not flake on a driver version or a font that happens to be installed. What it records is every
//! decision the [`Scene`](zgui_scene::Scene) made before a renderer saw it — the primitives in draw
//! order, with their paints, clips and transforms resolved through the side tables, and the
//! vector-pass plan beside them.
//!
//! # Two properties, and neither is optional
//!
//! **It is stable.** Rendering the same scene twice produces the same bytes: the primitives are
//! walked in draw order through the same batching the renderer uses, every number goes through
//! [`crate::text::number`], and no hash map's iteration order reaches the page.
//!
//! **It is complete enough to fail.** A transcript that omitted a field would be perfectly stable
//! and would pass every golden while the field regressed. Every primitive's paint, clip, transform
//! and geometry is therefore rendered — with fields at their default value omitted, so that a diff
//! shows what moved rather than a wall of zeroes.

pub mod clip;
pub mod paint;
pub mod path;
pub mod primitive;
pub mod scene;
pub mod tile;

pub use crate::transcript::scene::{Transcript, of};
