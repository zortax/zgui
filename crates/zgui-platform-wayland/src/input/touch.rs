//! A finger, as the same stream a mouse produces.

use std::collections::HashMap;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceId;

/// Where each contact is, and which surface it belongs to.
///
/// A contact's later events do not name the surface it started on, and an ending contact does not
/// name a position at all — so both are remembered from the event that did. Without the first, a
/// drag that leaves the surface it started on is delivered to the wrong window; without the
/// second, every release lands at the origin.
#[derive(Debug, Default)]
pub struct Contacts {
    /// Where each live contact is and which surface it belongs to.
    live: HashMap<i32, (SurfaceId, Point<CssPx, Css>)>,
}

impl Contacts {
    /// Records a contact starting on `surface` at `at`.
    pub fn down(&mut self, id: i32, surface: SurfaceId, at: Point<CssPx, Css>) {
        self.live.insert(id, (surface, at));
    }

    /// Records a contact moving, answering with the surface it belongs to.
    pub fn moved(&mut self, id: i32, at: Point<CssPx, Css>) -> Option<SurfaceId> {
        let held = self.live.get_mut(&id)?;
        held.1 = at;
        Some(held.0)
    }

    /// Takes a contact that has ended, with where it was.
    pub fn up(&mut self, id: i32) -> Option<(SurfaceId, Point<CssPx, Css>)> {
        self.live.remove(&id)
    }

    /// Takes every live contact, for a gesture the compositor cancelled.
    pub fn cancel(&mut self) -> Vec<(i32, SurfaceId, Point<CssPx, Css>)> {
        self.live
            .drain()
            .map(|(id, (surface, at))| (id, surface, at))
            .collect()
    }

    /// How many contacts are live.
    pub fn len(&self) -> usize {
        self.live.len()
    }

    /// Whether nothing is touching.
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::Contacts;
    use zgui_geom::{CssPx, Point};
    use zgui_platform::SurfaceId;

    fn at(x: f32, y: f32) -> Point<CssPx, zgui_geom::Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    #[test]
    fn a_contact_that_never_started_moves_nowhere() {
        let mut contacts = Contacts::default();
        assert_eq!(contacts.moved(0, at(1.0, 1.0)), None);
        assert_eq!(contacts.up(0), None);
    }

    #[test]
    fn a_contact_carries_the_surface_it_started_on_for_its_whole_life() {
        // A drag that leaves the window it started in is still that window's drag.
        let mut contacts = Contacts::default();
        let surface = SurfaceId::new(4);
        contacts.down(0, surface, at(10.0, 10.0));
        assert_eq!(contacts.moved(0, at(-50.0, 900.0)), Some(surface));
        assert_eq!(contacts.up(0), Some((surface, at(-50.0, 900.0))));
    }

    #[test]
    fn a_release_lands_where_the_finger_last_was() {
        // The protocol's up event carries no position at all.
        let mut contacts = Contacts::default();
        contacts.down(1, SurfaceId::new(1), at(3.0, 4.0));
        contacts.moved(1, at(7.0, 8.0));
        assert_eq!(contacts.up(1).map(|held| held.1), Some(at(7.0, 8.0)));
    }

    #[test]
    fn two_fingers_are_tracked_apart() {
        let mut contacts = Contacts::default();
        contacts.down(0, SurfaceId::new(1), at(0.0, 0.0));
        contacts.down(1, SurfaceId::new(1), at(100.0, 0.0));
        assert_eq!(contacts.len(), 2);
        contacts.up(0);
        assert_eq!(contacts.len(), 1);
        assert!(!contacts.is_empty());
    }

    #[test]
    fn a_cancelled_gesture_releases_every_finger_that_was_down() {
        // Anything left behind is a control that stays pressed for ever.
        let mut contacts = Contacts::default();
        contacts.down(0, SurfaceId::new(1), at(0.0, 0.0));
        contacts.down(1, SurfaceId::new(2), at(1.0, 1.0));
        let cancelled = contacts.cancel();
        assert_eq!(cancelled.len(), 2);
        assert!(contacts.is_empty());
    }
}
