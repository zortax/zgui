//! Finding libxkbcommon and its data, and saying so when the machine has neither.
//!
//! `cargo xtask ledger ignored` forbids switching a test off, and states the alternative: a test
//! that needs something the machine may lack looks for it, reports on standard error that it did
//! not find it, and returns. The refusal is then a fact about the machine, printed where it
//! happened, rather than a permanent property of the source.
//!
//! The two parts fail apart. libxkbcommon is one package and the keyboard data it reads is
//! another, so a machine can open the library and compile no keymap at all, and the message says
//! which of the two is missing.

use zgui_xkb::{Context, Keymap, RuleNames};

/// Returns a context over this machine's libxkbcommon, or nothing.
pub(crate) fn context(test: &str) -> Option<Context> {
    match Context::new() {
        Ok(context) => Some(context),
        Err(error) => {
            eprintln!(
                "{test}: {error}, so nothing was asserted; install libxkbcommon to run it, or \
                 put it on `LD_LIBRARY_PATH`"
            );
            None
        }
    }
}

/// Returns a keymap compiled from the names this machine is set to, or nothing.
///
/// The context goes out of scope with the keymap still in hand, which is the arrangement this
/// crate promises: a keymap takes its own reference on the context it was compiled through, so
/// every test here runs with the context already dropped.
pub(crate) fn keymap(test: &str) -> Option<Keymap> {
    let context = context(test)?;
    match context.keymap(&RuleNames::default()) {
        Ok(keymap) => Some(keymap),
        Err(error) => {
            eprintln!(
                "{test}: {error}, so nothing was asserted; install the `xkeyboard-config` data \
                 to run it"
            );
            None
        }
    }
}
