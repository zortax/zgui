//! The element vocabulary: what an interface is built out of, as typed builders.
//!
//! Sixteen names, each a function that starts a builder and a marker type that decides which
//! attributes that name takes. An element is one expression, nothing is created until the view is
//! built, and whether an attribute is written once or kept up to date is decided by what is passed
//! to it rather than by how it was written.
//!
//! ```
//! use zgui_elements::{column, control, text};
//! use zgui_reactive::prelude::*;
//! use zgui_reactive::{Mounted, RwSignal, install};
//! use zgui_view::stub::{StubDom, StubHost};
//! use zgui_view::{Anchor, BuildCxOwned, DocumentId, DomHandle, HostHandle, View};
//! use zgui_interned::ElementName;
//! use std::rc::Rc;
//!
//! install().unwrap();
//! let backend = Rc::new(StubDom::new(DocumentId::FIRST));
//! let dom = DomHandle::from_rc(backend.clone());
//! let window = Mounted::new();
//! let cx = BuildCxOwned::new(
//!     dom.clone(),
//!     HostHandle::new(StubHost::default()),
//!     window.owner().clone(),
//!     DocumentId::FIRST,
//! );
//! let root = dom.create_element(ElementName::new("box"));
//!
//! let count = window.with(|| RwSignal::new(0));
//! let mut built = window.with(|| {
//!     column()
//!         .class("counter")
//!         .child(text().child(move || count.get().to_string()))
//!         .child(
//!             control()
//!                 .on(zgui_view::events::CLICK, move |_| {
//!                     count.update(|n| *n += 1)
//!                 })
//!                 .child("+"),
//!         )
//!         .build(&mut cx.cx())
//! });
//! built.mount(&dom, root, None);
//! assert_eq!(backend.text_content(root), "0+");
//!
//! count.set(41);
//! zgui_reactive::flush();
//! assert_eq!(backend.text_content(root), "41+");
//! window.unmount();
//! ```
//!
//! # Why these names
//!
//! They are what an interface is made of, not what a document is made of. There is no element that
//! means "generic block": `box` ([`Box_`]) says that out loud. There is no element whose meaning depends
//! on a stylesheet: a [`row`] is a row. And every name here has an obvious equivalent in a document
//! language, so a backend over one can be written without changing a line of any view.
//!
//! A vocabulary of one's own — a browser's HTML, a diagram format — implements [`Tag`] and gets the
//! same builder, the same attributes and the same `view!` syntax. Neither vocabulary knows about
//! the other.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`tag`] | the sixteen names, their marker types, and [`Tag`] |
//! | [`element`] | [`Element`], its attributes, and what building one retains |
//! | [`focus`] | [`Focus`], which is what `tabindex` takes |
//! | [`expansion`] | the root the `view!` expansion names its crates through |
//! | [`vector`](mod@vector) | how a drawing's outlines and paint reach the backend |

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod element;
pub mod expansion;
pub mod focus;
pub mod tag;
pub mod vector;

use zgui_interned::ElementName;

/// The Bézier crate `<vector>`'s outlines are written in.
///
/// Re-exported because a path crosses from a view all the way to the rasteriser as the same type,
/// so an application that draws one names this crate and nothing else.
pub use kurbo;

pub use crate::element::{Element, ElementState};
pub use crate::focus::Focus;
pub use crate::tag::{
    Box_, Canvas, Column, Control, Editor, Field, Image, Label, OverlayRoot, Row, Scroll, Spacer,
    Stack, Surface, Tag, Text, Vector, r#box, canvas, column, control, editor, field, image, label,
    overlay_root, row, scroll, spacer, stack, surface, text, vector,
};

/// Every name in the vocabulary.
///
/// The list a consumer walks when it has to do something for each of them — install a style sheet
/// that gives each its layout, check that a backend answers for each, enumerate them in a tool.
pub fn names() -> [ElementName; 16] {
    [
        Box_::name(),
        Row::name(),
        Column::name(),
        Stack::name(),
        Text::name(),
        Label::name(),
        Image::name(),
        Vector::name(),
        Scroll::name(),
        Canvas::name(),
        Editor::name(),
        Field::name(),
        Control::name(),
        Surface::name(),
        Spacer::name(),
        OverlayRoot::name(),
    ]
}
