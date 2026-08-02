//! A class list.

use core::fmt::{self, Debug};

use zgui_interned::ClassName;

/// An ordered list of class names, with no duplicates.
///
/// Order is preserved because a caller's classes are merged *after* a component's own, and "the
/// caller's class wins" is only a stable contract if the order is.
///
/// ```
/// use zgui_interned::ClassName;
/// use zgui_view::Classes;
///
/// let base = Classes::from("button primary");
/// let merged = base.merged(&Classes::from("primary w-full"));
///
/// assert_eq!(
///     merged.names(),
///     [
///         ClassName::new("button"),
///         ClassName::new("primary"),
///         ClassName::new("w-full"),
///     ]
/// );
/// ```
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Classes(Vec<ClassName>);

impl Classes {
    /// An empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// The names, in order.
    pub fn names(&self) -> &[ClassName] {
        &self.0
    }

    /// How many names the list holds.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `name` is in the list.
    pub fn contains(&self, name: ClassName) -> bool {
        self.0.contains(&name)
    }

    /// Adds `name`, unless it is already there.
    pub fn push(&mut self, name: ClassName) {
        if !self.contains(name) {
            self.0.push(name);
        }
    }

    /// Removes `name`.
    pub fn remove(&mut self, name: ClassName) {
        self.0.retain(|existing| *existing != name);
    }

    /// This list with `other`'s names appended, skipping any that are already present.
    pub fn merged(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for name in &other.0 {
            merged.push(*name);
        }
        merged
    }
}

impl From<&str> for Classes {
    fn from(text: &str) -> Self {
        let mut classes = Self::new();
        for name in text.split_whitespace() {
            classes.push(ClassName::new(name));
        }
        classes
    }
}

impl From<String> for Classes {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
}

impl FromIterator<ClassName> for Classes {
    fn from_iter<I: IntoIterator<Item = ClassName>>(names: I) -> Self {
        let mut classes = Self::new();
        for name in names {
            classes.push(name);
        }
        classes
    }
}

impl Debug for Classes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.0.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::ClassName;

    use super::Classes;

    #[test]
    fn parsing_splits_on_whitespace_and_drops_repeats() {
        let classes = Classes::from("  a   b\ta  ");
        assert_eq!(classes.names(), [ClassName::new("a"), ClassName::new("b")]);
    }

    #[test]
    fn merging_keeps_the_first_position_of_a_repeated_name() {
        let merged = Classes::from("a b").merged(&Classes::from("c a"));
        assert_eq!(
            merged.names(),
            [
                ClassName::new("a"),
                ClassName::new("b"),
                ClassName::new("c")
            ]
        );
    }

    #[test]
    fn removing_a_name_that_is_not_there_changes_nothing() {
        let mut classes = Classes::from("a");
        classes.remove(ClassName::new("b"));
        assert_eq!(classes.len(), 1);
    }
}
