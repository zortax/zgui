//! A place in the tree that holds content which comes and goes.

use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::Anchor;

/// A marker node plus whatever content currently sits before it.
///
/// Every view whose content can be replaced — a conditional, a reactive hole, a type-erased view,
/// a list — is built on one of these. The marker is what makes replacement possible at all: a view
/// that has just become empty still has to remember *where* it was, or the content that replaces
/// it later has nowhere to go.
///
/// The marker takes part in sibling order and in nothing else. It is emphatically not an
/// invisible element: an element would shift what every positional selector in the document
/// matches, and it would do so once per conditional in the program.
pub struct Hole<S> {
    /// The position marker.
    marker: NodeId,
    /// The parent this hole is mounted under, when it is mounted.
    parent: Option<NodeId>,
    /// The content, when there is any.
    content: Option<S>,
}

impl<S: Anchor> Hole<S> {
    /// Creates an empty hole, with its marker made but not yet placed.
    pub fn new(dom: &DomHandle) -> Self {
        Self {
            marker: dom.create_marker(),
            parent: None,
            content: None,
        }
    }

    /// The position marker.
    pub fn marker(&self) -> NodeId {
        self.marker
    }

    /// The parent this hole is mounted under.
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// The content, when there is any.
    pub fn content(&self) -> Option<&S> {
        self.content.as_ref()
    }

    /// The content, when there is any.
    pub fn content_mut(&mut self) -> Option<&mut S> {
        self.content.as_mut()
    }

    /// Replaces the content, unmounting whatever was there and mounting whatever is new.
    ///
    /// Safe to call before the hole itself is mounted: the new content is simply held until it is.
    pub fn set(&mut self, dom: &DomHandle, content: Option<S>) {
        if let Some(mut old) = self.content.take() {
            old.unmount(dom);
        }
        self.content = content;
        if let (Some(parent), Some(content)) = (self.parent, self.content.as_mut()) {
            content.mount(dom, parent, Some(self.marker));
        }
    }

    /// Puts `content` in place without unmounting what was there, which the caller has taken.
    pub fn fill(&mut self, dom: &DomHandle, content: S) {
        self.content = Some(content);
        if let (Some(parent), Some(content)) = (self.parent, self.content.as_mut()) {
            content.mount(dom, parent, Some(self.marker));
        }
    }
}

impl<S: Anchor> Anchor for Hole<S> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        dom.insert(parent, self.marker, before);
        self.parent = Some(parent);
        if let Some(content) = self.content.as_mut() {
            content.mount(dom, parent, Some(self.marker));
        }
    }

    fn unmount(&mut self, dom: &DomHandle) {
        if let Some(content) = self.content.as_mut() {
            content.unmount(dom);
        }
        dom.detach(self.marker);
        self.parent = None;
    }

    fn first_node(&self) -> Option<NodeId> {
        self.content
            .as_ref()
            .and_then(Anchor::first_node)
            .or(Some(self.marker))
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use zgui_interned::ElementName;

    use super::Hole;
    use crate::dom::DomHandle;
    use crate::stub::StubDom;
    use crate::view::anchor::Anchor;
    use crate::view::text::TextState;
    use crate::{DocumentId, NodeId};

    fn tree() -> (Rc<StubDom>, DomHandle, NodeId) {
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let root = dom.create_element(ElementName::new("box"));
        (backend, dom, root)
    }

    #[test]
    fn content_lands_before_the_marker_so_siblings_keep_their_places() {
        let (backend, dom, root) = tree();
        let after = dom.create_text("after");

        let mut hole: Hole<TextState> = Hole::new(&dom);
        hole.mount(&dom, root, None);
        dom.insert(root, after, None);

        hole.set(&dom, Some(TextState::new(&dom, "inside")));
        assert_eq!(backend.text_content(root), "insideafter");
    }

    #[test]
    fn an_emptied_hole_still_knows_where_it_was() {
        let (backend, dom, root) = tree();
        let mut hole: Hole<TextState> = Hole::new(&dom);
        hole.mount(&dom, root, None);
        dom.insert(root, dom.create_text("after"), None);

        hole.set(&dom, Some(TextState::new(&dom, "first")));
        hole.set(&dom, None);
        assert_eq!(backend.text_content(root), "after");

        hole.set(&dom, Some(TextState::new(&dom, "second")));
        assert_eq!(backend.text_content(root), "secondafter");
    }

    #[test]
    fn content_set_before_the_hole_is_mounted_is_mounted_with_it() {
        let (backend, dom, root) = tree();
        let mut hole: Hole<TextState> = Hole::new(&dom);
        hole.set(&dom, Some(TextState::new(&dom, "early")));
        assert_eq!(backend.text_content(root), "");

        hole.mount(&dom, root, None);
        assert_eq!(backend.text_content(root), "early");
    }

    #[test]
    fn the_first_node_is_the_marker_only_when_there_is_no_content() {
        let (_backend, dom, root) = tree();
        let mut hole: Hole<TextState> = Hole::new(&dom);
        hole.mount(&dom, root, None);
        assert_eq!(hole.first_node(), Some(hole.marker()));

        let text = TextState::new(&dom, "x");
        let node = text.node();
        hole.set(&dom, Some(text));
        assert_eq!(hole.first_node(), Some(node));
    }
}
