//! What a view says about an element, seen from the style engine's side.
//!
//! Reading a state bit back off the document proves that a write landed in a column. It does not
//! prove the thing the write exists for: that `:disabled` and `:state(peeking)` now match, and
//! that the change reached the traversal at all. A state written into the arena without the
//! invalidation it owes reads back perfectly and never repaints.

mod support;

use std::sync::Arc;

use zgui_elements::control;
use zgui_geom::CssPx;
use zgui_interned::Ident;
use zgui_reactive::prelude::*;
use zgui_reactive::{RwSignal, flush};
use zgui_style::{SheetOrigin, SheetSource, StyleEngine, Viewport};
use zgui_text::FixedMetrics;
use zgui_view::{Anchor, IntoView, NodeId, UiState, View};

use crate::support::Window;

/// A window with one styled control under it, and the engine that styles it.
struct Styled {
    window: Window,
    engine: StyleEngine,
    control: NodeId,
    /// Kept alive: dropping the handle removes the sheet, and dropping the view unmounts the tree.
    _sheet: zgui_style::SheetHandle,
    _built: Box<dyn Anchor>,
}

impl Styled {
    /// Builds `<control state:disabled=… custom_state:peeking=…/>` under a sheet that matches both.
    fn open() -> (Self, RwSignal<bool>) {
        let window = Window::open();
        let on = window.window.with(|| RwSignal::new(false));
        let view = control()
            .class("c")
            .state(UiState::DISABLED, on)
            .custom_state(Ident::new("peeking"), on);
        let mut built = window
            .window
            .with(|| view.into_view().build(&mut window.cx.cx()));
        built.mount(&window.dom, window.root, None);
        let control = built.node();

        let mut engine = {
            let document = window.document.borrow();
            StyleEngine::new(
                &document,
                Arc::new(FixedMetrics::new()),
                Viewport::new(CssPx(1280.0), CssPx(800.0)),
            )
        };
        let sheet = {
            let document = window.document.borrow();
            let (handle, diagnostics) = engine.add_sheet(
                &document,
                SheetOrigin::Author,
                SheetSource::Text(
                    ".c { color: rgb(10, 10, 10) } \
                     .c:disabled { color: rgb(20, 20, 20) } \
                     .c:state(peeking) { color: rgb(30, 30, 30) }",
                ),
            );
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            handle
        };
        let styled = Self {
            window,
            engine,
            control,
            _sheet: sheet,
            _built: Box::new(built),
        };
        (styled, on)
    }

    /// Runs a restyle and reports how many elements it styled.
    fn restyle(&mut self) -> usize {
        let mut document = self.window.document.borrow_mut();
        self.engine.restyle(&mut document, None).styled
    }

    /// The control's computed text colour, in opaque eight-bit sRGB.
    fn colour(&self) -> [u8; 3] {
        let index = self.window.backend.index_of(self.control);
        let document = self.window.document.borrow();
        let style = document
            .node(index)
            .primary_style()
            .expect("the control was styled");
        let colour = zgui_css::values::color::to_color(zgui_css::values::color::current(&style));
        let [r, g, b, _] = colour.to_premultiplied_srgb();
        [
            (r * 255.0).round() as u8,
            (g * 255.0).round() as u8,
            (b * 255.0).round() as u8,
        ]
    }
}

/// `:disabled` and `:state(peeking)` both match, and the change that made them match cost one
/// element's restyle rather than the tree's.
#[test]
fn a_state_a_view_asserted_is_matched_by_a_selector_and_costs_one_elements_restyle() {
    let (mut styled, on) = Styled::open();

    assert!(styled.restyle() > 0, "the first pass reached the tree");
    assert_eq!(styled.colour(), [10, 10, 10], "neither state is asserted");

    on.set(true);
    flush();
    let restyled = styled.restyle();
    assert_eq!(
        styled.colour(),
        [30, 30, 30],
        "the last matching rule wins, so both states reached matching"
    );
    assert_eq!(
        restyled, 1,
        "a state on one element is one element's restyle, not the tree's"
    );

    on.set(false);
    flush();
    assert_eq!(styled.restyle(), 1);
    assert_eq!(styled.colour(), [10, 10, 10], "and it goes away again");

    styled.window.window.unmount();
}

/// The converse: only the interaction state, so a green
/// [`a_state_a_view_asserted_is_matched_by_a_selector_and_costs_one_elements_restyle`] cannot come
/// from the author-defined half alone.
#[test]
fn the_interaction_state_alone_is_enough_to_change_what_matches() {
    let window = Window::open();
    let on = window.window.with(|| RwSignal::new(false));
    let view = control().class("c").state(UiState::DISABLED, on);
    let mut built = window
        .window
        .with(|| view.into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    let control_node = built.node();

    let mut engine = {
        let document = window.document.borrow();
        StyleEngine::new(
            &document,
            Arc::new(FixedMetrics::new()),
            Viewport::new(CssPx(1280.0), CssPx(800.0)),
        )
    };
    let _sheet = {
        let document = window.document.borrow();
        engine.add_sheet(
            &document,
            SheetOrigin::Author,
            SheetSource::Text(
                ".c { color: rgb(10, 10, 10) } .c:disabled { color: rgb(20, 20, 20) }",
            ),
        )
    };
    {
        let mut document = window.document.borrow_mut();
        engine.restyle(&mut document, None);
    }
    let colour = |window: &Window| {
        let index = window.backend.index_of(control_node);
        let document = window.document.borrow();
        let style = document.node(index).primary_style().expect("styled");
        let [r, _, _, _] =
            zgui_css::values::color::to_color(zgui_css::values::color::current(&style))
                .to_premultiplied_srgb();
        (r * 255.0).round() as u8
    };
    assert_eq!(colour(&window), 10);

    on.set(true);
    flush();
    {
        let mut document = window.document.borrow_mut();
        assert_eq!(engine.restyle(&mut document, None).styled, 1);
    }
    assert_eq!(
        colour(&window),
        20,
        "`:disabled` sees what the view asserted"
    );

    built.unmount(&window.dom);
    window.window.unmount();
}
