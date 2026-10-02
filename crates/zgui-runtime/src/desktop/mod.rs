//! What an application has outside its windows: the application menu, the request to show it
//! again, and its exit.
//!
//! ```no_run
//! use zgui_runtime::desktop::{on_exit, on_menu, on_reopen};
//!
//! # fn example() {
//! let reopen = on_reopen(|| println!("show the main window"));
//! let menu = on_menu(|id| println!("picked {}", id.as_str()));
//! let exit = on_exit(|| println!("write what is unsaved"));
//! # drop((reopen, menu, exit));
//! # }
//! ```

mod hooks;
mod menu;
mod replay;

pub use crate::desktop::hooks::{AppHookGuard, on_exit, on_menu, on_reopen};

pub(crate) use crate::desktop::hooks::AppHooks;
pub(crate) use crate::desktop::menu::{MenuKeys, MenuSlot};
pub(crate) use crate::desktop::replay::replay;

#[cfg(test)]
mod tests;
