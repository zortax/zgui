//! iOS services winit does not provide: status bar style and the software keyboard's height.
//!
//! winit reports no keyboard frame, so a `UIKeyboardWillChangeFrameNotification` observer
//! measures how far the keyboard overlaps the screen and asks the loop to resend insets.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, Once, Weak};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, msg_send};
use objc2_core_foundation::CGRect;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue, NSValue};
use objc2_ui_kit::{
    UIKeyboardFrameEndUserInfoKey, UIKeyboardWillChangeFrameNotification, UIScreen,
};
use winit::event_loop::EventLoopProxy;
use winit::platform::ios::{StatusBarStyle, WindowExtIOS};
use winit::window::Window;

use crate::waker::UserEvent;

/// Every surface's window, for calls made from outside the loop.
static WINDOWS: Mutex<Vec<Weak<Window>>> = Mutex::new(Vec::new());

/// Height of the keyboard over the bottom of the screen, in points; zero while hidden.
static KEYBOARD_POINTS: AtomicU32 = AtomicU32::new(0);

/// Called for each new surface: remembers its window and starts watching the keyboard.
pub(crate) fn attach(window: &Arc<Window>, proxy: &EventLoopProxy<UserEvent>) {
    if let Ok(mut windows) = WINDOWS.lock() {
        windows.retain(|window| window.strong_count() > 0);
        windows.push(Arc::downgrade(window));
    }
    static OBSERVE: Once = Once::new();
    let proxy = proxy.clone();
    OBSERVE.call_once(move || observe_keyboard(proxy));
}

/// Keyboard height in physical pixels for a window at `scale`.
pub(crate) fn keyboard_px(scale: f64) -> f32 {
    (f64::from(KEYBOARD_POINTS.load(Ordering::Relaxed)) * scale) as f32
}

fn observe_keyboard(proxy: EventLoopProxy<UserEvent>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let changed = RcBlock::new(move |note: std::ptr::NonNull<NSNotification>| {
        // SAFETY: UIKit passes a live notification for the duration of the block.
        let note = unsafe { note.as_ref() };
        let Some(info) = note.userInfo() else { return };
        // SAFETY: the key is a UIKit constant; its value is an NSValue holding a CGRect.
        let Some(value) = (unsafe { info.objectForKey(UIKeyboardFrameEndUserInfoKey) }) else {
            return;
        };
        let Ok(value) = value.downcast::<NSValue>() else {
            return;
        };
        // SAFETY: `CGRectValue` is UIKit's accessor for rect-valued NSValues.
        let frame: CGRect = unsafe { msg_send![&*value, CGRectValue] };
        let screen = UIScreen::mainScreen(mtm).bounds();
        let overlap = (screen.size.height - frame.origin.y).clamp(0.0, screen.size.height);
        KEYBOARD_POINTS.store(overlap.round() as u32, Ordering::Relaxed);
        let _ = proxy.send_event(UserEvent::Insets);
    });
    // SAFETY: a nil object observes the notification from any sender; the main queue runs the
    // block on the main thread, where `mtm` is valid.
    let token: Retained<_> = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(UIKeyboardWillChangeFrameNotification),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &changed,
        )
    };
    // Observed for the life of the process.
    std::mem::forget(token);
}

/// Light status bar text over a dark interface, dark text over a light one.
pub fn status_bar_dark(dark: bool) {
    let style = if dark {
        StatusBarStyle::LightContent
    } else {
        StatusBarStyle::DarkContent
    };
    let Ok(windows) = WINDOWS.lock() else { return };
    for window in windows.iter().filter_map(Weak::upgrade) {
        window.set_preferred_status_bar_style(style);
    }
}
