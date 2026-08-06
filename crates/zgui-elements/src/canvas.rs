//! The canvas element: shapes an application builds and mutates, drawn by the vector pipeline.
//!
//! A canvas is the imperative sibling of [`vector`](crate::vector): where a vector element is
//! *given* its outlines — as notation or as a document — a canvas is *drawn into*, either through
//! a retained [`CanvasHandle`] the application keeps, or through a closure that re-runs when the
//! signals it reads change. Both feed the same pipeline as every other drawing: the shapes are
//! fitted to the content box, painted per shape, and rasterised by the vector backend, so a
//! canvas costs a texture nothing and shares the encoding cache with every icon on screen.
//!
//! What crosses the document is a token and a revision, never the shapes; see [`zgui_canvas`]
//! for the registry that resolves the token on the paint side.

use std::rc::Rc;

use zgui_canvas::{CanvasScene, SceneHandle};
use zgui_geom::{Css, CssPx, Size};
use zgui_reactive::prelude::*;
use zgui_reactive::{ArcTrigger, RenderEffect, RwSignal};
use zgui_view::{
    Anchor, BuildCx, DomHandle, NodeId, ObservationHandle, Observed, ObservedValue, PropKey,
    PropValue, View,
};
use zgui_vocab::prop::drawing;

use crate::element::{Element, ElementState};
use crate::tag::Canvas;

/// A retained canvas an application draws into, bound to elements through
/// [`Element::scene`](Element::<Canvas>::scene).
///
/// The handle owns two things: the scene — shapes in the shared vector vocabulary — and the
/// reactive edge that tells every bound element the scene moved. [`CanvasHandle::draw`] is the
/// one way through both, which is what makes "I mutated it and nothing repainted" impossible to
/// write.
///
/// Cloneable; clones share the scene and the edge. The scene half is `Send + Sync` through
/// [`SceneHandle`], but *this* type stays on the UI thread — a background producer clones the
/// [`SceneHandle`] out of [`CanvasHandle::scene`] and the UI side keeps the reactive edge.
#[derive(Clone)]
pub struct CanvasHandle {
    /// The scene itself.
    scene: SceneHandle,
    /// Who to tell when it changes.
    changed: ArcTrigger,
}

impl CanvasHandle {
    /// Registers an empty canvas.
    pub fn new() -> Self {
        Self {
            scene: SceneHandle::new(),
            changed: ArcTrigger::new(),
        }
    }

    /// Mutates the scene, and marks every element showing it for repaint.
    pub fn draw<R>(&self, draw: impl FnOnce(&mut CanvasScene) -> R) -> R {
        let result = self.scene.edit(draw);
        self.changed.notify();
        result
    }

    /// The thread-safe half, for a producer that draws from off the UI thread.
    ///
    /// Edits through it move the scene's revision but wake nobody: pair it with a signal or a
    /// [`CanvasHandle::draw`] call on the UI side, or the picture changes only when something
    /// else causes a repaint.
    pub fn scene(&self) -> &SceneHandle {
        &self.scene
    }
}

impl Default for CanvasHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl Element<Canvas> {
    /// Shows `handle`'s scene in this element.
    ///
    /// The binding writes the scene's token and revision as a property and tracks the handle's
    /// change edge, so a [`CanvasHandle::draw`] elsewhere becomes a value change here, and a
    /// value change is a repaint of exactly this element.
    #[must_use]
    pub fn scene(self, handle: &CanvasHandle) -> Self {
        let scene = handle.scene.clone();
        let changed = handle.changed.clone();
        self.property(PropKey::new(drawing::CANVAS), move || {
            changed.track();
            PropValue::Integer(drawing::canvas_value(scene.token().0, scene.revision()))
        })
    }

    /// The square of user space the shapes are written in, and the size they are drawn at.
    ///
    /// Exactly [`vector`](crate::vector)'s view box: with one, the shapes are scaled uniformly to
    /// fit the content box; without one, they are already in CSS pixels from the content box's
    /// top left corner — which is what a canvas that reads its own size from
    /// [`DrawCx::size`] wants.
    #[must_use]
    pub fn view_box(self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.property(
            PropKey::new(drawing::VIEW_BOX),
            PropValue::from(format!("{x} {y} {width} {height}").as_str()),
        )
    }

    /// Draws this canvas with `draw`, re-running it when anything it reads changes.
    ///
    /// The closure runs inside a render effect: signals it reads are tracked, and so is the
    /// element's own content size, so a canvas redraws when its data changes *and* when its box
    /// does. Each run starts from an empty scene — this is the immediate-mode face over the
    /// retained machinery, and a run is assumed to have changed what it drew.
    ///
    /// The first run happens before layout, at zero size; the size arrives with the first
    /// frame's geometry and re-runs the closure, so a canvas that draws relative to its box
    /// settles one frame after it appears.
    ///
    /// ```no_run
    /// use zgui_canvas::{Brush, ShapeBuilder};
    /// use zgui_color::Color;
    /// use zgui_elements::canvas;
    ///
    /// let view = canvas().draw(|cx| {
    ///     let (w, h) = (cx.size.width.0 as f64, cx.size.height.0 as f64);
    ///     let mut path = kurbo::BezPath::new();
    ///     path.move_to((0.0, h));
    ///     path.line_to((w / 2.0, 0.0));
    ///     path.line_to((w, h));
    ///     path.close_path();
    ///     cx.scene.push(
    ///         ShapeBuilder::new(path)
    ///             .fill(Brush::Solid(Color::srgb(0.2, 0.5, 0.9, 1.0)))
    ///             .build(),
    ///     );
    /// });
    /// ```
    #[must_use]
    pub fn draw<F: Fn(&mut DrawCx<'_>) + 'static>(self, draw: F) -> DrawnCanvas<F> {
        DrawnCanvas {
            element: self,
            draw,
        }
    }
}

/// One run of a canvas closure: the scene to draw into, and the box it is drawn in.
pub struct DrawCx<'a> {
    /// The scene, already cleared for this run.
    pub scene: &'a mut CanvasScene,
    /// The element's content box size, in CSS pixels. Zero until the first layout has run.
    pub size: Size<CssPx, Css>,
    /// Device pixels per CSS pixel, for a canvas that aligns hairlines to device pixels.
    pub scale: f32,
}

/// A canvas element with a draw closure over it. Built by
/// [`Element::draw`](Element::<Canvas>::draw).
#[must_use = "a view does nothing until it is built"]
pub struct DrawnCanvas<F> {
    /// The element, still undescribed to any document.
    element: Element<Canvas>,
    /// What draws it.
    draw: F,
}

/// The retained state of a drawn canvas: the element's own state plus the drawing machinery.
pub struct DrawnCanvasState {
    /// The element.
    element: ElementState,
    /// The canvas the closure draws into.
    handle: CanvasHandle,
    /// The observed content size, feeding the effect.
    size: RwSignal<Size<CssPx, Css>>,
    /// The effect that re-runs the closure. Dropped with the state, which stops the drawing.
    effect: Option<RenderEffect<()>>,
    /// The observation registration. Dropped with the state, which stops the deliveries.
    _observation: ObservationHandle,
}

impl Anchor for DrawnCanvasState {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.element.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        // The effect and the observation go first, deliberately: an unmounted canvas whose
        // closure could still run would be drawing into a scene nothing shows.
        self.effect = None;
        self.element.unmount(dom);
    }

    fn first_node(&self) -> Option<NodeId> {
        self.element.first_node()
    }
}

impl<F: Fn(&mut DrawCx<'_>) + 'static> View for DrawnCanvas<F> {
    type State = DrawnCanvasState;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let handle = CanvasHandle::new();
        let size = cx.with_owner(|| RwSignal::new(Size::new(CssPx(0.0), CssPx(0.0))));
        let scale = cx.host().scale();

        let element = self.element.scene(&handle).build(cx);
        let observation = observe_content_size(cx.dom(), element.node(), size, scale);
        let effect = start(cx, handle.clone(), size, scale, self.draw);

        DrawnCanvasState {
            element,
            handle,
            size,
            effect: Some(effect),
            _observation: observation,
        }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        // The element is described again in place; the canvas binding is re-attached to the same
        // handle, so the property and its tracking survive the rebuild.
        self.element
            .scene(&state.handle)
            .rebuild(&mut state.element, cx);
        // The closure is replaced, not accumulated: the state described by *this* description is
        // what the new closure draws, and the old effect dropped here is what stops the previous
        // one running beside it.
        let scale = cx.host().scale();
        state.effect = None;
        state.effect = Some(start(
            cx,
            state.handle.clone(),
            state.size,
            scale,
            self.draw,
        ));
    }
}

/// Registers the content-size observation that feeds `size`.
fn observe_content_size(
    dom: &DomHandle,
    node: NodeId,
    size: RwSignal<Size<CssPx, Css>>,
    scale: f32,
) -> ObservationHandle {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    dom.observe(
        node,
        Observed::ContentSize,
        Rc::new(move |value: ObservedValue| {
            if let Some(device) = value.as_content_size() {
                size.set(Size::new(
                    CssPx(device.width.0 / scale),
                    CssPx(device.height.0 / scale),
                ));
            }
        }),
    )
}

/// Starts the effect that redraws the canvas from the closure.
fn start<F: Fn(&mut DrawCx<'_>) + 'static>(
    cx: &BuildCx<'_>,
    handle: CanvasHandle,
    size: RwSignal<Size<CssPx, Css>>,
    scale: f32,
    draw: F,
) -> RenderEffect<()> {
    cx.with_owner(|| {
        RenderEffect::new(move |_| {
            let current = size.get();
            handle.draw(|scene| {
                scene.clear();
                draw(&mut DrawCx {
                    scene,
                    size: current,
                    scale,
                });
            });
        })
    })
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use zgui_canvas::{Brush, ShapeBuilder};
    use zgui_geom::{Device, DevicePx, Size};
    use zgui_interned::ElementName;
    use zgui_reactive::Mounted;
    use zgui_view::stub::{StubDom, StubHost};
    use zgui_view::{
        Anchor, BuildCxOwned, DocumentId, Dom, DomHandle, HostHandle, ObservedValue, PropKey,
        View,
    };
    use zgui_vocab::prop::drawing;

    use super::CanvasHandle;
    use crate::canvas;

    fn shape() -> zgui_canvas::Shape {
        let mut path = kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((8.0, 8.0));
        ShapeBuilder::new(path)
            .stroke(
                Brush::Solid(zgui_color::Color::srgb(0.0, 0.0, 0.0, 1.0)),
                1.0,
            )
            .build()
    }

    /// The stub scaffolding every test here mounts into.
    fn harness() -> (Rc<StubDom>, DomHandle, Mounted, BuildCxOwned) {
        zgui_reactive::install().ok();
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone() as Rc<dyn Dom>);
        let window = Mounted::new();
        let cx = BuildCxOwned::new(
            dom.clone(),
            HostHandle::new(StubHost::default()),
            window.owner().clone(),
            DocumentId::FIRST,
        );
        (backend, dom, window, cx)
    }

    #[test]
    fn a_bound_scene_writes_its_reference_and_rewrites_it_per_draw() {
        let (backend, dom, window, cx) = harness();
        let handle = CanvasHandle::new();
        let root = dom.create_element(ElementName::new("box"));
        let mut built = window.with(|| canvas().scene(&handle).build(&mut cx.cx()));
        built.mount(&dom, root, None);

        let node = built.first_node().expect("the element is a node");
        let key = PropKey::new(drawing::CANVAS);
        let before = backend.property(node, key).expect("the reference is written");

        handle.draw(|scene| scene.push(shape()));
        zgui_reactive::flush();
        let after = backend.property(node, key).expect("still written");
        assert_ne!(
            before, after,
            "a draw moves the revision, and the binding rewrites the property"
        );
        window.unmount();
    }

    #[test]
    fn a_draw_closure_redraws_when_its_size_arrives() {
        let (backend, dom, window, cx) = harness();
        let root = dom.create_element(ElementName::new("box"));
        let mut built = window.with(|| {
            canvas()
                .draw(|cx| {
                    let width = f64::from(cx.size.width.0);
                    let mut path = kurbo::BezPath::new();
                    path.move_to((0.0, 0.0));
                    path.line_to((width, 0.0));
                    cx.scene.push(
                        ShapeBuilder::new(path)
                            .stroke(
                                Brush::Solid(zgui_color::Color::srgb(0.0, 0.0, 0.0, 1.0)),
                                1.0,
                            )
                            .build(),
                    );
                })
                .build(&mut cx.cx())
        });
        built.mount(&dom, root, None);

        let node = built.first_node().expect("the element is a node");
        let key = PropKey::new(drawing::CANVAS);
        let at_zero = backend
            .property(node, key)
            .expect("the first run wrote a reference before any layout");

        // Layout happens, and the backend delivers the content size the way a frame would.
        let size: Size<DevicePx, Device> = Size::new(DevicePx(120.0), DevicePx(40.0));
        backend.deliver(node, ObservedValue::ContentSize(size));
        zgui_reactive::flush();
        let at_size = backend.property(node, key).expect("still written");
        assert_ne!(
            at_zero, at_size,
            "the size arriving re-ran the closure, which is a new revision"
        );
        window.unmount();
    }
}
