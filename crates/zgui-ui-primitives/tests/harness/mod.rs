//! Mounting a real component into a real window, and driving it.
//!
//! Every test in this directory builds the component under test through the ordinary view path,
//! mounts it under a window's root, and then asks the tree and the host what happened. Nothing is
//! hand-assembled: a behaviour that stopped being wired to anything fails here.

#![allow(dead_code, unreachable_pub)]

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view::ObservedValue;
use zgui_testkit_view::Window;

/// A window with something mounted in it.
pub struct Harness {
    /// The window.
    pub window: Window,
    /// The mounted view, held because dropping it takes the tree down.
    view: Option<Box<dyn Anchor>>,
}

impl Harness {
    /// Opens a window with nothing in it yet.
    ///
    /// The root is given a box, because in a real window it has one: a press that landed on
    /// nothing at all would never reach a listener on the root, and every outside-press case would
    /// pass by never being delivered.
    pub fn open() -> Self {
        let window = Window::open();
        window.place(window.root, 0.0, 0.0, 1000.0, 1000.0);
        Self { window, view: None }
    }

    /// Builds `view` and mounts it under the window's root, then runs a frame.
    pub fn mount<V: IntoView + 'static>(&self, view: impl FnOnce() -> V) {
        let mut built = self
            .window
            .scope
            .with(|| view().into_view().build(&mut self.window.cx.cx()));
        built.mount(&self.window.dom_handle, self.window.root, None);
        // The handle is leaked deliberately: these are short-lived test windows, and holding it
        // behind a `&self` method would need interior mutability for nothing.
        core::mem::forget(built);
        self.window.frame();
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.view.take();
    }
}

/// A rectangle in window pixels.
pub fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A measured content size, as the observation channel delivers one.
pub fn content_size(width: f32, height: f32) -> ObservedValue {
    ObservedValue::ContentSize(Size::new(DevicePx(width), DevicePx(height)))
}
