//! The name of an attribute.

use crate::name::define::interned_name;

interned_name! {
    /// The local name of an attribute.
    ///
    /// Attribute selectors compare this name against every candidate rule's, and an element's
    /// attribute list is searched by it on every attribute read, so it is interned for the same
    /// reason [`ElementName`](crate::ElementName) is.
    ///
    /// ```
    /// use zgui_interned::AttrName;
    ///
    /// let disabled = AttrName::new("disabled");
    /// assert_eq!(disabled, AttrName::new("disabled"));
    /// assert_eq!(disabled.as_str(), "disabled");
    /// ```
    AttrName
}
