//! Where a document's nodes live, and everything reachable from one.

pub mod class_pool;
pub mod columns;
pub mod document;
pub mod recycle;
pub mod store;

pub use crate::arena::class_pool::ClassPool;
pub use crate::arena::columns::Columns;
pub use crate::arena::document::Document;
pub use crate::arena::recycle::end_frame;
pub use crate::arena::store::DocumentStore;
