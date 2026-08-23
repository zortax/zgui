//! What a key means once the layout has been applied to it.

use smithay_client_toolkit::seat::keyboard::Keysym;
use zgui_vocab::{Key, NamedKey};

/// The key a symbol names, in the standard vocabulary.
///
/// `text` is what the compositor says this press actually produces, and it is preferred over
/// anything derived from the symbol because the two differ exactly where it matters: a dead key
/// that could not be combined produces two characters, and only the text knows that.
///
/// A key that merely *has* a text representation is not turned into that text. The return key is
/// the case that decides the rule: a field that inserted a carriage return every time it was
/// pressed would look correct in every test written against key names and would be unusable. So a
/// named key stays named, and what a named key inserts is decided once, in the vocabulary.
pub fn key(symbol: Keysym, text: Option<&str>) -> Key {
    if let Some(named) = named(symbol) {
        return Key::Named(named);
    }
    if symbol.is_modifier_key() {
        // A modifier this table has no name for is still a modifier, and dispatching it as text
        // would type whatever the layout happens to put on it.
        return Key::Unidentified;
    }
    if let Some(text) = text.filter(|text| !text.is_empty() && !is_control(text)) {
        return Key::Character(text.into());
    }
    // The symbol's own character, for the press whose text the modifiers destroyed. Holding
    // control turns what a letter produces into a control character — control and D produce U+0004
    // — and the symbol is still D. Falling past this is what makes every shortcut on a letter
    // unreachable while ordinary typing works, because ordinary typing never takes this branch.
    if let Some(character) = symbol
        .key_char()
        .filter(|character| !character.is_control())
    {
        return Key::Character(character.to_string().into());
    }
    // A symbol with no character and no name is still a key, and its X11 name is what every
    // configuration file on this desktop calls it — so it is carried rather than dropped.
    name_of(symbol).map_or(Key::Unidentified, Key::Other)
}

/// Whether this text is a control character the layout produced rather than something to insert.
///
/// The return, tab, escape and backspace keys all produce one. They are named keys, and the name
/// is what is dispatched; the control character would be inserted into the document as itself.
fn is_control(text: &str) -> bool {
    text.chars().all(|character| character.is_control())
}

/// The X11 name of a symbol, which is what this desktop's configuration files call it.
fn name_of(symbol: Keysym) -> Option<zgui_vocab::SharedString> {
    symbol.name().map(Into::into)
}

/// The standard name of a symbol, for the symbols the standard names.
fn named(symbol: Keysym) -> Option<NamedKey> {
    Some(match symbol {
        Keysym::Shift_L | Keysym::Shift_R => NamedKey::Shift,
        Keysym::Control_L | Keysym::Control_R => NamedKey::Control,
        Keysym::Alt_L | Keysym::Alt_R | Keysym::Meta_L | Keysym::Meta_R => NamedKey::Alt,
        Keysym::ISO_Level3_Shift | Keysym::ISO_Level5_Shift => NamedKey::AltGraph,
        Keysym::Super_L | Keysym::Super_R => NamedKey::Meta,
        Keysym::Hyper_L | Keysym::Hyper_R => NamedKey::Hyper,
        Keysym::Caps_Lock => NamedKey::CapsLock,
        Keysym::Num_Lock => NamedKey::NumLock,
        Keysym::Scroll_Lock => NamedKey::ScrollLock,

        Keysym::Return | Keysym::KP_Enter | Keysym::ISO_Enter => NamedKey::Enter,
        Keysym::Tab | Keysym::ISO_Left_Tab | Keysym::KP_Tab => NamedKey::Tab,
        Keysym::space | Keysym::KP_Space => NamedKey::Space,

        Keysym::Down | Keysym::KP_Down => NamedKey::ArrowDown,
        Keysym::Left | Keysym::KP_Left => NamedKey::ArrowLeft,
        Keysym::Right | Keysym::KP_Right => NamedKey::ArrowRight,
        Keysym::Up | Keysym::KP_Up => NamedKey::ArrowUp,
        Keysym::End | Keysym::KP_End => NamedKey::End,
        Keysym::Home | Keysym::KP_Home | Keysym::KP_Begin | Keysym::Begin => NamedKey::Home,
        Keysym::Next | Keysym::KP_Next => NamedKey::PageDown,
        Keysym::Prior | Keysym::KP_Prior => NamedKey::PageUp,

        Keysym::BackSpace => NamedKey::Backspace,
        Keysym::Clear | Keysym::XF86_Clear => NamedKey::Clear,
        Keysym::XF86_Copy => NamedKey::Copy,
        Keysym::XF86_Cut => NamedKey::Cut,
        Keysym::XF86_Paste => NamedKey::Paste,
        Keysym::Delete | Keysym::KP_Delete => NamedKey::Delete,
        Keysym::Insert | Keysym::KP_Insert => NamedKey::Insert,
        Keysym::Undo => NamedKey::Undo,

        Keysym::Cancel => NamedKey::Cancel,
        Keysym::Menu | Keysym::XF86_MenuKB => NamedKey::ContextMenu,
        Keysym::Escape => NamedKey::Escape,
        Keysym::Execute => NamedKey::Execute,
        Keysym::Find | Keysym::XF86_Search => NamedKey::Find,
        Keysym::Help => NamedKey::Help,
        Keysym::Pause | Keysym::Break => NamedKey::Pause,
        Keysym::Select => NamedKey::Select,
        Keysym::Print | Keysym::_3270_PrintScreen => NamedKey::PrintScreen,
        Keysym::Redo => NamedKey::Redo,

        Keysym::Multi_key => NamedKey::Compose,
        Keysym::Henkan => NamedKey::Convert,
        Keysym::Muhenkan => NamedKey::NonConvert,
        Keysym::Mode_switch => NamedKey::ModeChange,
        Keysym::Kana_Shift | Keysym::Kana_Lock => NamedKey::KanaMode,
        Keysym::Kanji => NamedKey::KanjiMode,
        Keysym::Hiragana => NamedKey::Hiragana,
        Keysym::Katakana => NamedKey::Katakana,
        Keysym::Zenkaku_Hankaku => NamedKey::ZenkakuHankaku,
        Keysym::Hangul => NamedKey::HangulMode,
        Keysym::Hangul_Hanja => NamedKey::HanjaMode,

        Keysym::XF86_Back => NamedKey::BrowserBack,
        Keysym::XF86_Forward => NamedKey::BrowserForward,
        Keysym::XF86_Refresh | Keysym::XF86_Reload => NamedKey::BrowserRefresh,
        Keysym::XF86_HomePage => NamedKey::BrowserHome,
        Keysym::XF86_AudioPlay => NamedKey::MediaPlayPause,
        Keysym::XF86_AudioStop => NamedKey::MediaStop,
        Keysym::XF86_AudioNext => NamedKey::MediaTrackNext,
        Keysym::XF86_AudioPrev => NamedKey::MediaTrackPrevious,
        Keysym::XF86_AudioLowerVolume => NamedKey::AudioVolumeDown,
        Keysym::XF86_AudioRaiseVolume => NamedKey::AudioVolumeUp,
        Keysym::XF86_AudioMute => NamedKey::AudioVolumeMute,
        Keysym::XF86_MonBrightnessDown => NamedKey::BrightnessDown,
        Keysym::XF86_MonBrightnessUp => NamedKey::BrightnessUp,

        Keysym::F1 | Keysym::KP_F1 => NamedKey::F1,
        Keysym::F2 | Keysym::KP_F2 => NamedKey::F2,
        Keysym::F3 | Keysym::KP_F3 => NamedKey::F3,
        Keysym::F4 | Keysym::KP_F4 => NamedKey::F4,
        Keysym::F5 => NamedKey::F5,
        Keysym::F6 => NamedKey::F6,
        Keysym::F7 => NamedKey::F7,
        Keysym::F8 => NamedKey::F8,
        Keysym::F9 => NamedKey::F9,
        Keysym::F10 => NamedKey::F10,
        Keysym::F11 => NamedKey::F11,
        Keysym::F12 => NamedKey::F12,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::key;
    use smithay_client_toolkit::seat::keyboard::Keysym;
    use zgui_vocab::{Key, NamedKey};

    #[test]
    fn a_letter_is_the_text_the_layout_produced() {
        assert_eq!(key(Keysym::a, Some("a")), Key::character("a"));
        // The layout, not the symbol: a symbol says which key, the text says what it typed.
        assert_eq!(key(Keysym::a, Some("A")), Key::character("A"));
    }

    #[test]
    fn a_named_key_stays_named_even_though_it_produces_text() {
        // A field that inserted a carriage return on every return press would be unusable, and
        // would pass every test written against key names.
        assert_eq!(key(Keysym::Return, Some("\r")), Key::Named(NamedKey::Enter));
        assert_eq!(key(Keysym::Tab, Some("\t")), Key::Named(NamedKey::Tab));
        assert_eq!(
            key(Keysym::BackSpace, Some("\u{8}")),
            Key::Named(NamedKey::Backspace)
        );
    }

    #[test]
    fn the_space_bar_is_named_and_the_vocabulary_decides_what_it_inserts() {
        let space = key(Keysym::space, Some(" "));
        assert_eq!(space, Key::Named(NamedKey::Space));
        assert_eq!(space.inserted_text(), Some(" "));
        assert_eq!(Key::Named(NamedKey::Enter).inserted_text(), None);
    }

    #[test]
    fn the_two_of_each_modifier_carry_the_same_name() {
        // A shortcut written against control has to match whichever one the user reached for.
        assert_eq!(key(Keysym::Control_L, None), key(Keysym::Control_R, None));
        assert_eq!(key(Keysym::Shift_L, None), Key::Named(NamedKey::Shift));
        assert_eq!(key(Keysym::Super_R, None), Key::Named(NamedKey::Meta));
        assert!(key(Keysym::Alt_L, None).is_modifier());
    }

    #[test]
    fn the_numeric_pad_produces_the_same_meanings_as_the_keys_it_duplicates() {
        assert_eq!(
            key(Keysym::KP_Enter, Some("\r")),
            Key::Named(NamedKey::Enter)
        );
        assert_eq!(key(Keysym::KP_Left, None), Key::Named(NamedKey::ArrowLeft));
    }

    #[test]
    fn a_symbol_with_no_name_and_no_text_keeps_the_name_this_desktop_calls_it() {
        // Still bindable, which is what a configuration file written against it needs.
        let launch = key(Keysym::XF86_Calculator, None);
        assert!(
            matches!(&launch, Key::Other(name) if name.as_str().contains("Calculator")),
            "an unnamed symbol lost its identity: {launch:?}"
        );
    }

    #[test]
    fn a_letter_held_with_control_is_still_that_letter() {
        // What the compositor reports for control and D is U+0004, because that is what the key
        // *produces*. A shortcut is written against the key, so a backend that dispatched the
        // production leaves every shortcut on a letter unreachable while typing works perfectly —
        // which is exactly how this presents.
        assert_eq!(key(Keysym::d, Some("\u{4}")), Key::character("d"));
        assert_eq!(key(Keysym::a, Some("\u{1}")), Key::character("a"));
        assert_eq!(
            key(Keysym::bracketleft, Some("\u{1b}")),
            Key::character("[")
        );
    }

    #[test]
    fn a_letter_held_with_control_and_shift_is_the_shifted_letter() {
        // The layout still applies: control does not shift, so the symbol is whatever the other
        // modifiers made it and the shortcut follows the key the person actually pressed.
        assert_eq!(key(Keysym::D, Some("\u{4}")), Key::character("D"));
    }

    #[test]
    fn a_named_key_held_with_control_keeps_its_name() {
        // The name is decided before anything looks at the text, so the character the symbol
        // carries never gets the chance to stand in for it.
        assert_eq!(key(Keysym::Return, Some("\r")), Key::Named(NamedKey::Enter));
        assert_eq!(key(Keysym::Tab, Some("\t")), Key::Named(NamedKey::Tab));
    }

    #[test]
    fn a_control_character_is_never_inserted_as_itself() {
        let escape = key(Keysym::Escape, Some("\u{1b}"));
        assert_eq!(escape, Key::Named(NamedKey::Escape));
        assert_eq!(escape.inserted_text(), None);
    }
}
