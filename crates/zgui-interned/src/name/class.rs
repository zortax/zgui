//! The name of a class.

use crate::name::define::interned_name;

interned_name! {
    /// One class name from an element's class list.
    ///
    /// A class list is stored pre-split and pre-interned, so matching `.card` against an element
    /// is a scan of a handful of pointers rather than a string search through an attribute value
    /// that has to be re-tokenised every time.
    ///
    /// ```
    /// use zgui_interned::ClassName;
    ///
    /// let classes: Vec<ClassName> = "card card--wide".split_whitespace().map(ClassName::new).collect();
    /// assert!(classes.contains(&ClassName::new("card")));
    /// assert!(!classes.contains(&ClassName::new("cards")));
    /// ```
    ClassName
}
