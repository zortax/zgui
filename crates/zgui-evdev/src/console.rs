//! The keyboard layout the kernel's console driver holds.
//!
//! `loadkeys` puts a keymap in the console driver, and `KDGKBENT` reads one entry of it back. This
//! is the layout of last resort. The kernel carries a keymap of its own and every distribution
//! loads one over it at boot, so a machine with no libxkbcommon — or with the library and none of
//! the keyboard data it reads — still has this one.
//!
//! # What a console keymap cannot express
//!
//! **Eight modifier bits.** A map index is one byte, so the combinations that can be asked for are
//! the 256 built from shift, altgr, control, alt and the four left-and-right variants. The kernel
//! numbers a ninth bit, `KG_CAPSSHIFT`, whose map index is 256, and `KDGKBENT` reaches none of it.
//! There is nothing here like an xkb group either, so a keymap describes one layout at a time and
//! switching layouts means loading another keymap.
//!
//! **No compose sequences.** The console composes one dead key with one base character, from a
//! table of at most 256 pairs. libxkbcommon reads a compose file where a sequence is any number of
//! keys. The pairs themselves are the console's too, read through `KDGKBDIACR`, so even the
//! one-pair case needs a table this module does not hold — `keyboard.h` names the 27 accents a
//! `KT_DEAD` entry indexes and says nothing about what each composes into.
//!
//! **Key codes stop at 255.** A console keymap holds `NR_KEYS` entries, and `NR_KEYS` is 256. A key
//! the kernel numbers above that has no entry at all, and [`Console::entry`] answers `None` for it.
//! Every `BTN_*` is up there, so a caller reading a mouse meets this on every click.
//!
//! # Key codes
//!
//! Below 256 a console key code and an evdev key code are the same number, because the console
//! driver indexes its keymap with the key code the input layer handed it. So [`Console::entry`]
//! takes this crate's own [`Key`], and a code read from a device goes straight in.
//!
//! # The keyboard mode
//!
//! A console keyboard is in one of the modes `kd.h` names, and [`Console::mode`] reports which.
//! The mode decides which entries exist. In every mode except [`Mode::Unicode`] the kernel answers
//! `K_HOLE` for each code point, so a German keymap read in [`Mode::Translate`] — the ordinary
//! mode — keeps its umlauts, which are Latin-1, and loses its euro sign. In [`Mode::Raw`] and
//! [`Mode::Off`] the console delivers nothing from the keymap at all, and what is read is the
//! layout that *would* apply.
//!
//! # State
//!
//! Nothing here follows a key over time. Every [`Console::entry`] is one ioctl against the table
//! as it stands, and a caller that wants the character a key press produces holds its own modifier
//! state and asks for the map that state selects.

use std::ffi::c_int;
use std::path::{Path, PathBuf};

use rustix::fd::{AsFd, OwnedFd};
use rustix::fs::OFlags;

use crate::code::Key;
use crate::discover::Skipped;
use crate::error::{Error, Result};
use crate::ioctl;
use crate::sys;

/// Where a console might be, in the order they are tried.
const PATHS: [&str; 3] = ["/dev/tty", "/dev/tty0", "/dev/console"];

/// A console keymap holds as many entries as one byte of `kb_index` can name.
///
/// So bounding a key code by `u8` bounds it by the header's own count. A kernel that grew
/// `NR_KEYS` past that field would fail this build rather than leave [`index`] declining codes the
/// keymap has.
const _: () = assert!(
    sys::NR_KEYS == 256,
    "a console keymap has 256 entries, which is every value of the one-byte `kb_index`"
);

/// A keymap has as many maps as one byte of `kb_table` can name.
///
/// So every [`Modifiers`] names a map the kernel accepts an index for.
const _: () = assert!(
    sys::MAX_NR_KEYMAPS == 256,
    "a keymap has 256 maps, which is every value of the one-byte `kb_table`"
);

/// The last entry type the header names, and the last one [`Entry`] has a variant for.
///
/// A kernel that renumbered the types fails here. A kernel that *adds* one past this passes, and
/// its entries arrive as [`Entry::Unknown`], which is the case that variant exists for. `KT_CSI` at
/// 15 is reported to be such an addition; the vendored headers are 7.0 and name no such type, so
/// nothing here can check that.
const _: () = assert!(
    sys::KT_BRL == 14,
    "`KT_BRL` is the last type this module names, and every variant below it is numbered from here"
);

/// What `KDGKBENT` exclusive-ors an entry with.
///
/// This is `U(x)` from the kernel's `include/linux/kbd_kern.h`, which is `(x) ^ 0xf000`, and
/// `vt_kdgkbent` answers through it. That header is internal to the kernel build, so it carries no
/// syscall-note and cannot be vendored beside the others: this is the one number in this crate
/// that comes from neither a vendored header nor the compiler.
///
/// `kd.h` has `UNI_DIRECT_BASE 0xF000` in it, which is the same number for the direct font region.
/// It confirms nothing about this one.
const BIAS: u16 = 0xf000;

/// The first `KTYP` that a code point can produce, and one past the last packed type.
///
/// The kernel stores a code point raw and a packed entry biased, so it can hold a code point only
/// below [`BIAS`] — a stored value at or above that reads as a packed type. A code point therefore
/// comes back as `point ^ BIAS`, whose `KTYP` is `(point >> 8) ^ 0xf0` over a `point >> 8` of
/// `0x00..=0xef`, and the smallest value that expression reaches is 16, at `U+E000`. Every packed
/// type is below 16 and every code point is at or above it, with no value in both.
///
/// The test `the_split_between_a_packed_type_and_a_code_point_has_no_overlap` walks every code
/// point to hold that claim up.
const FIRST_POINT_TYPE: u16 = 16;

/// A modifier combination, which is also the index of the map it selects.
///
/// The console keeps one map per combination, and `kb_table` is that combination as a bitmask. So
/// the two are one value here.
///
/// The kernel's ninth modifier, `KG_CAPSSHIFT`, has no constant here. The kernel numbers its bit
/// 8, so the map it selects is 256, and `kb_table` is one byte — no `KDGKBENT` can ask for that
/// map.
///
/// ```
/// use zgui_evdev::Modifiers;
///
/// let level = Modifiers::SHIFT | Modifiers::ALTGR;
///
/// assert_eq!(level.index(), 3);
/// assert_eq!(Modifiers::from_index(3), level);
/// assert!(level.contains(Modifiers::ALTGR));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifier: what a key produces on its own.
    pub const NONE: Self = Self(0);
    /// Either shift key. `KG_SHIFT`.
    pub const SHIFT: Self = Self(1 << sys::KG_SHIFT);
    /// The AltGr key, which a keymap uses for a third level. `KG_ALTGR`.
    pub const ALTGR: Self = Self(1 << sys::KG_ALTGR);
    /// Either control key. `KG_CTRL`.
    pub const CONTROL: Self = Self(1 << sys::KG_CTRL);
    /// Either alt key. `KG_ALT`.
    pub const ALT: Self = Self(1 << sys::KG_ALT);
    /// Left shift on its own, for a keymap that tells the two shift keys apart. `KG_SHIFTL`.
    pub const LEFT_SHIFT: Self = Self(1 << sys::KG_SHIFTL);
    /// Right shift on its own. `KG_SHIFTR`.
    pub const RIGHT_SHIFT: Self = Self(1 << sys::KG_SHIFTR);
    /// Left control on its own. `KG_CTRLL`.
    pub const LEFT_CONTROL: Self = Self(1 << sys::KG_CTRLL);
    /// Right control on its own. `KG_CTRLR`.
    pub const RIGHT_CONTROL: Self = Self(1 << sys::KG_CTRLR);

    /// Returns the combination `index` stands for.
    ///
    /// Every value is one, because a keymap holds as many maps as `kb_table` can name. A map the
    /// keymap left out reads as holes throughout.
    ///
    /// This takes a *mask*. [`Modifiers::from_bit`] is what takes a bit number, and the two differ
    /// wherever it matters: index 3 is shift over altgr, and bit 3 is alt.
    pub const fn from_index(index: u8) -> Self {
        Self(index)
    }

    /// Returns the combination one modifier is, from the bit number the keymap gives it.
    ///
    /// [`Entry::Modifier`], [`Entry::Lock`] and [`Entry::StickyLock`] carry a bit *number* —
    /// `KG_ALT` is 3 — where a map index is a bit *mask*, in which 3 is shift over altgr. Passing
    /// one to [`Modifiers::from_index`] compiles and means something else, so this is the way
    /// across.
    ///
    /// Answers `None` for a bit no map index can hold. `KG_CAPSSHIFT` is the one the kernel names:
    /// it numbers that bit 8, its mask is 256, and `kb_table` is one byte. An entry in the table
    /// can still name it, so the case is reachable from a real keymap.
    pub const fn from_bit(bit: u8) -> Option<Self> {
        if bit >= u8::BITS as u8 {
            return None;
        }
        Some(Self(1 << bit))
    }

    /// Returns the index of the map this combination selects.
    pub const fn index(self) -> u8 {
        self.0
    }

    /// Returns `true` if every modifier in `other` is held here too.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// What one entry of a console keymap holds.
///
/// The kernel packs a type and a value into sixteen bits: `KTYP(x)` is `x >> 8` and `KVAL(x)` is
/// `x & 0xff`. Most variants are one `KT_*` type, carrying the value as the kernel wrote it.
/// [`Entry::Unicode`] is the shape the kernel uses for a code point instead, and [`Entry::Hole`],
/// [`Entry::NoSuchMap`] and [`Entry::Allocated`] are single values worth their own names.
///
/// Three of them produce text on their own. See [`Entry::character`] for what the others need.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// A character of the console's eight-bit set. `KT_LATIN`.
    Latin(u8),
    /// A character of that set that caps lock acts on. `KT_LETTER`.
    ///
    /// Caps lock sends a caller back to the table. The kernel reads the map again with the shift
    /// bit of the index flipped and takes the entry it finds there, so a caller holding caps lock
    /// asks for `modifiers` with [`Modifiers::SHIFT`] toggled and uses that answer. Upper-casing
    /// the byte gives a different character on any keymap whose shifted key is something other
    /// than the ASCII upper case, and several are.
    Letter(u8),
    /// A code point, held whole. The kernel reports one only in [`Mode::Unicode`].
    Unicode(u16),
    /// A character the meta handling applies to: an escape byte before it, or the high bit set.
    /// `KT_META`.
    Meta(u8),
    /// A function key, whose string is in a table of its own. `KT_FN`.
    Function(u8),
    /// This map answers nothing for this key. `K_HOLE`.
    ///
    /// A hole is what the map answered. Two reachable cases read as one, so the answer says less
    /// about the key than it looks. The keymap may bind nothing here. The kernel also
    /// *substitutes* a hole for an entry it
    /// declines to report in the mode the console is in: outside [`Mode::Unicode`] every code point
    /// comes back this way, and [`Mode::Translate`] is the ordinary mode, so a German keymap
    /// answers `Hole` for AltGr over `KEY_E` while holding the euro sign there. [`Console::mode`]
    /// tells the two apart.
    Hole,
    /// The map itself was never loaded. `K_NOSUCHMAP`.
    ///
    /// `KDGKBENT` answers this for key code zero alone, so that code is how a caller asks whether
    /// a combination has a map. The three answers it can give are this one, [`Entry::Allocated`]
    /// for a map `loadkeys` built, and [`Entry::Hole`] for a compiled-in map that leaves code zero
    /// unbound.
    NoSuchMap,
    /// The map exists and the kernel allocated it. `K_ALLOCATED`, at key code zero.
    Allocated,
    /// An action the console takes itself, such as a caps-lock toggle. `KT_SPEC`.
    Special(u8),
    /// A keypad key, whose character depends on num lock. `KT_PAD`.
    Pad(u8),
    /// A dead key, as an index into the accents `keyboard.h` names `K_DGRAVE` to `K_DGREEK`.
    /// `KT_DEAD`.
    Dead(u8),
    /// A dead key, as the character it composes with. `KT_DEAD2`.
    DeadCharacter(u8),
    /// A switch to one virtual console, counted from zero. `KT_CONS`.
    ///
    /// A keymap's `Console_1` is `K(KT_CONS, 0)`: the kernel's `set_console` takes an index into
    /// its consoles, so terminal `n` arrives here as `n - 1`.
    Switch(u8),
    /// A cursor key, which sends an escape sequence. `KT_CUR`.
    Cursor(u8),
    /// A modifier key, as the `KG_*` bit number it holds down. `KT_SHIFT`. See
    /// [`Modifiers::from_bit`].
    Modifier(u8),
    /// One digit of a code point typed on the keypad. `KT_ASCII`.
    Ascii(u8),
    /// A modifier lock, as a `KG_*` bit number. `KT_LOCK`. See [`Modifiers::from_bit`].
    Lock(u8),
    /// A modifier lock that holds for one key, as a `KG_*` bit number. `KT_SLOCK`. See
    /// [`Modifiers::from_bit`].
    StickyLock(u8),
    /// A braille dot. `KT_BRL`.
    Braille(u8),
    /// A packed type this crate has no variant for, as the whole entry.
    ///
    /// A kernel newer than the vendored headers produces this. The addition it was written for is
    /// `KT_CSI` at 15, one past `KT_BRL`, for the cursor and editing keys that send a CSI sequence
    /// with the held modifiers in it; such an entry is reported to arrive in *every* mode. The
    /// vendored headers are 7.0 and name neither that type nor a count of types, so nothing here
    /// can check either claim. Reading such an entry as a code point would answer `U+FF01` for a
    /// Home key and type a fullwidth exclamation mark, so an unnamed type stays unnamed here.
    Unknown(u16),
}

impl Entry {
    /// Returns the entry `value` holds.
    ///
    /// `value` is `kb_value` as `KDGKBENT` wrote it.
    pub const fn decode(value: u16) -> Self {
        // The three entries `keyboard.h` composes by name. A hole is the commonest value in any
        // keymap, because every key a map leaves unbound is one; the other two answer at key code
        // zero and describe the map itself.
        if value as u32 == sys::ZGUI_K_HOLE {
            return Self::Hole;
        }
        if value as u32 == sys::ZGUI_K_NOSUCHMAP {
            return Self::NoSuchMap;
        }
        if value as u32 == sys::ZGUI_K_ALLOCATED {
            return Self::Allocated;
        }

        // A code point starts where the packed types stop, and the two ranges never meet. See
        // `FIRST_POINT_TYPE`.
        if value >> 8 >= FIRST_POINT_TYPE {
            return Self::Unicode(value ^ BIAS);
        }

        let held = (value & 0xff) as u8;
        match (value >> 8) as u32 {
            sys::KT_LATIN => Self::Latin(held),
            sys::KT_LETTER => Self::Letter(held),
            sys::KT_META => Self::Meta(held),
            sys::KT_FN => Self::Function(held),
            sys::KT_SPEC => Self::Special(held),
            sys::KT_PAD => Self::Pad(held),
            sys::KT_DEAD => Self::Dead(held),
            sys::KT_DEAD2 => Self::DeadCharacter(held),
            sys::KT_CONS => Self::Switch(held),
            sys::KT_CUR => Self::Cursor(held),
            sys::KT_SHIFT => Self::Modifier(held),
            sys::KT_ASCII => Self::Ascii(held),
            sys::KT_LOCK => Self::Lock(held),
            sys::KT_SLOCK => Self::StickyLock(held),
            sys::KT_BRL => Self::Braille(held),
            // A packed type below `FIRST_POINT_TYPE` that this module has no variant for. Only a
            // kernel newer than the vendored headers reaches here; `Entry::Unknown` says what is
            // known about the type that prompted it.
            _ => Self::Unknown(value),
        }
    }

    /// Returns the character this entry produces on its own.
    ///
    /// `KT_LATIN` and `KT_LETTER` hold a byte of the console's eight-bit set, which is Latin-1, so
    /// the byte is the code point. A Unicode entry holds the code point already, and answers
    /// `None` for a surrogate, which is a code point that is no character.
    ///
    /// ```
    /// use zgui_evdev::Entry;
    ///
    /// assert_eq!(Entry::Latin(0xe4).character(), Some('ä'));
    /// // A dead key produces its character after the key that follows it.
    /// assert_eq!(Entry::Dead(0).character(), None);
    /// ```
    ///
    /// Every other type answers `None`, each for a reason of its own. A dead key produces its
    /// character after the next key, through the accent table `KDGKBDIACR` reads. A keypad key
    /// produces one under num lock, through a table no header carries. A function key produces a
    /// string, held where `KDGKBSENT` reads it. A meta entry produces its character with an escape
    /// byte before it or with the high bit set, which is a decision about the terminal rather than
    /// about the layout.
    ///
    /// Caps lock is a question about which map to read rather than about the character this one
    /// holds. See [`Entry::Letter`].
    pub const fn character(self) -> Option<char> {
        match self {
            Self::Latin(byte) | Self::Letter(byte) => Some(byte as char),
            Self::Unicode(point) => char::from_u32(point as u32),
            _ => None,
        }
    }
}

/// Which mode a console keyboard is in.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Scan codes, with no translation. `K_RAW`.
    Raw,
    /// Key codes, with no translation. `K_MEDIUMRAW`.
    MediumRaw,
    /// Characters from the keymap, in the console's eight-bit set. `K_XLATE`.
    Translate,
    /// Characters from the keymap, as UTF-8. `K_UNICODE`.
    Unicode,
    /// The console reads no keys. `K_OFF`.
    Off,
    /// A mode this crate has no name for.
    Other(i32),
}

impl Mode {
    /// Returns the mode `KDGKBMODE` reported.
    fn from_raw(raw: c_int) -> Self {
        match raw {
            _ if raw == sys::K_RAW as c_int => Self::Raw,
            _ if raw == sys::K_MEDIUMRAW as c_int => Self::MediumRaw,
            _ if raw == sys::K_XLATE as c_int => Self::Translate,
            _ if raw == sys::K_UNICODE as c_int => Self::Unicode,
            _ if raw == sys::K_OFF as c_int => Self::Off,
            other => Self::Other(other),
        }
    }

    /// Returns `true` if an entry above the eight-bit types reads back as itself.
    ///
    /// The kernel answers `K_HOLE` for one in every mode except [`Mode::Unicode`], so a keymap
    /// read anywhere else has lost every character the eight-bit types cannot hold.
    pub const fn keeps_unicode_entries(self) -> bool {
        matches!(self, Self::Unicode)
    }
}

/// What a look for a console found.
///
/// Absence is a value here. A machine where no path answered is one where the console keymap is
/// out of reach as well as libxkbcommon, and a caller has to say so rather than type the wrong
/// characters.
#[derive(Debug)]
pub struct Search {
    /// The console that answered.
    pub console: Option<Console>,
    /// The paths that did not, in the order they were tried, with the reason each gave.
    pub refused: Vec<Skipped>,
}

/// An open console.
///
/// [`Console::find`] opens the first path that answers, and [`Console::entry`] reads one entry of
/// the keymap it holds.
///
/// ```
/// use zgui_evdev::{Console, Key, Modifiers};
///
/// let found = Console::find();
///
/// match found.console {
///     // `KEY_A` is inside every console keymap, so the map has an entry to answer with.
///     Some(console) => assert!(console.entry(Key::KEY_A, Modifiers::NONE)?.is_some()),
///     // Nothing answered, and each path that was tried says why.
///     None => assert!(!found.refused.is_empty()),
/// }
/// # Ok::<(), zgui_evdev::Error>(())
/// ```
#[derive(Debug)]
pub struct Console {
    /// The open descriptor.
    fd: OwnedFd,
    /// Which path answered, for messages.
    path: PathBuf,
}

impl Console {
    /// Opens the first console that answers.
    ///
    /// `/dev/tty` first, then `/dev/tty0`, then `/dev/console`. `/dev/tty` is the process's own
    /// controlling terminal and is world-readable, so a session running on a virtual console
    /// reaches its keymap with no privilege at all. The other two belong to root on every
    /// distribution.
    ///
    /// A session under a terminal emulator finds `/dev/tty` is a pseudo-terminal, which answers
    /// `ENOTTY`, and the walk moves on. So does a process with no controlling terminal, where the
    /// open itself answers `ENXIO`. Both land in [`Search::refused`] with the reason.
    pub fn find() -> Search {
        let mut refused = Vec::new();
        for path in PATHS {
            match Self::open(path) {
                Ok(console) => {
                    return Search {
                        console: Some(console),
                        refused,
                    };
                }
                Err(reason) => refused.push(Skipped {
                    path: PathBuf::from(path),
                    reason,
                }),
            }
        }
        Search {
            console: None,
            refused,
        }
    }

    /// Opens the console at `path`.
    ///
    /// The descriptor is opened read-only, and without becoming this process's controlling
    /// terminal: reading a keymap is a question, and a process that acquired a terminal by asking
    /// one would take a signal on the next hangup.
    ///
    /// It is opened non-blocking as well. Opening a terminal this process does not own can wait in
    /// `tty_port_block_til_ready` for carrier where `CLOCAL` is clear, which `/dev/console` on a
    /// serial line is. The descriptor carries nothing but ioctls, so the flag costs nothing and
    /// removes that wait.
    ///
    /// `KDGKBMODE` is issued here, because it tells a console from any other terminal. So a
    /// `Console` in hand is a descriptor that answered a console request at the moment it opened. A
    /// virtual-terminal switch or a hangup can still take it away afterwards, and every later call
    /// reports that itself.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Open`] when the path cannot be opened — permission and an absent
    /// controlling terminal are the ordinary cases — and [`Error::Ioctl`] when it opens and is no
    /// console.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let fd = rustix::fs::open(
            &path,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
            rustix::fs::Mode::empty(),
        )
        .map_err(|errno| Error::Open {
            path: path.clone(),
            source: errno.into(),
        })?;

        let console = Self { fd, path };
        console.mode()?;
        Ok(console)
    }

    /// Returns the path this console was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the mode the console keyboard is in.
    ///
    /// Read on every call. `kbd_mode` and `loadkeys` change it under a held descriptor, and the
    /// mode decides what [`Console::entry`] can report.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses.
    pub fn mode(&self) -> Result<Mode> {
        let mut raw: c_int = 0;
        ioctl::issue(self.fd.as_fd(), ioctl::KDGKBMODE, &mut raw)?;
        Ok(Mode::from_raw(raw))
    }

    /// Returns what the keymap holds for `key` under `modifiers`.
    ///
    /// Answers `Ok(None)` for a key code a console keymap has no room for. A keymap holds
    /// `NR_KEYS` entries and every code above 255 is past them, which covers every `BTN_*`, every
    /// `KEY_FN_*` and the whole numeric and braille blocks. That is an ordinary answer here: a
    /// caller feeding every `EV_KEY` code through this asks about such a code constantly.
    ///
    /// The answer depends on [`Console::mode`]. Outside [`Mode::Unicode`] the kernel reports
    /// [`Entry::Hole`] for every code point the keymap holds, so a caller that reads a keymap
    /// without reading the mode cannot tell an unbound key from a character the console declined
    /// to name.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses.
    pub fn entry(&self, key: Key, modifiers: Modifiers) -> Result<Option<Entry>> {
        let Some(kb_index) = index(key) else {
            return Ok(None);
        };
        let mut request = sys::kbentry {
            kb_table: modifiers.index(),
            kb_index,
            kb_value: 0,
        };
        ioctl::issue(self.fd.as_fd(), ioctl::KDGKBENT, &mut request)?;
        Ok(Some(Entry::decode(request.kb_value)))
    }
}

/// Returns the console's index for `key`, where a console keymap has one.
///
/// Answers `None` for a code past the last entry a keymap holds. Truncating one would name another
/// key: `KEY_FN_F1` is `0x1d2`, and the low byte of that is `KEY_PRINT`.
fn index(key: Key) -> Option<u8> {
    u8::try_from(key.raw()).ok()
}

#[cfg(test)]
mod tests {
    //! Decoding an entry, and choosing a map.
    //!
    //! Both are arithmetic over the numbers the kernel writes, so none of this needs a console —
    //! including every case no keymap on this machine produces.

    use super::*;

    /// The kernel's `K(t, v)`: the type in the top byte, the value in the bottom one.
    ///
    /// `keyboard.h` builds every named entry with this macro, and bindgen evaluates none of them,
    /// so the packing is written here. The test below holds it against the three the header
    /// composes through the C compiler.
    fn pack(kind: u32, value: u8) -> u16 {
        ((kind as u16) << 8) | u16::from(value)
    }

    #[test]
    fn packing_agrees_with_the_macro_the_header_builds_its_entries_with() {
        assert_eq!(u32::from(pack(sys::KT_SPEC, 0)), sys::ZGUI_K_HOLE);
        assert_eq!(u32::from(pack(sys::KT_SPEC, 127)), sys::ZGUI_K_NOSUCHMAP);
        assert_eq!(u32::from(pack(sys::KT_SPEC, 126)), sys::ZGUI_K_ALLOCATED);
    }

    #[test]
    fn an_entry_decodes_to_the_type_the_kernel_packed_into_it() {
        assert_eq!(Entry::decode(pack(sys::KT_LATIN, b'-')), Entry::Latin(b'-'));
        assert_eq!(
            Entry::decode(pack(sys::KT_LETTER, b'a')),
            Entry::Letter(b'a')
        );
        assert_eq!(Entry::decode(pack(sys::KT_META, b'a')), Entry::Meta(b'a'));
        assert_eq!(Entry::decode(pack(sys::KT_FN, 0)), Entry::Function(0));
        assert_eq!(Entry::decode(pack(sys::KT_SPEC, 1)), Entry::Special(1));
        assert_eq!(Entry::decode(pack(sys::KT_PAD, 0)), Entry::Pad(0));
        assert_eq!(Entry::decode(pack(sys::KT_DEAD, 2)), Entry::Dead(2));
        assert_eq!(
            Entry::decode(pack(sys::KT_DEAD2, b'^')),
            Entry::DeadCharacter(b'^')
        );
        assert_eq!(Entry::decode(pack(sys::KT_CONS, 3)), Entry::Switch(3));
        assert_eq!(Entry::decode(pack(sys::KT_CUR, 0)), Entry::Cursor(0));
        assert_eq!(Entry::decode(pack(sys::KT_SHIFT, 1)), Entry::Modifier(1));
        assert_eq!(Entry::decode(pack(sys::KT_ASCII, 0)), Entry::Ascii(0));
        assert_eq!(Entry::decode(pack(sys::KT_LOCK, 0)), Entry::Lock(0));
        assert_eq!(Entry::decode(pack(sys::KT_SLOCK, 0)), Entry::StickyLock(0));
        assert_eq!(Entry::decode(pack(sys::KT_BRL, 1)), Entry::Braille(1));
    }

    #[test]
    fn the_three_answers_key_code_zero_can_give_have_names_of_their_own() {
        // A hole is every unbound key in every map, so a caller meets it constantly. The other two
        // come back for key code zero alone: `K_NOSUCHMAP` says the map was never loaded and
        // `K_ALLOCATED` says the kernel built it, which together are how a caller asks whether a
        // modifier combination has a map at all.
        assert_eq!(Entry::decode(sys::ZGUI_K_HOLE as u16), Entry::Hole);
        assert_eq!(
            Entry::decode(sys::ZGUI_K_NOSUCHMAP as u16),
            Entry::NoSuchMap
        );
        assert_eq!(
            Entry::decode(sys::ZGUI_K_ALLOCATED as u16),
            Entry::Allocated
        );
        // All three are `KT_SPEC`, and the rest of that type stays an action the console takes.
        assert_eq!(Entry::decode(pack(sys::KT_SPEC, 7)), Entry::Special(7));
    }

    #[test]
    fn an_entry_at_or_above_the_first_code_point_type_is_a_code_point() {
        // A code point is stored raw and read back exclusive-ored with the bias. The euro sign is
        // the case that matters: it is on a German layout, and no eight-bit type can hold it.
        assert_eq!(Entry::decode(0x20ac ^ BIAS), Entry::Unicode(0x20ac));
        assert_eq!(Entry::decode(0x20ac ^ BIAS).character(), Some('€'));
        // A code point below the eight-bit boundary is written the same way and reads back whole.
        assert_eq!(Entry::decode(0x0041 ^ BIAS), Entry::Unicode(0x41));
        // The boundary itself, from both sides. `U+E000` is the code point that produces the
        // smallest type a code point can, and one below that type is a packed entry.
        assert_eq!(Entry::decode(0xe000 ^ BIAS), Entry::Unicode(0xe000));
        assert_eq!(Entry::decode(FIRST_POINT_TYPE << 8), Entry::Unicode(0xe000));
        assert!(matches!(
            Entry::decode(((FIRST_POINT_TYPE - 1) << 8) | 1),
            Entry::Unknown(_)
        ));
    }

    #[test]
    fn the_split_between_a_packed_type_and_a_code_point_has_no_overlap() {
        // The kernel can store a code point only below the bias, so every code point a keymap can
        // hold comes back with a type of at least `FIRST_POINT_TYPE`, and every packed type comes
        // back below it. Walking the whole range makes that a fact: an off-by-one here reads a
        // packed entry as a character.
        let smallest = (0..BIAS)
            .map(|point| (point ^ BIAS) >> 8)
            .min()
            .expect("there is at least one code point below the bias");
        assert_eq!(
            smallest, FIRST_POINT_TYPE,
            "no code point produces a smaller type"
        );

        for kind in 0..FIRST_POINT_TYPE {
            let decoded = Entry::decode((kind << 8) | 0x5a);
            assert!(
                !matches!(decoded, Entry::Unicode(_)),
                "type {kind} is packed, and decoded as {decoded:?}"
            );
        }
    }

    #[test]
    fn a_packed_type_the_headers_do_not_name_stays_unnamed() {
        // A kernel that adds a type past `KT_BRL` reaches this arm. `KT_CSI` at 15 is the reported
        // case, for the cursor and editing keys that send a CSI sequence carrying the held
        // modifiers, and such an entry is said to arrive in every mode. A decoding that took 15 for
        // a code point would answer `U+FF01` for a Home key bound that way, so a person pressing
        // Home would type a fullwidth exclamation mark.
        let home = pack(sys::KT_BRL + 1, 1);
        assert_eq!(Entry::decode(home), Entry::Unknown(home));
        assert_eq!(Entry::decode(home).character(), None);
        assert_ne!(Entry::decode(home), Entry::Unicode(0xff01));
    }

    #[test]
    fn only_the_entries_that_produce_text_on_their_own_answer_with_a_character() {
        assert_eq!(Entry::Latin(b'-').character(), Some('-'));
        assert_eq!(Entry::Letter(b'a').character(), Some('a'));
        // The eight-bit set is Latin-1, so a byte above ASCII is the code point it numbers.
        assert_eq!(Entry::Latin(0xe4).character(), Some('ä'));
        assert_eq!(Entry::Unicode(0x20ac).character(), Some('€'));
        // A surrogate is a code point that is no character. A keymap can hold one.
        assert_eq!(Entry::Unicode(0xd800).character(), None);
        // Each of these produces text through something this module does not hold: the next key
        // for a dead key, a num-lock table for the keypad, the function-key strings for `KT_FN`,
        // and a terminal's meta rule for `KT_META`.
        assert_eq!(Entry::Dead(0).character(), None);
        assert_eq!(Entry::Pad(0).character(), None);
        assert_eq!(Entry::Function(0).character(), None);
        assert_eq!(Entry::Meta(b'a').character(), None);
        assert_eq!(Entry::Hole.character(), None);
        // A type from a newer kernel is one whose meaning this crate has never read, so it
        // produces nothing at all rather than a character it guessed.
        assert_eq!(Entry::Unknown(0x0f01).character(), None);
    }

    #[test]
    fn a_modifier_combination_is_the_index_of_the_map_it_selects() {
        assert_eq!(Modifiers::NONE.index(), 0);
        assert_eq!(Modifiers::SHIFT.index(), 1);
        assert_eq!(Modifiers::ALTGR.index(), 2);
        assert_eq!(Modifiers::CONTROL.index(), 4);
        assert_eq!(Modifiers::ALT.index(), 8);
        assert_eq!(Modifiers::LEFT_SHIFT.index(), 16);
        assert_eq!(Modifiers::RIGHT_SHIFT.index(), 32);
        assert_eq!(Modifiers::LEFT_CONTROL.index(), 64);
        assert_eq!(Modifiers::RIGHT_CONTROL.index(), 128);
        // The bits add, so shift over AltGr is map 3 — the fourth level of a keymap that has one.
        assert_eq!((Modifiers::SHIFT | Modifiers::ALTGR).index(), 3);
        assert_eq!(
            Modifiers::from_index(3),
            Modifiers::SHIFT | Modifiers::ALTGR
        );
    }

    #[test]
    fn a_combination_holds_every_modifier_of_a_combination_it_contains() {
        let held = Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::ALTGR;

        // A single bit is the easy half, and the half that hides an `!= 0` written for an
        // `== other`. The question a caller asks is about a combination: two modifiers held out of
        // three is a match, and two out of one is not.
        assert!(held.contains(Modifiers::SHIFT | Modifiers::CONTROL));
        assert!(held.contains(held));
        assert!(!Modifiers::SHIFT.contains(Modifiers::SHIFT | Modifiers::CONTROL));
        assert!(!(Modifiers::SHIFT | Modifiers::ALT).contains(Modifiers::CONTROL | Modifiers::ALT));
        // Nothing holds every modifier of the empty combination, so everything does.
        assert!(Modifiers::NONE.contains(Modifiers::NONE));
        assert!(held.contains(Modifiers::NONE));
        assert!(!Modifiers::NONE.contains(Modifiers::SHIFT));
    }

    #[test]
    fn a_modifier_bit_and_a_map_index_are_different_numbers() {
        // `Entry::Modifier` and its two neighbours carry a `KG_*` bit number, and a map index is a
        // mask. Three is the pair that shows it: bit 3 is alt, and index 3 is shift over altgr.
        assert_eq!(Modifiers::from_bit(sys::KG_ALT as u8), Some(Modifiers::ALT));
        assert_eq!(
            Modifiers::from_bit(sys::KG_SHIFT as u8),
            Some(Modifiers::SHIFT)
        );
        assert_eq!(
            Modifiers::from_bit(sys::KG_CTRLR as u8),
            Some(Modifiers::RIGHT_CONTROL)
        );
        assert_ne!(
            Modifiers::from_bit(3).expect("bit 3 has a mask"),
            Modifiers::from_index(3)
        );
        // `KG_CAPSSHIFT` is the bit a table entry can name and a map index cannot hold. Shifting
        // by it would overflow the mask, so the answer is that there is no map.
        assert_eq!(Modifiers::from_bit(sys::KG_CAPSSHIFT as u8), None);
        assert_eq!(Modifiers::from_bit(u8::MAX), None);
    }

    #[test]
    fn a_key_code_past_the_last_entry_of_a_console_keymap_has_no_index() {
        // A truncated code names another key. `KEY_FN_F1` is `0x1d2` and the low byte of it is
        // `KEY_PRINT`, so a caller asking about a laptop's function key would be told what the
        // print key does.
        assert_eq!(index(Key::KEY_A), Some(30));
        assert_eq!(index(Key::new(255)), Some(255));
        assert_eq!(index(Key::new(256)), None);
        assert_eq!(index(Key::new(0x1d2)), None, "`KEY_FN_F1` has no entry");
        // Every button is past the keymap too, and a backend funnelling one device's events
        // through `Console::entry` meets one on every mouse click.
        assert_eq!(index(Key::BTN_LEFT), None);
    }

    #[test]
    fn the_mode_a_console_reports_is_read_rather_than_assumed() {
        assert_eq!(Mode::from_raw(sys::K_RAW as c_int), Mode::Raw);
        assert_eq!(Mode::from_raw(sys::K_MEDIUMRAW as c_int), Mode::MediumRaw);
        assert_eq!(Mode::from_raw(sys::K_XLATE as c_int), Mode::Translate);
        assert_eq!(Mode::from_raw(sys::K_UNICODE as c_int), Mode::Unicode);
        assert_eq!(Mode::from_raw(sys::K_OFF as c_int), Mode::Off);
        // A kernel with a sixth mode is reported as having one, because a caller reading `Raw`
        // for it would draw the wrong conclusion about every entry it then read.
        assert_eq!(Mode::from_raw(99), Mode::Other(99));
    }

    #[test]
    fn the_one_mode_that_keeps_a_code_point_entry_is_the_unicode_one() {
        // This pins which mode the answer names. That the *kernel* behaves this way is a claim
        // about a running console, and `tests/console.rs` is where it is held up against one.
        assert!(Mode::Unicode.keeps_unicode_entries());
        assert!(!Mode::Translate.keeps_unicode_entries());
        assert!(!Mode::MediumRaw.keeps_unicode_entries());
        assert!(!Mode::Raw.keeps_unicode_entries());
        assert!(!Mode::Off.keeps_unicode_entries());
    }
}
