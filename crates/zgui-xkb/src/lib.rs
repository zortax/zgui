//! The keyboard layout, as libxkbcommon reads it.
//!
//! A key code says which key moved. A layout says what the key means: which character it produces,
//! which level a held modifier puts it on, which dead key it begins, and which shortcut it stands
//! for. On Linux libxkbcommon holds that answer, and this crate is how it is asked.
//!
//! The crate names no zgui crate and is usable on its own. It is written to be named by
//! `zgui-platform-drm`, one step along the same layer.
//!
//! # Loading
//!
//! libxkbcommon is opened at run time with `dlopen` and is never linked. A build needs neither the
//! library nor the keyboard data it reads, and a machine that has neither still starts: the
//! absence arrives as [`Error::Library`], which a caller reads and answers. Every symbol is
//! resolved at run time, so the crate compiles on any host, and what it finds is a property of the
//! machine that runs it.
//!
//! # Usage
//!
//! [`Context::new`] opens the library and makes a context. [`Context::keymap`] compiles a keymap
//! from [`RuleNames`], and [`Keymap::state`] makes the state that is fed key transitions.
//! [`State::press`] and [`State::release`] take a [`Keycode`], which carries the offset between
//! the kernel's key codes and xkb's. [`Context::compose_table`] adds dead keys and compose
//! sequences on top.
//!
//! # Threads
//!
//! A [`Context`] and everything compiled through it stay on the thread that made them. None of
//! them is `Send` or `Sync`, because libxkbcommon refcounts and mutates them without a lock. A
//! [`Library`] is both, so a process with two keyboard threads opens the shared object once and
//! makes a context on each thread over the same share.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for one reason: libxkbcommon is opened at run
// time and called through the addresses that come back, so every entry point is an FFI call
// through a resolved symbol. Every unsafe block states what makes it sound.
#![allow(unsafe_code)]

pub mod compose;
pub mod context;
pub mod error;
pub mod keymap;
pub mod library;
pub mod state;

pub use crate::compose::{ComposeState, ComposeTable, Feed, Status, locale_from_environment};
pub use crate::context::{Context, RuleNames};
pub use crate::error::{Error, Result};
pub use crate::keymap::{EVDEV_OFFSET, Keycode, Keymap, Keysym, Layout, Level};
pub use crate::library::Library;
pub use crate::state::{Changed, Modifiers, Press, State};
