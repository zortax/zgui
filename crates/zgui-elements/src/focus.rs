//! How an element is reached with the keyboard.

/// How an element takes focus.
///
/// A node is focusable when it says so here, or when it is one of the names that always is —
/// [`control`](crate::control), [`field`](crate::field), [`editor`](crate::editor) — and is not
/// disabled, not hidden, and not outside whatever focus trap is in force.
///
/// There are two values and not an integer. A tab order that is a number is a tab order nobody
/// maintains: every positive value has to be kept consistent with every other one across a whole
/// application, and the result is reliably worse than the order the elements are already in.
///
/// ```
/// use zgui_elements::{Focus, control};
///
/// // Reachable by tabbing, in the order it appears.
/// let button = control().tabindex(Focus::Sequential);
///
/// // Focusable, but only when something focuses it — a menu item, a list row.
/// let item = control().tabindex(Focus::Programmatic);
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Focus {
    /// Reached by tabbing, in the order the element appears.
    #[default]
    Sequential,
    /// Focusable only when something focuses it deliberately.
    ///
    /// What every item in a composite control uses: one of them is sequentially focusable at a
    /// time and the arrow keys move focus between them, so tabbing past the whole group is one
    /// press rather than one per item.
    Programmatic,
}

impl Focus {
    /// How this is written as an attribute, which is what a selector and a web backend both see.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sequential => "0",
            Self::Programmatic => "-1",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Focus;

    #[test]
    fn the_two_values_are_the_two_a_web_backend_would_write() {
        assert_eq!(Focus::Sequential.as_str(), "0");
        assert_eq!(Focus::Programmatic.as_str(), "-1");
        assert_eq!(Focus::default(), Focus::Sequential);
    }
}
