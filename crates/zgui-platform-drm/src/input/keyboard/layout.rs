//! What a key *means*, and the two things on a Linux machine that can say.
//!
//! A device reports which key moved. A window system would answer what the key means; a console
//! has nothing to ask, so this backend asks a layout itself. Two answer, they are not equally
//! good, and which one a program got is the first thing anybody wants to know when the wrong
//! letters appear — so [`find`] reports it and the frame loop states it once at start-up.
//!
//! * **libxkbcommon** ([`Source::Xkb`]) reads the keyboard data the machine has installed. It
//!   knows every level of every key, every layout in a group, and which keys repeat. *Which*
//!   keyboard it compiles is a question of its own, and the `names` module beside this one answers
//!   it: a console session has no session manager to state the names, so they are read from what
//!   the machine itself states.
//! * **the console keymap** ([`Source::Console`]) is what the kernel's own console driver holds,
//!   put there by `loadkeys` at boot. A machine with no libxkbcommon — or with the library and none
//!   of the keyboard data it reads — still has this one.
//!
//! # Which source is opened first
//!
//! libxkbcommon, wherever the machine states which keyboard it has, because it expresses everything
//! a console keymap does and more.
//!
//! **A machine that states no keyboard is read from the kernel instead.** libxkbcommon's answer
//! there is the names it was built with, which are `evdev`, `pc105` and `us`, and `us` is the wrong
//! keyboard everywhere outside the United States. The console keymap is the table the terminal
//! itself types with, so it is right on the machine it is read from — measured on a German machine
//! that states no xkb names: libxkbcommon typed US there and the console keymap typed German. What
//! reading it costs is below, and it is paid only by a machine that states nothing.
//!
//! # What a program gives up on the console keymap
//!
//! Three things, and a person will meet all three:
//!
//! * **Characters the keymap holds and the console cannot report.** Outside `K_UNICODE` the kernel
//!   substitutes a hole for every code point, so a German keymap read in `K_XLATE` — the ordinary
//!   mode — keeps its umlauts, which are Latin-1, and loses its euro sign. Which mode a console is
//!   in reaches the line this reports at start-up.
//! * **No name for a key that types nothing.** Escape, enter, the arrows and the function keys are
//!   actions the console driver takes on itself rather than names, so each is named from the
//!   position it sits at instead. The position table is exact, so the name is right for a standard
//!   keyboard whatever the layout.
//! * **No caps lock and no command modifier.** The kernel builds a map index out of eight modifier
//!   bits. Caps lock sits outside them, as one more of the driver's own actions, and there is no
//!   super key among the eight either — so a shortcut that names meta can never match here.
//!
//! # The terminal a key asks for
//!
//! Both sources bind `Ctrl+Alt+Fn` themselves, so a chord that asks for another terminal arrives
//! here as a reading. [`Reading::terminal`] carries it, and the `terminal` module beside this one
//! is where each source's answer becomes the number a person says.
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
//! A release is read before it is recorded, for the same reason: a modifier that is already up
//! selects another level of every key it was holding. [`Layout::reading_before_release`] is the one
//! call that does both, in that order.
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

use crate::input::keyboard::{code, modifiers, names, terminal};

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
/// The third reading of a press — where the key sits — is no layout's business, and the position
/// table answers it instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// What the key means with the modifiers applied. This is what gets inserted.
    pub key: zgui_vocab::Key,
    /// What the key means with the layout applied and the modifiers not applied.
    ///
    /// This is what keeps a shortcut on its key when a modifier would have remapped it.
    pub without_modifiers: zgui_vocab::Key,
    /// The terminal this key asks for, where it asks for one.
    ///
    /// A key that asks for a terminal belongs to the session, the way a key that toggles caps lock
    /// belongs to the console driver. Both layouts hold the answer already — `Ctrl+Alt+F1` is a
    /// keysym of its own under libxkbcommon and a `KT_CONS` entry in a console keymap — and the two
    /// count differently, so both cross to the number a person says: `Ctrl+Alt+F1` is terminal 1.
    pub terminal: Option<u32>,
}

/// A keyboard layout: what each key means, and which modifiers are held.
///
/// One implementation stands for one keyboard's state. Which keys are down is state rather than
/// layout, so a caller that holds one of these for two keyboards has shift held on either of them
/// shifting a key struck on the other.
///
/// # The modifier count
///
/// Both implementations count a modifier's transitions, because libxkbcommon does and the two have
/// to agree: shift held on two keyboards at once is two transitions, and it stays held until both
/// come up. So every [`Layout::press`] and every [`Layout::hold`] needs exactly one
/// [`Layout::release`], and the caller keeps that true — a repeat that recorded a press, or a
/// release with no press behind it, moves the count somewhere no later key brings it back from.
pub trait Layout {
    /// Returns which source this layout reads.
    fn source(&self) -> Source;

    /// Returns what this layout is, in one line a person reads at start-up.
    fn describe(&self) -> String;

    /// Records a key going down, and reports what it produced.
    ///
    /// The reading is taken before the state is updated. See the module documentation.
    fn press(&mut self, key: Key) -> Reading;

    /// Returns what this key means as the layout stands, recording nothing.
    ///
    /// Two things are read without recording. **A repeat** is no transition: the kernel reports a
    /// held key over and over, and an implementation that recorded one would count more presses
    /// than releases and leave a modifier held with nothing to release it. **A release** reports
    /// what the key meant while it was down, read before [`Layout::release`] records that it came
    /// up.
    ///
    /// It takes `&self`, so an implementation cannot record either of them. A caller still has to
    /// ask for this rather than for [`Layout::press`], and the value the kernel put on the event
    /// decides which.
    fn reading(&self, key: Key) -> Reading;

    /// Records a key coming up.
    fn release(&mut self, key: Key);

    /// Returns `true` if holding this key down repeats it.
    ///
    /// A letter repeats and a modifier does not, so a caller that repeated everything would report
    /// shift over and over while somebody held it. Which keys repeat is each layout's own answer.
    ///
    /// This is asked by the source that has to make the repeats itself. The kernel makes them for
    /// a reader of its own stream, so that reader asks nothing.
    fn repeats(&self, key: Key) -> bool;

    /// Reads what the key meant, and then records that it came up.
    ///
    /// One call rather than two lines a caller writes in an order, for the same reason
    /// `zgui_xkb::State::press` is one call: reading *after* the release reports what the key means
    /// with itself already up, and for a modifier that is a different level of every key it was
    /// holding — so releasing shift would report the unshifted key.
    ///
    /// It is provided rather than required, so an implementation gets the order without writing it.
    /// Neither implementation here overrides it, and one that did would be taking on the ordering
    /// this exists to settle.
    fn reading_before_release(&mut self, key: Key) -> Reading {
        let reading = self.reading(key);
        self.release(key);
        reading
    }

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
    /// Which keyboard this machine states it has, and where it states it.
    ///
    /// One line, whichever source answered, because it also says why: a machine that states no
    /// keyboard is a machine whose console keymap is read first.
    pub stated: String,
}

impl Search {
    /// Takes what one source opened, and keeps the first that answered.
    ///
    /// The source is a closure, so a source behind one that answered is never opened: opening a
    /// console holds a descriptor on a terminal, and compiling a keymap reads files.
    fn opened<L: Layout + 'static>(&mut self, open: impl FnOnce() -> Result<L, String>) {
        if self.layout.is_some() {
            return;
        }
        match open() {
            Ok(layout) => self.layout = Some(Box::new(layout)),
            Err(reason) => self.refused.push(reason),
        }
    }
}

/// Opens the best layout this machine has.
///
/// libxkbcommon first, because it expresses everything a console keymap does and more, and then the
/// keymap the console driver holds. **A machine that states no keyboard at all reads the console
/// first**, for the reason the module documentation gives. A machine with neither source answers
/// with no layout, and every key then reaches a document by its position alone.
///
/// Which names libxkbcommon is asked for is the `names` module's answer: the environment, then
/// `/etc/vconsole.conf`, then `/etc/default/keyboard`, then
/// `/etc/X11/xorg.conf.d/00-keyboard.conf`. A console session has no session manager to state them,
/// so they are read from the machine itself.
///
/// ```no_run
/// use zgui_platform_drm::input::keyboard::layout::{Layout, find};
///
/// let found = find();
///
/// // A source answered, or every source that refused said why.
/// assert!(found.layout.is_some() || !found.refused.is_empty());
/// if let Some(layout) = &found.layout {
///     assert!(!layout.describe().is_empty(), "the line a person reads at start-up");
/// }
/// ```
pub fn find() -> Search {
    let asked = names::of(&names::Machine::read());
    let mut search = Search {
        layout: None,
        refused: Vec::new(),
        stated: asked.to_string(),
    };
    if names::reads_the_console_first(asked.from) {
        search.opened(Console::open);
        search.opened(|| Xkb::over(asked));
    } else {
        search.opened(|| Xkb::over(asked));
        search.opened(Console::open);
    }
    search
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
    // The key beside the space bar is *super*, and it stays super here because that is what the
    // windowing backend calls it. The two are the same key and the vocabulary has a name for each,
    // so a shortcut written against one of them has to find the same name on both backends. What
    // agrees either way is the modifier bit: both reach `Modifiers::META`.
    ("Super_L", NamedKey::Super),
    ("Super_R", NamedKey::Super),
    ("Meta_L", NamedKey::Meta),
    ("Meta_R", NamedKey::Meta),
    ("Hyper_L", NamedKey::Hyper),
    ("Hyper_R", NamedKey::Hyper),
    ("Caps_Lock", NamedKey::CapsLock),
    ("Shift_Lock", NamedKey::CapsLock),
    ("Num_Lock", NamedKey::NumLock),
    ("Scroll_Lock", NamedKey::ScrollLock),
    // Level three is AltGr, latched or held. Level *five* is a different modifier that a handful
    // of layouts use for a fourth and fifth level, and it has no name in the vocabulary — so it is
    // left out rather than folded in here, and arrives carrying its own name. Calling it AltGraph
    // would make a shortcut matcher read the two as one key.
    ("ISO_Level3_Shift", NamedKey::AltGraph),
    ("ISO_Level3_Latch", NamedKey::AltGraph),
    ("ISO_Level3_Lock", NamedKey::AltGraph),
    ("ISO_Group_Shift", NamedKey::ModeChange),
    ("ISO_Enter", NamedKey::Enter),
    ("KP_Tab", NamedKey::Tab),
    ("KP_F1", NamedKey::F1),
    ("KP_F2", NamedKey::F2),
    ("KP_F3", NamedKey::F3),
    ("KP_F4", NamedKey::F4),
    ("Multi_key", NamedKey::Compose),
    ("Menu", NamedKey::ContextMenu),
    ("Print", NamedKey::PrintScreen),
    ("Sys_Req", NamedKey::PrintScreen),
    ("Break", NamedKey::Pause),
    ("Henkan_Mode", NamedKey::Convert),
    ("Muhenkan", NamedKey::NonConvert),
    ("Hiragana_Katakana", NamedKey::KanaMode),
    ("Kana_Lock", NamedKey::KanaMode),
    ("Kana_Shift", NamedKey::KanaMode),
    ("Kanji", NamedKey::KanjiMode),
    ("Zenkaku_Hankaku", NamedKey::ZenkakuHankaku),
    ("MultipleCandidate", NamedKey::AllCandidates),
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

/// The accent each dead key stands for.
///
/// A dead key produces its character after the next one, and until then what a person has typed is
/// the accent itself. That is what [`zgui_vocab::Key::Dead`] carries, and what something drawing a
/// preedit shows. libxkbcommon states a dead key as a keysym of its own and offers no character for
/// one, so the correspondence is written down.
///
/// These are the accents the layouts `xkeyboard-config` ships for Latin Europe use, as their
/// *spacing* characters. Every other dead key answers [`zgui_vocab::Key::Dead`] with nothing, which
/// the vocabulary allows: the sequence still composes, and what is lost is the glyph a preedit
/// would have shown while it was in progress.
const ACCENTS: &[(&str, char)] = &[
    ("dead_grave", '`'),
    ("dead_acute", '´'),
    ("dead_circumflex", '^'),
    ("dead_tilde", '~'),
    ("dead_macron", '¯'),
    ("dead_breve", '˘'),
    ("dead_abovedot", '˙'),
    ("dead_diaeresis", '¨'),
    ("dead_abovering", '˚'),
    ("dead_doubleacute", '˝'),
    ("dead_caron", 'ˇ'),
    ("dead_cedilla", '¸'),
    ("dead_ogonek", '˛'),
    ("dead_stroke", '/'),
    ("dead_currency", '¤'),
];

/// Returns the accent a dead key called `name` stands for.
fn accent(name: &str) -> Option<char> {
    ACCENTS
        .iter()
        .find(|(dead, _)| *dead == name)
        .map(|(_, accent)| *accent)
}

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

/// What a sequence machine's answer means for the key that was fed to it.
///
/// Split out from the call that feeds it, so the decision can be exercised with no library at all.
/// A caller has to get three things right — which answers show the key, which show an accent and
/// which replace the key entirely — and each of the three is wrong in a way a person notices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sequence {
    /// The key means what it means.
    Key,
    /// A sequence has begun and has not finished.
    Begun,
    /// A sequence finished, and what it made replaces the key.
    Made,
}

impl Sequence {
    /// Returns what `feed` and `status` together amount to.
    ///
    /// **The feed is read first.** An ignored keysym changed nothing, and the status still
    /// describes whatever sequence was already under way. Reading the status alone would report a
    /// modifier pressed in the middle of a sequence as beginning it, and would insert nothing while
    /// a person held shift.
    fn of(feed: zgui_xkb::Feed, status: zgui_xkb::Status) -> Self {
        if feed == zgui_xkb::Feed::Ignored {
            return Self::Key;
        }
        match status {
            zgui_xkb::Status::Composing => Self::Begun,
            zgui_xkb::Status::Composed => Self::Made,
            // A key that continues no sequence throws away whatever was under way, and a key that
            // began none never had one. Both leave the key meaning what it means.
            zgui_xkb::Status::Cancelled | zgui_xkb::Status::Nothing => Self::Key,
            // A status a newer libxkbcommon grew. Showing the key is what two of the four above
            // already do, and it is the answer that loses nothing.
            _ => Self::Key,
        }
    }
}

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
    /// How far through a dead key or a compose sequence this keyboard is.
    ///
    /// Nothing on a machine with no compose data. That is a separate package from the keyboard
    /// data — the sequences live in the X11 locale directory — so a machine that compiles every
    /// keymap can still hold none of them, and a dead key there types the base character on its
    /// own.
    compose: Option<zgui_xkb::ComposeState>,
    /// Why there is no sequence machine, for the line a person reads at start-up.
    without_compose: Option<String>,
    /// The names the keymap was asked for and where they came from, for the line a person reads at
    /// start-up.
    asked: names::Asked,
    /// The locale the sequences were compiled for.
    locale: String,
}

impl Xkb {
    /// Opens libxkbcommon and compiles the keymap `asked` names.
    ///
    /// [`find`] works out the names, from what the machine states. A test states them
    /// itself, because the compose path can be walked only on a layout that has a dead key and
    /// which layout a machine is set to is no test's choice.
    ///
    /// # Errors
    ///
    /// Returns the reason as a sentence, because every one of them is something a caller reports
    /// rather than branches on: the library is absent, its keyboard data is absent, or the names
    /// state a layout the rules do not know.
    fn over(asked: names::Asked) -> Result<Self, String> {
        // The context takes libxkbcommon's diagnostics away from standard error as it is made, so
        // the routing is done before anything below can produce a message.
        let context = zgui_xkb::Context::new().map_err(|error| error.to_string())?;
        let keymap = context
            .keymap(&asked.names)
            .map_err(|error| error.to_string())?;
        let state = keymap.state().map_err(|error| error.to_string())?;
        // The one part of this that may be absent on a machine where everything else worked, so
        // its refusal is carried rather than returned: a keyboard with no compose data still types
        // every key it has.
        let locale = zgui_xkb::locale_from_environment();
        let (compose, without_compose) = match context
            .compose_table(&locale)
            .and_then(|table| table.state())
        {
            Ok(compose) => (Some(compose), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Ok(Self {
            context,
            keymap,
            state,
            compose,
            without_compose,
            asked,
            locale,
        })
    }

    /// Returns what one keysym did to a sequence.
    ///
    /// Every keysym a press produces is fed here first, which is the order libxkbcommon states.
    /// A keysym the machine ignored — a modifier going down is the ordinary one — leaves whatever
    /// sequence is under way alone and answers nothing, so the key means what it means.
    ///
    /// Nothing is reset afterwards. libxkbcommon starts a fresh sequence on the next keysym fed
    /// after a finished or cancelled one, so a reset would only throw away a sequence that has
    /// already ended.
    fn composed(&mut self, sym: zgui_xkb::Keysym) -> Option<zgui_vocab::Key> {
        let (sequence, text, produced) = {
            let compose = self.compose.as_mut()?;
            let feed = compose.feed(sym);
            (
                Sequence::of(feed, compose.status()),
                compose.text(),
                compose.sym(),
            )
        };
        match sequence {
            // Nothing to show for the sequence, so the key means what it means.
            Sequence::Key => None,
            // A sequence has begun and has not finished, so this press produced nothing to insert.
            // It did produce the accent, which is the meaning of a dead key.
            Sequence::Begun => Some(zgui_vocab::Key::Dead(
                self.context.keysym_name(sym).as_deref().and_then(accent),
            )),
            // A sequence finished, and what it made replaces what this key would have produced.
            Sequence::Made => match text.as_deref().filter(|text| is_typed(text)) {
                Some(text) => Some(typed_key(text)),
                // A sequence whose result is a keysym with no text of its own.
                None => produced.map(|produced| self.named(produced, None)),
            },
        }
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
                // Reached where a sequence machine would have answered first: a machine with no
                // compose data, and the modifier-free reading of a dead key, which begins no
                // sequence because nothing was pressed.
                return zgui_vocab::Key::Dead(accent(name));
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

    /// Returns the terminal this keysym asks for, where it asks for one.
    ///
    /// Read off the keysym the modifiers selected, because that is the level the chord reaches:
    /// `XF86Switch_VT_1` sits on level five of a `CTRL+ALT` key type, and level zero of the same
    /// key is `F1`.
    ///
    /// The keysym's *value* is what answers. Naming one costs a call into libxkbcommon and a string
    /// on every key event, and thirty of each a second while a key repeats — and one keysym has two
    /// names, which [`terminal::from_keysym`] states.
    fn terminal(sym: zgui_xkb::Keysym) -> Option<u32> {
        terminal::from_keysym(sym.raw())
    }
}

impl Layout for Xkb {
    fn source(&self) -> Source {
        Source::Xkb
    }

    fn describe(&self) -> String {
        let composing = match &self.without_compose {
            Some(reason) => format!("no dead key or compose sequence works ({reason})"),
            None => format!("composing in {}", self.locale),
        };
        format!("libxkbcommon, compiled from {}, {composing}", self.asked)
    }

    fn press(&mut self, key: Key) -> Reading {
        let code = zgui_xkb::Keycode::from_evdev(key.raw());
        // Before the update, for the reason the module documentation gives.
        let without_modifiers = self.printed(code);
        let press = self.state.press(code);
        Reading {
            key: self
                .composed(press.sym)
                .unwrap_or_else(|| self.named(press.sym, press.text.as_deref())),
            without_modifiers,
            terminal: Self::terminal(press.sym),
        }
    }

    /// Reads the key without advancing a sequence, which `&self` enforces.
    ///
    /// A repeat and a release both arrive here, and neither is a keysym to feed: a sequence is
    /// advanced by keys somebody pressed. So a key held down inside a sequence repeats its own
    /// meaning rather than composing again, as holding a letter through a dead key does on a
    /// desktop too.
    fn reading(&self, key: Key) -> Reading {
        let code = zgui_xkb::Keycode::from_evdev(key.raw());
        let sym = self.state.sym(code);
        Reading {
            key: self.named(sym, self.state.text(code).as_deref()),
            without_modifiers: self.printed(code),
            terminal: Self::terminal(sym),
        }
    }

    fn release(&mut self, key: Key) {
        self.state.release(zgui_xkb::Keycode::from_evdev(key.raw()));
    }

    /// Returns `true` if the keymap repeats this key. A key it has no entry for does not repeat.
    fn repeats(&self, key: Key) -> bool {
        self.keymap
            .key_repeats(zgui_xkb::Keycode::from_evdev(key.raw()))
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
/// The layout of last resort. What a program gives up by landing here is in the module
/// documentation, because it is what somebody reading that line at start-up needs.
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

    /// Returns the modifier this key holds down, when the keymap says it holds one.
    ///
    /// Read from the unmodified map, because a modifier key is a modifier in every map a keymap
    /// defines and the unmodified one is the map every keymap has.
    ///
    /// The entry carries a bit *number* and a map index is a bit *mask*, so the two cross through
    /// `Modifiers::from_bit`. It answers nothing for `KG_CAPSSHIFT`, whose mask is 256 and whose
    /// map no `KDGKBENT` can ask for — a keymap that binds a key to it therefore has a key this
    /// layout leaves alone.
    fn modifier(&self, key: Key) -> Option<zgui_evdev::console::Modifiers> {
        match self
            .console
            .entry(key, zgui_evdev::console::Modifiers::NONE)
        {
            Ok(Some(zgui_evdev::Entry::Modifier(bit))) => {
                zgui_evdev::console::Modifiers::from_bit(bit)
            }
            _ => None,
        }
    }

    /// Returns what the keymap holds for this key under `held`.
    ///
    /// A key code above 255 answers nothing, which is an ordinary answer rather than a failure: a
    /// console keymap holds 256 entries and every code past them is outside the table. A console
    /// that has stopped answering answers nothing here too, which reads as a keyboard with no
    /// layout.
    fn entry(&self, key: Key, held: zgui_evdev::console::Modifiers) -> Option<zgui_evdev::Entry> {
        self.console.entry(key, held).ok().flatten()
    }

    /// Returns the character an entry produces, where a document can hold it.
    fn character(entry: Option<zgui_evdev::Entry>) -> Option<char> {
        entry
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
    ///
    /// The entry under what is held is read once and asked both questions, because the terminal a
    /// key asks for is one more thing that entry says: `Ctrl+Alt+F1` is a `KT_CONS` entry in the
    /// map the two modifiers select, and the same key in the unmodified map is a function key.
    fn read(&self, key: Key) -> Reading {
        let at = code::physical(key);
        let held = self.entry(key, self.mask());
        Reading {
            key: Self::key_of(Self::character(held), at),
            without_modifiers: Self::key_of(
                Self::character(self.entry(key, zgui_evdev::console::Modifiers::NONE)),
                at,
            ),
            terminal: held.and_then(terminal::from_entry),
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
        let reading = self.read(key);
        self.record(key);
        reading
    }

    fn reading(&self, key: Key) -> Reading {
        self.read(key)
    }

    /// Gives back one hold of this key rather than every hold of it.
    ///
    /// A count, because the layout above it counts: one state serves every keyboard on the seat, so
    /// a modifier held on two of them is two transitions and needs two releases. Clearing every
    /// entry would stop reporting shift while a finger was still on the other keyboard's.
    fn release(&mut self, key: Key) {
        if let Some(at) = self.held.iter().position(|(held, _)| *held == key) {
            self.held.remove(at);
        }
    }

    /// Returns `true` if the console keymap repeats this key.
    ///
    /// Six entry types repeat, and they are the ones that produce a character or a sequence over
    /// and over: `Latin`, `Letter`, `Unicode` and `Meta`, the keypad's `Pad`, and the four arrows
    /// of `Cursor`. Every other entry answers `false`.
    ///
    /// A key that holds a modifier, a lock or a sticky lock is held rather than struck, so
    /// repeating it would change nothing and report the same hold over and over. A key that asks
    /// for a terminal would repeat into a switch asked for thirty times a second. A key the keymap
    /// has nothing for has no meaning to repeat.
    ///
    /// **A console keymap states no repeat flag of its own, so the list above is this backend's**,
    /// and it is narrower than the xkb path. `KT_FN` covers the function keys and the editing block
    /// on this layout, and `KT_SPEC` covers enter, so none of those repeats here. libxkbcommon
    /// repeats all of them: enter, keypad enter, `F1`, `F12`, Home, PageUp, Insert and Delete were
    /// measured repeating on an `evdev`/`pc105`/`us` keymap, and shift and caps lock were not.
    fn repeats(&self, key: Key) -> bool {
        matches!(
            self.entry(key, self.mask()),
            Some(
                zgui_evdev::Entry::Latin(_)
                    | zgui_evdev::Entry::Letter(_)
                    | zgui_evdev::Entry::Unicode(_)
                    | zgui_evdev::Entry::Meta(_)
                    | zgui_evdev::Entry::Pad(_)
                    | zgui_evdev::Entry::Cursor(_)
            )
        )
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

    use super::{
        ACCENTS, Console, RENAMED, Sequence, Xkb, accent, character, is_typed, named_key, typed_key,
    };
    use crate::input::keyboard::layout::Layout;
    use crate::input::keyboard::{names, terminal};
    // `Key` in this module is the vocabulary's. A kernel key code is `Code`, so the two cannot
    // be confused where both appear in one line.
    use zgui_evdev::Key as Code;
    use zgui_vocab::{Key, NamedKey};

    /// A context over this machine's libxkbcommon, or nothing with the reason printed.
    ///
    /// The two tables below are the only things here that need the library, and they need it for
    /// the one question no amount of arithmetic answers: whether a name written down by hand is the
    /// name the library gives back.
    fn library(test: &str) -> Option<zgui_xkb::Context> {
        match zgui_xkb::Context::new() {
            Ok(context) => Some(context),
            Err(error) => {
                eprintln!(
                    "{test}: {error}, so no keysym name was checked; install libxkbcommon to run \
                     it"
                );
                None
            }
        }
    }

    /// Every row of `table` whose name libxkbcommon does not answer with.
    fn aliases(
        context: &zgui_xkb::Context,
        table: impl Iterator<Item = &'static str>,
    ) -> Vec<String> {
        let mut wrong = Vec::new();
        for name in table {
            let Some(sym) = context.keysym_from_name(name) else {
                wrong.push(format!("`{name}` is no keysym at all"));
                continue;
            };
            let canonical = context.keysym_name(sym);
            if canonical.as_deref() != Some(name) {
                wrong.push(format!(
                    "`{name}` is an alias for `{}`",
                    canonical.unwrap_or_default()
                ));
            }
        }
        wrong
    }

    #[test]
    fn every_row_is_the_name_the_library_gives_back() {
        // A press is looked up under whatever `keysym_name` answers, so a row written with an
        // alias reads correctly and matches nothing at all. Nothing else finds one: the key still
        // arrives, at the right position, carrying a name instead of the meaning it has.
        let test = "every_row_is_the_name_the_library_gives_back";
        let Some(context) = library(test) else {
            return;
        };

        let wrong = aliases(&context, RENAMED.iter().map(|(name, _)| *name));

        assert!(
            wrong.is_empty(),
            "{} row(s) can never match:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    #[test]
    fn every_dead_key_is_the_name_the_library_gives_back() {
        let test = "every_dead_key_is_the_name_the_library_gives_back";
        let Some(context) = library(test) else {
            return;
        };

        let wrong = aliases(&context, ACCENTS.iter().map(|(name, _)| *name));

        assert!(
            wrong.is_empty(),
            "{} row(s) can never match:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    #[test]
    fn every_terminal_keysym_carries_the_value_the_switch_reads_it_by() {
        // The switch matches the keysym by value, so this is what holds the twelve numbers against
        // the twelve names. Both spellings are asked for: `keysym_name` answers one and
        // `xkeyboard-config` builds its keymaps from the other, and both have to name the same
        // number. A table that moved under either would take `Ctrl+Alt+Fn` to the application as a
        // key nobody bound, which reads as a machine that cannot leave the program at all.
        let test = "every_terminal_keysym_carries_the_value_the_switch_reads_it_by";
        let Some(context) = library(test) else {
            return;
        };

        let mut wrong = Vec::new();
        for asked in 1..=12u32 {
            let spellings = [
                format!("XF86Switch_VT_{asked}"),
                format!("XF86_Switch_VT_{asked}"),
            ];
            for name in spellings {
                let Some(sym) = context.keysym_from_name(&name) else {
                    wrong.push(format!("`{name}` is no keysym at all"));
                    continue;
                };
                let read = terminal::from_keysym(sym.raw());
                if read != Some(asked) {
                    wrong.push(format!(
                        "`{name}` is {:#x}, which reads as {read:?} rather than terminal {asked}",
                        sym.raw()
                    ));
                }
            }
        }

        assert!(
            wrong.is_empty(),
            "{} terminal(s) can never be asked for:\n{}",
            wrong.len(),
            wrong.join("\n")
        );
    }

    #[test]
    fn a_keysym_the_machine_ignored_leaves_the_key_meaning_what_it_means() {
        // The feed is read before the status, and this is why. A modifier pressed in the middle of
        // a sequence is ignored by the machine and leaves the status at `Composing` — so reading
        // the status alone would report shift going down as beginning the sequence, and it would
        // insert nothing for as long as a person held it.
        for status in [
            zgui_xkb::Status::Nothing,
            zgui_xkb::Status::Composing,
            zgui_xkb::Status::Composed,
            zgui_xkb::Status::Cancelled,
        ] {
            assert_eq!(
                Sequence::of(zgui_xkb::Feed::Ignored, status),
                Sequence::Key,
                "{status:?}"
            );
        }
    }

    #[test]
    fn a_keysym_the_machine_took_is_read_against_where_the_sequence_got_to() {
        let taken = |status| Sequence::of(zgui_xkb::Feed::Accepted, status);

        assert_eq!(taken(zgui_xkb::Status::Composing), Sequence::Begun);
        assert_eq!(taken(zgui_xkb::Status::Composed), Sequence::Made);
        // A key that continues no sequence throws away whatever was under way, and a key that began
        // none never had one. Swallowing either would lose a keystroke outright.
        assert_eq!(taken(zgui_xkb::Status::Cancelled), Sequence::Key);
        assert_eq!(taken(zgui_xkb::Status::Nothing), Sequence::Key);
    }

    /// A layout over `layout`, or nothing with the reason printed.
    ///
    /// The machine's own layout is not something a test may choose, and the compose path can only
    /// be walked on one that has a dead key.
    fn layout_named(test: &str, layout: &str) -> Option<Xkb> {
        // Stated by nothing on this machine, because a test chose them. What a machine states is
        // the `names` module's own subject and is tested there.
        let asked = names::Asked {
            names: zgui_xkb::RuleNames {
                layout: Some(layout.to_owned()),
                ..zgui_xkb::RuleNames::default()
            },
            from: names::Origin::Nowhere,
        };
        match Xkb::over(asked) {
            Ok(built) => Some(built),
            Err(reason) => {
                eprintln!(
                    "{test}: no `{layout}` layout on this machine ({reason}), so nothing was \
                     asserted; install libxkbcommon and the `xkeyboard-config` data to run it"
                );
                None
            }
        }
    }

    /// The first key of `layout` that begins a dead-key sequence.
    ///
    /// Found by reading rather than by pressing, so the state is left exactly as it was.
    fn dead_key(layout: &Xkb) -> Option<Code> {
        (1..=255)
            .map(Code::new)
            .find(|key| matches!(layout.reading(*key).key, Key::Dead(_)))
    }

    #[test]
    fn a_dead_key_and_the_letter_after_it_compose_into_one_character() {
        // The reason libxkbcommon is preferred over the console keymap at all. On a German layout
        // `´` then `e` is `é`, and a backend that fed the sequence machine nothing inserts a bare
        // `e` — the accent silently dropped, on every layout in Europe that has one.
        let test = "a_dead_key_and_the_letter_after_it_compose_into_one_character";
        let Some(mut layout) = layout_named(test, "de") else {
            return;
        };
        let Some(dead) = dead_key(&layout) else {
            eprintln!("{test}: this machine's `de` layout has no dead key");
            return;
        };

        let plain = layout.press(Code::KEY_E).key;
        layout.release(Code::KEY_E);

        let begun = layout.press(dead);
        layout.release(dead);
        let composed = layout.press(Code::KEY_E);
        layout.release(Code::KEY_E);

        assert_eq!(
            begun.key,
            Key::Dead(Some('´')),
            "the dead key began a sequence and said which accent it is"
        );
        assert_eq!(
            begun.key.inserted_text(),
            None,
            "and inserted nothing while the sequence was under way"
        );
        assert_eq!(plain.inserted_text(), Some("e"));
        assert_eq!(
            composed.key.inserted_text(),
            Some("é"),
            "the key after it inserted what the sequence made"
        );
    }

    #[test]
    fn a_modifier_pressed_inside_a_sequence_does_not_take_it_over() {
        // Shift held between the dead key and the letter is the ordinary way to type `É`. The
        // sequence machine ignores the modifier, and a caller that read the status without the feed
        // would report shift going down as a dead key and insert nothing for it.
        let test = "a_modifier_pressed_inside_a_sequence_does_not_take_it_over";
        let Some(mut layout) = layout_named(test, "de") else {
            return;
        };
        let Some(dead) = dead_key(&layout) else {
            eprintln!("{test}: this machine's `de` layout has no dead key");
            return;
        };

        layout.press(dead);
        layout.release(dead);
        let shift = layout.press(Code::KEY_LEFTSHIFT);
        let composed = layout.press(Code::KEY_E);

        assert_eq!(
            shift.key,
            zgui_vocab::Key::Named(NamedKey::Shift),
            "shift is shift in the middle of a sequence"
        );
        assert_eq!(composed.key.inserted_text(), Some("É"));
    }

    #[test]
    fn a_key_that_continues_no_sequence_is_still_typed() {
        // Two dead keys in a row continue nothing in most tables. Swallowing the second would lose
        // a keystroke outright, which is the failure a person reports as "it dropped a letter".
        let test = "a_key_that_continues_no_sequence_is_still_typed";
        let Some(mut layout) = layout_named(test, "de") else {
            return;
        };
        let Some(dead) = dead_key(&layout) else {
            eprintln!("{test}: this machine's `de` layout has no dead key");
            return;
        };

        layout.press(dead);
        layout.release(dead);
        layout.press(dead);
        layout.release(dead);
        let after = layout.press(Code::KEY_E);

        assert!(
            after.key.inserted_text().is_some(),
            "the key after a sequence that led nowhere still types: {:?}",
            after.key
        );
    }

    #[test]
    fn the_chord_a_layout_binds_reads_as_the_terminal_it_asks_for() {
        // The switch, over real keyboard data. `symbols/pc` includes `srvr_ctrl(fkey2vt)`, which
        // puts `XF86Switch_VT_1` on level five of a `CTRL+ALT` key type — so the chord is the
        // layout's own and this reads what the layout answered. A layout is asked for by name
        // because which one a machine is set to is not something a test may choose.
        let test = "the_chord_a_layout_binds_reads_as_the_terminal_it_asks_for";
        let Some(mut layout) = layout_named(test, "us") else {
            return;
        };

        let plain = layout.press(Code::KEY_F1);
        layout.release(Code::KEY_F1);
        layout.press(Code::KEY_LEFTCTRL);
        layout.press(Code::KEY_LEFTALT);
        let chord = layout.press(Code::KEY_F1);

        assert_eq!(plain.terminal, None, "`F1` on its own asks for no terminal");
        assert_eq!(
            chord.terminal,
            Some(1),
            "`Ctrl+Alt+F1` asks for terminal 1: {chord:?}"
        );
    }

    #[test]
    fn letting_go_of_control_first_leaves_f1_as_an_ordinary_key() {
        // Why the seat remembers which key it swallowed. The terminal sits on the level the
        // modifiers select, so a chord taken apart before the finger comes off `F1` reads as `F1`
        // again here — and a caller that read this alone would send the release to the application
        // with no press behind it. `Keys::batch` is where that is closed.
        let test = "letting_go_of_control_first_leaves_f1_as_an_ordinary_key";
        let Some(mut layout) = layout_named(test, "us") else {
            return;
        };

        layout.press(Code::KEY_LEFTCTRL);
        layout.press(Code::KEY_LEFTALT);
        let chord = layout.press(Code::KEY_F1);
        layout.release(Code::KEY_LEFTCTRL);
        let after = layout.reading(Code::KEY_F1);

        assert_eq!(chord.terminal, Some(1), "the press asked for terminal 1");
        assert_eq!(
            after.terminal, None,
            "and with control gone the same key asks for nothing: {after:?}"
        );
        assert_eq!(after.key, Key::Named(NamedKey::F1));
    }

    #[test]
    fn the_console_keymap_reads_the_chord_out_of_the_map_the_modifiers_select() {
        // The half of the switch no other test covers. `Ctrl+Alt+F1` is a `KT_CONS` entry in the
        // map those two bits select, and reading it needs the ioctl and the mask together — so this
        // runs wherever a console answers and says why where none does.
        let test = "the_console_keymap_reads_the_chord_out_of_the_map_the_modifiers_select";
        let mut layout = match Console::open() {
            Ok(layout) => layout,
            Err(reason) => {
                eprintln!(
                    "{test}: no console this process may read ({reason}), so nothing was \
                     asserted; run this from a virtual console"
                );
                return;
            }
        };

        let plain = layout.press(Code::KEY_F1);
        layout.release(Code::KEY_F1);
        layout.press(Code::KEY_LEFTCTRL);
        layout.press(Code::KEY_LEFTALT);
        let chord = layout.press(Code::KEY_F1);

        assert_eq!(plain.terminal, None, "`F1` on its own asks for no terminal");
        assert_eq!(
            chord.terminal,
            Some(1),
            "`Ctrl+Alt+F1` asks for terminal 1: {chord:?}"
        );
    }

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
    fn the_key_beside_the_space_bar_is_named_the_way_the_other_backend_names_it() {
        // The vocabulary names super and meta apart, and the windowing backend reaches `Super` for
        // this key. A shortcut written against one of the two names has to match on both backends,
        // and the modifier bit agreeing is what would otherwise hide the difference.
        assert_eq!(named_key("Super_L"), Some(NamedKey::Super));
        assert_eq!(named_key("Super_R"), Some(NamedKey::Super));
        assert_eq!(named_key("Meta_L"), Some(NamedKey::Meta));
    }

    #[test]
    fn a_dead_key_carries_the_accent_it_stands_for() {
        // What something drawing a preedit shows while a sequence is in progress, and what the
        // windowing backend puts in the same field.
        assert_eq!(accent("dead_acute"), Some('´'));
        assert_eq!(accent("dead_grave"), Some('`'));
        assert_eq!(accent("dead_diaeresis"), Some('¨'));
        assert_eq!(accent("dead_circumflex"), Some('^'));
        // Every accent is a character in its own right, so a preedit has something to draw.
        for (name, accent) in ACCENTS {
            assert!(
                !accent.is_control(),
                "{name} stands for a control character"
            );
        }
    }

    #[test]
    fn a_dead_key_this_table_has_no_accent_for_is_still_a_dead_key() {
        // The vocabulary allows a dead key that does not say which it is, and the sequence composes
        // either way: what is lost is the glyph a preedit would have shown.
        assert_eq!(accent("dead_belowdot"), None);
        assert_eq!(accent("Escape"), None);
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
