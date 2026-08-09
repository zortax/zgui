//! An `xkb_context`, and the names a keymap is compiled from.
//!
//! A context is where libxkbcommon keeps its include paths and its log. Every keymap and every
//! compose table is compiled through one, and one context serves as many of them as a caller
//! wants.

use std::ffi::{CString, c_char};
use std::fmt;
use std::ptr;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::compose::ComposeTable;
use crate::error::{Error, Result};
use crate::keymap::{Keymap, Keysym};
use crate::library::{Library, NO_FLAGS, RuleNamesRaw, XkbContext, read_text};

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

impl RuleNames {
    /// The names, each with the field it belongs to.
    fn fields(&self) -> [(&'static str, &Option<String>); 5] {
        [
            ("rules", &self.rules),
            ("model", &self.model),
            ("layout", &self.layout),
            ("variant", &self.variant),
            ("options", &self.options),
        ]
    }
}

/// Writes each name that is set, and names the machine's own settings when every field is empty.
impl fmt::Display for RuleNames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut written = 0;
        for (field, name) in self.fields() {
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
struct Held {
    /// The names, in the order [`RuleNames::fields`] gives them.
    names: [Option<CString>; 5],
}

impl Held {
    /// Copies every name that is set into a C string.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Name`] when a name holds a zero byte.
    fn new(names: &RuleNames) -> Result<Self> {
        let mut held = [const { None }; 5];
        for (slot, (field, name)) in held.iter_mut().zip(names.fields()) {
            if let Some(name) = name {
                *slot = Some(CString::new(name.as_str()).map_err(|_| Error::Name { field })?);
            }
        }
        Ok(Self { names: held })
    }

    /// The structure libxkbcommon reads the names out of.
    ///
    /// The pointers borrow `self` and carry no lifetime of their own, so the caller keeps the
    /// [`Held`] alive across the call that takes this. An empty name crosses as null, which is how
    /// the C interface spells "use the default".
    fn raw(&self) -> RuleNamesRaw {
        let at = |index: usize| -> *const c_char {
            self.names[index]
                .as_ref()
                .map_or(ptr::null(), |name| name.as_ptr())
        };
        RuleNamesRaw {
            rules: at(0),
            model: at(1),
            layout: at(2),
            variant: at(3),
            options: at(4),
        }
    }
}

/// An `xkb_context`: what keymaps and compose tables are compiled through.
///
/// The context holds a share of the loaded [`Library`], so the shared object stays mapped for at
/// least as long as this does.
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
    /// Returns [`Error::Refused`] when the context cannot be built, which is an allocation
    /// failure and nothing else.
    pub fn over(library: Arc<Library>) -> Result<Self> {
        // SAFETY: the symbol is `xkb_context_new`, which takes a flag word and returns an owned
        // context or nothing. `NO_FLAGS` asks for the default include paths and the environment's
        // own names, which a session on this machine is already configured with.
        let handle = unsafe { (library.symbols.context_new)(NO_FLAGS) };
        let handle = NonNull::new(handle).ok_or(Error::Refused {
            what: "xkb_context_new",
        })?;
        Ok(Self { library, handle })
    }

    /// Returns the library this context calls through.
    ///
    /// This is what a second context is made over, so that one mapping serves several keyboards.
    pub fn library(&self) -> &Arc<Library> {
        &self.library
    }

    /// Compiles a keymap from `names`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Name`] when a name holds a zero byte, and [`Error::Keymap`] when nothing
    /// compiles from the names — a layout the rules do not know, and keyboard data that is not
    /// installed, both arrive that way.
    pub fn keymap(&self, names: &RuleNames) -> Result<Keymap> {
        let held = Held::new(names)?;
        let raw = held.raw();
        // SAFETY: the symbol is `xkb_keymap_new_from_names`. The context is live, and `raw` points
        // at a structure whose five members point into `held`, which outlives the call. The keymap
        // that comes back is owned here, and it takes its own reference on the context, so this
        // context may go first.
        let handle = unsafe {
            (self.library.symbols.keymap_new_from_names)(self.handle.as_ptr(), &raw, NO_FLAGS)
        };
        let handle = NonNull::new(handle).ok_or_else(|| Error::Keymap {
            names: names.clone(),
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
    /// Returns [`Error::Name`] when the locale holds a zero byte, and [`Error::Compose`] when the
    /// machine has no compose file for it. The compose data ships apart from the keyboard data, so
    /// this fails on machines where [`Context::keymap`] works.
    pub fn compose_table(&self, locale: &str) -> Result<ComposeTable> {
        let held = CString::new(locale).map_err(|_| Error::Name { field: "locale" })?;
        // SAFETY: the symbol is `xkb_compose_table_new_from_locale`. The context is live, and the
        // locale is a C string that outlives the call. The table that comes back is owned here and
        // holds its own reference on the context.
        let handle = unsafe {
            (self.library.symbols.compose_table_new_from_locale)(
                self.handle.as_ptr(),
                held.as_ptr(),
                NO_FLAGS,
            )
        };
        let handle = NonNull::new(handle).ok_or_else(|| Error::Compose {
            locale: locale.to_owned(),
        })?;
        Ok(ComposeTable::new(Arc::clone(&self.library), handle))
    }

    /// Returns what a keysym is called: `a`, `A`, `Shift_L`, `Multi_key`.
    ///
    /// The name is the one xkb data files are written in, so it is what a shortcut table and a log
    /// line are keyed by. A number the table does not hold is named in hexadecimal instead, and a
    /// number past the range keysyms are drawn from answers nothing.
    pub fn keysym_name(&self, sym: Keysym) -> Option<String> {
        read_text(|buffer, size| {
            // SAFETY: the symbol is `xkb_keysym_get_name`, which writes into `buffer` up to `size`
            // bytes. `read_text` passes the buffer it owns and the length of that buffer.
            unsafe { (self.library.symbols.keysym_get_name)(sym.raw(), buffer, size) }
        })
    }
}

/// Gives the context back to the library.
///
/// The body runs before the fields go, so the library is still mapped when the call is made.
impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the symbol is `xkb_context_unref`, and this is the reference taken by
        // `xkb_context_new`. Nothing here holds another, so it is dropped exactly once.
        unsafe { (self.library.symbols.context_unref)(self.handle.as_ptr()) }
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
