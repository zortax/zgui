//! Which cursor the window shows, and where the answer comes from.
//!
//! # Why the innermost element is the whole answer
//!
//! `cursor` is inherited, so an element that says nothing about it has already computed to whatever
//! its nearest ancestor said. The element under the pointer therefore carries the answer outright,
//! and there is no chain to walk: a button inside a card inside a dialog computes the button's own
//! `cursor: pointer`, and a plain label inside the same button computes `pointer` too.
//!
//! `auto` is the one keyword that is not a cursor. It means *decide from what this is*, which for a
//! desktop application is: the text bar over something a caret can be put in, and the arrow over
//! everything else. That decision is made here rather than in the style engine because the engine
//! has no idea what is editable.
//!
//! # Why the answer is remembered
//!
//! Setting a cursor is a call into the windowing system, and the pointer sits still over one
//! element for most of a session. So what was last asked for is held, and a frame that would ask
//! for the same thing again asks for nothing.

use zgui_css::values::ui::{CursorKind, CursorValue};
use zgui_dom::NodeKey;
use zgui_platform::CursorStyle;

use crate::window::Window;

impl Window {
    /// Shows whatever the element under the pointer asks for, if that has changed.
    ///
    /// Returns whether the window was told to change it, which is what a test asserts on and what
    /// keeps the call off the frames where nothing moved.
    pub(crate) fn update_cursor(&mut self) -> bool {
        let wanted = self.wanted_cursor();
        if self.cursor == wanted {
            return false;
        }
        self.cursor = wanted;
        self.handle.set_cursor(wanted);
        true
    }

    /// The cursor the element under the pointer asks for.
    ///
    /// The ordinary arrow when the pointer is over nothing at all, which is what a window with no
    /// pointer in it shows.
    fn wanted_cursor(&self) -> CursorStyle {
        let Some(node) = self.router.interaction().hover.target() else {
            return CursorStyle::Default;
        };
        let document = self.document.borrow();
        let Some(index) = document.store().index_of(node) else {
            return CursorStyle::Default;
        };
        let Some(style) = document.node(index).primary_style() else {
            return CursorStyle::Default;
        };
        let cursor: &CursorValue = &style.get_inherited_ui().cursor;
        match style_of(cursor.keyword) {
            Some(known) => known,
            None => self.automatic_cursor(node),
        }
    }

    /// What `cursor: auto` resolves to over one element.
    ///
    /// The text bar over anything a caret can be placed in, and the arrow over everything else.
    /// That is what `auto` means on a desktop, and it is the reason the keyword cannot be resolved
    /// where the property is lowered: whether an element is editable is a fact about the document
    /// and its host, which a style says nothing about.
    fn automatic_cursor(&self, node: NodeKey) -> CursorStyle {
        if self.editable_at(Some(node)).is_some() {
            CursorStyle::Text
        } else {
            CursorStyle::Default
        }
    }
}

/// The platform's spelling of one `cursor` keyword, or nothing for `auto`.
///
/// Every keyword the engine parses is answered. Several map onto one platform cursor because the
/// platforms themselves have one — `cell` and `crosshair` are the same crosshair everywhere, and
/// the eight directional resize arrows collapse onto the four a desktop draws. Answering the
/// nearest cursor is what every browser does; answering the arrow instead would make half the
/// vocabulary silently do nothing.
fn style_of(keyword: CursorKind) -> Option<CursorStyle> {
    Some(match keyword {
        CursorKind::Auto => return None,
        CursorKind::None => CursorStyle::None,
        CursorKind::Default => CursorStyle::Default,
        CursorKind::Pointer => CursorStyle::Pointer,
        CursorKind::Text => CursorStyle::Text,
        CursorKind::VerticalText => CursorStyle::VerticalText,
        CursorKind::Crosshair | CursorKind::Cell => CursorStyle::Crosshair,
        CursorKind::Grab => CursorStyle::Grab,
        CursorKind::Grabbing => CursorStyle::Grabbing,
        CursorKind::Wait => CursorStyle::Wait,
        CursorKind::Progress => CursorStyle::Progress,
        CursorKind::NotAllowed | CursorKind::NoDrop => CursorStyle::NotAllowed,
        CursorKind::Move | CursorKind::AllScroll => CursorStyle::Move,
        CursorKind::ColResize => CursorStyle::ResizeColumn,
        CursorKind::RowResize => CursorStyle::ResizeRow,
        CursorKind::EResize | CursorKind::WResize | CursorKind::EwResize => {
            CursorStyle::ResizeEastWest
        }
        CursorKind::NResize | CursorKind::SResize | CursorKind::NsResize => {
            CursorStyle::ResizeNorthSouth
        }
        CursorKind::NeResize | CursorKind::SwResize | CursorKind::NeswResize => {
            CursorStyle::ResizeNorthEastSouthWest
        }
        CursorKind::NwResize | CursorKind::SeResize | CursorKind::NwseResize => {
            CursorStyle::ResizeNorthWestSouthEast
        }
        // The rest have no cursor of their own on any platform this runs on: a context menu, a
        // help mark, an alias, a copy and the two magnifiers are all drawn as the arrow, which is
        // what a desktop toolkit gives back when asked for them.
        CursorKind::ContextMenu
        | CursorKind::Help
        | CursorKind::Alias
        | CursorKind::Copy
        | CursorKind::ZoomIn
        | CursorKind::ZoomOut => CursorStyle::Default,
    })
}

#[cfg(test)]
mod tests {
    use zgui_css::values::ui::CursorKind;
    use zgui_platform::CursorStyle;

    use super::style_of;

    /// `auto` is the one keyword this cannot answer on its own.
    #[test]
    fn auto_is_left_for_the_document_to_resolve() {
        assert_eq!(style_of(CursorKind::Auto), None);
        assert_eq!(style_of(CursorKind::Default), Some(CursorStyle::Default));
    }

    /// Every keyword that names a cursor gets one, and the named ones get the right one.
    ///
    /// The second half is the control: a table that answered the arrow to everything would satisfy
    /// the first half while making the whole property do nothing.
    #[test]
    fn every_keyword_answers_and_the_named_ones_answer_distinctly() {
        for keyword in [
            CursorKind::None,
            CursorKind::Pointer,
            CursorKind::Text,
            CursorKind::Crosshair,
            CursorKind::Grab,
            CursorKind::Wait,
            CursorKind::Move,
            CursorKind::EwResize,
            CursorKind::NwseResize,
            CursorKind::ContextMenu,
        ] {
            assert!(style_of(keyword).is_some(), "{keyword:?}");
        }
        assert_eq!(style_of(CursorKind::Pointer), Some(CursorStyle::Pointer));
        assert_eq!(style_of(CursorKind::Text), Some(CursorStyle::Text));
        assert_eq!(style_of(CursorKind::None), Some(CursorStyle::None));
        assert_eq!(
            style_of(CursorKind::NeResize),
            Some(CursorStyle::ResizeNorthEastSouthWest),
        );
    }
}
