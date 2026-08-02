//! The name of an element.

use crate::name::define::interned_name;

interned_name! {
    /// The local name of an element — the part of its tag that is not its namespace.
    ///
    /// This is the string a type selector matches against, so it is compared once per element per
    /// candidate rule during selector matching, which is why it is interned rather than stored.
    ///
    /// Names are stored exactly as given. A document language whose element names are
    /// case-insensitive normalises them before it gets here, so that matching stays a pointer
    /// comparison.
    ///
    /// ```
    /// use zgui_interned::ElementName;
    ///
    /// let button = ElementName::new("button");
    /// assert_eq!(button, ElementName::new("button"));
    /// assert_eq!(button.as_str(), "button");
    /// assert_ne!(button, ElementName::new("BUTTON"));
    /// ```
    ElementName
}
