//! The computed value types, under names that do not change when the engine behind them does.
//!
//! A crate that reads a property has to destructure its value, and destructuring means naming the
//! type. These are those names, grouped the way the properties are.
//!
//! | Module | Contents |
//! |---|---|
//! | [`length`] | the units everything geometric resolves to |
//! | [`size`] | what a box asks to be, and where it sits |
//! | [`align`] | where a container puts its items, and where an item puts itself |
//! | [`flex`] | the two flex-container properties nothing else shares |
//! | [`grid`] | track lists, line names, template areas and item placement |
//! | [`list`] | what a list item marks itself with |
//! | [`content`] | what a generated-content box is made of |
//! | [`border`] | the four sides, the four corners and their widths |
//! | [`image`] | backgrounds, gradients and the ramps they interpolate along |
//! | [`effect`] | filters, shadows, opacity and blending |
//! | [`mod@transform`] | the four properties that move a box without moving anything around it |
//! | [`font`] | face selection and sizing |
//! | [`text`] | spacing, wrapping and alignment |
//! | [`color`] | the one conversion into the colour type the rest of the tree draws with |
//! | [`ui`] | what the pointer and the caret interact with |

pub mod align;
pub mod border;
pub mod color;
pub mod content;
pub mod custom;
pub mod effect;
pub mod flex;
pub mod font;
pub mod grid;
pub mod image;
pub mod length;
pub mod list;
pub mod size;
pub mod text;
pub mod transform;
pub mod ui;
