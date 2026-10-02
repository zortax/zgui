//! The end of a change of full screen, which comes after the last resize the change makes.

use core::ptr::NonNull;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSWindowDidEnterFullScreenNotification, NSWindowDidExitFullScreenNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};
use zgui_platform::{SurfaceId, WakeReason, Waker};

use crate::macos::window::ns_window;

thread_local! {
    /// The observers of each surface, removed with it.
    static OBSERVERS: RefCell<HashMap<SurfaceId, Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>>> =
        RefCell::new(HashMap::new());
}

/// Tells `waker` when `window` has finished going into full screen or out of it.
pub(crate) fn observe(window: &winit::window::Window, surface: SurfaceId, waker: Arc<dyn Waker>) {
    let Some(ns_window) = ns_window(window) else {
        return;
    };
    let center = NSNotificationCenter::defaultCenter();
    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        waker.wake(WakeReason::SurfaceStateChanged(surface));
    });
    let object: &AnyObject = AsRef::<AnyObject>::as_ref(&*ns_window);
    // SAFETY: the names are AppKit's own, the object is the window they are posted for, and the
    // block only posts a wake, which any thread may do.
    let observers = unsafe {
        [
            NSWindowDidEnterFullScreenNotification,
            NSWindowDidExitFullScreenNotification,
        ]
        .map(|name| {
            center.addObserverForName_object_queue_usingBlock(
                Some(name),
                Some(object),
                None,
                &block,
            )
        })
    };
    OBSERVERS.with(|held| held.borrow_mut().insert(surface, observers.into()));
}

/// Stops telling anyone about `surface`.
pub(crate) fn forget(surface: SurfaceId) {
    let Some(observers) = OBSERVERS.with(|held| held.borrow_mut().remove(&surface)) else {
        return;
    };
    let center = NSNotificationCenter::defaultCenter();
    for observer in observers {
        // SAFETY: the observer is one the center handed out.
        unsafe { center.removeObserver(AsRef::<AnyObject>::as_ref(&*observer)) };
    }
}
