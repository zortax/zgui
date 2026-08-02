//! Content that renders somewhere else in the window.

use crate::cx::BuildCx;
use crate::dom::{DomHandle, OverlayLayer};
use crate::id::NodeId;
use crate::view::{Anchor, AnyView, AnyViewState, View};

/// What a portal retains: a place it is written, and content that is somewhere else.
pub struct PortalState {
    /// Where the portal was written, so its siblings keep their order.
    marker: NodeId,
    /// Which band the content is on.
    layer: OverlayLayer,
    /// The content.
    content: AnyViewState,
    /// The overlay root the content is under, once mounted.
    overlay: Option<NodeId>,
}

impl Anchor for PortalState {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        dom.insert(parent, self.marker, before);
        let overlay = dom.overlay_root(self.marker, self.layer);
        self.overlay = Some(overlay);
        self.content.mount(dom, overlay, None);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.content.unmount(dom);
        dom.detach(self.marker);
        self.overlay = None;
    }

    fn first_node(&self) -> Option<NodeId> {
        // The marker, not the content: as far as this portal's siblings are concerned, the content
        // is not here.
        Some(self.marker)
    }
}

/// Renders its children on an overlay band rather than where it is written.
///
/// A dialog, a menu, a tooltip and a toast all have to escape whatever clipped, transformed or
/// stacked ancestor they were written inside. Naming the band rather than relying on mount order
/// is what stops a toast raised before a dialog from painting beneath it.
///
/// The portal still occupies its written position with a marker, so its siblings keep their order
/// and the portal can come and go without disturbing them.
///
/// ```
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{
///     Anchor, AnyView, BuildCxOwned, DocumentId, Dom, DomHandle, HostHandle, OverlayLayer,
///     Portal, View,
/// };
/// use zgui_interned::ElementName;
/// use std::rc::Rc;
///
/// install().unwrap();
/// let backend = Rc::new(StubDom::new(DocumentId::FIRST));
/// let dom = DomHandle::from_rc(backend.clone());
/// let window = Mounted::new();
/// let cx = BuildCxOwned::new(
///     dom.clone(), HostHandle::new(StubHost::default()),
///     window.owner().clone(), DocumentId::FIRST,
/// );
/// let root = dom.create_element(ElementName::new("box"));
///
/// let mut state = Portal::new(|| AnyView::new("in the overlay"))
///     .layer(OverlayLayer::Modal)
///     .build(&mut cx.cx());
/// state.mount(&dom, root, None);
///
/// assert_eq!(backend.text_content(root), "", "nothing landed where it was written");
/// let overlay = dom.overlay_root(root, OverlayLayer::Modal);
/// assert_eq!(backend.text_content(overlay), "in the overlay");
/// window.unmount();
/// ```
pub struct Portal<C> {
    /// The content.
    children: C,
    /// Which band it goes on.
    layer: OverlayLayer,
}

impl<C: FnOnce() -> AnyView + 'static> Portal<C> {
    /// Renders `children` on the popover band.
    pub fn new(children: C) -> Self {
        Self {
            children,
            layer: OverlayLayer::default(),
        }
    }

    /// Renders on `layer` instead.
    #[must_use]
    pub fn layer(mut self, layer: OverlayLayer) -> Self {
        self.layer = layer;
        self
    }
}

impl<C: FnOnce() -> AnyView + 'static> View for Portal<C> {
    type State = PortalState;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let marker = cx.dom().create_marker();
        let content = (self.children)().build(cx);
        PortalState {
            marker,
            layer: self.layer,
            content,
            overlay: None,
        }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        (self.children)().rebuild(&mut state.content, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::Portal;
    use crate::dom::OverlayLayer;
    use crate::fixture::Fixture;
    use crate::view::{Anchor, AnyView, View};

    #[test]
    fn the_content_goes_to_the_named_band_and_the_marker_stays_where_it_was_written() {
        let f = Fixture::new();
        let before = f.dom.create_text("[");
        let after = f.dom.create_text("]");
        f.dom.insert(f.root, before, None);
        f.dom.insert(f.root, after, None);

        let mut state = f.window.with(|| {
            Portal::new(|| AnyView::new("floating"))
                .layer(OverlayLayer::Toast)
                .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, Some(after));

        assert_eq!(f.text(), "[]");
        let overlay = f.dom.overlay_root(f.root, OverlayLayer::Toast);
        assert_eq!(f.backend.text_content(overlay), "floating");

        state.unmount(&f.dom);
        assert_eq!(f.backend.text_content(overlay), "");
        f.window.unmount();
    }

    #[test]
    fn two_bands_are_two_overlay_roots() {
        let f = Fixture::new();
        let popover = f.dom.overlay_root(f.root, OverlayLayer::Popover);
        let modal = f.dom.overlay_root(f.root, OverlayLayer::Modal);
        assert_ne!(popover, modal);
        f.window.unmount();
    }
}
