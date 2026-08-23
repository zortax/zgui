//! Which button was used.

use zgui_vocab::PointerButton;

/// The button a kernel code names.
///
/// The numbers are the kernel's own, shared with every input device on this system, and only the
/// five a desktop agrees about are named. A mouse with eight buttons keeps the other three as
/// numbers rather than collapsing them into the primary one, because a mouse with eight buttons is
/// a mouse somebody bound all eight of.
pub const fn button(code: u32) -> PointerButton {
    match code {
        0x110 => PointerButton::Primary,
        0x111 => PointerButton::Secondary,
        0x112 => PointerButton::Middle,
        0x113 => PointerButton::Back,
        0x114 => PointerButton::Forward,
        other => PointerButton::Other(other as u16),
    }
}

#[cfg(test)]
mod tests {
    use super::button;
    use zgui_vocab::PointerButton;

    #[test]
    fn the_three_buttons_every_mouse_has_are_named() {
        assert_eq!(button(0x110), PointerButton::Primary);
        assert_eq!(button(0x111), PointerButton::Secondary);
        assert_eq!(button(0x112), PointerButton::Middle);
    }

    #[test]
    fn the_side_buttons_are_the_ones_a_browser_navigates_with() {
        assert_eq!(button(0x113), PointerButton::Back);
        assert_eq!(button(0x114), PointerButton::Forward);
    }

    #[test]
    fn a_button_nobody_named_keeps_its_number_rather_than_becoming_a_click() {
        // Collapsing it into the primary button would make a thumb button select text.
        assert_eq!(button(0x115), PointerButton::Other(0x115));
        assert_ne!(button(0x115), PointerButton::Primary);
    }
}
