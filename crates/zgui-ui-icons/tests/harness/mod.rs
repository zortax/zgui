//! Mounting a real component into a real window, and reading what it built.

#![allow(dead_code, unreachable_pub)]

use zgui::prelude::*;
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
    pub fn only_child(&self) -> NodeId {
        let children = self.window.dom.tree().children(self.window.root);
        assert_eq!(children.len(), 1, "one component was mounted");
        children[0]
    }
}
