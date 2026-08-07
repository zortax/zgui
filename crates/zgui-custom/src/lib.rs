//! Trait-based custom elements: retained widgets that measure themselves, place their children
//! and paint their own primitives, on the engine's own pipelines.
//!
//! A custom element is the middle ground the element vocabulary otherwise lacks. Composing
//! existing elements is declarative and pays per node; a `surface` element brings a whole
//! renderer and pays a texture; a `canvas` pays a rasterisation. A [`CustomElement`] pays what a
//! built-in element pays — quads through the quad pipeline, shapes through the vector pipeline,
//! children through ordinary boxes — while keeping its own state and deciding its own geometry.
//!
//! # The shape of one
//!
//! ```no_run
//! use zgui_custom::{CustomElement, CustomLayoutCx, CustomMeasured, ScenePainter};
//! use zgui_geom::{DevicePx, Point, Rect, Size};
//!
//! /// A meter: a track with a fill proportional to its value.
//! struct Meter {
//!     value: f32,
//! }
//!
//! impl CustomElement for Meter {
//!     fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
//!         CustomMeasured {
//!             width: cx.known_width.unwrap_or(120.0 * cx.scale),
//!             height: 8.0 * cx.scale,
//!             ..CustomMeasured::default()
//!         }
//!     }
//!
//!     fn paint(&mut self, painter: &mut ScenePainter<'_>) {
//!         let size = painter.size();
//!         let track = Rect::new(Point::new(DevicePx(0.0), DevicePx(0.0)), size);
//!         painter.fill(track, 4.0, zgui_color::Color::srgb(0.2, 0.2, 0.25, 1.0));
//!         let filled = Rect::new(
//!             Point::new(DevicePx(0.0), DevicePx(0.0)),
//!             Size::new(DevicePx(size.width.0 * self.value), size.height),
//!         );
//!         painter.fill(filled, 4.0, painter.current_color());
//!     }
//! }
//!
//! let (view, handle) = zgui_custom::custom(Meter { value: 0.4 });
//! // …mount `view`; later: handle.update(|meter| meter.value = 0.7); handle.repaint();
//! ```
//!
//! # How it reaches the engine
//!
//! The element carries a token — the same arrangement a canvas's shapes have — and the
//! implementation lives in a registry on the UI thread. The window's layout pass resolves the
//! token through [`CustomLayoutSource`] and the paint walk through
//! [`CustomPaintSource`]; both sources are this crate's [`sources`], installed by
//! the umbrella crate for every window. Layout wraps the element's answer in the ordinary CSS
//! shell, and paint records its primitives in the ordinary replay cache — an element that has not
//! said it changed is replayed without being asked.

#![forbid(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use rustc_hash::FxHashMap;
use zgui_elements::Element;
use zgui_elements::tag::Tag;
use zgui_reactive::ArcTrigger;
use zgui_reactive::prelude::*;
use zgui_vocab::{PropKey, PropValue, prop};

pub use zgui_layout::custom::{
    ChildMeasure, CustomLayoutCx, CustomLayoutSource, CustomMeasured, LayoutAccess, Space,
};
pub use zgui_paint::content::custom::{CustomPaintSource, ScenePainter};

/// A retained element the application implements rather than composes.
///
/// Both methods run on the UI thread, inside the frame: `layout` wherever the pass needs a size
/// — several times, with [`CustomLayoutCx::final_pass`] saying which answer is kept — and `paint`
/// only on frames where the element's recorded primitives cannot be replayed. Neither may touch
/// the document; what an element wants changed it changes in its own state, through
/// [`CustomHandle::update`], and says so through the handle.
pub trait CustomElement: 'static {
    /// Measures the content and, on the final pass, places the children.
    fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured;

    /// Emits the element's own primitives for this frame.
    fn paint(&mut self, painter: &mut ScenePainter<'_>);
}

/// One registered implementation, shared between the handle and the frame adapters.
///
/// The element is held as `dyn Any` with monomorphised shims beside it, because two parties need
/// two different views of one object: the frame adapters call the trait, and the handle reaches
/// the concrete type — and Rust will not cast between the two trait objects at run time.
struct Slot {
    /// The implementation, concretely typed behind `Any`.
    element: RefCell<Box<dyn std::any::Any>>,
    /// Calls [`CustomElement::layout`] on it.
    layout: fn(&mut dyn std::any::Any, &mut CustomLayoutCx<'_>) -> CustomMeasured,
    /// Calls [`CustomElement::paint`] on it.
    paint: fn(&mut dyn std::any::Any, &mut ScenePainter<'_>),
    /// Moved by [`CustomHandle::relayout`]; part of the element's property reference.
    layout_revision: Cell<u16>,
    /// Moved by both invalidations; the paint walk's replay record reads it.
    paint_revision: Cell<u16>,
}

thread_local! {
    /// The registered implementations, weakly: an element whose view and handle are both gone
    /// resolves to nothing, which draws nothing.
    static REGISTRY: RefCell<FxHashMap<u32, Weak<Slot>>> = RefCell::new(FxHashMap::default());
}

/// The implementation `token` names, while something keeps it alive.
fn resolve(token: u32) -> Option<Rc<Slot>> {
    REGISTRY.with(|registry| registry.borrow().get(&token)?.upgrade())
}

/// The element vocabulary's newcomer: the tag custom elements are built on.
///
/// Declared here rather than in the vocabulary crate because the vocabulary's names each carry a
/// meaning of their own and this one deliberately does not: what a `custom` element *is* is
/// whatever its implementation says, and the framework's own sheet gives it only a layout.
pub struct Custom;

impl Tag for Custom {
    fn name() -> zgui_interned::ElementName {
        zgui_interned::ElementName::new("custom")
    }
}

/// The application's side of one mounted custom element.
///
/// Cloneable. This is how the element's state is reached from event handlers and effects, and
/// how changes announce themselves: mutate through [`update`](CustomHandle::update), then call
/// [`repaint`](CustomHandle::repaint) — or [`relayout`](CustomHandle::relayout) when the mutation
/// can change the element's size or its children's placement.
pub struct CustomHandle<T: CustomElement> {
    /// The registered implementation.
    slot: Rc<Slot>,
    /// The reactive edge the element's property binding tracks.
    changed: ArcTrigger,
    /// Which concrete type lives in the slot, so `update` needs no downcast fallibility.
    marker: std::marker::PhantomData<T>,
}

// By hand rather than derived, because the derive would demand `T: Clone` for a clone that
// copies two reference counts and a marker — the element itself is shared, never duplicated.
impl<T: CustomElement> Clone for CustomHandle<T> {
    fn clone(&self) -> Self {
        Self {
            slot: Rc::clone(&self.slot),
            changed: self.changed.clone(),
            marker: std::marker::PhantomData,
        }
    }
}

impl<T: CustomElement> CustomHandle<T> {
    /// Reads or mutates the element's state.
    ///
    /// Mutation announces nothing by itself — pair it with
    /// [`repaint`](CustomHandle::repaint) or [`relayout`](CustomHandle::relayout), or the change
    /// waits for whatever next repaints the element anyway.
    pub fn update<R>(&self, with: impl FnOnce(&mut T) -> R) -> R {
        let mut element = self.slot.element.borrow_mut();
        // The slot was filled by `custom` with exactly `T`; nothing else can write it.
        let concrete = element
            .downcast_mut::<T>()
            .expect("the slot holds the type the handle was created with");
        with(concrete)
    }

    /// Says the element paints differently: its recorded primitives are re-encoded.
    pub fn repaint(&self) {
        self.slot
            .paint_revision
            .set(self.slot.paint_revision.get().wrapping_add(1));
        self.changed.notify();
    }

    /// Says the element's size or its children's placement changed: it is measured again.
    pub fn relayout(&self) {
        self.slot
            .layout_revision
            .set(self.slot.layout_revision.get().wrapping_add(1));
        self.repaint();
    }
}

/// Builds a custom element view over `element`, and the handle that reaches it afterwards.
///
/// The returned builder is an ordinary [`Element`]: it takes classes, styles, listeners and
/// children like any other, and its children are the element's to place in
/// [`CustomElement::layout`].
pub fn custom<T: CustomElement>(element: T) -> (Element<Custom>, CustomHandle<T>) {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let token = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let slot = Rc::new(Slot {
        element: RefCell::new(Box::new(element)),
        layout: |any, cx| {
            any.downcast_mut::<T>()
                .expect("the shim was monomorphised with the slot's type")
                .layout(cx)
        },
        paint: |any, painter| {
            any.downcast_mut::<T>()
                .expect("the shim was monomorphised with the slot's type")
                .paint(painter)
        },
        layout_revision: Cell::new(0),
        paint_revision: Cell::new(0),
    });
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.retain(|_, held| held.upgrade().is_some());
        registry.insert(token, Rc::downgrade(&slot));
    });
    let changed = ArcTrigger::new();
    let handle = CustomHandle {
        slot: Rc::clone(&slot),
        changed: changed.clone(),
        marker: std::marker::PhantomData,
    };
    let view = Element::<Custom>::new().property(PropKey::new(prop::custom::ELEMENT), move || {
        changed.track();
        PropValue::Integer(prop::custom::reference(
            token,
            slot.layout_revision.get(),
            slot.paint_revision.get(),
        ))
    });
    (view, handle)
}

/// The two halves of the registry, as the sources a window installs.
///
/// The umbrella crate wires this for every window; an application driving the runtime by hand
/// passes it to `App::with_custom`.
pub fn sources() -> (
    Box<dyn CustomLayoutSource>,
    Box<dyn CustomPaintSource>,
) {
    (Box::new(LayoutHalf), Box::new(PaintHalf))
}

/// The registry, answering the layout seam.
struct LayoutHalf;

impl CustomLayoutSource for LayoutHalf {
    fn layout(&self, token: u32, cx: &mut CustomLayoutCx<'_>) -> Option<CustomMeasured> {
        let slot = resolve(token)?;
        let mut element = slot.element.borrow_mut();
        Some((slot.layout)(&mut **element, cx))
    }
}

/// The registry, answering the paint seam.
struct PaintHalf;

impl CustomPaintSource for PaintHalf {
    fn revision(&self, token: u32) -> u64 {
        resolve(token).map_or(0, |slot| {
            (u64::from(slot.layout_revision.get()) << 16) | u64::from(slot.paint_revision.get())
        })
    }

    fn paint(&self, token: u32, painter: &mut ScenePainter<'_>) {
        if let Some(slot) = resolve(token) {
            let mut element = slot.element.borrow_mut();
            (slot.paint)(&mut **element, painter);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe;

    impl CustomElement for Probe {
        fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
            CustomMeasured {
                width: cx.known_width.unwrap_or(10.0),
                height: 10.0,
                ..CustomMeasured::default()
            }
        }

        fn paint(&mut self, _painter: &mut ScenePainter<'_>) {}
    }

    #[test]
    fn the_revisions_move_the_way_the_invalidations_say() {
        zgui_reactive::install().ok();
        let (_view, handle) = custom(Probe);
        let read = || {
            (
                handle.slot.layout_revision.get(),
                handle.slot.paint_revision.get(),
            )
        };
        assert_eq!(read(), (0, 0));
        handle.repaint();
        assert_eq!(read(), (0, 1), "a repaint moves paint alone");
        handle.relayout();
        assert_eq!(read(), (1, 2), "a relayout moves both, because remeasured is repainted");
    }

    #[test]
    fn a_dropped_element_resolves_to_nothing() {
        zgui_reactive::install().ok();
        let (view, handle) = custom(Probe);
        // Find the token back out of the slot the only way a frame would: through the registry.
        let token = REGISTRY.with(|registry| {
            registry
                .borrow()
                .iter()
                .find(|(_, held)| {
                    held.upgrade()
                        .is_some_and(|slot| Rc::ptr_eq(&slot, &handle.slot))
                })
                .map(|(token, _)| *token)
                .expect("just registered")
        });
        assert!(resolve(token).is_some());
        drop(view);
        drop(handle);
        assert!(
            resolve(token).is_none(),
            "an element whose view and handle are gone draws nothing rather than dangling"
        );
    }
}
