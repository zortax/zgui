//! Which window answers a key equivalent first.
//!
//! AppKit offers a command chord to the menu before the focused view. The framework offers it to
//! the focused element first, as every other desktop does, so a text field or an editor keeps the
//! chords it answers. The menu then answers the chords of its actions that nothing claimed. The
//! chords of the desktop's own roles stay with AppKit.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::encode::{Encode, Encoding};
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{ffi, msg_send, sel};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSView};
use zgui_platform::Shortcut;
use zgui_vocab::{Key, Modifiers};

/// The chords AppKit keeps: the shortcuts of the desktop roles in the menu, and window cycling.
static NATIVE: Mutex<Vec<(String, Modifiers)>> = Mutex::new(Vec::new());

/// Whether the content view answers key equivalents, which it does once a menu is installed.
static CLAIMING: AtomicBool = AtomicBool::new(false);

/// Whether the method is on the content view's class.
static INSTALLED: AtomicBool = AtomicBool::new(false);

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

/// Answers `performKeyEquivalent:` on the content view.
///
/// The view takes the chord as an ordinary key press, so the framework dispatches it to the
/// focused element. A view whose window is not the key window takes nothing.
extern "C-unwind" fn perform_key_equivalent(
    view: &NSView,
    _selector: Sel,
    event: &NSEvent,
) -> Bool {
    if !CLAIMING.load(Ordering::Relaxed) {
        return Bool::NO;
    }
    let key = view.window().is_some_and(|window| window.isKeyWindow());
    if !key {
        return Bool::NO;
    }
    let held = modifiers(event.modifierFlags());
    let characters = event
        .charactersIgnoringModifiers()
        .map(|text| text.to_string())
        .unwrap_or_default();
    if is_native(&characters, held) {
        return Bool::NO;
    }
    // SAFETY: the view is winit's content view, which answers `keyDown:` with an event.
    let _: () = unsafe { msg_send![view, keyDown: event] };
    Bool::YES
}

/// Adds the method to the class of `view`, once.
pub(crate) fn install(view: &NSView) {
    if INSTALLED.swap(true, Ordering::Relaxed) {
        return;
    }
    let class: &AnyClass = AsRef::<AnyObject>::as_ref(view).class();
    let function: extern "C-unwind" fn(&NSView, Sel, &NSEvent) -> Bool = perform_key_equivalent;
    let types = std::ffi::CString::new(format!(
        "{}{}{}{}",
        Bool::ENCODING,
        Encoding::Object,
        Encoding::Sel,
        Encoding::Object,
    ))
    .expect("an encoding has no interior nul");
    // SAFETY: the function has the signature `performKeyEquivalent:` has: an object, a selector
    // and an event in, a boolean out. The type string says the same.
    unsafe {
        let imp: Imp = core::mem::transmute(function);
        ffi::class_addMethod(
            core::ptr::from_ref(class).cast_mut(),
            sel!(performKeyEquivalent:),
            imp,
            types.as_ptr(),
        );
    }
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
