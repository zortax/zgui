//! The keyboard layout, as libxkbcommon reads it.
//!
//! A key code says which key moved. A layout says what the key means: which character it produces,
//! which level a held modifier puts it on, which dead key it begins, and which shortcut it stands
//! for. On Linux libxkbcommon holds that answer, and this crate is how it is asked.
//!
//! The crate names no zgui crate and is usable on its own. It is written to be named by
//! `zgui-platform-drm`, one step along the same layer, and by nothing else.
//!
//! # Loading
//!
//! libxkbcommon is opened at run time with `dlopen` and is never linked. A build needs neither the
//! library nor the keyboard data it reads, and a machine that has neither still starts: the absence
//! arrives as [`Error::Library`], which a caller reads and answers. Every symbol is resolved at run
//! time, so the crate compiles on any host, and what it finds is a property of the machine that
//! runs it.
//!
//! # Usage
//!
//! [`Context::new`] opens the library and makes a context. [`Context::keymap`] compiles a keymap
//! from [`RuleNames`], and [`Keymap::state`] makes the state that is fed key transitions.
//! [`State::press`] and [`State::release`] take a [`Keycode`], which carries the offset between the
//! kernel's key codes and xkb's. [`Context::compose_table`] adds dead keys and compose sequences on
//! top.
//!
//! ```no_run
//! use zgui_xkb::{Context, Keycode, RuleNames};
//!
//! let context = Context::new()?;
//! let keymap = context.keymap(&RuleNames::default())?;
//! let mut state = keymap.state()?;
//!
//! let key = Keycode::from_evdev(30);
//! let press = state.press(key);
//! println!("{:?} produced {:?}", press.sym, press.text);
//! state.release(key);
//! # Ok::<(), zgui_xkb::Error>(())
//! ```
//!
//! # Diagnostics
//!
//! libxkbcommon prints its own diagnostics to standard error. Every [`Context`] takes the messages
//! away from the library when it is made, so this crate writes nothing there.
//! [`Context::set_log_sink`] is how a caller asks for them. The reason a keymap refused to compile
//! reaches [`Error::Keymap`] whether a sink is set or not.
//!
//! # Threads
//!
//! A [`Context`] and everything compiled through it stay on the thread that made them. None of them
//! is `Send` or `Sync`, because libxkbcommon refcounts and mutates them without a lock. A
//! [`Library`] is both, so a process with two keyboard threads opens the shared object once and
//! makes a context on each thread over the same share.

#![deny(missing_docs)]
// libxkbcommon hands its diagnostics over as a format string and a `va_list`, exactly as `vprintf`
// takes them. Receiving one needs this feature, and a received message is one that never reached
// standard error. See `log.rs`; nothing else in the crate uses it.
#![feature(c_variadic)]
// This crate is on the unsafe ledger's allowlist for one reason: libxkbcommon is opened at run
// time and called through the addresses that come back, so every entry point is an FFI call
// through a resolved symbol. Every unsafe block states what makes it sound.
#![allow(unsafe_code)]

pub mod compose;
pub mod context;
pub mod error;
pub mod keymap;
pub mod library;
pub mod log;
pub mod state;

pub use crate::compose::{ComposeState, ComposeTable, Feed, Status, locale_from_environment};
pub use crate::context::{Context, RuleNames};
pub use crate::error::{Error, Result};
pub use crate::keymap::{EVDEV_OFFSET, Keycode, Keymap, Keysym, Layout, Level};
pub use crate::library::Library;
pub use crate::log::{LogLevel, Sink};
pub use crate::state::{Changed, Modifiers, Press, State};
