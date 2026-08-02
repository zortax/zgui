//! The documents the cases are written against.
//!
//! Two of them, both built once and both named: every element carries a short name the cases quote,
//! so an expectation reads as a list of elements rather than as a list of slot numbers.
//!
//! The shapes are chosen for what they let a selector get wrong. Text nodes sit between elements, so
//! a sibling combinator that walked the plain chain would answer differently from one that walked
//! the element chain. A marker sits among the card's children, so a positional selector that counted
//! nodes rather than elements would be off by one. One element is genuinely empty and one contains
//! only text, so `:empty` has both answers to give. Links carry an attribute rather than a state
//! bit, so the installed link resolver is what turns them into `:link` and `:visited`.

use std::sync::Arc;

use zgui_dom::{Document, LinkResolver, Node, NodeIndex, NodeKind};
use zgui_interned::{AttrName, ClassName, ElementName, Ident};
use zgui_vocab::{SharedString, UiState};

/// A built document whose elements have names.
pub(crate) struct Tree {
    /// The document.
    pub(crate) document: Document,
    /// Every element's name, in document order.
    names: Vec<(&'static str, NodeIndex)>,
}

impl Tree {
    /// The element called `name`.
    ///
    /// # Panics
    ///
    /// Panics if no element has that name.
    pub(crate) fn at(&self, name: &str) -> NodeIndex {
        self.names
            .iter()
            .find(|(held, _)| *held == name)
            .map(|(_, index)| *index)
            .unwrap_or_else(|| panic!("no element in this fixture is called `{name}`"))
    }

    /// Every named element, in document order.
    pub(crate) fn all(&self) -> impl Iterator<Item = (&'static str, NodeIndex)> + '_ {
        self.names.iter().copied()
    }

    /// The names of every element that `predicate` accepts, in document order.
    pub(crate) fn names_where(
        &self,
        predicate: impl Fn(&Document, NodeIndex) -> bool,
    ) -> Vec<&'static str> {
        self.names
            .iter()
            .filter(|(_, index)| predicate(&self.document, *index))
            .map(|(name, _)| *name)
            .collect()
    }
}

/// Builds a tree while recording each element's name.
struct Builder {
    /// The document under construction.
    document: Document,
    /// The names recorded so far.
    names: Vec<(&'static str, NodeIndex)>,
}

impl Builder {
    /// An empty document.
    fn new() -> Self {
        Self {
            document: Document::new(),
            names: Vec::new(),
        }
    }

    /// Appends an element called `name` with tag `tag` and classes `classes` to `parent`.
    fn element(
        &mut self,
        parent: NodeIndex,
        name: &'static str,
        tag: &str,
        classes: &[&str],
    ) -> NodeIndex {
        let index = self
            .document
            .append(parent, NodeKind::Element, ElementName::new(tag));
        if !classes.is_empty() {
            let names: Vec<ClassName> = classes.iter().map(|c| ClassName::new(c)).collect();
            self.document.set_classes(index, &names);
        }
        self.names.push((name, index));
        index
    }

    /// Appends a text node holding `text`.
    fn text(&mut self, parent: NodeIndex, text: &str) {
        let index = self
            .document
            .append(parent, NodeKind::Text, ElementName::new("#text"));
        zgui_dom::text::node::set_text(self.document.store_mut(), index, text);
    }

    /// Appends a marker, which is not an element and holds a place in the child list.
    fn marker(&mut self, parent: NodeIndex) {
        self.document
            .append(parent, NodeKind::Marker, ElementName::new("#marker"));
    }

    /// Gives `index` an identifier.
    fn id(&mut self, index: NodeIndex, id: &str) {
        self.document.set_id(index, Some(Ident::new(id)));
    }

    /// Gives `index` an attribute.
    fn attr(&mut self, index: NodeIndex, name: &str, value: &str) {
        self.document
            .set_attribute(index, AttrName::new(name), Some(SharedString::from(value)));
    }

    /// Gives `index` an interaction state.
    fn state(&mut self, index: NodeIndex, state: UiState) {
        self.document.set_state(index, state);
    }

    /// The finished tree.
    fn finish(self) -> Tree {
        Tree {
            document: self.document,
            names: self.names,
        }
    }
}

/// The resolver the page fixture installs: anything with an `href` is a link, and one of them has
/// been visited.
struct HrefLinks;

impl LinkResolver for HrefLinks {
    fn is_link(&self, element: Node<'_>) -> bool {
        element.attr("href").is_some()
    }

    fn is_visited(&self, element: Node<'_>) -> bool {
        element
            .attr("href")
            .is_some_and(|href| href.as_str() == "/b")
    }
}

/// The document most cases are written against.
///
/// ```text
/// root
///   header.bar
///     label.title#heading
///     "Title"
///     span.badge
///   nav.bar.sticky
///     a.link[href=/a]
///     "  "
///     a.link.active[href=/b][data-state=open]
///   box.card#main
///     box.item
///     "  "
///     box.item.hot        :hover
///     box.item
///     <marker>
///     box.item.last
///     label[data-kind=leaf]
///   box.card.empty        (no children at all)
///   box.card
///     box.item
///       box.deep
///   form.controls
///     button.ctl#save     :enabled
///     button.ctl          :disabled
///     input.ctl           :checked
/// ```
pub(crate) fn page() -> Tree {
    let mut build = Builder::new();
    let document_index = build.document.document_index();
    let root = build.element(document_index, "root", "root", &[]);

    let header = build.element(root, "header", "header", &["bar"]);
    let title = build.element(header, "title", "label", &["title"]);
    build.id(title, "heading");
    build.text(header, "Title");
    build.element(header, "badge", "span", &["badge"]);

    let nav = build.element(root, "nav", "nav", &["bar", "sticky"]);
    let link_a = build.element(nav, "linkA", "a", &["link"]);
    build.attr(link_a, "href", "/a");
    build.text(nav, "  ");
    let link_b = build.element(nav, "linkB", "a", &["link", "active"]);
    build.attr(link_b, "href", "/b");
    build.attr(link_b, "data-state", "open");

    let card = build.element(root, "card", "box", &["card"]);
    build.id(card, "main");
    build.element(card, "i1", "box", &["item"]);
    build.text(card, "  ");
    let hot = build.element(card, "i2", "box", &["item", "hot"]);
    build.state(hot, UiState::HOVER);
    build.element(card, "i3", "box", &["item"]);
    build.marker(card);
    build.element(card, "i4", "box", &["item", "last"]);
    let leaf = build.element(card, "leaf", "label", &[]);
    build.attr(leaf, "data-kind", "leaf");

    build.element(root, "empty", "box", &["card", "empty"]);

    let card2 = build.element(root, "card2", "box", &["card"]);
    let i5 = build.element(card2, "i5", "box", &["item"]);
    build.element(i5, "deep", "box", &["deep"]);

    let form = build.element(root, "form", "form", &["controls"]);
    let save = build.element(form, "save", "button", &["ctl"]);
    build.id(save, "save");
    build.state(save, UiState::ENABLED);
    let cancel = build.element(form, "cancel", "button", &["ctl"]);
    build.state(cancel, UiState::DISABLED);
    let check = build.element(form, "check", "input", &["ctl"]);
    build.state(check, UiState::CHECKED);

    let mut tree = build.finish();
    // Installing the resolver after the elements exist is deliberate: it has to reach back over
    // what is already there, which is exactly what a consumer attaching a document language to a
    // built document does.
    tree.document.install_link_resolver(Arc::new(HrefLinks));
    tree
}

/// A list of ten rows, with a text node between the third and the fourth.
///
/// The text node is the point: it shifts nothing. A positional selector that counted nodes rather
/// than elements would put the fourth row in fifth place.
pub(crate) fn list() -> Tree {
    let mut build = Builder::new();
    let document_index = build.document.document_index();
    let root = build.element(document_index, "root", "root", &[]);
    let list = build.element(root, "list", "ul", &["list"]);

    const NAMES: [&str; 10] = ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9"];
    for (position, name) in NAMES.iter().enumerate() {
        if position == 3 {
            build.text(list, " ");
        }
        let classes: &[&str] = if position % 2 == 0 {
            &["row", "even"]
        } else {
            &["row"]
        };
        let tag = if position % 3 == 0 { "li" } else { "div" };
        build.element(list, name, tag, classes);
    }
    build.finish()
}
