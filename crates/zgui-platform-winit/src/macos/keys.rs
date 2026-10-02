//! Which window answers a command chord first.
//!
//! AppKit offers a command chord to the menu before the focused view. The framework offers it to
//! the focused element first, as every other desktop does, so a text field or an editor keeps the
//! chords it answers. The menu then answers the chords of its actions that nothing claimed. The
//! chords of the desktop's own roles stay with AppKit.

use core::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use block2::RcBlock;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::{NSApplication, NSEvent, NSEventMask, NSEventModifierFlags, NSView};
use zgui_platform::Shortcut;
use zgui_vocab::{Key, Modifiers};

/// The chords AppKit keeps: the shortcuts of the desktop roles in the menu, and window cycling.
static NATIVE: Mutex<Vec<(String, Modifiers)>> = Mutex::new(Vec::new());

/// Whether the content view answers key equivalents, which it does once a menu is installed.
static CLAIMING: AtomicBool = AtomicBool::new(false);

/// Leaves `shortcuts` to AppKit, and gives every other chord to the focused window first.
pub(crate) fn keep_native(shortcuts: &[Shortcut]) {
    let mut native: Vec<(String, Modifiers)> = shortcuts
        .iter()
        .filter_map(|shortcut| match &shortcut.key {
            Key::Character(text) => Some((text.to_lowercase(), shortcut.modifiers)),
            _ => None,
        })
        .collect();
    // Window cycling has no menu entry, and AppKit answers it from the key equivalent.
    native.push(("`".to_owned(), Modifiers::META));
    native.push(("`".to_owned(), Modifiers::META | Modifiers::SHIFT));
    if let Ok(mut held) = NATIVE.lock() {
        *held = native;
    }
    CLAIMING.store(true, Ordering::Relaxed);
}

/// The modifiers AppKit reports, as the contract's set.
fn modifiers(flags: NSEventModifierFlags) -> Modifiers {
    Modifiers::NONE
        .with(
            Modifiers::SHIFT,
            flags.contains(NSEventModifierFlags::Shift),
        )
        .with(
            Modifiers::CONTROL,
            flags.contains(NSEventModifierFlags::Control),
        )
        .with(Modifiers::ALT, flags.contains(NSEventModifierFlags::Option))
        .with(
            Modifiers::META,
            flags.contains(NSEventModifierFlags::Command),
        )
}

/// Whether AppKit keeps the chord `key` under `held`.
fn is_native(key: &str, held: Modifiers) -> bool {
    let key = key.to_lowercase();
    NATIVE.lock().is_ok_and(|native| {
        native
            .iter()
            .any(|(own, modifiers)| *own == key && *modifiers == held)
    })
}

/// Takes a command chord to the focused view of the key window, when the framework draws it.
///
/// Reports whether the chord was taken, which keeps it from the menu.
fn route(event: &NSEvent) -> bool {
    if !CLAIMING.load(Ordering::Relaxed) {
        return false;
    }
    let held = modifiers(event.modifierFlags());
    if !held.meta() {
        return false;
    }
    let characters = event
        .charactersIgnoringModifiers()
        .map(|text| text.to_string())
        .unwrap_or_default();
    if is_native(&characters, held) {
        return false;
    }
    let Some(main) = MainThreadMarker::new() else {
        return false;
    };
    let Some(window) = NSApplication::sharedApplication(main).keyWindow() else {
        return false;
    };
    let Some(view) = window.contentView() else {
        return false;
    };
    if !is_framework_view(&view) {
        return false;
    }
    // SAFETY: the view is winit's content view, which answers `keyDown:` with an event.
    let _: () = unsafe { msg_send![&*view, keyDown: event] };
    true
}

/// Whether `view` is a view this backend draws into.
///
/// The accessibility adapter gives the view a subclass of its own, so the class is looked for
/// among the view's superclasses as well.
fn is_framework_view(view: &NSView) -> bool {
    let Some(name) = VIEW_CLASS.get() else {
        return false;
    };
    let mut class = Some(AsRef::<AnyObject>::as_ref(view).class());
    while let Some(held) = class {
        if held.name().to_bytes() == name.as_bytes() {
            return true;
        }
        class = held.superclass();
    }
    false
}

/// The class of the content views this backend draws into.
static VIEW_CLASS: OnceLock<String> = OnceLock::new();

/// Starts routing command chords to the focused window, once.
///
/// The monitor runs as AppKit receives a key press, before the menu sees it.
pub(crate) fn install(view: &NSView) {
    let name = AsRef::<AnyObject>::as_ref(view)
        .class()
        .name()
        .to_string_lossy()
        .into_owned();
    if VIEW_CLASS.set(name).is_err() {
        return;
    }
    let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a live event for the length of the call.
        let taken = route(unsafe { event.as_ref() });
        if taken {
            core::ptr::null_mut()
        } else {
            event.as_ptr()
        }
    });
    // SAFETY: the handler returns the event it was given or null, as the monitor requires.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &handler)
    };
    // The monitor stands for the life of the application.
    core::mem::forget(monitor);
}

#[cfg(test)]
mod tests {
    use super::{is_native, keep_native};
    use zgui_platform::{MenuRole, Shortcut};
    use zgui_vocab::Modifiers;

    #[test]
    fn the_roles_of_the_desktop_and_window_cycling_stay_with_appkit() {
        let shortcuts: Vec<Shortcut> = [MenuRole::Hide, MenuRole::Minimize]
            .into_iter()
            .filter_map(MenuRole::shortcut)
            .collect();
        keep_native(&shortcuts);
        assert!(is_native("h", Modifiers::META));
        assert!(is_native("M", Modifiers::META));
        assert!(is_native("`", Modifiers::META));
        assert!(!is_native("k", Modifiers::META));
        assert!(!is_native("h", Modifiers::META | Modifiers::SHIFT));
    }
}
