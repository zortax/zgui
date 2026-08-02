//! The style engine over a document: the surface it is styled against, the sheets that apply to
//! it, the restyle itself, and what a restyle costs the rest of the frame.
//!
//! ```
//! use std::sync::Arc;
//! use zgui_dom::{Document, NodeKind};
//! use zgui_geom::CssPx;
//! use zgui_interned::ElementName;
//! use zgui_style::{SheetOrigin, SheetSource, StyleEngine, Viewport};
//! use zgui_text::FixedMetrics;
//!
//! let mut document = Document::new();
//! let root = document.append(
//!     document.document_index(),
//!     NodeKind::Element,
//!     ElementName::new("root"),
//! );
//!
//! let mut engine = StyleEngine::new(
//!     &document,
//!     Arc::new(FixedMetrics::new()),
//!     Viewport::new(CssPx(1280.0), CssPx(800.0)),
//! );
//! engine.add_sheet(
//!     &document,
//!     SheetOrigin::Author,
//!     SheetSource::Text("root { color: rgb(1, 2, 3) }"),
//! );
//!
//! let pass = engine.restyle(&mut document, None);
//! assert_eq!(pass.styled, 1);
//! assert!(document.node(root).primary_style().is_some());
//! ```
//!
//! # The shape of a frame, from this crate's side
//!
//! Four things happen here, in this order, and the order is the point.
//!
//! 1. **The device.** If the surface moved, a new one is built. What that invalidates is three
//!    different things reaching the document by three different routes — see [`device`].
//! 2. **The filters.** If the installed sheets changed, the answers that let a mutation skip the
//!    engine entirely stop being answers, so they are switched off for that one frame — see
//!    [`deps`].
//! 3. **The restyle.** One traversal, or two when the root's own font metrics moved underneath it.
//!    The set of elements it styled is collected *by the traversal*, because reading it back off
//!    the tree afterwards costs about fifteen times the incremental restyle it describes.
//! 4. **The damage, and the retirement.** What the engine computed becomes obligations for the
//!    stages that follow, the filters are rebuilt now that the rule set has been flushed, and the
//!    obligations this pair consumed are retired — by an explicit walk, because the engine owns
//!    the traversal and no other walk drains them.
//!
//! | Module | Contents |
//! |---|---|
//! | [`engine`] | the rule set, the frame phases, and the worker pool |
//! | [`device`] | the surface, and what changing it invalidates |
//! | [`sheets`] | installing sheets, their order, and what the parser dropped |
//! | [`driver`] | the traversal, and the set it collects |
//! | [`deps`] | deciding that a change cannot affect any computed style |
//! | [`damage`] | turning what the engine computed into what the frame owes |
//!
//! # What this crate does not decide
//!
//! It does not paint, lay out or shape anything, and it holds no table any of those own. The text
//! colours a restyle changed leave as a list of values rather than as writes into a paint table,
//! because the table belongs to the scene and this crate does not depend on it.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod damage;
pub mod deps;
pub mod device;
pub mod driver;
pub mod engine;
pub mod sheets;

pub mod parity;

pub use crate::damage::{DamageSink, TextWork};
pub use crate::deps::{StyleDependencies, StyleFilterView};
pub use crate::device::{ColorScheme, DeviceEpoch, Viewport};
pub use crate::driver::animations::{
    AnimatedProperties, AnimationEdge, AnimationReport, AnimationTime, Animations,
    ElementAnimation, Lifecycle, TimedKind,
};
pub use crate::driver::{Restyle, traversal::Restyled};
pub use crate::engine::thread_pool::StylePool;
pub use crate::engine::{StyleEngine, TextPaintUpdate, TextRun};
pub use crate::sheets::SheetSource;
pub use crate::sheets::errors::{CssDiagnostic, CssDiagnostics, DropKind};
pub use crate::sheets::loader::{EmbeddedSheets, FilesystemSheets};
pub use crate::sheets::origin::{OriginMask, SheetOrigin};
pub use crate::sheets::set::{SheetHandle, SheetId};
