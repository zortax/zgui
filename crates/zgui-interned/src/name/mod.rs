//! The interned name types, one per kind of name the tree speaks.
//!
//! All of them are the same eight bytes over the same [`Atom`](crate::Atom); they are separate
//! types so that an attribute name cannot be passed where an element name is meant. Each carries
//! its own meaning and nothing else, which is what makes the distinction free.

mod attr;
mod class;
mod custom_property;
mod define;
mod element;
mod ident;

#[cfg(test)]
mod tests;

pub use crate::name::attr::AttrName;
pub use crate::name::class::ClassName;
pub use crate::name::custom_property::CustomPropertyName;
pub use crate::name::element::ElementName;
pub use crate::name::ident::Ident;
