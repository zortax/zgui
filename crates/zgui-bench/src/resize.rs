//! What a scroll position does when the extent under it changes.
//!
//! A scroll offset is a number the content decides the range of: how far a container may be
//! scrolled is its content's height less its scrollport's, and a window that changes size changes
//! the second of those and often the first. So every configure is a moment at which a held offset
//! may have become one the content no longer allows.
//!
//! Two things are owed to a reader, and they are not the same thing:
//!
//! * a window that grows **shorter** than the offset allows must **clamp** — an offset past the end
//!   is content drawn off the top with blank below it;
//! * a window that grows **taller**, or changes only its width, must **not move the reader** — the
//!   line they were looking at stays where it is, and only the amount of document around it
//!   changes.
//!
//! This module reads both off a live window: every container that can be scrolled, what it is
//! scrolled to, what it is allowed to be scrolled to, and where a chosen anchor sits on the screen.

use zgui::geom::{Device, DevicePx, Rect};
use zgui::runtime::Window;
use zgui::view::ViewHost;

/// One scrollable container, as a frame left it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Container {
    /// How tall the visible part is, in device pixels.
    pub(crate) port: f32,
    /// How far the content reaches inside it.
    pub(crate) content: f32,
    /// The largest offset the content allows.
    pub(crate) limit: f32,
    /// What the container reports being scrolled to.
    pub(crate) offset: f32,
    /// The same with any elastic displacement added, which is what the fragments compose against.
    pub(crate) composed: f32,
}

impl Container {
    /// Whether the offset is one the content still allows.
    pub(crate) fn within(&self) -> bool {
        self.offset <= self.limit + 0.5
    }
}

impl std::fmt::Display for Container {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "port={:.1} content={:.1} limit={:.1} offset={:.1} composed={:.1}{}",
            self.port,
            self.content,
            self.limit,
            self.offset,
            self.composed,
            if self.within() { "" } else { "  PAST THE END" }
        )
    }
}

/// Every container in the document whose content is taller than its scrollport.
///
/// Containers that cannot be scrolled at all are left out: their offset is zero, it has always been
/// zero, and a list that named them would bury the one the reader is actually in.
pub(crate) fn containers(window: &Window) -> Vec<Container> {
    let layout = window.layout().borrow();
    let scroll = window.scroll().borrow();
    let mut found = Vec::new();
    for key in layout.keys() {
        let Some(region) = zgui_layout::scroll_region::region_of(&layout, key) else {
            continue;
        };
        let Some(element) = layout.get(key).and_then(|node| node.source) else {
            continue;
        };
        if region.limit().y.0 <= 0.0 {
            continue;
        }
        found.push(Container {
            port: region.scrollport.size.height.0,
            content: region.content.height.0,
            limit: region.limit().y.0,
            offset: scroll.offset_of(element).y.0,
            composed: scroll.composed().of(element).y.0,
        });
    }
    found
}

/// Where the topmost fragment of the deepest box carrying `name` sits on the screen.
///
/// The anchor of "did the reader move": a line of the document, in the window's own coordinates,
/// which a resize that only added room beneath it must leave exactly where it was.
pub(crate) fn anchor(window: &Window, name: &str) -> Option<Rect<DevicePx, Device>> {
    let attribute = zgui::view::AttrName::new("data-testid");
    let dom = window.dom();
    let mut stack = vec![dom.root_node()];
    while let Some(node) = stack.pop() {
        if dom.attribute(node, attribute).as_deref() == Some(name) {
            return window.host().border_box(node);
        }
        stack.extend(dom.children(node));
    }
    None
}
