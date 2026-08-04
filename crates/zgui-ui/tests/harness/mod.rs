//! Mounting a real component into a real window, and reading what it built.

#![allow(dead_code, unreachable_pub)]

use zgui::prelude::*;
use zgui::vocab::{Semantics, UiState};
use zgui_testkit_view::Window;

/// A window with something mounted in it.
pub struct Harness {
    /// The window.
    pub window: Window,
}

impl Harness {
    /// Opens a window with nothing in it yet.
    pub fn open() -> Self {
        let window = Window::open();
        window.place(window.root, 0.0, 0.0, 1000.0, 1000.0);
        Self { window }
    }

    /// Builds `view` and mounts it under the window's root, then runs a frame.
    pub fn mount<V: IntoView + 'static>(&self, view: impl FnOnce() -> V) {
        let mut built = self
            .window
            .scope
            .with(|| view().into_view().build(&mut self.window.cx.cx()));
        built.mount(&self.window.dom_handle, self.window.root, None);
        // Leaked deliberately: these are short-lived test windows, and holding the handle behind a
        // `&self` method would need interior mutability for nothing.
        core::mem::forget(built);
        self.window.frame();
    }

    /// The only child of the window's root, which is what a single mounted component is.
    ///
    /// Markers are not children for this purpose. A component's content is bracketed by a pair of
    /// them in an instrumented build, and every conditional inside one leaves another behind — so a
    /// count that included them would be counting where things may go rather than what is there.
    pub fn only_child(&self) -> NodeId {
        let children = self.children(self.window.root);
        assert_eq!(children.len(), 1, "one component was mounted");
        children[0]
    }

    /// A node's children, without the markers among them.
    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.window
            .dom
            .tree()
            .children(node)
            .into_iter()
            .filter(|child| !self.window.dom.tree().is_marker(*child))
            .collect()
    }

    /// Every element under the window's root, in tree order, the root itself first.
    pub fn all(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![self.window.root];
        while let Some(node) = stack.pop() {
            out.push(node);
            let mut children = self.children(node);
            children.reverse();
            stack.extend(children);
        }
        out
    }

    /// The first element under the root carrying `class`.
    ///
    /// # Panics
    ///
    /// Panics naming the class when nothing carries it, because every caller is about to assert
    /// something about the element and `None` would make that assertion pass by never running.
    pub fn find(&self, class: &str) -> NodeId {
        let name = zgui::view::ClassName::new(class);
        self.all()
            .into_iter()
            .find(|node| self.window.dom.tree().classes(*node).contains(&name))
            .unwrap_or_else(|| panic!("nothing under the root carries `{class}`"))
    }

    /// What an element means to an accessibility tree.
    ///
    /// # Panics
    ///
    /// Panics when the element says nothing, which for a control is the defect the caller is
    /// looking for rather than a reason to skip the assertion.
    pub fn semantics(&self, node: NodeId) -> Semantics {
        self.window
            .dom
            .tree()
            .semantics(node)
            .expect("the element says what it is")
    }

    /// The interaction states an element is asserting.
    pub fn state(&self, node: NodeId) -> UiState {
        self.window.dom.tree().ui_state(node)
    }

    /// One attribute's value.
    pub fn attribute(&self, node: NodeId, name: &str) -> Option<String> {
        self.window
            .dom
            .tree()
            .attribute(node, zgui::view::AttrName::new(name))
    }

    /// Presses a key on an element, exactly as a window does — including what the framework
    /// itself does about that key when no handler refused it.
    pub fn press(&self, node: NodeId, key: zgui::vocab::NamedKey) {
        self.window
            .dispatcher()
            .key(node, zgui::vocab::Key::Named(key));
        self.window.frame();
    }

    /// Types one character into an element.
    pub fn type_char(&self, node: NodeId, character: char) {
        self.window.dispatcher().key(
            node,
            zgui::vocab::Key::Character(zgui::vocab::SharedString::from(character.to_string())),
        );
        self.window.frame();
    }

    /// Clicks an element, without a hit test.
    pub fn click(&self, node: NodeId) {
        self.window.dispatcher().send_to(
            node,
            zgui::vocab::EventKind::Click,
            zgui::vocab::Payload::Pointer(zgui::vocab::PointerEvent::mouse(
                zgui::geom::Point::new(zgui::geom::CssPx(0.0), zgui::geom::CssPx(0.0)),
            )),
        );
        self.window.frame();
    }
}
