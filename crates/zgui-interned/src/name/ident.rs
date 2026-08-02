//! A style-sheet identifier.

use crate::name::define::interned_name;

interned_name! {
    /// An identifier written in a style sheet: an element's id, a named grid line or area, an
    /// animation or counter name, a font family name, a view-transition name.
    ///
    /// One type covers all of them because they are the same thing to everything that handles
    /// them — a name a rule wrote down and a later stage looks up — and giving each its own
    /// newtype would multiply the conversions without ruling out a single mistake that matters.
    ///
    /// This is also the identifier type a layout engine is handed for named grid lines and areas,
    /// which is why the type satisfies [`CheapCloneStr`](crate::CheapCloneStr): a layout pass
    /// clones these names freely while resolving a grid, and doing so must not allocate.
    ///
    /// ```
    /// use zgui_interned::Ident;
    ///
    /// let sidebar = Ident::new("sidebar");
    /// assert_eq!(sidebar, Ident::new("sidebar"));
    /// assert_eq!(sidebar.to_string(), "sidebar");
    /// assert_eq!(Ident::default().as_str(), "");
    /// ```
    Ident
}
