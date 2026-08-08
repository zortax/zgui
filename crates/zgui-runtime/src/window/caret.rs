//! Working out where this frame's caret and selection go, and what that owes the damage set.
//!
//! Run after layout and before painting, because it is a reading of the lines this frame produced.
//! What it writes is a plan the emit walk consults and a set of rectangles the renderer has to
//! clear: a caret that changed phase changes no fragment's geometry, so nothing else in the frame
//! would ever put its pixels into the damage.

use std::time::Instant;

use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_layout::FragmentKind;
use zgui_layout::fragment::ParagraphId;

use crate::caret::{Located, Plan, plan_for};
use crate::window::Window;

/// The colour one element's caret is drawn in.
///
/// `caret-color: auto` is the initial value and means *the text's own colour*, which is what makes
/// a caret visible in a field whose text a theme has recoloured without anybody writing a second
/// declaration. Anything else is the colour that was written.
fn caret_color(style: &zgui_css::ComputedStyle) -> zgui_color::Color {
    use zgui_css::values::ui::ColorOrAuto;
    let current = zgui_css::values::color::current(style);
    match &style.get_inherited_ui().caret_color.0 {
        ColorOrAuto::Auto => zgui_css::values::color::to_color(current),
        ColorOrAuto::Color(color) => zgui_css::values::color::resolve(color, current),
    }
}

impl Window {
    /// Computes this frame's caret and selection, and damages whatever that moved.
    pub(crate) fn plan_caret(&mut self, now: Instant) {
        let (plan, drawn) = self.caret_plan(now);
        for rect in self.carets.install(plan, drawn) {
            if rect.is_empty() {
                continue;
            }
            self.damage
                .absorb(zgui_layout::fragment::diff::pixels(rect));
        }
        // Where an input method should put its candidate window is a reading of the plan that was
        // just installed, so it is answered here rather than where the caret was asked to move.
        // Answering it at the keystroke reports the rectangle the *previous* frame planned — a
        // candidate window one character behind the text being composed — and answering it when
        // the surface hands the keyboard back reports no rectangle at all, because the frame that
        // plans one has not run yet. The surface is told only when the answer really changed.
        self.report_caret();
    }

    /// This frame's plan, and the absolute rectangles it will draw.
    fn caret_plan(&self, now: Instant) -> (Plan, Vec<Rect<DevicePx, Device>>) {
        let Some(node) = self.router.interaction().focus.focused() else {
            return (Plan::empty(), Vec::new());
        };
        let document = self.document.borrow();
        if !crate::editing::Editors::is_editable(&document, node) {
            return (Plan::empty(), Vec::new());
        }
        // The model's own selection, not the record beside it: the record is a byte range and has
        // forgotten which end the caret is at and which of the two places a boundary offset means.
        let Some(selection) = self.editors.selection(node) else {
            return (Plan::empty(), Vec::new());
        };
        let layout = self.layout.borrow();
        let Some(located) = Located::of(&layout, &self.text, node) else {
            return (Plan::empty(), Vec::new());
        };
        let Some(box_) = crate::caret::place::text_box(&layout, node) else {
            return (Plan::empty(), Vec::new());
        };
        // The *element's* own style, not the anonymous box that holds the lines: an anonymous box
        // inherits everything read here anyway, and reading the element's is what makes a caret
        // that a stylesheet can move by writing on the field.
        let element = layout.boxes_of(node).first().copied().unwrap_or(box_);
        let color = caret_color(&layout.node(element).style);
        let plan = plan_for(
            &located,
            selection,
            color,
            self.scale,
            self.carets.blink().is_visible(now),
        );
        let drawn = self.absolute_rects(&layout, box_, &plan);
        (plan, drawn)
    }

    /// Every rectangle the plan will draw, on the device.
    ///
    /// Placed against the line fragments themselves rather than against the resolution's own line
    /// boxes, because a fragment is where the scroll offset has already been applied — and then
    /// carried through the coordinate system the fragment is in, because that is the one thing a
    /// fragment's own rectangle does *not* have applied to it.
    ///
    /// Damage is measured in real pixels and in nothing else. A caret in a field inside a panel
    /// that is sliding would otherwise damage the rectangle the field would occupy if the panel
    /// had never moved: pixels cleared where there is no caret, and the caret's own pixels left
    /// behind wherever it was drawn last.
    fn absolute_rects(
        &self,
        layout: &zgui_layout::LayoutStore,
        box_: zgui_dom::side::BoxKey,
        plan: &Plan,
    ) -> Vec<Rect<DevicePx, Device>> {
        let placements = self.host.placements();
        let mut out = Vec::new();
        for frag in layout.fragments_of_box(box_) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            let FragmentKind::Line { paragraph, line } = fragment.kind else {
                continue;
            };
            out.extend(
                plan.rects_of(paragraph, line, fragment.border_box.origin)
                    .into_iter()
                    .map(|rect| {
                        zgui_layout::fragment::transform::placed::onto_device(
                            rect,
                            fragment.transform,
                            &placements,
                        )
                    }),
            );
        }
        out
    }

    /// The colour this frame's caret is being drawn in, if one is planned.
    ///
    /// Public for the same reason [`Window::caret_rect`] is: a caret is drawn from a plan rather
    /// than from a fragment, so what colour it came out is not in the document, the fragment tree
    /// or the display list.
    pub fn caret_color(&self) -> Option<zgui_color::Color> {
        self.carets.plan().caret_color()
    }

    /// Where the caret is on the screen right now, for the surface's own sake.
    ///
    /// Used to tell an input method where to put its candidate window. Nothing when no caret is
    /// planned, which is when there is nothing to place one beside.
    ///
    /// On the device, through the coordinate system the line is in: a candidate window is placed by
    /// the compositor against the screen, so a rectangle in the paragraph's own space would open it
    /// next to where the field would be if nothing above it had been moved.
    ///
    /// Public because it is the one rectangle this window hands to something outside the process
    /// that no display list, no fragment and no accessibility node carries: an input method places
    /// its candidate window against it, and nothing else observes it.
    pub fn caret_rect(&self) -> Option<Rect<DevicePx, Device>> {
        let node = self.router.interaction().focus.focused()?;
        let layout = self.layout.borrow();
        let box_ = crate::caret::place::text_box(&layout, node)?;
        let placements = self.host.placements();
        let plan = self.carets.plan();
        for frag in layout.fragments_of_box(box_) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            let FragmentKind::Line { paragraph, line } = fragment.kind else {
                continue;
            };
            if let Some(rect) = plan
                .rects_of(paragraph, line, fragment.border_box.origin)
                .last()
            {
                return Some(zgui_layout::fragment::transform::placed::onto_device(
                    *rect,
                    fragment.transform,
                    &placements,
                ));
            }
        }
        None
    }

    /// The model offset a point on the surface lands on inside an editable element, and which of
    /// that offset's two carets the point meant.
    ///
    /// Absolute device pixels in, an offset into the element's own text out. Nothing when the
    /// element has no laid-out text, which is a field that has not been through a frame yet.
    pub(crate) fn offset_at(
        &self,
        node: zgui_dom::NodeKey,
        point: Point<DevicePx, Device>,
    ) -> Option<(usize, zgui_edit::Affinity)> {
        let layout = self.layout.borrow();
        let located = Located::of(&layout, &self.text, node)?;
        located.hit(point, &self.scene.spatial)
    }

    /// The paragraph an editable element's lines belong to, for a caller inspecting the plan.
    pub fn caret_paragraph(&self, node: zgui_dom::NodeKey) -> Option<ParagraphId> {
        let layout = self.layout.borrow();
        let box_ = crate::caret::place::text_box(&layout, node)?;
        layout
            .inline_resolution(box_)
            .map(|resolution| resolution.paragraph)
    }
}
