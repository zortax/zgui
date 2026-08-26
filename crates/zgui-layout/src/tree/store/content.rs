//! Sparse payloads carried only by boxes with replaced or custom content.

use zgui_dom::host::ReplacedId;
use zgui_dom::side::BoxKey;
use zgui_geom::{Css, CssPx, Size};

use crate::tree::store::LayoutStore;

/// Intrinsic data used only by a replaced box.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ReplacedBox {
    /// The content the paint stage resolves.
    pub(crate) id: ReplacedId,
    /// Width over height, when known.
    pub(crate) ratio: Option<f32>,
    /// Natural size in CSS pixels, when known.
    pub(crate) natural: Option<Size<CssPx, Css>>,
}

/// The registry reference used only by a custom box.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CustomBox {
    /// Registry token.
    pub(crate) token: u32,
    /// Layout revision captured when the box was built.
    pub(crate) layout_revision: u16,
    /// Paint revision captured when the box was built.
    pub(crate) paint_revision: u16,
}

impl From<(u32, u16, u16)> for CustomBox {
    fn from((token, layout_revision, paint_revision): (u32, u16, u16)) -> Self {
        Self {
            token,
            layout_revision,
            paint_revision,
        }
    }
}

impl CustomBox {
    /// The packed registry reference in the document seam's order.
    pub(crate) fn reference(self) -> (u32, u16, u16) {
        (self.token, self.layout_revision, self.paint_revision)
    }
}

/// Payloads established while one box is built.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BoxContent {
    /// Replaced-content metadata, if any.
    pub(crate) replaced: Option<ReplacedBox>,
    /// Custom-element metadata, if any.
    pub(crate) custom: Option<CustomBox>,
    /// Whether this element draws vector content in preference to replaced content.
    pub(crate) draws_vector: bool,
}

impl LayoutStore {
    /// Replaced metadata for `key`.
    pub(crate) fn replaced(&self, key: BoxKey) -> Option<ReplacedBox> {
        self.replaced.get(key).copied().flatten()
    }

    /// Custom metadata for `key`.
    pub(crate) fn custom_content(&self, key: BoxKey) -> Option<CustomBox> {
        self.custom.get(key).copied().flatten()
    }

    /// Whether any box of this document may carry custom content.
    ///
    /// From the sparse table's page count, so a document with none answers without a walk — which
    /// is every document, in an application that installs a custom source it never uses.
    /// Conservative by one compaction: a page whose last custom box left reads as occupied until
    /// the recycle drops it, and what that costs is a walk that finds nothing to pin.
    pub(crate) fn may_hold_custom_boxes(&self) -> bool {
        self.custom.pages() > 0
    }

    /// The custom registry reference captured by `key`, when it is custom content.
    ///
    /// Exposes the compact value rather than the sparse table's private storage type; paint needs
    /// the revisions for replay invalidation but has no reason to depend on layout's payload.
    pub fn custom_reference(&self, key: BoxKey) -> Option<(u32, u16, u16)> {
        self.custom_content(key).map(CustomBox::reference)
    }
}
