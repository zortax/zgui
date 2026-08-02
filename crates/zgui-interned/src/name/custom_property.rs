//! The name of a custom property.

use crate::name::define::interned_name;

interned_name! {
    /// The name of a custom property, stored without the leading `--`.
    ///
    /// Dropping the prefix at the boundary rather than carrying it means a lookup never has to
    /// decide whether the caller wrote it, and a name is never interned twice under two spellings
    /// of the same thing. [`CustomPropertyName::parse`] is the boundary; anything reaching
    /// [`CustomPropertyName::new`] is already in stored form.
    ///
    /// Custom property names are case-sensitive, and this type preserves case exactly.
    ///
    /// ```
    /// use zgui_interned::CustomPropertyName;
    ///
    /// let accent = CustomPropertyName::parse("--accent").expect("a custom property name");
    /// assert_eq!(accent.as_str(), "accent");
    /// assert_eq!(accent.to_declaration(), "--accent");
    /// assert_eq!(accent, CustomPropertyName::new("accent"));
    ///
    /// assert!(CustomPropertyName::parse("accent").is_none());
    /// assert!(CustomPropertyName::parse("--").is_none());
    /// ```
    CustomPropertyName
}

impl CustomPropertyName {
    /// The prefix that marks a declaration as a custom property.
    const PREFIX: &'static str = "--";

    /// Reads a name as it is written in a style sheet, or `None` when `declaration` is not a
    /// custom property name at all.
    ///
    /// A name is a custom property name when it begins with `--` and has at least one character
    /// after it.
    pub fn parse(declaration: &str) -> Option<Self> {
        let name = declaration.strip_prefix(Self::PREFIX)?;
        (!name.is_empty()).then(|| Self::new(name))
    }

    /// The name as it would be written in a style sheet, with the `--` prefix restored.
    pub fn to_declaration(self) -> String {
        let mut declaration = String::with_capacity(Self::PREFIX.len() + self.len());
        declaration.push_str(Self::PREFIX);
        declaration.push_str(self.as_str());
        declaration
    }
}
