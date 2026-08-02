//! Nodes whose content comes from outside the document.

use zgui_geom::{Css, CssPx, Size};

use crate::id::node_key::NodeKey;
use crate::node::flags::NodeFlags;
use crate::node::handle::Node;

/// Names one node whose content comes from outside the document.
///
/// A replaced node is one whose size and appearance are decided by something the document does not
/// own — an image, a video frame, a canvas, an externally rendered surface. The identifier is the
/// node's own generation-checked name, so a stale one resolves to nothing rather than to whatever
/// took the slot over.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ReplacedId(NodeKey);

impl ReplacedId {
    /// The identifier for `node`.
    pub const fn new(node: NodeKey) -> Self {
        Self(node)
    }

    /// The node this identifies.
    pub const fn node(self) -> NodeKey {
        self.0
    }
}

/// What a replaced node's content is intrinsically, in CSS pixels.
///
/// Every field is optional and they are independent: a raster image has a size and therefore a
/// ratio, an SVG with a `viewBox` and no width has a ratio and no size, and a live surface that
/// has produced no frame yet has neither. Layout resolves `auto` sizing against whichever of the
/// three are present, so reporting a guess where the answer is unknown is worse than reporting
/// nothing.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct Intrinsic {
    /// The content's natural size, if it has one.
    pub size: Option<Size<CssPx, Css>>,
    /// The content's natural width-to-height ratio, if it has one.
    pub ratio: Option<f32>,
    /// Where the content's baseline sits, measured down from its top edge.
    pub baseline: Option<f32>,
}

/// Supplies the intrinsic sizing of replaced nodes.
///
/// # What this deliberately does not do
///
/// It does not paint. A replaced node's pixels are an atlas sprite or an external texture, which
/// are concepts belonging to the scene rather than to the document, and a document that could name
/// them would have an edge to the renderer it is designed not to have. The painting half is a
/// separate hook installed beside the scene, keyed by the same [`ReplacedId`].
///
/// The split is not only tidiness: this half is consulted from layout workers and so must be
/// shareable across threads, while a paint source holds device resources that usually cannot be.
pub trait ReplacedContent: Send + Sync + 'static {
    /// The natural size, ratio and baseline of `id`'s content.
    ///
    /// Called for a node whose record carries the replaced flag, whenever layout needs a size for
    /// it. An identifier naming a node that is gone must be answered with
    /// [`Intrinsic::default`] rather than by panicking: a frame in flight can outlive the node it
    /// was measuring.
    fn intrinsic(&self, id: ReplacedId) -> Intrinsic;
}

/// The source zgui installs by default, for which no content has an intrinsic size.
pub struct NoReplacedContent;

impl ReplacedContent for NoReplacedContent {
    fn intrinsic(&self, _id: ReplacedId) -> Intrinsic {
        Intrinsic::default()
    }
}

impl Node<'_> {
    /// This node's replaced-content identifier, if it is a replaced node.
    ///
    /// A node is replaced because its record says so, and nothing else: the flag is what a
    /// consumer sets when it attaches outside content to a node, and it is what makes the
    /// difference between "this node's size comes from its children" and "this node's size comes
    /// from something the document cannot see".
    pub fn replaced_id(self) -> Option<ReplacedId> {
        self.record()
            .has_flags(NodeFlags::IS_REPLACED)
            .then(|| ReplacedId::new(self.key()))
    }
}
