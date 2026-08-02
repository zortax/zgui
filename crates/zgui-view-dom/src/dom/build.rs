//! The tree shape every window has before any view is built.

use zgui_dom::{Document, EverythingMatters};
use zgui_interned::{AttrName, ElementName};
use zgui_view::{NodeId, OverlayLayer};

use crate::id;

/// The nodes a window has before any view is built.
///
/// One root element, one overlay root under it, and one node per overlay layer under that. The
/// framework's own style sheet is written against exactly this shape: it positions the layer nodes
/// and gives each a stacking order, so content portalled into one is always a *grandchild* of the
/// overlay root and never a child.
pub struct Roots {
    /// The window's root element.
    root: NodeId,
    /// Where portalled content goes.
    overlay_root: NodeId,
    /// One node per layer, in ascending order.
    layers: [NodeId; 4],
}

impl Roots {
    /// Creates the shape in `document`.
    ///
    /// Through the document's own batch API, like every other change this crate makes. A window's
    /// first six nodes are not a special case that may skip the protocol: they are elements a
    /// selector matches and a traversal has to reach, and the batch is what says so.
    pub(crate) fn create(document: &Document) -> Self {
        let (root, overlay_root, layers) = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let overlay_root = edit.create_element(ElementName::new("overlay_root"));
                edit.insert_before(root, overlay_root, None);

                let mut layers = Vec::with_capacity(OverlayLayer::ALL.len());
                for layer in OverlayLayer::ALL.iter().copied() {
                    let node = edit.create_element(ElementName::new("overlay_layer"));
                    edit.insert_before(overlay_root, node, None);
                    edit.set_attribute(node, AttrName::new("data-layer"), Some(name(layer).into()));
                    layers.push(node);
                }
                (root, overlay_root, layers)
            })
            .expect("a fresh document is not poisoned");

        let key = |index| id::to_view(document.store().key_of(index));
        Self {
            root: key(root),
            overlay_root: key(overlay_root),
            layers: core::array::from_fn(|position| key(layers[position])),
        }
    }

    /// The window's root element.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// The element portalled content is put under, one layer down.
    pub fn overlay_root(&self) -> NodeId {
        self.overlay_root
    }

    /// The node content on `layer` is portalled into.
    pub fn layer(&self, layer: OverlayLayer) -> NodeId {
        let position = OverlayLayer::ALL
            .iter()
            .position(|held| *held == layer)
            .expect("every layer has a node");
        self.layers[position]
    }
}

/// What a layer is written as in the attribute the framework's own style sheet orders them by.
fn name(layer: OverlayLayer) -> &'static str {
    match layer {
        OverlayLayer::Content => "content",
        OverlayLayer::Popover => "popover",
        OverlayLayer::Modal => "modal",
        OverlayLayer::Toast => "toast",
        // A layer this build has never heard of cannot be ordered against the four it has, so it
        // is carried by name and ordered last by the sheet's fallback.
        _ => "content",
    }
}
