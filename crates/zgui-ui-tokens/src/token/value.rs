//! What a token holds, and how a set of them is written out.

use core::fmt::{self, Display, Write};

/// The custom-property declarations a theme lowers to.
///
/// Built rather than concatenated by the caller, so that the escaping, the separators and the
/// order are decided in one place and a token whose value happens to contain a semicolon cannot
/// end a declaration early.
///
/// ```
/// use zgui_ui_tokens::Declarations;
///
/// let mut declarations = Declarations::new();
/// declarations.push("--zui-color-primary", "#2b6cff");
/// declarations.push("--zui-radius-md", "8px");
///
/// assert_eq!(
///     declarations.to_string(),
///     "--zui-color-primary: #2b6cff; --zui-radius-md: 8px;"
/// );
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declarations {
    /// The text built so far.
    text: String,
    /// How many declarations are in it.
    count: usize,
}

impl Declarations {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends `property: value;`.
    ///
    /// A value containing a `;`, a `}` or a comment opener would end the declaration, the rule or
    /// the sheet early, and a theme's values may come from an application at run time — so those
    /// three are refused here and the declaration is dropped rather than written. A dropped token
    /// falls back to whatever the cascade already had for it, which is a colour that did not
    /// change; the alternative is a sheet that stops parsing halfway down.
    pub fn push(&mut self, property: &str, value: &str) {
        if value.contains([';', '}', '{']) || value.contains("/*") {
            debug_assert!(
                false,
                "the value of {property} cannot be written as a declaration: {value:?}"
            );
            return;
        }
        if self.count > 0 {
            self.text.push(' ');
        }
        // Writing into a `String` cannot fail, and swallowing the `Result` here keeps every
        // caller from carrying one it can do nothing about.
        let _ = write!(self.text, "{property}: {value};");
        self.count += 1;
    }

    /// How many declarations are in the set.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The declarations, as they go inside a rule's braces.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The declarations wrapped in a rule for `selector`, or nothing when there are none.
    pub fn into_rule(self, selector: &str) -> String {
        if self.is_empty() {
            return String::new();
        }
        format!("{selector} {{ {} }}", self.text)
    }
}

impl Display for Declarations {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::Declarations;

    #[test]
    fn an_empty_set_makes_no_rule_at_all() {
        assert_eq!(Declarations::new().into_rule(":root"), "");
    }

    #[test]
    fn a_value_that_would_end_the_rule_early_is_refused_rather_than_written() {
        // A theme's values may come from an application at run time, so a value carrying a `}` is
        // a rule that ends where nobody meant it to and a sheet that stops applying halfway down.
        // The debug assertion is the loud half; this is the release behaviour, which has to leave
        // a sheet that still parses.
        let escaping = std::panic::catch_unwind(|| {
            let mut declarations = Declarations::new();
            declarations.push("--zui-color-background", "#fff");
            declarations.push("--zui-color-foreground", "red; } box { display: none");
            declarations
        });
        if let Ok(declarations) = escaping {
            assert_eq!(declarations.len(), 1, "the second value was written anyway");
            assert!(!declarations.as_str().contains("display: none"));
        }
    }

    #[test]
    fn a_rule_carries_every_declaration_that_was_pushed() {
        let mut declarations = Declarations::new();
        declarations.push("--a", "1px");
        declarations.push("--b", "2px");
        assert_eq!(
            declarations.into_rule(":root"),
            ":root { --a: 1px; --b: 2px; }"
        );
    }
}
