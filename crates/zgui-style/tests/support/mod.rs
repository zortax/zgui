//! One document, one style engine, and the frame phases that drive them.
//!
//! Every case is written against this rather than against the engine directly, because a frame is
//! an *order*: the device epoch precedes the filter decision, which precedes the restyle, which
//! precedes the retirement. A case that called the phases in its own order would be testing an
//! arrangement nothing ships.

// The harness is compiled into several test targets and each uses a different part of it.
#![allow(dead_code)]

use std::sync::Arc;

use zgui_bits::Dirty;
use zgui_dom::dirty::walk;
use zgui_dom::{Document, NodeIndex, NodeKind, StyleFilter};
use zgui_geom::CssPx;
use zgui_interned::{ClassName, ElementName};
use zgui_style::{
    CssDiagnostics, DeviceEpoch, Restyle, SheetHandle, SheetOrigin, SheetSource, StyleEngine,
    StylePool, Viewport,
};
use zgui_text::FixedMetrics;

/// The surface every case starts at.
pub(crate) const START: Viewport = Viewport {
    width: CssPx(1280.0),
    height: CssPx(800.0),
    scale: 1.0,
    scheme: zgui_style::ColorScheme::Light,
};

/// A document with a style engine over it.
pub(crate) struct Harness {
    /// The document.
    pub(crate) document: Document,
    /// The engine styling it.
    pub(crate) engine: StyleEngine,
    /// The root element.
    pub(crate) root: NodeIndex,
    /// The handles of the installed sheets, in installation order.
    ///
    /// Held, because dropping a handle removes its sheet: an application keeps its handles for as
    /// long as it wants its styles, and a case that discarded them would be testing a document
    /// whose sheets are removed again before the next frame.
    sheets: Vec<Option<SheetHandle>>,
}

impl Harness {
    /// A document holding one root element, styled by an engine with only the user-agent sheet.
    pub(crate) fn new() -> Self {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let engine = StyleEngine::new(&document, Arc::new(FixedMetrics::new()), START);
        Self {
            document,
            engine,
            root,
            sheets: Vec::new(),
        }
    }

    /// Appends an element of `name` to `parent`, through the mutation protocol.
    ///
    /// Through the protocol rather than through the document's own builder, because a node linked
    /// in without it is a node no traversal is ever told about — which is correct while a document
    /// is being built and wrong the moment one has been styled.
    pub(crate) fn append(&mut self, parent: NodeIndex, name: &str) -> NodeIndex {
        self.edit(|edit| {
            let node = edit.create_element(ElementName::new(name));
            edit.insert_before(parent, node, None);
            node
        })
    }

    /// Inserts an element of `name` as `parent`'s first child, through the mutation protocol.
    pub(crate) fn prepend(&mut self, parent: NodeIndex, name: &str) -> NodeIndex {
        let first = self.document.store().core(parent).first_child();
        self.edit(|edit| {
            let node = edit.create_element(ElementName::new(name));
            edit.insert_before(parent, node, first);
            node
        })
    }

    /// Appends a text node to `parent`, through the mutation protocol.
    pub(crate) fn append_text(&mut self, parent: NodeIndex, text: &str) -> NodeIndex {
        self.edit(|edit| {
            let node = edit.create_text(text);
            edit.insert_before(parent, node, None);
            node
        })
    }

    /// Installs an author-origin sheet, and reports what the parser dropped.
    pub(crate) fn add_author(&mut self, css: &str) -> CssDiagnostics {
        self.add_sheet(SheetOrigin::Author, css)
    }

    /// Installs a sheet at `origin`, and reports what the parser dropped.
    pub(crate) fn add_sheet(&mut self, origin: SheetOrigin, css: &str) -> CssDiagnostics {
        let (handle, diagnostics) =
            self.engine
                .add_sheet(&self.document, origin, SheetSource::Text(css));
        self.sheets.push(Some(handle));
        diagnostics
    }

    /// Installs a sheet whose source comes from the document's loader.
    pub(crate) fn add_named(&mut self, origin: SheetOrigin, name: &str) -> CssDiagnostics {
        let (handle, diagnostics) =
            self.engine
                .add_sheet(&self.document, origin, SheetSource::Named(name));
        self.sheets.push(Some(handle));
        diagnostics
    }

    /// Installs a sheet at `origin`, immediately before the one installed `at`.
    pub(crate) fn insert_before(
        &mut self,
        origin: SheetOrigin,
        css: &str,
        at: usize,
    ) -> CssDiagnostics {
        let before = self.sheets[at].take().expect("the sheet is installed");
        let (handle, diagnostics) = self.engine.insert_sheet_before(
            &self.document,
            origin,
            SheetSource::Text(css),
            &before,
        );
        self.sheets[at] = Some(before);
        self.sheets.push(Some(handle));
        diagnostics
    }

    /// Replaces the text of the sheet installed `at`.
    pub(crate) fn replace(&mut self, at: usize, css: &str) -> CssDiagnostics {
        let handle = self.sheets[at].take().expect("the sheet is installed");
        let diagnostics =
            self.engine
                .replace_sheet(&self.document, &handle, SheetSource::Text(css));
        self.sheets[at] = Some(handle);
        diagnostics
    }

    /// Drops the handle of the sheet installed `at`, which is what removes it.
    pub(crate) fn drop_sheet(&mut self, at: usize) {
        self.sheets[at] = None;
    }

    /// Changes the document through the mutation protocol, consulting the engine's filter.
    ///
    /// This is the path a running application takes, filter and all — a case that edited the
    /// document without one would never exercise the cheap paths.
    pub(crate) fn edit<R>(&mut self, body: impl FnOnce(&mut zgui_dom::Edit<'_>) -> R) -> R {
        let filter = self.engine.filter();
        self.document
            .edit(&filter, body)
            .expect("the document is not poisoned")
    }

    /// Changes the document with a filter that proves nothing irrelevant.
    pub(crate) fn edit_unfiltered<R>(
        &mut self,
        body: impl FnOnce(&mut zgui_dom::Edit<'_>) -> R,
    ) -> R {
        self.document
            .edit(&zgui_dom::EverythingMatters, body)
            .expect("the document is not poisoned")
    }

    /// Whether the engine's filter can currently narrow anything.
    pub(crate) fn filter_is_disabled(&self) -> bool {
        self.engine.filter().is_disabled()
    }

    /// Runs the style half of one frame: the filter decision, then the restyle.
    pub(crate) fn frame(&mut self) -> Restyle {
        self.engine.disable_filters_if_sheets_changed();
        self.engine.restyle(&mut self.document, None)
    }

    /// The same, across a pool of workers.
    pub(crate) fn frame_on(&mut self, pool: &StylePool) -> Restyle {
        self.engine.disable_filters_if_sheets_changed();
        self.engine.restyle(&mut self.document, Some(pool))
    }

    /// Runs the device epoch for a new surface, as the frame's first step.
    pub(crate) fn resize(&mut self, width: f32, height: f32) -> DeviceEpoch {
        let next = Viewport {
            width: CssPx(width),
            height: CssPx(height),
            ..self.engine.viewport()
        };
        self.engine.device_epoch(&mut self.document, next)
    }

    /// The same, for a change of device pixel ratio.
    pub(crate) fn rescale(&mut self, scale: f32) -> DeviceEpoch {
        let next = self.engine.viewport().at_scale(scale);
        self.engine.device_epoch(&mut self.document, next)
    }

    /// The obligations `node` owes right now.
    pub(crate) fn owed(&self, node: NodeIndex) -> Dirty {
        self.document.store().core(node).dirty().own()
    }

    /// The obligations the subtree at `node` owes right now.
    pub(crate) fn owed_below(&self, node: NodeIndex) -> Dirty {
        self.document.store().core(node).dirty().subtree()
    }

    /// Retires every obligation in the document, the way the stages after the restyle do.
    ///
    /// Without this a case reading obligations after a second frame would read the first frame's
    /// as well, because nothing else in this crate services a relayout or a repaint.
    pub(crate) fn retire_all(&mut self) {
        let from = self.document.document_index();
        walk::walk(
            self.document.store_mut(),
            from,
            Dirty::all(),
            &mut |_store, _node| {},
        );
    }

    /// Sets `node`'s classes through the mutation protocol.
    pub(crate) fn set_classes(&mut self, node: NodeIndex, classes: &[&str]) {
        let classes: Vec<ClassName> = classes.iter().copied().map(ClassName::new).collect();
        self.edit(|edit| edit.set_classes(node, &classes));
    }

    /// Sets or clears an interaction-state bit through the mutation protocol.
    pub(crate) fn set_state(&mut self, node: NodeIndex, state: zgui_vocab::UiState, on: bool) {
        self.edit(|edit| edit.set_state(node, state, on));
    }

    /// How many elements the document holds.
    pub(crate) fn element_count(&self) -> usize {
        let mut count = 0;
        let mut stack = vec![self.document.document_index()];
        while let Some(index) = stack.pop() {
            if self.document.store().core(index).kind() == NodeKind::Element {
                count += 1;
            }
            let mut child = self.document.store().core(index).first_child();
            while let Some(current) = child {
                stack.push(current);
                child = self.document.store().core(current).next_sibling();
            }
        }
        count
    }
}

/// One frame's animation stage, in the order and with the policy the runtime uses.
///
/// The tick, then a mark for every element whose animations the runtime could not discharge with a
/// repaint, then the restyle. Which elements those are is the *caller's* decision everywhere in
/// this framework — the engine is told, never asked — so it is made here too rather than being
/// borrowed from the engine, and a case written against this is a case written against what a
/// window does.
pub(crate) fn animation_frame(harness: &mut Harness, now: f64) -> Restyle {
    let report = harness
        .engine
        .animation_tick(&harness.document, zgui_style::AnimationTime(now));
    for element in &report.elements {
        if element.properties.is_paint_only() || !(element.advancing || element.crossed) {
            continue;
        }
        harness
            .engine
            .mark_animation_restyle(&harness.document, element.index);
    }
    let pass = harness.engine.restyle(&mut harness.document, None);
    harness.retire_all();
    pass
}

/// The computed colour of `node`, as eight-bit sRGB.
///
/// # Panics
///
/// Panics if the element has no computed style.
pub(crate) fn color(harness: &Harness, node: NodeIndex) -> (u8, u8, u8) {
    let style = harness
        .document
        .node(node)
        .primary_style()
        .expect("the element is styled");
    let [r, g, b, _] = *style
        .get_inherited_text()
        .clone_color()
        .into_srgb_legacy()
        .raw_components();
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// The computed background colour of `node`, as eight-bit sRGB.
///
/// # Panics
///
/// Panics if the element has no computed style.
pub(crate) fn background(harness: &Harness, node: NodeIndex) -> (u8, u8, u8) {
    let style = harness
        .document
        .node(node)
        .primary_style()
        .expect("the element is styled");
    let [r, g, b, _] = *style
        .get_background()
        .clone_background_color()
        .resolve_to_absolute(&style.get_inherited_text().clone_color())
        .into_srgb_legacy()
        .raw_components();
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

/// The computed `display` of `node`.
///
/// # Panics
///
/// Panics if the element has no computed style.
pub(crate) fn display(harness: &Harness, node: NodeIndex) -> style::values::computed::Display {
    harness
        .document
        .node(node)
        .primary_style()
        .expect("the element is styled")
        .get_box()
        .clone_display()
}

/// The computed `border-top-left-radius` of `node`, in CSS pixels.
///
/// A reset property, deliberately: it distinguishes the elements a rule matched from the ones that
/// merely inherited from one, which an inherited property cannot.
pub(crate) fn radius(harness: &Harness, node: NodeIndex) -> f32 {
    let Some(style) = harness.document.node(node).primary_style() else {
        return 0.0;
    };
    style
        .get_border()
        .clone_border_top_left_radius()
        .0
        .width
        .0
        .to_used_value(app_units::Au(0))
        .to_f32_px()
}

/// The computed `width` of `node`, in CSS pixels, or zero when it is not a length.
pub(crate) fn width(harness: &Harness, node: NodeIndex) -> f32 {
    let Some(style) = harness.document.node(node).primary_style() else {
        return 0.0;
    };
    match style.get_position().clone_width() {
        style::values::computed::Size::LengthPercentage(length) => length
            .0
            .to_used_value(app_units::Au::from_f32_px(0.0))
            .to_f32_px(),
        _ => 0.0,
    }
}
