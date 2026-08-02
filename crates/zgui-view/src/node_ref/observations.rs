//! Sharing one registration between every view that asked for it.

use core::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use zgui_reactive::ArcRwSignal;
use zgui_reactive::prelude::*;

use crate::dom::{DomHandle, ObservationHandle, Observed, ObservedValue};
use crate::id::NodeId;

/// One shared registration.
struct Entry {
    /// How many callers are holding it.
    count: usize,
    /// The last value delivered, shared by every caller.
    ///
    /// Reference-counted rather than arena-backed, so it belongs to no reactive scope and cannot
    /// be disposed of by whichever caller happens to unmount first.
    value: ArcRwSignal<Option<ObservedValue>>,
    /// The backend registration, dropped when the last caller lets go.
    _handle: ObservationHandle,
}

/// The registrations one window's views share.
///
/// Two views observing the same quantity of the same node cost the backend one registration, not
/// two — and a popover that follows an ancestor's scroll offset while a virtualised list follows
/// the same one is the ordinary case, not an exotic one.
///
/// **The shared thing is the registration; the signal each caller is handed is its own.** Handing
/// back one signal would attach that signal's arena entry to whichever scope called first, and the
/// first of those callers to unmount would dispose of a signal every other one is still reading.
/// So the value behind the registration is reference-counted and each caller gets a derived view
/// of it, which dies with that caller and with nobody else.
#[derive(Default)]
pub struct ObservationRegistry {
    /// The live registrations.
    entries: RefCell<BTreeMap<(NodeId, Observed), Entry>>,
}

impl ObservationRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many registrations are live.
    ///
    /// Zero when nothing is being observed, which is what lets a frame skip its whole geometry
    /// delivery pass rather than walking an empty table.
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    /// Whether nothing is being observed.
    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    /// Takes a share in the registration for `(node, what)`, registering it if it is new.
    pub fn acquire(
        self: &Rc<Self>,
        dom: &DomHandle,
        node: NodeId,
        what: Observed,
    ) -> ArcRwSignal<Option<ObservedValue>> {
        let key = (node, what);
        if let Some(entry) = self.entries.borrow_mut().get_mut(&key) {
            entry.count += 1;
            return entry.value.clone();
        }

        let value = ArcRwSignal::new(None);
        let sink = {
            let value = value.clone();
            Rc::new(move |delivered: ObservedValue| value.set(Some(delivered)))
        };
        let handle = dom.observe(node, what, sink);
        self.entries.borrow_mut().insert(
            key,
            Entry {
                count: 1,
                value: value.clone(),
                _handle: handle,
            },
        );
        value
    }

    /// Gives up one share, deregistering when the last one goes.
    pub fn release(&self, node: NodeId, what: Observed) {
        let mut entries = self.entries.borrow_mut();
        let key = (node, what);
        let Some(entry) = entries.get_mut(&key) else {
            return;
        };
        entry.count -= 1;
        if entry.count == 0 {
            entries.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use zgui_geom::{DevicePx, Point, Rect, Size};
    use zgui_reactive::prelude::*;

    use super::ObservationRegistry;
    use crate::DocumentId;
    use crate::dom::{DomHandle, Observed, ObservedValue};
    use crate::stub::StubDom;
    use zgui_interned::ElementName;

    fn bounds(width: f32) -> ObservedValue {
        ObservedValue::BorderBox(Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(width), DevicePx(10.0)),
        ))
    }

    #[test]
    fn two_observers_of_one_node_cost_one_registration_and_both_see_the_value() {
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let node = dom.create_element(ElementName::new("box"));
        let registry = Rc::new(ObservationRegistry::new());

        let first = registry.acquire(&dom, node, Observed::BorderBox);
        let second = registry.acquire(&dom, node, Observed::BorderBox);
        assert_eq!(backend.observation_count(), 1);
        assert_eq!(registry.len(), 1);

        backend.deliver(node, bounds(40.0));
        assert_eq!(first.get(), Some(bounds(40.0)));
        assert_eq!(second.get(), Some(bounds(40.0)));
    }

    #[test]
    fn the_registration_survives_the_first_release_and_goes_on_the_last() {
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let node = dom.create_element(ElementName::new("box"));
        let registry = Rc::new(ObservationRegistry::new());

        let _first = registry.acquire(&dom, node, Observed::BorderBox);
        let _second = registry.acquire(&dom, node, Observed::BorderBox);

        registry.release(node, Observed::BorderBox);
        assert_eq!(
            backend.observation_count(),
            1,
            "one caller is still holding it"
        );

        registry.release(node, Observed::BorderBox);
        assert_eq!(backend.observation_count(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn two_quantities_of_one_node_are_two_registrations() {
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let node = dom.create_element(ElementName::new("box"));
        let registry = Rc::new(ObservationRegistry::new());

        let _boxes = registry.acquire(&dom, node, Observed::BorderBox);
        let _scroll = registry.acquire(&dom, node, Observed::ScrollPosition);
        assert_eq!(backend.observation_count(), 2);
    }

    #[test]
    fn releasing_something_nobody_registered_does_nothing() {
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend);
        let node = dom.create_element(ElementName::new("box"));
        let registry = ObservationRegistry::new();
        registry.release(node, Observed::BorderBox);
        assert!(registry.is_empty());
    }
}
