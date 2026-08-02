//! Finding an item by typing the beginning of what it says.

use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use zgui_ui_primitives::{CollectionItem, ItemId};

use crate::overlay::Delayed;

/// How long a typed prefix survives before the next character starts a new search.
pub const RESET_AFTER: Duration = Duration::from_millis(1000);

/// A prefix a user is part way through typing.
///
/// Typing `p`, `r`, `o` in a menu should reach *Properties*, not stop at *Print* — so the
/// characters accumulate, and they stop accumulating after a pause. That pause is the whole design:
/// with no reset, a menu is unusable ten seconds after the first stray keystroke; with a reset per
/// character, no word longer than one letter is ever searchable.
///
/// Typing the *same* character repeatedly is a separate gesture, and it cycles: `s`, `s`, `s` walks
/// through every item beginning with S rather than looking for "sss".
///
/// ```
/// use core::time::Duration;
/// use std::rc::Rc;
/// use zgui::reactive::{Mounted, install};
/// use zgui::view::stub::StubHost;
/// use zgui::view::{HostHandle, provide_host};
/// use zgui_ui::menu::{RESET_AFTER, Typeahead};
///
/// install().ok();
/// // The reset is scheduled against the window's clock, so there has to be one.
/// let clock = Rc::new(StubHost::new());
/// let scope = Mounted::new();
/// scope.with(|| provide_host(HostHandle::from_rc(Rc::clone(&clock) as Rc<_>)));
/// scope.with(|| {
///     let typed = Typeahead::new();
///     assert_eq!(typed.push("p"), "p");
///     assert_eq!(typed.push("r"), "pr");
///     assert!(!typed.is_cycling(), "two different letters are a prefix");
///
///     // A pause ends that search rather than leaving the prefix to grow for ever.
///     clock.advance(RESET_AFTER + Duration::from_millis(1));
///     assert!(typed.buffer().is_empty());
///
///     assert_eq!(typed.push("s"), "s");
///     assert_eq!(typed.push("s"), "ss");
///     assert!(typed.is_cycling(), "the same letter twice walks the items beginning with it");
/// });
/// scope.unmount();
/// ```
#[derive(Clone, Default)]
pub struct Typeahead {
    /// What has been typed so far.
    buffer: Rc<RefCell<String>>,
    /// The pending reset, replaced on every character.
    reset: Delayed,
}

impl Typeahead {
    /// An empty prefix.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one character, and reports the whole prefix now.
    ///
    /// The reset is pushed back to a full [`RESET_AFTER`] from this character, so a user typing
    /// steadily keeps one search going.
    pub fn push(&self, character: &str) -> String {
        self.buffer.borrow_mut().push_str(&character.to_lowercase());
        let buffer = Rc::clone(&self.buffer);
        self.reset
            .after(RESET_AFTER, move || buffer.borrow_mut().clear());
        self.buffer.borrow().clone()
    }

    /// Throws the prefix away, and any pending reset with it.
    pub fn clear(&self) {
        self.reset.cancel();
        self.buffer.borrow_mut().clear();
    }

    /// The prefix as it stands.
    #[must_use]
    pub fn buffer(&self) -> String {
        self.buffer.borrow().clone()
    }

    /// Whether the prefix is one character repeated, which means *walk* rather than *match*.
    #[must_use]
    pub fn is_cycling(&self) -> bool {
        let buffer = self.buffer.borrow();
        let mut characters = buffer.chars();
        match characters.next() {
            None => false,
            Some(first) => buffer.chars().count() > 1 && characters.all(|next| next == first),
        }
    }

    /// What to search for, given the prefix: the whole of it, or one character when cycling.
    #[must_use]
    pub fn search(&self) -> String {
        if self.is_cycling() {
            self.buffer.borrow().chars().next().into_iter().collect()
        } else {
            self.buffer()
        }
    }
}

/// The item after `from` whose text begins with `search`, wrapping round to reach it.
///
/// Searching starts *after* the item the keyboard is on, so typing the same letter twice moves on
/// rather than staying put — and it wraps, so the last item beginning with S leads back to the
/// first. `from` reading `None` searches from the start.
///
/// An item that reads as nothing matches nothing: a separator or a decorative row has no text to
/// begin with `search`, and skipping it is what stops a typed letter landing on a horizontal rule.
#[must_use]
pub fn matching(
    items: &[CollectionItem],
    from: Option<ItemId>,
    search: &str,
) -> Option<CollectionItem> {
    if search.is_empty() || items.is_empty() {
        return None;
    }
    let start = from
        .and_then(|id| items.iter().position(|item| item.id() == id))
        .map_or(0, |at| at + 1);
    (0..items.len())
        .map(|step| items[(start + step) % items.len()])
        .find(|item| begins_with(&item.node().text_content(), search))
}

/// Whether `text` begins with `search`, ignoring case and leading space.
fn begins_with(text: &str, search: &str) -> bool {
    text.trim_start().to_lowercase().starts_with(search)
}

#[cfg(test)]
mod tests {
    use super::begins_with;

    #[test]
    fn a_match_ignores_case_and_the_space_a_layout_left_in_front() {
        assert!(begins_with("Properties", "pro"));
        assert!(begins_with("  Save as…", "sa"));
        assert!(!begins_with("Print", "pro"));
        assert!(!begins_with("", "p"));
    }
}
