//! Whether a prop written as an attribute has been given.
//!
//! A control that can be written as a tag takes its props through a builder, and a prop that must
//! be given is a type parameter of that builder, flipped from [`Unset`] to [`Set`] by its own
//! setter. Building the props requires every one of them to be [`Set`], through one trait per prop
//! whose unimplemented message names it — which is what turns a missing required prop from a
//! panic while the interface runs into a compile error that says which prop is missing.
//!
//! Nothing here is written by hand in an application.

/// A prop that has not been given.
pub struct Unset;

/// A prop that has been given.
pub struct Set;

/// Satisfied once a list's `each` prop has been given.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`For` is missing the required prop `each`",
    label = "`each` was never given"
)]
pub trait HasEach {}
impl HasEach for Set {}

/// Satisfied once a list's `key` prop has been given.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`For` is missing the required prop `key`",
    label = "`key` was never given"
)]
pub trait HasKey {}
impl HasKey for Set {}

/// Satisfied once a list has been told what one row looks like.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`For` is missing its children: what one row looks like",
    label = "no row was written"
)]
pub trait HasRow {}
impl HasRow for Set {}

/// Satisfied once a conditional's `when` prop has been given.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`Show` is missing the required prop `when`",
    label = "`when` was never given"
)]
pub trait HasWhen {}
impl HasWhen for Set {}

/// Satisfied once a conditional has been told what to show.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`Show` is missing its children: what is shown while the condition holds",
    label = "nothing was written to show"
)]
pub trait HasShown {}
impl HasShown for Set {}
