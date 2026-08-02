//! Watching one of a node's measurements, over a real document.
//!
//! Two things have to stay in step: the channels this backend holds, and the per-node mask the
//! document keeps so that the frame can ask "is anything watching this?" of every node that moved
//! and get the answer for free. A mask left set is a node the frame measures for nobody; a mask
//! left clear is a view that is never told anything again.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_dom::side::observed::ObservedMask;
use zgui_elements::r#box;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_view::{Anchor, IntoView, NodeId, Observed, ObservedValue, View};

use crate::support::Window;

/// A rectangle to deliver, distinct per call so a test can tell deliveries apart.
fn box_at(x: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(0.0)),
        Size::new(DevicePx(4.0), DevicePx(4.0)),
    )
}

/// The mask the document is holding for `node`.
fn mask(window: &Window, node: NodeId) -> ObservedMask {
    let index = window.backend.index_of(node);
    let document = window.document.borrow();
    let key = document.store().key_of(index);
    document
        .store()
        .columns()
        .observed
        .get(key)
        .map(|slots| slots.mask)
        .unwrap_or_default()
}

#[test]
fn a_registration_writes_the_documents_mask_and_dropping_the_handle_clears_it() {
    let window = Window::open();
    let mut built = window
        .window
        .with(|| r#box().into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    let node = built.node();
    assert_eq!(mask(&window, node), ObservedMask::empty());

    let seen: Rc<RefCell<Vec<ObservedValue>>> = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&seen);
    let handle = window.dom.observe(
        node,
        Observed::BorderBox,
        Rc::new(move |value| record.borrow_mut().push(value)),
    );
    assert_eq!(window.backend.observation_count(), 1);
    assert_eq!(
        mask(&window, node),
        ObservedMask::BORDER_BOX,
        "the document has to be able to answer `is anything watching this` for free"
    );

    window
        .backend
        .deliver(node, ObservedValue::BorderBox(box_at(1.0)));
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0].as_border_box(), Some(box_at(1.0)));

    drop(handle);
    assert_eq!(window.backend.observation_count(), 0);
    assert_eq!(mask(&window, node), ObservedMask::empty());

    window
        .backend
        .deliver(node, ObservedValue::BorderBox(box_at(2.0)));
    assert_eq!(
        seen.borrow().len(),
        1,
        "a handle that was dropped is still being delivered to"
    );

    built.unmount(&window.dom);
    window.window.unmount();
}

/// Two watchers of one measurement share the document's mask but not each other's handle: the
/// first to go must not take the second's deliveries with it.
#[test]
fn dropping_one_of_two_watchers_leaves_the_other_watching() {
    let window = Window::open();
    let mut built = window
        .window
        .with(|| r#box().into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    let node = built.node();

    let first = Rc::new(RefCell::new(0usize));
    let second = Rc::new(RefCell::new(0usize));
    let count = |slot: &Rc<RefCell<usize>>| {
        let slot = Rc::clone(slot);
        Rc::new(move |_: ObservedValue| *slot.borrow_mut() += 1) as zgui_view::ObservationSink
    };
    let one = window.dom.observe(node, Observed::BorderBox, count(&first));
    let two = window
        .dom
        .observe(node, Observed::BorderBox, count(&second));
    assert_eq!(
        window.backend.observation_count(),
        1,
        "two watchers of one measurement are one entry"
    );

    drop(one);
    assert_eq!(
        mask(&window, node),
        ObservedMask::BORDER_BOX,
        "the last watcher has not gone, so the mask has not either"
    );
    window
        .backend
        .deliver(node, ObservedValue::BorderBox(box_at(1.0)));
    assert_eq!(*first.borrow(), 0);
    assert_eq!(
        *second.borrow(),
        1,
        "the survivor stopped being delivered to"
    );

    drop(two);
    assert_eq!(mask(&window, node), ObservedMask::empty());
    assert_eq!(window.backend.observation_count(), 0);

    built.unmount(&window.dom);
    window.window.unmount();
}

/// A watcher whose node has gone is forgotten by the end of the frame, exactly as a listener's
/// handler is. Its handle outliving the node is ordinary — the view holding it is being dropped —
/// and dropping it afterwards must not resurrect anything.
#[test]
fn an_observation_of_a_node_that_has_gone_is_dropped_when_the_frame_ends() {
    let window = Window::open();
    let mut built = window
        .window
        .with(|| r#box().into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    let node = built.node();

    let handle = window
        .dom
        .observe(node, Observed::ContentSize, Rc::new(|_| {}));
    assert_eq!(window.backend.observation_count(), 1);

    built.unmount(&window.dom);
    drop(built);
    window.backend.end_frame();
    assert_eq!(
        window.backend.observation_count(),
        0,
        "the backend is watching a node the document no longer has"
    );

    drop(handle);
    assert_eq!(window.backend.observation_count(), 0);
    window.window.unmount();
}
