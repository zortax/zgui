//! Dead keys and compose sequences.
//!
//! A compose sequence is several keys that together make one character: the compose key, `a` and
//! `e` make `æ`. A dead key is the same machine with the sequence started by the key itself, which
//! is how `´` then `e` makes `é`.
//!
//! The sequences live in the X11 locale data, which ships apart from the keyboard data. So a
//! machine that compiles every keymap can still hold no compose file, and
//! [`crate::Context::compose_table`] answers [`crate::Error::Compose`] there. A library with no
//! compose interface at all answers [`crate::Error::Symbol`] and costs a keyboard nothing else.
//!
//! Every keysym a press produces is fed here first. While [`Status::Composing`] holds, the press
//! produced nothing a caller should show. [`Status::Composed`] replaces what the key would have
//! produced with [`ComposeState::text`], and [`Status::Cancelled`] throws the sequence away.

use std::env;
use std::ffi::c_uint;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::keymap::Keysym;
use crate::library::{Compose, Library, NO_FLAGS, XkbComposeState, XkbComposeTable, read_text};

/// Returns the locale a session composes in, as libxkbcommon's own callers read it.
///
/// `LC_ALL` overrides everything, then `LC_CTYPE`, then `LANG`. A session that sets none of them
/// composes in the `C` locale.
///
/// ```
/// let locale = zgui_xkb::locale_from_environment();
///
/// assert!(!locale.is_empty());
/// ```
pub fn locale_from_environment() -> String {
    for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(locale) = env::var(name)
            && !locale.is_empty()
        {
            return locale;
        }
    }
    "C".to_owned()
}

/// Whether a keysym meant anything to the sequence machine.
///
/// A caller keeping a record of what was typed so far reads this: an ignored keysym changed
/// nothing and belongs in no record.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feed {
    /// The keysym meant nothing here. A modifier going down is the ordinary case.
    Ignored,
    /// The keysym was taken, and [`ComposeState::status`] says what it did.
    Accepted,
}

impl Feed {
    /// Reads what `xkb_compose_state_feed` answered.
    fn from_raw(raw: c_uint) -> Self {
        if raw == 0 {
            Self::Ignored
        } else {
            Self::Accepted
        }
    }
}

/// Where a sequence has got to.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// No sequence is under way. The key produced what it usually produces.
    Nothing,
    /// A sequence has begun and has not finished. The key produced nothing to show.
    Composing,
    /// A sequence finished. [`ComposeState::text`] carries what it produced, in place of the key's
    /// own.
    Composed,
    /// A sequence was thrown away, because a key arrived that continues none of it.
    Cancelled,
}

impl Status {
    /// Reads what `xkb_compose_state_get_status` answered.
    ///
    /// A value from a libxkbcommon that has grown a fifth status reads as [`Status::Nothing`],
    /// which leaves the key producing what it usually produces.
    fn from_raw(raw: c_uint) -> Self {
        match raw {
            1 => Self::Composing,
            2 => Self::Composed,
            3 => Self::Cancelled,
            _ => Self::Nothing,
        }
    }
}

/// The compose sequences of one locale, compiled.
///
/// A table is read-only once it is compiled, and one table serves as many [`ComposeState`]s as
/// there are keyboards.
///
/// The table holds a share of the loaded [`Library`] and takes its own reference on the context it
/// was compiled through, so the [`crate::Context`] may be dropped first.
///
/// ```no_run
/// use zgui_xkb::{Context, Keysym, Status, locale_from_environment};
///
/// let context = Context::new()?;
/// let table = context.compose_table(&locale_from_environment())?;
/// let mut compose = table.state()?;
///
/// // `XKB_KEY_Multi_key`, the compose key.
/// compose.feed(Keysym::from_raw(0xff20));
/// assert_eq!(compose.status(), Status::Composing);
/// # Ok::<(), zgui_xkb::Error>(())
/// ```
#[derive(Debug)]
pub struct ComposeTable {
    /// The library every call goes through, held so that the mapping outlives this.
    library: Arc<Library>,
    /// The compose half of the interface, resolved when the table was compiled.
    symbols: Compose,
    /// The table itself.
    handle: NonNull<XkbComposeTable>,
}

impl ComposeTable {
    /// Takes ownership of a compiled table.
    ///
    /// The symbols are copied in rather than looked up again. A table exists only because the
    /// whole group resolved, so nothing below here has an absent symbol to answer for.
    pub(crate) fn new(
        library: Arc<Library>,
        symbols: Compose,
        handle: NonNull<XkbComposeTable>,
    ) -> Self {
        Self {
            library,
            symbols,
            handle,
        }
    }

    /// Creates the sequence machine that keysyms are fed to.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Refused`] when the state cannot be built, which is an allocation failure
    /// and nothing else.
    pub fn state(&self) -> Result<ComposeState> {
        // SAFETY: the symbol is `xkb_compose_state_new`. The table is live, and the state that
        // comes back is owned by the caller and takes its own reference on the table.
        let handle = unsafe { (self.symbols.state_new)(self.handle.as_ptr(), NO_FLAGS) };
        let handle = NonNull::new(handle).ok_or(Error::Refused {
            what: "xkb_compose_state_new",
        })?;
        Ok(ComposeState {
            _library: Arc::clone(&self.library),
            symbols: self.symbols,
            handle,
        })
    }
}

/// Gives the table back to the library.
///
/// The body runs before the fields go, so the library is still mapped when the call is made.
impl Drop for ComposeTable {
    fn drop(&mut self) {
        // SAFETY: the symbol is `xkb_compose_table_unref`, and this is the reference taken by
        // `xkb_compose_table_new_from_locale`. Nothing here holds another, so it is dropped
        // exactly once.
        unsafe { (self.symbols.table_unref)(self.handle.as_ptr()) }
    }
}

/// How far through a sequence one keyboard is.
///
/// One state stands for one keyboard, for the same reason a [`crate::State`] does: a sequence
/// begun on one keyboard is not continued by a key on another.
#[derive(Debug)]
pub struct ComposeState {
    /// The open shared object, held and never read: every address this calls points inside it, so
    /// the mapping has to outlive this state.
    _library: Arc<Library>,
    /// The compose half of the interface.
    symbols: Compose,
    /// The state itself.
    handle: NonNull<XkbComposeState>,
}

impl ComposeState {
    /// Feeds one keysym to the sequence machine.
    ///
    /// The keysym is the one the press produced, which is [`crate::Press::sym`]. Read
    /// [`ComposeState::status`] afterwards to find what the press should show.
    pub fn feed(&mut self, sym: Keysym) -> Feed {
        // SAFETY: the symbol is `xkb_compose_state_feed`, which advances the state and answers
        // whether the keysym meant anything. Any number is a keysym, so there is nothing to check.
        let raw = unsafe { (self.symbols.state_feed)(self.handle.as_ptr(), sym.raw()) };
        Feed::from_raw(raw)
    }

    /// Returns where the sequence has got to.
    pub fn status(&self) -> Status {
        // SAFETY: the symbol is `xkb_compose_state_get_status`, which reads the state.
        let raw = unsafe { (self.symbols.state_get_status)(self.handle.as_ptr()) };
        Status::from_raw(raw)
    }

    /// Returns what a finished sequence produced.
    ///
    /// Answers nothing while the status is anything but [`Status::Composed`], and for a sequence
    /// whose result is a keysym with no text of its own.
    pub fn text(&self) -> Option<String> {
        read_text(|buffer, size| {
            // SAFETY: the symbol is `xkb_compose_state_get_utf8`, which writes into `buffer` up to
            // `size` bytes and answers how many the whole string needs. `read_text` passes the
            // buffer it owns and the length of that buffer.
            unsafe { (self.symbols.state_get_utf8)(self.handle.as_ptr(), buffer, size) }
        })
    }

    /// Returns the keysym a finished sequence produced.
    ///
    /// Answers nothing while the status is anything but [`Status::Composed`], and for a sequence
    /// that produced text with no single keysym behind it.
    pub fn sym(&self) -> Option<Keysym> {
        // SAFETY: the symbol is `xkb_compose_state_get_one_sym`, which reads the state.
        let raw = unsafe { (self.symbols.state_get_one_sym)(self.handle.as_ptr()) };
        Some(Keysym::from_raw(raw)).filter(|sym| !sym.is_none())
    }

    /// Throws away whatever sequence is under way.
    ///
    /// This is what a caller does when the keyboard's focus moves: a half-typed sequence belongs
    /// to the window it was begun in.
    pub fn reset(&mut self) {
        // SAFETY: the symbol is `xkb_compose_state_reset`, which returns the state to
        // `XKB_COMPOSE_NOTHING` and discards the sequence.
        unsafe { (self.symbols.state_reset)(self.handle.as_ptr()) }
    }
}

/// Gives the sequence machine back to the library.
///
/// The body runs before the fields go, so the library is still mapped when the call is made.
impl Drop for ComposeState {
    fn drop(&mut self) {
        // SAFETY: the symbol is `xkb_compose_state_unref`, and this is the reference taken by
        // `xkb_compose_state_new`. Nothing here holds another, so it is dropped exactly once.
        unsafe { (self.symbols.state_unref)(self.handle.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    //! The two enumerations, over no library at all.

    use super::*;

    #[test]
    fn the_four_statuses_read_as_the_library_numbers_them() {
        assert_eq!(Status::from_raw(0), Status::Nothing);
        assert_eq!(Status::from_raw(1), Status::Composing);
        assert_eq!(Status::from_raw(2), Status::Composed);
        assert_eq!(Status::from_raw(3), Status::Cancelled);
    }

    #[test]
    fn a_status_this_crate_does_not_know_leaves_the_key_alone() {
        // A libxkbcommon with a fifth status would otherwise take a character away from whoever
        // typed it. `Nothing` is the answer that shows what the key produced.
        assert_eq!(Status::from_raw(99), Status::Nothing);
    }

    #[test]
    fn a_keysym_the_machine_took_reads_as_accepted() {
        assert_eq!(Feed::from_raw(0), Feed::Ignored);
        assert_eq!(Feed::from_raw(1), Feed::Accepted);
    }

    #[test]
    fn a_session_that_names_no_locale_composes_in_c() {
        // The three names are read in the order every C library reads them, so a session that sets
        // `LC_ALL` gets that one whatever `LANG` says. What is asserted here is the floor: the
        // answer is a locale name rather than an empty string a lookup would fail on.
        let locale = locale_from_environment();

        assert!(!locale.is_empty(), "there is always a locale to compose in");
    }
}
