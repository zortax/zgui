//! Finding libxkbcommon and its data, and saying so when the machine has neither.
//!
//! `cargo xtask ledger ignored` forbids switching a test off, and states the alternative: a test
//! that needs something the machine may lack looks for it, reports on standard error that it did
//! not find it, and returns. The refusal is then a fact about the machine, printed where it
//! happened, rather than a permanent property of the source.
//!
//! The parts fail apart, so the message says which part is missing. libxkbcommon is one package,
//! the keyboard data it reads is another, and neither is what a refused allocation means.

use zgui_xkb::{Context, Error, Keymap, RuleNames};

/// Returns a context over this machine's libxkbcommon, or nothing.
pub(crate) fn context(test: &str) -> Option<Context> {
    match Context::new() {
        Ok(context) => Some(context),
        // The three ways this fails want three different things done about them, and a person
        // reading one line of standard error has only that line to act on.
        Err(error @ Error::Library { .. }) => {
            eprintln!(
                "{test}: {error}, so nothing was asserted; install libxkbcommon to run it, or \
                 put it on `LD_LIBRARY_PATH`"
            );
            None
        }
        Err(error @ Error::Symbol { .. }) => {
            eprintln!(
                "{test}: {error}, so nothing was asserted; this machine's libxkbcommon is older \
                 than the interface this crate calls"
            );
            None
        }
        Err(error) => {
            eprintln!("{test}: {error}, so nothing was asserted");
            None
        }
    }
}

/// Returns a keymap compiled from the names libxkbcommon reads for itself, or nothing.
///
/// The context goes out of scope with the keymap still in hand, which is the arrangement this
/// crate promises: a keymap takes its own reference on the context it was compiled through, so
/// every test here runs with the context already dropped.
pub(crate) fn keymap(test: &str) -> Option<Keymap> {
    keymap_from(test, &RuleNames::default())
}

/// Returns a keymap compiled from `names`, or nothing.
///
/// A test that needs a layout of its own comes through here. The names reach the message, because
/// a keymap that will not compile from `de` is a different missing package from one that will not
/// compile at all.
pub(crate) fn keymap_from(test: &str, names: &RuleNames) -> Option<Keymap> {
    let context = context(test)?;
    match context.keymap(names) {
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
