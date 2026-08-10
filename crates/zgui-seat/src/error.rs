//! Refusals, and what each one carries.

use std::fmt;

/// The result of a call into libseat.
pub type Result<T> = std::result::Result<T, Error>;

/// What a call into libseat refused with.
///
/// The enumeration grows with the tasks that produce its cases: `cargo xtask ledger inert` fails a
/// variant that is matched on and built nowhere, and it is right to. A case with a branch and no
/// producer reads exactly like a working feature that is merely unused.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// libseat could not be opened.
    ///
    /// The crate is shaped around this case. The library is opened at run time, so a build needs
    /// neither it nor a session daemon, and a machine without it hands the caller this value to
    /// read. A console session answers it by opening the devices itself, which needs root and a
    /// terminal reserved in advance.
    ///
    /// A machine that has libseat lands here too when something libseat needs is missing, such as
    /// libsystemd. `reason` tells the two apart, so a report carries it.
    Library {
        /// The sonames that were tried, in order.
        tried: Vec<String>,
        /// What the loader said about the last one.
        reason: String,
    },
    /// The library is here, and it does not carry a symbol this crate calls.
    ///
    /// A libseat older than the interface this was written against lands here, and so does a shared
    /// object that is not libseat at all. It is a separate case from an absent library because the
    /// answer differs: a caller reports which symbol is missing, and a person reads a version out
    /// of that.
    Symbol {
        /// The symbol that was asked for.
        name: &'static str,
        /// What the loader said, or why the address it gave cannot be used.
        reason: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library { tried, reason } => write!(
                f,
                "libseat could not be opened; tried {}: {reason}",
                tried.join(", ")
            ),
            Self::Symbol { name, reason } => write!(f, "libseat has no `{name}`: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    //! What each case reads as.

    use super::*;

    #[test]
    fn an_absent_library_names_every_soname_and_says_why() {
        // A person reading this has to be able to check the machine against it, so the message
        // carries the whole list rather than the last name tried. It says nothing about why the
        // open failed beyond what the loader said: a present libseat with an absent libsystemd
        // arrives here too.
        let error = Error::Library {
            tried: vec!["libseat.so.1".to_owned(), "libseat.so".to_owned()],
            reason: "cannot open shared object file".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "libseat could not be opened; tried libseat.so.1, libseat.so: cannot open shared \
             object file"
        );
    }

    #[test]
    fn a_missing_symbol_is_named() {
        let error = Error::Symbol {
            name: "libseat_switch_session",
            reason: "undefined symbol".to_owned(),
        };

        assert!(error.to_string().contains("libseat_switch_session"));
    }
}
