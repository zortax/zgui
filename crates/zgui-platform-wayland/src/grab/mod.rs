//! Hyprland's focus grab: the keyboard and the pointer, limited to a set of surfaces.
//!
//! The protocol description is vendored beside this file, unchanged, because no published crate
//! carries it. A press on any surface outside the set clears the grab, a surface of this
//! application included, and the compositor says so with `cleared`.

use std::cell::{Cell, RefCell};

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_client::protocol::wl_surface::WlSurface;
use zgui_platform::{FocusGrabId, WakeReason, Waker as _};

use crate::driver::WaylandState;

/// The code generated from the vendored protocol description.
#[allow(
    missing_docs,
    unreachable_pub,
    dead_code,
    unused_imports,
    non_upper_case_globals,
    non_camel_case_types,
    clippy::all
)]
pub mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;

    /// The interface descriptions the generated code is built on.
    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("src/grab/hyprland-focus-grab-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("src/grab/hyprland-focus-grab-v1.xml");
}

use crate::grab::protocol::hyprland_focus_grab_manager_v1::HyprlandFocusGrabManagerV1;
use crate::grab::protocol::hyprland_focus_grab_v1::{self, HyprlandFocusGrabV1};

/// Every focus grab this application has made, and the global they are made from.
///
/// Behind cells, because the borrowed context makes and changes grabs through `&self`.
#[derive(Debug, Default)]
pub struct Grabs {
    /// `hyprland_focus_grab_manager_v1`, where the compositor offers it.
    manager: Option<HyprlandFocusGrabManagerV1>,
    /// The grabs that exist, with the surfaces each was last committed with.
    held: RefCell<Vec<Held>>,
    /// The number the next grab gets.
    next: Cell<u64>,
}

/// One grab, as the compositor knows it.
#[derive(Debug)]
struct Held {
    /// What the application calls it.
    id: FocusGrabId,
    /// The protocol object.
    grab: HyprlandFocusGrabV1,
    /// The surfaces the last commit left on the whitelist.
    committed: Vec<WlSurface>,
}

impl Grabs {
    /// Binds the manager where the compositor advertised it.
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<WaylandState>) -> Self {
        Self {
            manager: globals.bind(qh, 1..=1, GlobalData).ok(),
            held: RefCell::new(Vec::new()),
            next: Cell::new(1),
        }
    }

    /// Whether this compositor can limit focus to a set of surfaces.
    pub const fn offered(&self) -> bool {
        self.manager.is_some()
    }

    /// Makes a grab with an empty whitelist, or nothing where the compositor offers none.
    pub fn create(&self, qh: &QueueHandle<WaylandState>) -> Option<FocusGrabId> {
        let manager = self.manager.as_ref()?;
        let id = FocusGrabId::new(self.next.get());
        self.next.set(id.get() + 1);
        let grab = manager.create_grab(qh, id);
        self.held.borrow_mut().push(Held {
            id,
            grab,
            committed: Vec::new(),
        });
        Some(id)
    }

    /// Makes `wanted` the whitelist of grab `id`, and commits it when it changed.
    ///
    /// A surface already destroyed is left alone: the compositor removed it from the whitelist
    /// when it went, and a request naming it is a protocol error.
    pub fn set(&self, id: FocusGrabId, wanted: &[WlSurface]) {
        let mut held = self.held.borrow_mut();
        let Some(held) = held.iter_mut().find(|held| held.id == id) else {
            return;
        };
        let (gone, kept) = changes(&held.committed, wanted);
        for surface in &gone {
            if surface.is_alive() {
                held.grab.remove_surface(surface);
            }
        }
        let added: Vec<&WlSurface> = wanted
            .iter()
            .filter(|surface| !held.committed.contains(surface))
            .collect();
        for surface in &added {
            held.grab.add_surface(surface);
        }
        if !gone.is_empty() || !added.is_empty() {
            held.grab.commit();
        }
        held.committed = kept.into_iter().chain(added.into_iter().cloned()).collect();
    }

    /// Destroys grab `id`, which stops it.
    pub fn destroy(&self, id: FocusGrabId) {
        self.held.borrow_mut().retain(|held| {
            if held.id == id {
                held.grab.destroy();
                false
            } else {
                true
            }
        });
    }

    /// Records that the compositor cleared grab `id`, which empties its whitelist.
    fn cleared(&self, id: FocusGrabId) {
        if let Some(held) = self.held.borrow_mut().iter_mut().find(|held| held.id == id) {
            held.committed.clear();
        }
    }
}

/// Which of `committed` leave the whitelist for `wanted`, and which stay.
fn changes<T: PartialEq + Clone>(committed: &[T], wanted: &[T]) -> (Vec<T>, Vec<T>) {
    committed
        .iter()
        .cloned()
        .partition(|surface| !wanted.contains(surface))
}

impl Dispatch<HyprlandFocusGrabManagerV1, GlobalData> for WaylandState {
    fn event(
        _state: &mut Self,
        _manager: &HyprlandFocusGrabManagerV1,
        _event: <HyprlandFocusGrabManagerV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // The manager sends nothing.
    }
}

impl Dispatch<HyprlandFocusGrabV1, FocusGrabId> for WaylandState {
    fn event(
        state: &mut Self,
        _grab: &HyprlandFocusGrabV1,
        event: hyprland_focus_grab_v1::Event,
        id: &FocusGrabId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            hyprland_focus_grab_v1::Event::Cleared => {
                state.grabs.cleared(*id);
                state.live.waker.wake(WakeReason::FocusGrabCleared(*id));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::changes;

    #[test]
    fn only_the_surfaces_that_left_are_removed() {
        let (gone, kept) = changes(&[1, 2, 3], &[2, 3, 4]);
        assert_eq!(gone, [1]);
        assert_eq!(kept, [2, 3]);
    }

    #[test]
    fn an_empty_set_removes_everything() {
        let (gone, kept) = changes(&[1, 2], &[]);
        assert_eq!(gone, [1, 2]);
        assert!(kept.is_empty());
    }
}
