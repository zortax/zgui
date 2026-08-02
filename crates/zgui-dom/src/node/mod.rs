//! The node: its record, the handle that reaches it, and the rule its fields are declared under.

pub mod atomics;
pub mod discipline;
pub mod element;
pub mod flags;
pub mod handle;
pub mod inner;
pub mod kind;
pub mod links;

pub use crate::node::discipline::CellDisciplined;
pub use crate::node::flags::NodeFlags;
pub use crate::node::handle::Node;
pub use crate::node::inner::NodeInner;
pub use crate::node::kind::NodeKind;
