//! Texture-atlas policy: where a rasterised tile goes, how long it stays, and when its space
//! comes back.
//!
//! There is no GPU in this crate. Allocation, eviction, reference counting and the upload queue
//! are ordinary data structures, and the one thing that genuinely needs a device — writing bytes
//! into a texture — is behind [`TextureSink`]. [`MemorySink`] implements that trait over plain
//! `Vec<u8>` storage, so the whole policy is exercised at CPU speed, in a unit test, with no
//! adapter and no window.
//!
//! # The shape of it
//!
//! | Type | Role |
//! |---|---|
//! | [`AtlasKey`] | what a caller caches *by*: an opaque handle plus an explicit [`TextureKind`] |
//! | [`AtlasTile`] | where the content landed: a texture, a tile id and a rectangle |
//! | [`Atlas`] | the policy — allocation, refcounts, use marking, eviction, deferred uploads |
//! | [`TextureSink`] | the one seam a real device implements |
//!
//! ```
//! use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, MemorySink, TextureKind};
//! use zgui_geom::Size;
//!
//! let mut sink = MemorySink::new();
//! let mut atlas = Atlas::new(AtlasLimits::default());
//!
//! let key = AtlasKey::new(0xbeef, TextureKind::Mono);
//! let tile = atlas
//!     .get_or_insert(key, Size::new(8, 8), || vec![0xff; 64])
//!     .expect("an 8x8 tile fits in a fresh atlas");
//! assert_eq!(tile.bounds.size, Size::new(8, 8));
//!
//! // Nothing reaches the device until the frame flushes: not the texels, and not the texture
//! // they went into.
//! assert_eq!(sink.textures_created(), 0);
//! assert_eq!(sink.bytes_written(), 0);
//! atlas.flush_uploads(&mut sink).expect("the in-memory sink accepts every write");
//! assert_eq!(sink.textures_created(), 1);
//! assert_eq!(sink.bytes_written(), 64);
//! ```
//!
//! # Why the key is opaque
//!
//! A closed `Glyph | Svg | Image` enum makes the atlas's taxonomy of content the whole world's
//! taxonomy, and a consumer with a fourth kind of cacheable raster has nowhere to put it. So a key
//! is a `u64` the caller derives however it likes — a hash of glyph id, size, subpixel offset and
//! font, or of an image URL and a decode size — paired with the [`TextureKind`] that decides which
//! pool and which pixel format it lands in. The atlas compares keys and never interprets them.
//!
//! # What the policy actually promises
//!
//! * **Tile space comes back.** [`Atlas::remove`] returns the rectangle to the allocator it came
//!   from, so a long session that churns glyph variants does not grow without bound.
//! * **Reference counts saturate rather than wrap.** [`Atlas::release`] on an entry at zero is a
//!   no-op, not an underflow.
//! * **Eviction is by generation, newest kept.** Each frame is a generation; touching an entry
//!   moves it to the current one. [`Atlas::evict_least_recently_used`] frees exactly the entries
//!   sharing the oldest generation that are unreferenced and untouched this frame.
//! * **Uploads are deferred.** Bytes queue up and leave in one flush, so a frame issues one batch
//!   of writes at a point it chooses rather than one write per glyph wherever the glyph was needed.
//! * **So are texture lifetimes.** Growing a pool records a creation rather than issuing one, and
//!   [`Atlas::flush_uploads`] performs it just before the writes that land in it. Nothing but that
//!   one call needs a device, so caching a glyph's pixels is something a walk with no renderer in
//!   reach can do.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod atlas;
pub mod error;
pub mod key;
pub mod sink;
pub mod texture;
pub mod tile;

pub use crate::atlas::{Atlas, AtlasLimits, AtlasReport, Eviction};
pub use crate::error::AtlasError;
pub use crate::key::AtlasKey;
pub use crate::sink::{MemorySink, SinkError, TextureSink};
pub use crate::texture::{TextureFormat, TextureId, TextureKind};
pub use crate::tile::{AtlasTile, TileId};
