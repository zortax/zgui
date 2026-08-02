//! The hooks a downstream consumer implements to put a document language on top of this document.
//!
//! The document core carries no markup language, no scripting and no URL loading, and these traits
//! are why it can afford not to. Each names one decision the core cannot make on its own, each has
//! a do-nothing implementation installed by default, and each has a consumer *inside* this crate
//! or a named one above it — a hook nothing ever calls is a promise rather than a seam.
//!
//! | Hook | Consumer | What zgui's own implementation does |
//! |---|---|---|
//! | [`PresentationalHints`] | the style engine's legacy-attribute hook, on every restyled element | contributes nothing |
//! | [`LinkResolver`] | [`Document::refresh_link_state`](crate::Document::refresh_link_state), on every attribute write | reports that nothing is a link |
//! | [`ReplacedContent`] | [`Document::intrinsic_of`](crate::Document::intrinsic_of), for every node flagged replaced | reports no intrinsic size |
//! | [`SheetLoader`] | the stylesheet parser's `@import` arm | rejects every request |
//!
//! # Where the fifth and sixth hooks are
//!
//! Two more hooks belong to this family and cannot live here.
//!
//! A replaced node's *paint* content — an atlas sprite, an external texture — is a scene concept,
//! and a document knows nothing about scenes. [`ReplacedContent`] therefore answers the sizing
//! question only, and the painting question is a separate hook declared beside the scene. The two
//! halves also differ in what they may hold: an intrinsic size is plain data that a layout worker
//! reads, while a paint source holds device resources, so only the first can be required to be
//! shareable across threads.
//!
//! The hooks a *script* engine needs — an animation-frame callback, a microtask checkpoint, event
//! dispatch interception, geometry observation — are frame-loop concepts and are installed on the
//! runtime rather than on the document.
//!
//! # Why each of these may be shared across threads
//!
//! Everything installed here is reachable from a node handle, and a node handle is copied to every
//! style worker. That is why each trait requires [`Send`] + [`Sync`]: an implementation holding a
//! reference count or a borrow counter would be a data race the moment a worker consulted it, and
//! the store carries a compile-time assertion that stops one being installed.

pub mod hints;
pub mod links;
pub mod replaced;
pub mod sheets;

use std::sync::Arc;

pub use crate::host::hints::{NoPresentationalHints, PresentationalHints};
pub use crate::host::links::{LinkResolver, NoLinks};
pub use crate::host::replaced::{Intrinsic, NoReplacedContent, ReplacedContent, ReplacedId};
pub use crate::host::sheets::{NoSheetLoader, SheetLoader, SheetRequest};

/// The hooks installed on one document.
///
/// Held by the store rather than by the document, because every one of them is consulted from a
/// node handle and a node handle reaches the store and nothing else.
pub struct HostSeams {
    /// Where declarations derived from markup attributes come from.
    hints: Arc<dyn PresentationalHints>,
    /// Which elements are links.
    links: Arc<dyn LinkResolver>,
    /// What a replaced node's content is intrinsically.
    replaced: Arc<dyn ReplacedContent>,
    /// Where an imported or linked stylesheet's source comes from.
    sheets: Arc<dyn SheetLoader>,
}

impl HostSeams {
    /// The default hooks: nothing contributes hints, nothing is a link, nothing is replaced, and
    /// no stylesheet can be loaded.
    pub fn new() -> Self {
        Self {
            hints: Arc::new(NoPresentationalHints),
            links: Arc::new(NoLinks),
            replaced: Arc::new(NoReplacedContent),
            sheets: Arc::new(NoSheetLoader),
        }
    }

    /// The installed presentational-hint source.
    pub fn hints(&self) -> &dyn PresentationalHints {
        self.hints.as_ref()
    }

    /// The installed link resolver.
    pub fn links(&self) -> &dyn LinkResolver {
        self.links.as_ref()
    }

    /// The installed replaced-content source.
    pub fn replaced(&self) -> &dyn ReplacedContent {
        self.replaced.as_ref()
    }

    /// The installed stylesheet loader.
    pub fn sheets(&self) -> &dyn SheetLoader {
        self.sheets.as_ref()
    }

    /// Replaces the presentational-hint source.
    pub fn set_hints(&mut self, hints: Arc<dyn PresentationalHints>) {
        self.hints = hints;
    }

    /// Replaces the link resolver.
    pub fn set_links(&mut self, links: Arc<dyn LinkResolver>) {
        self.links = links;
    }

    /// Replaces the replaced-content source.
    pub fn set_replaced(&mut self, replaced: Arc<dyn ReplacedContent>) {
        self.replaced = replaced;
    }

    /// Replaces the stylesheet loader.
    pub fn set_sheets(&mut self, sheets: Arc<dyn SheetLoader>) {
        self.sheets = sheets;
    }
}

impl Default for HostSeams {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::HostSeams;
    use crate::host::sheets::SheetRequest;

    #[test]
    fn the_default_hooks_answer_the_way_a_document_with_no_language_on_top_should() {
        let seams = HostSeams::new();
        assert!(matches!(
            seams.sheets().load("zgui:///", "theme.css"),
            SheetRequest::Rejected
        ));
    }
}
