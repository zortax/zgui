//! Which position on the keyboard was pressed, independent of what is printed on it.

use core::str::FromStr;

use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey as WinitPhysical};
use zgui_vocab::PhysicalKey;

/// The bit that separates a position this vocabulary has no name for from a real scan code.
///
/// Native scan codes are small — a few hundred on every keyboard protocol in use — so nothing
/// above this can collide with one, and a position carried this way stays bindable.
const SURROGATE: u32 = 0x8000_0000;

/// Where a key sits, in the standard vocabulary.
pub(crate) fn physical(key: WinitPhysical) -> PhysicalKey {
    match key {
        WinitPhysical::Code(code) => position(code),
        WinitPhysical::Unidentified(native) => PhysicalKey::Unidentified(native_number(native)),
    }
}

/// A named position, or a stable number when this vocabulary has no name for it.
///
/// Both vocabularies name positions after the standard code strings, so the correspondence is the
/// string rather than a hand-written table of two hundred rows — a table that would be wrong in
/// exactly one place and silently. A position the standard set does not hold is carried as a
/// number derived from that same string, so a keyboard with an extra key can still have something
/// bound to it even though nothing can say what it is.
fn position(code: KeyCode) -> PhysicalKey {
    let standard = standard_name(code);
    zgui_vocab::KeyCode::from_str(&standard).map_or_else(
        |_| PhysicalKey::Unidentified(surrogate(&standard)),
        PhysicalKey::Code,
    )
}

/// The standard code string for a position.
///
/// Two positions are named differently by the two vocabularies and are corrected here: the
/// platform calls the key beside the space bar *super*, and the standard calls it *meta*. Every
/// other position shares its name, which is what makes the correspondence checkable rather than
/// transcribed.
fn standard_name(code: KeyCode) -> String {
    match code {
        KeyCode::SuperLeft => "MetaLeft".to_owned(),
        KeyCode::SuperRight => "MetaRight".to_owned(),
        other => format!("{other:?}"),
    }
}

/// A stable number for a position with no standard name.
fn surrogate(name: &str) -> u32 {
    // FNV-1a, chosen because it is four lines and deterministic across runs and machines, which is
    // all a binding needs from it.
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash | SURROGATE
}

/// The platform's own number for a position it could not name either.
const fn native_number(native: NativeKeyCode) -> u32 {
    match native {
        NativeKeyCode::Xkb(code) | NativeKeyCode::Android(code) => code,
        NativeKeyCode::Windows(code) | NativeKeyCode::MacOS(code) => code as u32,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{SURROGATE, physical, position, surrogate};
    use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey as WinitPhysical};
    use zgui_vocab::PhysicalKey;

    /// Positions whose correspondence is worth pinning: the ones a shortcut is bound to, the ones
    /// a text field reads, and the two the two vocabularies disagree about.
    const CORRESPONDENCE: &[(KeyCode, zgui_vocab::KeyCode)] = &[
        (KeyCode::KeyA, zgui_vocab::KeyCode::KeyA),
        (KeyCode::KeyZ, zgui_vocab::KeyCode::KeyZ),
        (KeyCode::Digit0, zgui_vocab::KeyCode::Digit0),
        (KeyCode::Enter, zgui_vocab::KeyCode::Enter),
        (KeyCode::NumpadEnter, zgui_vocab::KeyCode::NumpadEnter),
        (KeyCode::Escape, zgui_vocab::KeyCode::Escape),
        (KeyCode::Tab, zgui_vocab::KeyCode::Tab),
        (KeyCode::Space, zgui_vocab::KeyCode::Space),
        (KeyCode::Backspace, zgui_vocab::KeyCode::Backspace),
        (KeyCode::Delete, zgui_vocab::KeyCode::Delete),
        (KeyCode::ArrowUp, zgui_vocab::KeyCode::ArrowUp),
        (KeyCode::ArrowDown, zgui_vocab::KeyCode::ArrowDown),
        (KeyCode::ArrowLeft, zgui_vocab::KeyCode::ArrowLeft),
        (KeyCode::ArrowRight, zgui_vocab::KeyCode::ArrowRight),
        (KeyCode::Home, zgui_vocab::KeyCode::Home),
        (KeyCode::End, zgui_vocab::KeyCode::End),
        (KeyCode::PageUp, zgui_vocab::KeyCode::PageUp),
        (KeyCode::PageDown, zgui_vocab::KeyCode::PageDown),
        (KeyCode::ShiftLeft, zgui_vocab::KeyCode::ShiftLeft),
        (KeyCode::ShiftRight, zgui_vocab::KeyCode::ShiftRight),
        (KeyCode::ControlLeft, zgui_vocab::KeyCode::ControlLeft),
        (KeyCode::AltLeft, zgui_vocab::KeyCode::AltLeft),
        (KeyCode::AltRight, zgui_vocab::KeyCode::AltRight),
        (KeyCode::F1, zgui_vocab::KeyCode::F1),
        (KeyCode::F12, zgui_vocab::KeyCode::F12),
        (KeyCode::IntlBackslash, zgui_vocab::KeyCode::IntlBackslash),
        (KeyCode::Backquote, zgui_vocab::KeyCode::Backquote),
        // The two the platform and the standard name differently.
        (KeyCode::SuperLeft, zgui_vocab::KeyCode::MetaLeft),
        (KeyCode::SuperRight, zgui_vocab::KeyCode::MetaRight),
    ];

    #[test]
    fn every_position_a_shortcut_is_bound_to_survives_the_crossing() {
        for (platform, standard) in CORRESPONDENCE {
            assert_eq!(
                position(*platform),
                PhysicalKey::Code(*standard),
                "{platform:?} did not cross to {standard:?}"
            );
        }
    }

    #[test]
    fn the_command_key_is_meta_here_whatever_the_platform_calls_it() {
        // This is the one correspondence a string comparison gets wrong on its own, so it is
        // asserted rather than trusted: a shortcut bound to the command key would otherwise
        // silently stop matching.
        assert_ne!(position(KeyCode::SuperLeft), position(KeyCode::SuperRight));
        assert_eq!(
            position(KeyCode::SuperLeft),
            PhysicalKey::Code(zgui_vocab::KeyCode::MetaLeft)
        );
    }

    #[test]
    fn a_position_with_no_standard_name_is_carried_rather_than_dropped() {
        // Positions the platform names and the standard set does not. Each has to keep its own
        // identity, or a binding to one of them fires for all of them.
        let unnamed = [
            KeyCode::Lang1,
            KeyCode::Lang2,
            KeyCode::NumpadStar,
            KeyCode::NumpadParenLeft,
            KeyCode::BrowserFavorites,
            KeyCode::Power,
            KeyCode::F25,
        ];
        let mut seen = Vec::new();
        for code in unnamed {
            let PhysicalKey::Unidentified(number) = position(code) else {
                panic!("{code:?} is claimed to have a standard name");
            };
            assert!(
                number & SURROGATE != 0,
                "{code:?} was numbered where a real scan code could collide with it"
            );
            assert!(
                !seen.contains(&number),
                "{code:?} collided with another key"
            );
            seen.push(number);
        }
    }

    #[test]
    fn a_surrogate_number_can_never_be_a_scan_code() {
        // Real scan codes are small; the surrogate range starts above everything a keyboard
        // protocol in use can report, so the two can share one field without ambiguity.
        assert!(surrogate("Lang1") > u32::from(u16::MAX));
        assert_eq!(
            physical(WinitPhysical::Unidentified(NativeKeyCode::Xkb(49))),
            PhysicalKey::Unidentified(49)
        );
    }

    #[test]
    fn the_platforms_own_number_is_kept_whichever_protocol_reported_it() {
        assert_eq!(
            physical(WinitPhysical::Unidentified(NativeKeyCode::Windows(41))),
            PhysicalKey::Unidentified(41)
        );
        assert_eq!(
            physical(WinitPhysical::Unidentified(NativeKeyCode::Unidentified)),
            PhysicalKey::Unidentified(0)
        );
    }
}
