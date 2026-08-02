//! Interned names: the strings the whole tree compares, and nothing else.
//!
//! Element names, attribute names, class names, identifiers and custom property names are read
//! far more often than they are created, and almost every read is a comparison. Interning turns
//! each of them into one shared copy plus an eight-byte handle, so a comparison is a pointer test
//! and a name can sit inside a hot per-node record without costing anything to copy.
//!
//! | Type | Names |
//! |---|---|
//! | [`ElementName`] | an element's local name |
//! | [`AttrName`] | an attribute's local name |
//! | [`ClassName`] | one entry of an element's class list |
//! | [`Ident`] | an identifier written in a style sheet |
//! | [`CustomPropertyName`] | a custom property, stored without its `--` prefix |
//! | [`NamespaceId`] | which namespace a name belongs to, as a one-byte index |
//! | [`Atom`] | the interned string all of the above are newtypes over |
//!
//! ```
//! use zgui_interned::{AttrName, ClassName, ElementName};
//!
//! let button = ElementName::new("button");
//! assert_eq!(button, ElementName::new("button"));
//!
//! // Distinct kinds of name are distinct types, so one cannot stand in for another.
//! let classes = [ClassName::new("primary"), ClassName::new("large")];
//! assert!(classes.contains(&ClassName::new("primary")));
//! assert!(AttrName::new("disabled") == "disabled");
//! ```
//!
//! # Why these types exist rather than the engines' own
//!
//! A style engine and a document language each have an interned-string type of their own, and
//! those types carry their language's vocabulary with them. Naming one of them in a shared
//! signature would spread that vocabulary across every crate that reads a name, and a core built
//! that way could never be reused by a document language that is not the one the engine was
//! written for.
//!
//! So the types here are the only name types the tree above the document speaks, and this crate
//! depends on no engine at all. A crate that does adapt an engine translates at its own boundary,
//! in one direction, in one place — and because the translation is a lookup rather than a
//! conversion of a value, it costs nothing that the engine was not already paying.
//!
//! # What interning promises, and what it does not
//!
//! Interning is exact and case-sensitive: `"DIV"` and `"div"` are different names. A document
//! language whose names are case-insensitive normalises before it interns, so that matching stays
//! a pointer comparison rather than becoming a character-by-character fold.
//!
//! Interned strings are never freed. Names come from a vocabulary that is small and effectively
//! fixed — the elements, attributes and properties a program uses — so the memory is bounded and
//! reached early. Interning attacker-controlled or unbounded text is the one use this is wrong
//! for.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod atom;
pub mod cheap_clone;
pub mod name;
pub mod namespace;

pub use crate::atom::Atom;
pub use crate::cheap_clone::CheapCloneStr;
pub use crate::name::{AttrName, ClassName, CustomPropertyName, ElementName, Ident};
pub use crate::namespace::NamespaceId;
