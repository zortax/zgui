//! What a window registers for budgeting, each wrapped in what the budget needs of it.
//!
//! An adapter rather than an implementation on the cache itself, and for one reason: three of these
//! caches need something the cache does not own to do their part. The glyph atlas cannot free a
//! texture without the renderer's sink; the shaping cache cannot drop a paragraph without
//! invalidating every measurement taken from one, which is the layout store's business; and the
//! render-target pool is reached only through the renderer seam. An adapter is where a window puts
//! those two halves together, and it exists for exactly as long as one visit of the registry.
//!
//! # What is registered, and what each states
//!
//! | Cache | Unit | Level | Why that level |
//! |---|---|---|---|
//! | [`PaintChunksBudget`] | bytes | [`PAINT_CHUNK_BYTES`](crate::budget::limits::PAINT_CHUNK_BYTES) | records outlive visits, so nothing else bounds an unvirtualised document's paintings |
//! | [`GlyphAtlasBudget`] | bytes | [`ATLAS_SOFT_BYTES`](crate::window::ATLAS_SOFT_BYTES) | several times a text-heavy document's glyphs, well under an unbounded atlas |
//! | [`DecodedImagesBudget`] | bytes | [`DECODED_IMAGE_BYTES`](crate::budget::limits::DECODED_IMAGE_BYTES) | the loader can decode a named source again, so off-screen history is honestly freeable |
//! | [`ParagraphShapingBudget`] | entries | [`SHAPED_PARAGRAPHS`](crate::budget::limits::SHAPED_PARAGRAPHS) | the largest document whose every element is live, with room |
//! | [`VectorResourcesBudget`] | entries | [`PLACED_DRAWINGS`](crate::budget::limits::PLACED_DRAWINGS) | the same, for what the per-frame retain does not bound |
//! | [`RenderTargetsBudget`] | bytes | none | the pool enforces a ceiling of its own that it never exceeds |
//! | [`DeviceMemoryBudget`] | bytes | none | pipelines, swapchain and scratch are live resources, not remembered results |
//!
//! The last of those is not a cache and frees nothing. It is registered because a registry that
//! accounted for a megabyte of five hundred could not answer the one question anybody asks of it.

pub mod atlas;
pub mod chunks;
pub mod device;
pub mod images;
pub mod shaping;
pub mod targets;
pub mod vectors;

pub use crate::budget::caches::atlas::GlyphAtlasBudget;
pub use crate::budget::caches::chunks::PaintChunksBudget;
pub use crate::budget::caches::device::DeviceMemoryBudget;
pub use crate::budget::caches::images::DecodedImagesBudget;
pub use crate::budget::caches::shaping::ParagraphShapingBudget;
pub use crate::budget::caches::targets::RenderTargetsBudget;
pub use crate::budget::caches::vectors::VectorResourcesBudget;
