//! The kernel's input interface: devices, capabilities, event batches and the console keymap.
//!
//! This is what libevdev is, written in Rust against the uapi headers. It links no C. Every call
//! is an ioctl or a read on a file descriptor, issued through `rustix`'s `linux_raw` backend.
//!
//! The crate names no zgui crate and is usable on its own. What depends on it is
//! `zgui-platform-drm`, one step along the same layer. Most of it is tested without a device: a
//! capability bitmap and a stream of event records are data, so the classification and the batching
//! run with nothing plugged in.
//!
//! # Devices
//!
//! [`nodes`] answers every node under `/dev/input`, in the order the kernel numbers them. A caller
//! opens each one the way its own machine allows: [`Device::open`] opens the node, which needs the
//! group it belongs to, and [`Device::over`] builds a device on a descriptor a session daemon
//! opened and handed over.
//!
//! A [`Device`] says what it is, what it can report, and what it just reported. It hands out its
//! descriptor through [`AsFd`](std::os::fd::AsFd) so a loop can park on it, and a read answers
//! [`Batch`]es. The kernel groups everything that happened at one moment, and a reader that took a
//! group apart would move a pointer twice for one movement.
//!
//! # Devices that arrive later
//!
//! [`nodes`] reads the directory once. [`Watch`] asks the same directory a second way: it holds an
//! inotify descriptor a loop parks on, and it names the nodes that arrive while the program runs.
//! It watches the change of ownership as well as the creation, because udev sets a new node's owner
//! after the kernel makes it.
//!
//! # Layout
//!
//! A device says which key moved. What a key *means* is a layout's answer, and [`Console`] reads
//! the one the kernel's own console driver holds. That is the layout of last resort: a machine with
//! libxkbcommon and its keyboard data has a better one, and this is what a machine with neither
//! still has. [`console`] says plainly what it cannot express.
//!
//! # The re-exports
//!
//! Every type a public method returns or takes is re-exported here, so a caller writes
//! `zgui_evdev::Bitmap` and reaches into a module only to read about it. The modules stay public
//! because they group what a reader is looking for.
//!
//! # Platform
//!
//! Every module is the kernel's interface or something built directly on it. On any other platform
//! this crate holds nothing at all.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for one reason: every call into the kernel is an
// ioctl, and an event record is read out of a byte buffer. Every unsafe block states why it is
// sound.
#![allow(unsafe_code)]

// Each module is gated and the crate itself is not, so `cargo check --workspace` passes on a
// machine this code could never run on.
#[cfg(target_os = "linux")]
pub mod code;
#[cfg(target_os = "linux")]
pub mod console;
#[cfg(target_os = "linux")]
pub mod device;
#[cfg(target_os = "linux")]
pub mod discover;
#[cfg(target_os = "linux")]
pub mod error;
#[cfg(target_os = "linux")]
pub mod event;
#[cfg(target_os = "linux")]
mod ioctl;
#[cfg(target_os = "linux")]
mod sys;
#[cfg(target_os = "linux")]
pub mod watch;

#[cfg(target_os = "linux")]
pub use crate::code::{Absolute, Code, EventType, Key, Relative, Synchronisation};
#[cfg(target_os = "linux")]
pub use crate::console::{Console, ENTER, Entry, Mode, Modifiers, Screen, Search};
#[cfg(target_os = "linux")]
pub use crate::device::{AxisRange, Bitmap, Capabilities, Device, Identity, Role, Roles};
#[cfg(target_os = "linux")]
pub use crate::discover::{DIRECTORY, Skipped, nodes, nodes_in};
#[cfg(target_os = "linux")]
pub use crate::error::{Error, Result};
#[cfg(target_os = "linux")]
pub use crate::event::{Batch, Event, Reader};
#[cfg(target_os = "linux")]
pub use crate::watch::Watch;
