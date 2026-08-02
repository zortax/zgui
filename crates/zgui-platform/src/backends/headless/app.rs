//! An application that records what it was told, which is what a scripted test needs.

use std::time::Instant;

use crate::app::{AppHandler, IdlePolicy, WakeReason};
use crate::cx::PlatformCx;
use crate::surface::{SurfaceAttributes, SurfaceEvent, SurfaceId};

/// An application that records what it was told.
#[derive(Default)]
pub(super) struct RecordingApp {
    /// How many times surfaces became available.
    pub(super) surfaces_available: u32,
    /// Every surface event, rendered as text.
    pub(super) events: Vec<String>,
    /// How many wakes arrived.
    pub(super) wakes: u32,
    /// How many deadlines were reported reached.
    pub(super) deadlines_reached: u32,
    /// The deadline this application currently wants the loop to park until.
    pub(super) park_until: Option<Instant>,
}

impl AppHandler for RecordingApp {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.surfaces_available += 1;
        cx.create_surface(&SurfaceAttributes::new("headless"))
            .expect("a headless surface is always creatable");
    }

    fn surface_event(&mut self, _cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        self.events.push(format!("{surface:?} {event:?}"));
    }

    fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {
        self.wakes += 1;
    }

    fn idle(&mut self, _cx: &dyn PlatformCx) -> IdlePolicy {
        self.park_until
            .map_or(IdlePolicy::Block, IdlePolicy::BlockUntil)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.deadlines_reached += 1;
        self.park_until = None;
        for surface in cx.surfaces() {
            surface.request_redraw();
        }
    }
}
