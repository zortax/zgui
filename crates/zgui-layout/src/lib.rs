//! Boxes, their sizes and their positions: what a styled document turns into before anything is
//! drawn.
//!
//! # Three levels, not one
//!
//! An element, a box and a fragment are three different things and CSS forces the distinction. One
//! element generates no box (`display: none`), or several (a block inside an inline splits into
//! three), or boxes with no element of their own (`::before`, an anonymous wrapper, a list mark).
//! One box then produces several fragments — one per line, one per column, one per page. So this
//! crate holds two of the three levels: the [box tree](crate::boxtree), which is what the layout
//! algorithms walk, and the [fragment] tree, which is what painting, hit testing and
//! accessible geometry read. The element level belongs to the document.
//!
//! A box is named by the same handle the document records against the element that generated it, so
//! there is one box identity in the workspace and not two. The record for a box lives here; the
//! name is declared where the document can reach it.
//!
//! # No layout-engine style is ever built
//!
//! The layout algorithms read styles through traits, and what implements those traits is
//! [`StyleRef`] — a small borrow of one box's computed style. Lowering a computed
//! style into a second style struct, per box, per frame, would cost more than the layout it feeds.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`node`] | [`BoxNode`], the formatting-context tag and the child lists |
//! | [`boxtree`] | building the tree: anonymous boxes, flattening, `order`, out-of-flow re-parenting, generated content |
//! | [`tree`] | the store, the layout-algorithm traits, invalidation and the tree dump |
//! | [`style`] | the borrowed style view and every conversion out of a computed value |
//! | [`intrinsic`] | how narrow and how wide a box's content can be |
//! | [`inline`] | sizing a box whose height comes from its content |
//! | [`round`] | putting every edge on a whole device pixel |
//! | [`invariants`] | the checks that say the three levels still agree |
//! | [`fragment`] | the pieces painting, hit testing and accessibility read, and the index over them |
//! | [`scroll_region`] | what scrolls, how far, and what it reserves |
//! | [`container_query`] | laying out again when a container's own size changed the styles inside it |
//! | [`measure`] | the seam whoever drives a pass answers content questions through |
//! | [`parity`] | which CSS longhands this crate reads, declared beside their readers |
//!
//! ```
//! use zgui_arena::DocumentId;
//! use zgui_css::StyleDraft;
//! use zgui_layout::measure::NoContent;
//! use zgui_layout::node::box_node::BoxNode;
//! use zgui_layout::node::kind::{BoxKind, FormattingContext};
//! use zgui_layout::style::DeviceStyle;
//! use zgui_layout::tree::LayoutTree;
//! use zgui_layout::tree::store::LayoutStore;
//!
//! let mut store = LayoutStore::new(DocumentId::FIRST);
//! let root = store.insert(BoxNode::new(
//!     StyleDraft::initial().build(),
//!     BoxKind::Element,
//!     FormattingContext::Block,
//! ));
//! store.get_mut(root).expect("live").block_level = true;
//! store.set_root(root);
//!
//! let mut content = NoContent;
//! let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
//! assert!(tree.layout_root(taffy::Size { width: 800.0, height: 600.0 }));
//!
//! // A block-level root stretch-fits the viewport it was laid out in.
//! let layout = store.layout_of(root).expect("laid out");
//! assert_eq!(layout.size.width.0, 800.0);
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod axis;
pub mod boxtree;
pub mod container_query;
pub mod fragment;
pub mod inline;
pub mod intrinsic;
pub mod invariants;
pub mod key;
pub mod measure;
pub mod node;
pub mod parity;
pub mod round;
pub mod scroll_region;
pub mod style;
pub mod text;
pub mod tree;

pub use crate::axis::Axis;
pub use crate::fragment::hit::{HitEntry, HitIndex, PointerEvents};
pub use crate::fragment::{FragKey, Fragment, FragmentFlags, FragmentKind};
pub use crate::key::{from_node_id, to_node_id};
pub use crate::measure::{MeasureContent, MeasureRequest, Measured, NoContent};
pub use crate::node::box_node::BoxNode;
pub use crate::node::kind::{BoxKind, FormattingContext, PseudoKind};
pub use crate::style::{DeviceStyle, StyleRef};
pub use crate::text::Paragraphs;
pub use crate::tree::LayoutTree;
pub use crate::tree::store::{LayoutStore, ResolvedLayout};

/// A generation-checked name for one box of the box tree.
///
/// Re-exported rather than redefined: the document records which boxes each element generated, so
/// the name has to be one both sides can spell, and there is exactly one of it.
pub use zgui_dom::side::BoxKey;
