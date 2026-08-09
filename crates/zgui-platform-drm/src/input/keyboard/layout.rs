//! What a key *means*, and the two things on a Linux machine that can say.
//!
//! A device reports which key moved. A window system would answer what the key means; a console
//! has nothing to ask, so this backend asks a layout itself. Two answer, they are not equally
//! good, and which one a program got is the first thing anybody wants to know when the wrong
//! letters appear — so [`find`] reports it and the frame loop states it once at start-up.
//!
//! * **libxkbcommon** ([`Source::Xkb`]) reads the keyboard data the machine has installed. It
//!   knows every level of every key, every layout in a group, and which keys repeat.
//! * **the console keymap** ([`Source::Console`]) is what the kernel's own console driver holds,
//!   put there by `loadkeys` at boot. It is the layout of last resort, and it is worse in ways a
//!   person will notice.
//!
//! # The order of a reading and an update
//!
//! For a press the keysym is read *before* the state is told about the press, because a press
//! spends a latched modifier: a level-three latch is set, `Q` is pressed, and the update that
//! records the press clears the latch. Reading after the update answers `q` where the person typed
//! `@`, and only for the one key that follows each latch, so it reads as an occasional dropped
//! modifier rather than as a fault. `zgui_xkb::State::press` does both in the right order so that
//! a caller cannot invert them, and [`Layout::press`] is written against it.
//!
//! # The library's diagnostics
//!
//! libxkbcommon writes its own diagnostics to standard error unless it is told otherwise, and on
//! this backend standard error can be the very terminal being drawn on. A `zgui_xkb::Context`
//! takes the messages away from the library the moment it is made, and nothing here gives them
//! back: no sink is set, so they are dropped. The one message that matters — why a keymap refused
//! to compile — is carried in the error instead and reaches [`Search::refused`].

use std::str::FromStr;

use zgui_evdev::Key;
use zgui_vocab::{Modifiers, NamedKey, PhysicalKey};

use crate::input::keyboard::{code, modifiers};

/// Which source a keyboard's layout came from.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// libxkbcommon, over the keyboard data this machine has installed.
    Xkb,
    /// The keymap the kernel's own console driver holds.
    Console,
}

/// What one key means, under the two readings a layout answers.
///
/// The third reading of a press — where the key sits — is no layout's business and comes from
/// [`code::physical`](crate::input::keyboard::code) instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// What the key means with the modifiers applied. This is what gets inserted.
    pub key: zgui_vocab::Key,
    /// What the key means with the layout applied and the modifiers not applied.
    ///
    /// This is what keeps a shortcut on its key when a modifier would have remapped it.
    pub without_modifiers: zgui_vocab::Key,
}

/// A keyboard layout: what each key means, and which modifiers are held.
///
/// One implementation stands for one keyboard's state. Which keys are down is state rather than
/// layout, so a caller that holds one of these for two keyboards has shift held on either of them
/// shifting a key struck on the other.
pub trait Layout {
    /// Returns which source this layout reads.
    fn source(&self) -> Source;

    /// Returns what this layout is, in one line a person reads at start-up.
    fn describe(&self) -> String;

    /// Records a key going down, and reports what it produced.
    ///
    /// The reading is taken before the state is updated. See the module documentation.
    fn press(&mut self, key: Key) -> Reading;

    /// What a key that is being held produces, recording nothing.
    ///
    /// The kernel reports a held key over and over, and each report is a reading rather than a
    /// transition: an implementation that recorded one would count more presses than releases and
    /// leave a modifier held for good. It takes `&self`, so an implementation cannot record one —
    /// what is still a caller's to get right is asking for this rather than for
    /// [`Layout::press`], which is what the value on the event decides.
    fn repeat(&self, key: Key) -> Reading;

    /// Records a key coming up.
    fn release(&mut self, key: Key);

    /// Records a key that was already down before this layout existed.
    ///
    /// This is [`Layout::press`] with no reading, because a key that was already down produced its
    /// character before anything was listening. The release that balances it is the one the kernel
    /// will send when the finger comes up.
    fn hold(&mut self, key: Key);

    /// Returns which modifiers are on right now.
    fn modifiers(&self) -> Modifiers;
}

/// What looking for a layout found.
///
/// Absence is a value. A machine with neither libxkbcommon nor a reachable console has no layout
/// at all, and a caller has to say so rather than type the wrong characters.
pub struct Search {
    /// The layout, when a source answered.
    pub layout: Option<Box<dyn Layout>>,
    /// What each source that refused said, in the order they were tried.
    pub refused: Vec<String>,
}

/// Opens the best layout this machine has.
///
/// libxkbcommon first, because it expresses everything a console keymap does and more, then the
/// keymap the console driver holds. A machine with neither answers with no layout, and every key
/// then reaches a document by its position alone.
///
/// The names libxkbcommon is asked for are empty. Empty names tell it to use the ones the machine
/// is already set to: `XKB_DEFAULT_LAYOUT` and its siblings, and the defaults the library was
/// built with. A console session has no other source for them.
pub fn find() -> Search {
    let mut refused = Vec::new();
    match Xkb::open() {
        Ok(layout) => {
            return Search {
                layout: Some(Box::new(layout)),
                refused,
            };
        }
        Err(reason) => refused.push(reason),
    }
    match Console::open() {
        Ok(layout) => Search {
            layout: Some(Box::new(layout)),
            refused,
        },
        Err(reason) => {
            refused.push(reason);
            Search {
                layout: None,
                refused,
            }
        }
    }
}

/// The keysym names that differ from the standard key value they stand for.
///
/// Every other name crosses through [`NamedKey::from_str`].
// Only the differences are written down. The two vocabularies name most keys identically, and a
// transcribed table of three hundred rows would be wrong in one place and silently.
// `zgui-platform-winit` reads the same correspondence for the same reason.
const RENAMED: &[(&str, NamedKey)] = &[
    ("Return", NamedKey::Enter),
    ("KP_Enter", NamedKey::Enter),
    ("BackSpace", NamedKey::Backspace),
    ("ISO_Left_Tab", NamedKey::Tab),
    ("KP_Delete", NamedKey::Delete),
    ("KP_Insert", NamedKey::Insert),
    ("KP_Home", NamedKey::Home),
    ("KP_End", NamedKey::End),
    ("Prior", NamedKey::PageUp),
    ("KP_Prior", NamedKey::PageUp),
    ("Next", NamedKey::PageDown),
    ("KP_Next", NamedKey::PageDown),
    ("Up", NamedKey::ArrowUp),
    ("KP_Up", NamedKey::ArrowUp),
    ("Down", NamedKey::ArrowDown),
    ("KP_Down", NamedKey::ArrowDown),
    ("Left", NamedKey::ArrowLeft),
    ("KP_Left", NamedKey::ArrowLeft),
    ("Right", NamedKey::ArrowRight),
    ("KP_Right", NamedKey::ArrowRight),
    ("KP_Begin", NamedKey::Clear),
    ("Shift_L", NamedKey::Shift),
    ("Shift_R", NamedKey::Shift),
    ("Control_L", NamedKey::Control),
    ("Control_R", NamedKey::Control),
    ("Alt_L", NamedKey::Alt),
    ("Alt_R", NamedKey::Alt),
    // xkb calls the key beside the space bar super, and the standard calls the command modifier
    // meta. `Meta_L` is a modifier of xkb's own that almost no keymap binds, and a program that
    // saw it would want the command modifier from it too.
    ("Super_L", NamedKey::Meta),
    ("Super_R", NamedKey::Meta),
    ("Meta_L", NamedKey::Meta),
    ("Meta_R", NamedKey::Meta),
    ("Hyper_L", NamedKey::Hyper),
    ("Hyper_R", NamedKey::Hyper),
    ("Caps_Lock", NamedKey::CapsLock),
    ("Shift_Lock", NamedKey::CapsLock),
    ("Num_Lock", NamedKey::NumLock),
    ("Scroll_Lock", NamedKey::ScrollLock),
    ("ISO_Level3_Shift", NamedKey::AltGraph),
    ("ISO_Level3_Latch", NamedKey::AltGraph),
    ("ISO_Level5_Shift", NamedKey::AltGraph),
    ("Mode_switch", NamedKey::ModeChange),
    ("Multi_key", NamedKey::Compose),
    ("Menu", NamedKey::ContextMenu),
    ("Print", NamedKey::PrintScreen),
    ("Sys_Req", NamedKey::PrintScreen),
    ("Break", NamedKey::Pause),
    ("Henkan", NamedKey::Convert),
    ("Muhenkan", NamedKey::NonConvert),
    ("Hiragana_Katakana", NamedKey::KanaMode),
    ("Zenkaku_Hankaku", NamedKey::ZenkakuHankaku),
    ("Hangul", NamedKey::HangulMode),
    ("Hangul_Hanja", NamedKey::HanjaMode),
    ("XF86AudioLowerVolume", NamedKey::AudioVolumeDown),
    ("XF86AudioRaiseVolume", NamedKey::AudioVolumeUp),
    ("XF86AudioMute", NamedKey::AudioVolumeMute),
    ("XF86AudioPlay", NamedKey::MediaPlayPause),
    ("XF86AudioStop", NamedKey::MediaStop),
    ("XF86AudioNext", NamedKey::MediaTrackNext),
    ("XF86AudioPrev", NamedKey::MediaTrackPrevious),
    ("XF86MonBrightnessDown", NamedKey::BrightnessDown),
    ("XF86MonBrightnessUp", NamedKey::BrightnessUp),
    ("XF86Copy", NamedKey::Copy),
    ("XF86Cut", NamedKey::Cut),
    ("XF86Paste", NamedKey::Paste),
    ("XF86Back", NamedKey::BrowserBack),
    ("XF86Forward", NamedKey::BrowserForward),
    ("XF86Refresh", NamedKey::BrowserRefresh),
    ("XF86Search", NamedKey::BrowserSearch),
    ("XF86HomePage", NamedKey::BrowserHome),
];

/// Returns what a keysym called `name` is, when the vocabulary names it.
fn named_key(name: &str) -> Option<NamedKey> {
    RENAMED
        .iter()
        .find(|(keysym, _)| *keysym == name)
        .map(|(_, named)| *named)
        .or_else(|| NamedKey::from_str(name).ok())
}

/// Returns `true` if this is text a document can hold.
///
/// Both layouts answer a control character where somebody pressed a key that types nothing. The
/// return key answers a carriage return, backspace answers `\u{7f}`, and libxkbcommon's control
/// transformation turns `Ctrl+A` into `\u{1}`. A field that inserted any of them would look right
/// in a test written against key names and would be unusable.
fn is_typed(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

/// Returns the key that produces `text`.
///
/// The space bar is the one key whose standard value is the text it types, so a key that types a
/// space *is* the space key. A shortcut, and the framework's own "space activates what has focus",
/// both match it that way. Every other key that types something is a character.
fn typed_key(text: &str) -> zgui_vocab::Key {
    if text == NamedKey::Space.as_str() {
        return zgui_vocab::Key::Named(NamedKey::Space);
    }
    zgui_vocab::Key::character(text)
}

/// Returns the character a keysym stands for, under the rule the X11 protocol states.
///
/// A keysym from `0x20` to `0x7e` and from `0xa0` to `0xff` is the code point it numbers, and a
/// keysym with [`UNICODE`] set carries its code point in its low twenty-four bits.
///
/// Every other keysym answers nothing here, including the legacy blocks that hold Greek and
/// Cyrillic. That costs nothing in the ordinary case: this is only reached for a press whose text
/// the control transformation took away, so what is lost is the letter behind a control chord on
/// one of those layouts, and the position the chord was struck at is reported either way.
fn character(sym: zgui_xkb::Keysym) -> Option<char> {
    let raw = sym.raw();
    let point = match raw {
        0x20..=0x7e | 0xa0..=0xff => raw,
        _ if raw & UNICODE != 0 => raw & !UNICODE,
        _ => return None,
    };
    char::from_u32(point)
}

/// The bit that marks a keysym as carrying a code point rather than naming something.
const UNICODE: u32 = 0x0100_0000;

/// What a dead keysym's name begins with.
const DEAD: &str = "dead_";

/// A layout libxkbcommon reads.
///
/// The context is held beside the keymap because this crate matches a keysym by its *name*, and
/// naming one is a call on the context.
///
/// Every key code crosses through `zgui_xkb::Keycode::from_evdev`, which adds the eight that
/// separates the two numberings: an xkb key code is the evdev code plus 8.
struct Xkb {
    /// What keysyms are named through, and what the keymap was compiled by.
    context: zgui_xkb::Context,
    /// Every key, every level and every layout of this keyboard.
    keymap: zgui_xkb::Keymap,
    /// Which keys are down, which modifiers are on, and which layout is active.
    state: zgui_xkb::State,
    /// The names the keymap was asked for, for the line a person reads at start-up.
    names: zgui_xkb::RuleNames,
}

impl Xkb {
    /// Opens libxkbcommon and compiles the keymap this machine is set to.
    ///
    /// # Errors
    ///
    /// Returns the reason as a sentence, because every one of them is something a caller reports
    /// rather than branches on: the library is absent, its keyboard data is absent, or the names
    /// the machine is set to name a layout the rules do not know.
    fn open() -> Result<Self, String> {
        // The context takes libxkbcommon's diagnostics away from standard error as it is made, so
        // the routing is done before anything below can produce a message.
        let context = zgui_xkb::Context::new().map_err(|error| error.to_string())?;
        let names = zgui_xkb::RuleNames::default();
        let keymap = context.keymap(&names).map_err(|error| error.to_string())?;
        let state = keymap.state().map_err(|error| error.to_string())?;
        Ok(Self {
            context,
            keymap,
            state,
            names,
        })
    }

    /// Returns what is printed on this key, whatever is held.
    ///
    /// Level zero of the layout the key is read in, which is the symbol on the keycap. Read before
    /// a press is recorded, because which layout a key is read in is state and a group latch is
    /// spent by the very press that follows it.
    fn printed(&self, code: zgui_xkb::Keycode) -> zgui_vocab::Key {
        let layout = self.state.layout(code);
        match self.keymap.unmodified_sym(code, layout) {
            Some(sym) => self.named(sym, None),
            None => zgui_vocab::Key::Unidentified,
        }
    }

    /// Returns the key a keysym and the text it produced amount to.
    ///
    /// The name is asked for first, because a key whose meaning is a name has to stay named: enter
    /// produces a carriage return and a field that inserted one every time enter was pressed would
    /// be unusable.
    ///
    /// A libxkbcommon built without `xkb_keysym_get_name` names nothing, and every key then arrives
    /// as the text it produced. That costs the named keys and costs typing nothing.
    fn named(&self, sym: zgui_xkb::Keysym, text: Option<&str>) -> zgui_vocab::Key {
        if sym.is_none() {
            return zgui_vocab::Key::Unidentified;
        }
        let name = self.context.keysym_name(sym);
        if let Some(name) = name.as_deref() {
            if let Some(named) = named_key(name) {
                return zgui_vocab::Key::Named(named);
            }
            if name.starts_with(DEAD) {
                // Which accent it is stays unreported. libxkbcommon states it as a keysym of its
                // own and offers no character for one, and the vocabulary allows a dead key that
                // does not say which it is.
                return zgui_vocab::Key::Dead(None);
            }
        }
        if let Some(text) = text.filter(|text| is_typed(text)) {
            return typed_key(text);
        }
        // The text the control transformation took away. The keysym still carries the letter, so
        // `Ctrl+A` reports `a` rather than the control character it typed.
        if let Some(character) = character(sym) {
            return typed_key(&character.to_string());
        }
        name.map_or(zgui_vocab::Key::Unidentified, |name| {
            zgui_vocab::Key::Other(name.into())
        })
    }
}

impl Layout for Xkb {
    fn source(&self) -> Source {
        Source::Xkb
    }

    fn describe(&self) -> String {
        format!("libxkbcommon, compiled from {}", self.names)
    }

    fn press(&mut self, key: Key) -> Reading {
        let code = zgui_xkb::Keycode::from_evdev(key.raw());
        // Before the update, for the reason the module documentation gives.
        let without_modifiers = self.printed(code);
        let press = self.state.press(code);
        Reading {
            key: self.named(press.sym, press.text.as_deref()),
            without_modifiers,
        }
    }

    fn repeat(&self, key: Key) -> Reading {
        let code = zgui_xkb::Keycode::from_evdev(key.raw());
        Reading {
            key: self.named(self.state.sym(code), self.state.text(code).as_deref()),
            without_modifiers: self.printed(code),
        }
    }

    fn release(&mut self, key: Key) {
        self.state.release(zgui_xkb::Keycode::from_evdev(key.raw()));
    }

    fn hold(&mut self, key: Key) {
        self.state.hold(zgui_xkb::Keycode::from_evdev(key.raw()));
    }

    fn modifiers(&self) -> Modifiers {
        modifiers::from_xkb(self.state.modifiers())
    }
}

/// A layout the kernel's console driver holds.
///
/// This is the layout of last resort: `loadkeys` puts a keymap in the console driver at boot, so a
/// machine with no libxkbcommon — or with the library and none of the keyboard data it reads —
/// still has this one. What a program gives up by landing here is worth stating, because a person
/// will meet all three:
///
/// * **Characters the keymap holds and the console cannot report.** Outside `K_UNICODE` the kernel
///   substitutes a hole for every entry above its eight-bit types, so a German keymap read in
///   `K_XLATE` keeps its umlauts, which are Latin-1, and loses its euro sign.
/// * **No name for a key that types nothing.** Escape, enter, the arrows and the function keys are
///   actions the console driver takes on itself rather than names, so each is named from the
///   position it sits at instead. The position table is exact, so the name is right for a standard
///   keyboard whatever the layout.
/// * **No caps lock and no command modifier.** The kernel builds a map index out of eight modifier
///   bits. Caps lock sits outside them, as one more of the driver's own actions, and there is no
///   super key among the eight either, so a shortcut that names meta can never match.
///
/// # The modifier keys
///
/// A console keymap holds no state, so which modifiers are down is tracked here. Which keys those
/// are is read out of the keymap rather than assumed: an entry that decodes to
/// [`Entry::Modifier`](zgui_evdev::Entry::Modifier) carries the bit that key holds down, which is
/// the same bit the kernel would have set.
struct Console {
    /// The open console.
    console: zgui_evdev::Console,
    /// The modifier keys that are down, each with the bit it holds.
    ///
    /// A list rather than a mask, because two keys can hold one bit — the two shift keys of a
    /// keymap that binds the shared group — and releasing one of them must not clear it.
    held: Vec<(Key, zgui_evdev::console::Modifiers)>,
}

impl Console {
    /// Opens the first console that answers and reads its keymap through that.
    ///
    /// # Errors
    ///
    /// Returns the reason every path refused, as one sentence. A session under a terminal emulator
    /// and a session with no controlling terminal both land here, and both are ordinary.
    fn open() -> Result<Self, String> {
        let found = zgui_evdev::Console::find();
        let Some(console) = found.console else {
            let refused: Vec<String> = found
                .refused
                .iter()
                .map(|skipped| format!("{}: {}", skipped.path.display(), skipped.reason))
                .collect();
            return Err(format!(
                "no console holds a keymap this process can read ({})",
                refused.join("; ")
            ));
        };
        Ok(Self {
            console,
            held: Vec::new(),
        })
    }

    /// Returns which modifier combination is held, which is also the map it selects.
    fn mask(&self) -> zgui_evdev::console::Modifiers {
        self.held
            .iter()
            .fold(zgui_evdev::console::Modifiers::NONE, |mask, (_, bit)| {
                mask | *bit
            })
    }

    /// Returns the bit this key holds down, when the keymap says it holds one.
    ///
    /// Read from the unmodified map, because a modifier key is a modifier in every map a keymap
    /// defines and the unmodified one is the map every keymap has.
    fn modifier(&self, key: Key) -> Option<zgui_evdev::console::Modifiers> {
        match self
            .console
            .entry(key, zgui_evdev::console::Modifiers::NONE)
        {
            Ok(zgui_evdev::Entry::Modifier(bit)) => Some(
                zgui_evdev::console::Modifiers::from_index(1_u8.checked_shl(u32::from(bit))?),
            ),
            _ => None,
        }
    }

    /// What this key produces under `held`, when it produces text at all.
    fn character(&self, key: Key, held: zgui_evdev::console::Modifiers) -> Option<char> {
        self.console
            .entry(key, held)
            .ok()
            .and_then(zgui_evdev::Entry::character)
            .filter(|character| is_typed(&character.to_string()))
    }

    /// Returns the key a character amounts to, or the name the position carries when there is no
    /// character.
    fn key_of(character: Option<char>, at: PhysicalKey) -> zgui_vocab::Key {
        match character {
            Some(character) => typed_key(&character.to_string()),
            None => code::name(at).map_or(zgui_vocab::Key::Unidentified, zgui_vocab::Key::Named),
        }
    }

    /// Returns what this key means now, and what it means with nothing held.
    fn reading(&self, key: Key) -> Reading {
        let at = code::physical(key);
        Reading {
            key: Self::key_of(self.character(key, self.mask()), at),
            without_modifiers: Self::key_of(
                self.character(key, zgui_evdev::console::Modifiers::NONE),
                at,
            ),
        }
    }

    /// Records a key going down, with no reading taken.
    fn record(&mut self, key: Key) {
        if let Some(bit) = self.modifier(key) {
            self.held.push((key, bit));
        }
    }
}

impl Layout for Console {
    fn source(&self) -> Source {
        Source::Console
    }

    fn describe(&self) -> String {
        let mode = match self.console.mode() {
            Ok(mode) if mode.keeps_unicode_entries() => "which reports every character it holds",
            Ok(_) => "which reports only the characters its eight-bit set can hold",
            Err(_) => "whose mode it no longer answers for",
        };
        format!(
            "the kernel's console keymap on {}, {mode}",
            self.console.path().display()
        )
    }

    fn press(&mut self, key: Key) -> Reading {
        // Before the record, so that a modifier key reports what it meant rather than what the
        // map it just selected says. This is the same order libxkbcommon's own is written in.
        let reading = self.reading(key);
        self.record(key);
        reading
    }

    fn repeat(&self, key: Key) -> Reading {
        self.reading(key)
    }

    fn release(&mut self, key: Key) {
        self.held.retain(|(held, _)| *held != key);
    }

    fn hold(&mut self, key: Key) {
        self.record(key);
    }

    fn modifiers(&self) -> Modifiers {
        modifiers::from_console(self.mask())
    }
}

#[cfg(test)]
mod tests {
    //! The naming rules, over no library and no console at all.
    //!
    //! Every rule here is arithmetic over a keysym or over a string, so all of it runs on a machine
    //! with neither source. What needs a source is the wiring, and `tests/keyboard.rs` is where
    //! that lives.

    use std::str::FromStr;

    use super::{RENAMED, character, is_typed, named_key, typed_key};
    use zgui_vocab::{Key, NamedKey};

    #[test]
    fn a_keysym_the_two_vocabularies_name_alike_crosses_on_its_name() {
        // No row for any of these. Both vocabularies call them the same thing, so the
        // correspondence is checkable rather than transcribed.
        for (name, named) in [
            ("Escape", NamedKey::Escape),
            ("Tab", NamedKey::Tab),
            ("Delete", NamedKey::Delete),
            ("Insert", NamedKey::Insert),
            ("Home", NamedKey::Home),
            ("End", NamedKey::End),
            ("F1", NamedKey::F1),
            ("F24", NamedKey::F24),
            ("Pause", NamedKey::Pause),
            ("Help", NamedKey::Help),
            ("Undo", NamedKey::Undo),
        ] {
            assert_eq!(named_key(name), Some(named), "{name}");
            assert!(
                !RENAMED.iter().any(|(keysym, _)| *keysym == name),
                "{name} needs no row and has one"
            );
        }
    }

    #[test]
    fn a_keysym_the_two_vocabularies_disagree_about_crosses_on_its_row() {
        for (name, named) in [
            ("Return", NamedKey::Enter),
            ("KP_Enter", NamedKey::Enter),
            ("BackSpace", NamedKey::Backspace),
            ("Prior", NamedKey::PageUp),
            ("Next", NamedKey::PageDown),
            ("Up", NamedKey::ArrowUp),
            ("Left", NamedKey::ArrowLeft),
            ("Shift_L", NamedKey::Shift),
            ("Control_R", NamedKey::Control),
            ("Caps_Lock", NamedKey::CapsLock),
            ("Multi_key", NamedKey::Compose),
            ("Menu", NamedKey::ContextMenu),
            ("Print", NamedKey::PrintScreen),
            ("ISO_Level3_Shift", NamedKey::AltGraph),
        ] {
            assert_eq!(named_key(name), Some(named), "{name}");
        }
    }

    #[test]
    fn the_command_key_is_meta_here_whatever_xkb_calls_it() {
        // xkb calls the key beside the space bar super. A shortcut written against the command key
        // has to match it, and the only way that happens is if it is one name by the time anything
        // above this crate sees it.
        assert_eq!(named_key("Super_L"), Some(NamedKey::Meta));
        assert_eq!(named_key("Super_R"), Some(NamedKey::Meta));
    }

    #[test]
    fn a_key_that_types_something_is_named_by_nothing() {
        // A letter, a digit and a punctuation mark all mean the text they type, so naming one here
        // would put its keysym name into a document.
        for name in ["a", "A", "1", "period", "ssharp", "Cyrillic_a"] {
            assert_eq!(named_key(name), None, "{name} was named");
        }
    }

    #[test]
    fn no_row_names_a_key_the_names_alone_would_reach() {
        // A row that duplicated the general rule is a row nothing tests: it would keep working if
        // it were deleted, and it would keep working if it were wrong in the same way the general
        // rule is.
        for (keysym, named) in RENAMED {
            assert_ne!(
                NamedKey::from_str(keysym).ok(),
                Some(*named),
                "{keysym} needs no row"
            );
        }
    }

    #[test]
    fn text_that_is_a_control_character_is_not_text() {
        // What both layouts answer where somebody pressed a key that types nothing.
        assert!(!is_typed("\r"), "the return key");
        assert!(!is_typed("\t"), "the tab key");
        assert!(!is_typed("\u{8}"), "backspace");
        assert!(
            !is_typed("\u{7f}"),
            "what a console keymap puts on backspace"
        );
        assert!(
            !is_typed("\u{1}"),
            "`Ctrl+A` under the control transformation"
        );
        assert!(!is_typed(""), "a key the layout leaves empty");
    }

    #[test]
    fn text_a_document_can_hold_is_text() {
        assert!(is_typed("a"));
        assert!(is_typed(" "), "the space bar types a space");
        assert!(is_typed("ä"));
        assert!(is_typed("€"));
    }

    #[test]
    fn the_key_that_types_a_space_is_the_space_key() {
        // The vocabulary's own value for the space bar *is* a space, and the framework activates
        // whatever has focus on `Key::Named(Space)`. A space that arrived as a character would
        // insert correctly and activate nothing.
        assert_eq!(typed_key(" "), Key::Named(NamedKey::Space));
        assert_eq!(typed_key(" ").inserted_text(), Some(" "));
        assert_eq!(typed_key("a"), Key::character("a"));
        assert_eq!(typed_key("  "), Key::character("  "), "two spaces is text");
    }

    #[test]
    fn a_keysym_that_numbers_a_character_answers_with_it() {
        // The two Latin-1 ranges the X11 protocol states, which is where every ASCII letter and
        // every Western European accented letter sits.
        assert_eq!(character(zgui_xkb::Keysym::from_raw(0x0061)), Some('a'));
        assert_eq!(character(zgui_xkb::Keysym::from_raw(0x0041)), Some('A'));
        assert_eq!(character(zgui_xkb::Keysym::from_raw(0x0020)), Some(' '));
        assert_eq!(character(zgui_xkb::Keysym::from_raw(0x00e4)), Some('ä'));
    }

    #[test]
    fn a_keysym_that_carries_a_code_point_answers_with_it() {
        // The euro sign, which is `XKB_KEY_EuroSign` — a code point with the Unicode bit set.
        assert_eq!(
            character(zgui_xkb::Keysym::from_raw(0x0100_20ac)),
            Some('€')
        );
    }

    #[test]
    fn a_keysym_that_names_something_carries_no_character() {
        // `XKB_KEY_Escape`, `XKB_KEY_Shift_L` and no symbol at all. A character read out of one of
        // these would type the private-use character its number happens to name.
        assert_eq!(character(zgui_xkb::Keysym::from_raw(0xff1b)), None);
        assert_eq!(character(zgui_xkb::Keysym::from_raw(0xffe1)), None);
        assert_eq!(character(zgui_xkb::Keysym::NONE), None);
        // The gap between the two Latin-1 ranges is where the control characters sit, and no
        // keysym there is a character either.
        assert_eq!(character(zgui_xkb::Keysym::from_raw(0x001b)), None);
        assert_eq!(character(zgui_xkb::Keysym::from_raw(0x0080)), None);
    }
}
