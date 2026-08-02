//! Box alignment: where a container puts its items, and where an item puts itself.
//!
//! Every one of the six properties computes to the same packed byte — a keyword in the low bits
//! and the `legacy`, `safe` and `unsafe` modifiers above it — so they share one flag type and are
//! told apart by the property they were read from rather than by their representation.

/// The computed value of `align-content` and `justify-content`.
pub use style::values::computed::ContentDistribution as ContentDistributionValue;
/// The computed value of `align-items`.
pub use style::values::computed::ItemPlacement as ItemPlacementValue;
/// The computed value of `justify-items`, whose initial value is `legacy`.
pub use style::values::computed::JustifyItems as JustifyItemsValue;
/// The computed value of `align-self` and `justify-self`.
pub use style::values::computed::SelfAlignment as SelfAlignmentValue;
/// The packed keyword-and-modifier byte every alignment property computes to.
///
/// The keyword is read with `value()` and the modifiers with `flags()`; comparing a whole byte
/// against a keyword constant is wrong whenever a modifier is present.
pub use style::values::specified::align::AlignFlags;
