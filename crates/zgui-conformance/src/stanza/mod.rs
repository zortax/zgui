//! The style engine's own property definitions, read as data.
//!
//! # Why a second input exists at all
//!
//! The census's denominator is the list of properties the engine *generated*, and that list
//! structurally cannot contain a property built only for another engine: the generator returns
//! before defining one. So a whole class of unreachable property is not unclassified but
//! **invisible** — the census is green on it and always will be, and the only record that it exists
//! is prose in a register. Twenty-one SVG paint longhands are the worked example: each is
//! unreachable for a reason no census could state, and prose describing them carried the wrong
//! reason for as long as prose was the only record.
//!
//! The engine's definitions, on the other hand, list every property for every engine, with the key
//! that decides which builds get it and the preference that gates it. Reading them turns *why* a
//! property is unreachable from prose into something a build can be failed on.
//!
//! ```no_run
//! use zgui_conformance::stanza::Definitions;
//!
//! let definitions = Definitions::load().expect("the engine's definitions are readable");
//! let fill = definitions.get("fill").expect("the engine defines it");
//! assert!(fill.is_other_engine_only());
//! ```

pub mod locate;
pub mod parse;

pub use crate::stanza::locate::{LocateError, source_path};
pub use crate::stanza::parse::{Definitions, Stanza};
