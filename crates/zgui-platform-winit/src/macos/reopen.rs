//! The dock's request to show the application again.

use std::ffi::CString;
use std::sync::{Arc, OnceLock};

use objc2::encode::{Encode, Encoding};
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{MainThreadMarker, ffi, sel};
use objc2_app_kit::NSApplication;
use zgui_platform::{AppEvent, WakeReason, Waker};

/// Where a reopen request goes.
static WAKER: OnceLock<Arc<dyn Waker>> = OnceLock::new();

/// Answers `applicationShouldHandleReopen:hasVisibleWindows:`.
///
/// AppKit still restores a minimized window. The application shows a hidden one.
extern "C-unwind" fn should_handle_reopen(
    _delegate: &AnyObject,
    _selector: Sel,
    _application: &AnyObject,
    _visible: Bool,
) -> Bool {
    if let Some(waker) = WAKER.get() {
        waker.wake(WakeReason::App(AppEvent::Reopen));
    }
    Bool::YES
}

/// Teaches winit's application delegate to pass reopen requests to `waker`.
///
/// winit's delegate answers only the launch and the termination, so the method is added to its
/// class. Installing twice does nothing.
pub(crate) fn install(waker: Arc<dyn Waker>) {
    if WAKER.set(waker).is_err() {
        return;
    }
    let Some(main) = MainThreadMarker::new() else {
        return;
    };
    let Some(delegate) = NSApplication::sharedApplication(main).delegate() else {
        return;
    };
    let class: &AnyClass = AsRef::<AnyObject>::as_ref(&*delegate).class();
    let types = CString::new(format!(
        "{}{}{}{}{}",
        Bool::ENCODING,
        Encoding::Object,
        Encoding::Sel,
        Encoding::Object,
        Bool::ENCODING,
    ))
    .expect("an encoding has no interior nul");
    let function: extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, Bool) -> Bool =
        should_handle_reopen;
    // SAFETY: the function has the signature the selector's type encoding states, and adding a
    // method to a registered class is what `class_addMethod` exists for. A class that already
    // answers the selector keeps its own method.
    unsafe {
        let imp: Imp = core::mem::transmute(function);
        ffi::class_addMethod(
            core::ptr::from_ref(class).cast_mut(),
            sel!(applicationShouldHandleReopen:hasVisibleWindows:),
            imp,
            types.as_ptr(),
        );
    }
}
