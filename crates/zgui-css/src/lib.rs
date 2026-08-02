//! The computed-style facade: one name for a cascaded style, and one place the engine is named.
//!
//! A computed style is what the cascade produces for one element — every CSS property resolved to
//! a value with no keywords, no inheritance and no relative units left in it. Sixteen crates read
//! them. If each of those named the style engine directly, then vendoring the engine, patching it
//! or replacing it would touch sixteen manifests; with this crate in between it touches one.
//!
//! # What is here
//!
//! | Module | Contents |
//! |---|---|
//! | [`computed`] | [`ComputedStyle`], the style-struct accessors, [`StyleDraft`] and [`inherited_style`] |
//! | [`values`] | the computed value types a reader has to name to destructure a property |
//! | [`prefs`] | the feature flags a style sheet has to be parsed with |
//! | [`name`] | the bridge between interned names and the engine's own atoms |
//! | [`engine`] | the engine's own length unit and geometry, and the width the cascade may run at |
//! | [`parity`] | which CSS this framework actually implements, as a number rather than a claim |
//!
//! ```
//! use zgui_css::{ComputedStyle, StyleDraft};
//!
//! // A style with nothing but initial values, which is where every cascade starts.
//! let initial: ComputedStyle = StyleDraft::initial().build();
//! assert_eq!(initial.get_font().font_size.used_size().px(), 16.0);
//!
//! // Drafts exist so a reader of computed styles can be tested without a cascade behind it.
//! let larger = StyleDraft::initial().with_font_size(zgui_geom::CssPx(32.0)).build();
//! assert_eq!(larger.get_font().font_size.used_size().px(), 32.0);
//! ```
//!
//! # Why a style is behind a reference count
//!
//! [`ComputedStyle`] is a shared pointer, and cheaply cloning one is the mechanism the whole
//! pipeline's memoisation rests on: two elements that cascaded to the same result share the
//! allocation, so a consumer can key a cache on the pointer and lower one style where it has a
//! thousand elements. The individual property groups behind it are shared the same way and for the
//! same reason, which is why [`StructPtr`] and its constructors exist.
//!
//! # Where the firewall runs
//!
//! Nothing above the document crate names the style engine, the engine's length unit or the
//! geometry library its container-query hook answers in. Those three edges stop here, which is what
//! makes replacing or patching the engine a change to three manifests rather than to sixteen. A
//! crate that finds itself wanting one of those names wants a re-export from this crate instead.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod computed;
pub mod engine;
pub mod name;
pub mod parity;
pub mod prefs;
pub mod values;

pub use crate::computed::draft::StyleDraft;
pub use crate::computed::inherit::inherited_style;
pub use crate::computed::pinned::PinnedGroup;
pub use crate::computed::style::{ComputedStyle, StructPtr};
pub use crate::engine::{Au, ContainerSize, MAX_STYLE_THREADS};
pub use crate::name::{CheapCloneStr, Ident, atom_to_ident, ident_to_atom};
pub use crate::prefs::enable_css_features;
