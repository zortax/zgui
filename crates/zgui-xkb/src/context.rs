//! An `xkb_context`, and the names a keymap is compiled from.
//!
//! A context is where libxkbcommon keeps its include paths and its log. Every keymap and every
//! compose table is compiled through one, and one context serves as many of them as a caller
//! wants.

use std::ffi::{CString, c_char};
use std::fmt;
use std::marker::PhantomData;
use std::ptr;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::compose::ComposeTable;
use crate::error::{Error, Result};
use crate::keymap::{Keymap, Keysym};
use crate::library::{Library, NO_FLAGS, RuleNamesRaw, XkbContext, read_text};
use crate::log::{self, LogLevel, Sink};

/// The names a keymap is compiled from.
///
/// This is xkb's RMLVO: the rules file that says how to read the rest, the model of the hardware,
/// the layout, the variant of that layout, and the options. A name left empty takes libxkbcommon's
/// own default, which it reads from `XKB_DEFAULT_RULES` and its four siblings and then from the
/// defaults its build was given. So [`RuleNames::default`] is what the environment is set to, and
/// the defaults libxkbcommon was built with where nothing set it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleNames {
    /// The rules file: `evdev` on Linux.
    pub rules: Option<String>,
    /// The model of the hardware: `pc105`.
    pub model: Option<String>,
    /// The layout, or several separated by commas: `us`, `de,us`.
    pub layout: Option<String>,
    /// The variant of each layout: `dvorak`, `nodeadkeys`.
    pub variant: Option<String>,
    /// The options, separated by commas: `compose:ralt,caps:swapescape`.
    pub options: Option<String>,
}

/// Writes each name that is set, and names the machine's own settings when every field is empty.
impl fmt::Display for RuleNames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fields = [
            ("rules", &self.rules),
            ("model", &self.model),
            ("layout", &self.layout),
            ("variant", &self.variant),
            ("options", &self.options),
        ];
        let mut written = 0;
        for (field, name) in fields {
            if let Some(name) = name {
                if written > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{field}={name}")?;
                written += 1;
            }
        }
        if written == 0 {
            f.write_str("the names this machine is set to")?;
        }
        Ok(())
    }
}

/// The five names as C strings, alive for the length of one call.
///
/// Each name is held in a field of its own and read back out of that field by name, so a field
/// added or moved here reaches the C structure through its own name rather than through a
/// position.
struct Held {
    /// The rules file.
    rules: Option<CString>,
    /// The model of the hardware.
    model: Option<CString>,
    /// The layout.
    layout: Option<CString>,
    /// The variant.
    variant: Option<CString>,
    /// The options.
    options: Option<CString>,
}

/// Copies one name into a C string.
///
/// # Errors
///
/// Returns [`Error::Name`] when the name holds a zero byte.
fn hold(field: &'static str, name: Option<&String>) -> Result<Option<CString>> {
    name.map(|name| CString::new(name.as_str()).map_err(|_| Error::Name { field }))
        .transpose()
}

impl Held {
    /// Copies every name that is set into a C string.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Name`] when a name holds a zero byte.
    fn new(names: &RuleNames) -> Result<Self> {
        Ok(Self {
            rules: hold("rules", names.rules.as_ref())?,
            model: hold("model", names.model.as_ref())?,
            layout: hold("layout", names.layout.as_ref())?,
            variant: hold("variant", names.variant.as_ref())?,
            options: hold("options", names.options.as_ref())?,
        })
    }

    /// Returns the structure libxkbcommon reads the names out of.
    ///
    /// The answer borrows `self`, so the strings live for at least as long as the structure that
    /// points into them. A name that is not set crosses as null, which the C interface reads as
    /// "use the default".
    fn raw(&self) -> RuleNamesRaw<'_> {
        let at = |name: &Option<CString>| -> *const c_char {
            name.as_ref().map_or(ptr::null(), |name| name.as_ptr())
        };
        RuleNamesRaw {
            rules: at(&self.rules),
            model: at(&self.model),
            layout: at(&self.layout),
            variant: at(&self.variant),
            options: at(&self.options),
            held: PhantomData,
        }
    }
}

/// An `xkb_context`: what keymaps and compose tables are compiled through.
///
/// The context holds a share of the loaded [`Library`], so the shared object stays mapped for at
/// least as long as this does.
///
/// libxkbcommon prints its diagnostics to standard error unless it is told otherwise, and a new
/// context tells it otherwise straight away. See [`crate::log`] for where the messages go instead
/// and how a caller asks for them.
///
/// ```no_run
/// use zgui_xkb::{Context, Keysym, RuleNames};
///
/// let context = Context::new()?;
/// let _keymap = context.keymap(&RuleNames::default())?;
///
/// assert_eq!(context.keysym_name(Keysym::from_raw(0x0061)).as_deref(), Some("a"));
/// # Ok::<(), zgui_xkb::Error>(())
/// ```
#[derive(Debug)]
pub struct Context {
    /// The library every call goes through.
    library: Arc<Library>,
    /// The context itself.
    handle: NonNull<XkbContext>,
}

impl Context {
    /// Opens libxkbcommon and makes a context over it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] when libxkbcommon is not on this machine, [`Error::Symbol`] when
    /// it is too old for this interface, and [`Error::Refused`] when the context cannot be built.
    pub fn new() -> Result<Self> {
        Self::over(Library::load()?)
    }

    /// Makes a context over a library that is already open.
    ///
    /// Two keyboards on one machine read the same layout data, so they share one library and pay
    /// for one mapping.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Refused`] when the context cannot be built.
    pub fn over(library: Arc<Library>) -> Result<Self> {
        // SAFETY: the symbol is `xkb_context_new`, which takes a flag word and returns an owned
        // context or nothing. `NO_FLAGS` asks for the default include paths and the environment's
        // own names, which a session on this machine is already configured with.
        let handle = unsafe { (library.symbols.core.context_new)(NO_FLAGS) };
        let handle = NonNull::new(handle).ok_or(Error::Refused {
            what: "xkb_context_new",
        })?;
        let context = Self { library, handle };
        context.take_the_log();
        Ok(context)
    }

    /// Returns the library this context calls through.
    ///
    /// This is what a second context is made over, so that one mapping serves several keyboards.
    pub fn library(&self) -> &Arc<Library> {
        &self.library
    }

    /// Sends libxkbcommon's diagnostics on this thread to `sink`.
    ///
    /// Nothing is written anywhere until this is called, and after it every message reaches `sink`
    /// and nothing else. `None` puts the messages back in the bin. The sink belongs to the thread
    /// rather than to this context; [`crate::log`] says why.
    ///
    /// A reason a keymap refused to compile reaches [`Error::Keymap`] whether a sink is set or
    /// not, because that reason is captured around the call that produced it.
    pub fn set_log_sink(&self, sink: Option<Sink>) {
        log::set_sink(sink);
    }

    /// Raises how much libxkbcommon has to say.
    ///
    /// The library's own default is [`LogLevel::Error`], and `XKB_LOG_LEVEL` in the environment
    /// can move it. This overrides both. Nothing reaches standard error at any level, because the
    /// context routes its messages the moment it is made.
    pub fn set_log_level(&self, level: LogLevel) {
        let Ok(logging) = self.library.symbols.logging else {
            return;
        };
        // SAFETY: the symbol is `xkb_context_set_log_level`, which stores a number in the context.
        // The context is live and the level is one the library names.
        unsafe { (logging.context_set_log_level)(self.handle.as_ptr(), level.raw()) }
    }

    /// Compiles a keymap from `names`.
    ///
    /// `xkb_keymap_new_from_names` compiled the V1 keymap text format until libxkbcommon 1.11.0
    /// and compiles V2 from that release on. This crate runs against whichever library the machine
    /// has, so the format a keymap is read in is the machine's answer. The two agree on every
    /// layout `xkeyboard-config` ships; they differ over keymap files written by hand that use the
    /// newer syntax.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Name`] when a name holds a zero byte, and [`Error::Keymap`] when nothing
    /// compiles from the names — a layout the rules do not know, and keyboard data that is not
    /// installed, both arrive that way, and the error carries libxkbcommon's own reason.
    pub fn keymap(&self, names: &RuleNames) -> Result<Keymap> {
        let held = Held::new(names)?;
        let raw = held.raw();
        let (handle, said) = log::capturing(|| {
            // SAFETY: the symbol is `xkb_keymap_new_from_names`. The context is live, and `raw`
            // borrows `held`, so the five names it points at outlive the call. The keymap that
            // comes back is owned here, and it takes its own reference on the context, so this
            // context may go first.
            unsafe {
                (self.library.symbols.core.keymap_new_from_names)(
                    self.handle.as_ptr(),
                    &raw,
                    NO_FLAGS,
                )
            }
        });
        let handle = NonNull::new(handle).ok_or_else(|| Error::Keymap {
            names: Box::new(names.clone()),
            reason: said,
        })?;
        Ok(Keymap::new(Arc::clone(&self.library), handle))
    }

    /// Compiles the compose sequences of `locale`.
    ///
    /// `locale` is a name such as `en_US.UTF-8`. [`crate::locale_from_environment`] reads the one
    /// the session is set to.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Symbol`] when the loaded library carries no compose interface,
    /// [`Error::Name`] when the locale holds a zero byte, and [`Error::Compose`] when the machine
    /// has no compose file for it. The compose data ships apart from the keyboard data, so this
    /// fails on machines where [`Context::keymap`] works.
    pub fn compose_table(&self, locale: &str) -> Result<ComposeTable> {
        let symbols = self.library.symbols.compose.clone().map_err(Error::from)?;
        let held = CString::new(locale).map_err(|_| Error::Name { field: "locale" })?;
        let (handle, said) = log::capturing(|| {
            // SAFETY: the symbol is `xkb_compose_table_new_from_locale`. The context is live, and
            // the locale is a C string that outlives the call. The table that comes back is owned
            // here and holds its own reference on the context.
            unsafe {
                (symbols.table_new_from_locale)(self.handle.as_ptr(), held.as_ptr(), NO_FLAGS)
            }
        });
        let handle = NonNull::new(handle).ok_or_else(|| Error::Compose {
            locale: locale.to_owned(),
            reason: said,
        })?;
        Ok(ComposeTable::new(
            Arc::clone(&self.library),
            symbols,
            handle,
        ))
    }

    /// Returns what a keysym is called: `a`, `A`, `Shift_L`, `Multi_key`.
    ///
    /// The name is the one xkb data files are written in, so it is what a shortcut table and a log
    /// line are keyed by. A number the table does not hold is named in hexadecimal instead, and a
    /// number past the range keysyms are drawn from answers nothing. So does every number when the
    /// loaded library carries no `xkb_keysym_get_name`, which costs a name and nothing else.
    pub fn keysym_name(&self, sym: Keysym) -> Option<String> {
        let naming = self.library.symbols.naming.as_ref().ok()?;
        read_text(|buffer, size| {
            // SAFETY: the symbol is `xkb_keysym_get_name`, which writes into `buffer` up to `size`
            // bytes. `read_text` passes the buffer it owns and the length of that buffer.
            unsafe { (naming.keysym_get_name)(sym.raw(), buffer, size) }
        })
    }

    /// Returns the keysym a name stands for: `a`, `Shift_L`, `Multi_key`.
    ///
    /// This is the other direction of [`Context::keysym_name`], and the two together check each
    /// other. A caller that keys a table by keysym *name* has written down names by hand, and a
    /// name libxkbcommon knows as an alias — `Mode_switch` for `ISO_Group_Shift` — is a row that
    /// reads correctly and matches nothing, because a press is looked up under whatever
    /// [`Context::keysym_name`] answers. Feeding a name through here and back through that call
    /// finds one.
    ///
    /// Answers nothing for a name no keysym has, and for every name when the loaded library
    /// carries no `xkb_keysym_from_name`. A name holding a zero byte answers nothing as well: a C
    /// string ends at its first zero, so such a name would be looked up cut short.
    ///
    /// The lookup is exact. libxkbcommon can also be asked to fold case, which this does not: `a`
    /// and `A` are two keysyms, and a case-folded lookup answers with the lower-case one.
    pub fn keysym_from_name(&self, name: &str) -> Option<Keysym> {
        let naming = self.library.symbols.naming.as_ref().ok()?;
        let held = CString::new(name).ok()?;
        // SAFETY: the symbol is `xkb_keysym_from_name`, which reads the C string it is given and
        // answers a keysym or `XKB_KEY_NoSymbol`. `held` outlives the call.
        let raw = unsafe { (naming.keysym_from_name)(held.as_ptr(), NO_FLAGS) };
        Some(Keysym::from_raw(raw)).filter(|sym| !sym.is_none())
    }

    /// Takes the diagnostics away from the library's own handler.
    ///
    /// Standard error is where libxkbcommon writes by default, and on a bare console that is often
    /// the terminal the caller is drawing on. Only the handler is replaced. How much the library
    /// has to say stays its own decision, which is errors by default and whatever `XKB_LOG_LEVEL`
    /// says otherwise. That setting now costs a longer reason rather than a corrupted screen, and
    /// [`Context::set_log_level`] overrides it either way.
    fn take_the_log(&self) {
        let Ok(logging) = self.library.symbols.logging else {
            return;
        };
        // SAFETY: the symbol is `xkb_context_set_log_fn`, which stores a callback in the context.
        // `deliver` has the signature the header gives for it, it reads its arguments once through
        // `vsnprintf`, and it is `extern "C"`, so a panic inside it aborts rather than unwinding
        // into C.
        unsafe { (logging.context_set_log_fn)(self.handle.as_ptr(), Some(log::deliver)) }
    }
}

/// Gives the context back to the library.
///
/// The body runs before the fields go, so the library is still mapped when the call is made.
impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the symbol is `xkb_context_unref`, and this is the reference taken by
        // `xkb_context_new`. Nothing here holds another, so it is dropped exactly once.
        unsafe { (self.library.symbols.core.context_unref)(self.handle.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    //! The names, over no library at all.

    use super::*;

    #[test]
    fn names_that_are_all_empty_read_as_the_machines_own() {
        assert_eq!(
            RuleNames::default().to_string(),
            "the names this machine is set to"
        );
    }

    #[test]
    fn a_name_that_is_set_is_named_with_its_field() {
        let names = RuleNames {
            layout: Some("de".to_owned()),
            variant: Some("nodeadkeys".to_owned()),
            ..RuleNames::default()
        };

        assert_eq!(names.to_string(), "layout=de variant=nodeadkeys");
    }

    #[test]
    fn a_name_with_a_zero_byte_is_refused_before_it_reaches_c() {
        // A C string ends at its first zero. `us\0gb` would compile a `us` keymap and drop the
        // rest without a word, so it is refused where the caller can still see it.
        let names = RuleNames {
            layout: Some("us\0gb".to_owned()),
            ..RuleNames::default()
        };

        match Held::new(&names) {
            Err(Error::Name { field }) => assert_eq!(field, "layout"),
            other => panic!("a zero byte is refused: {other:?}", other = other.err()),
        }
    }

    #[test]
    fn every_name_reaches_the_field_it_belongs_to() {
        // Five names of the same type, so a position that slipped would put the layout where the
        // model goes and compile a keymap that works. Each is read back by the field it went into.
        let names = RuleNames {
            rules: Some("evdev".to_owned()),
            model: Some("pc105".to_owned()),
            layout: Some("de".to_owned()),
            variant: Some("nodeadkeys".to_owned()),
            options: Some("caps:swapescape".to_owned()),
        };
        let held = Held::new(&names).expect("no name holds a zero byte");
        let raw = held.raw();

        let read = |pointer: *const c_char| {
            // SAFETY: every pointer came from a `CString` this `Held` owns and is still alive.
            unsafe { std::ffi::CStr::from_ptr(pointer) }
                .to_str()
                .expect("the names went in as text")
                .to_owned()
        };
        assert_eq!(read(raw.rules), "evdev");
        assert_eq!(read(raw.model), "pc105");
        assert_eq!(read(raw.layout), "de");
        assert_eq!(read(raw.variant), "nodeadkeys");
        assert_eq!(read(raw.options), "caps:swapescape");
    }

    #[test]
    fn a_name_that_is_empty_crosses_as_nothing() {
        // Null is how the C interface spells "use the default", so an unset name has to arrive as
        // null rather than as an empty string the rules would look up.
        let held = Held::new(&RuleNames {
            layout: Some("us".to_owned()),
            ..RuleNames::default()
        })
        .expect("the layout holds no zero byte");
        let raw = held.raw();

        assert!(raw.rules.is_null());
        assert!(raw.model.is_null());
        assert!(!raw.layout.is_null());
        assert!(raw.variant.is_null());
        assert!(raw.options.is_null());
    }
}
