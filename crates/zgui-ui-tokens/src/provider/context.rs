//! What a theme provider publishes to everything below it.

use std::cell::Cell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Store};

use crate::scheme::ColorScheme;
use crate::theme::Theme;
use crate::token::Declarations;

/// The two token sets a provider carries: one for each scheme.
///
/// Both are always present. Which one is in force is not a property of the theme — it is a
/// property of the *surface*, and under [`ColorScheme::System`] the answer is only known inside
/// the style engine's media query. Carrying both is what lets that answer stay there.
#[derive(Clone, Debug, PartialEq, Eq, zgui::reactive::Store, zgui::reactive::Patch)]
pub struct Themes {
    /// The tokens in force on a light surface.
    pub light: Theme,
    /// The tokens in force on a dark surface.
    pub dark: Theme,
}

// The two derives above expand to code that names the store engine, and it lands here.
#[allow(unused_imports)]
use zgui::reactive::store::reactive_stores;

impl Themes {
    /// The framework's own two sets.
    pub fn defaults() -> Self {
        Self {
            light: Theme::light(),
            dark: Theme::dark(),
        }
    }

    /// Writes both sets out as a style sheet declared on `selector`, for `scheme`.
    pub fn sheet(&self, selector: &str, scheme: ColorScheme) -> String {
        crate::theme::theme_sheet(selector, &self.light, &self.dark, scheme)
    }

    /// Every token in the light set, as declarations.
    ///
    /// What a tool that documents a theme reads, and what a test that has to name a value asks.
    pub fn light_declarations(&self) -> Declarations {
        let mut declarations = Declarations::new();
        self.light.declare(&mut declarations);
        declarations
    }
}

impl Default for Themes {
    fn default() -> Self {
        Self::defaults()
    }
}

/// What [`ThemeProvider`](crate::ThemeProvider) publishes to everything below it.
///
/// Reach it with [`use_theme`]. It is `Clone` and cheap: the tokens are behind a store and the
/// scheme is a signal, so holding one in a closure costs a reference count.
#[derive(Clone)]
pub struct ThemeContext {
    /// The two token sets, per field.
    themes: Store<Themes>,
    /// Which scheme the provider was asked for.
    scheme: Signal<ColorScheme, LocalStorage>,
    /// What this provider's tokens are declared on.
    selector: Rc<str>,
}

impl ThemeContext {
    /// Assembles a context. [`ThemeProvider`](crate::ThemeProvider) is what calls this.
    pub fn new(
        themes: Store<Themes>,
        scheme: Signal<ColorScheme, LocalStorage>,
        selector: Rc<str>,
    ) -> Self {
        Self {
            themes,
            scheme,
            selector,
        }
    }

    /// The two token sets, addressed field by field.
    ///
    /// Reading one group subscribes to that group alone: a component that reads the motion
    /// durations is not woken by a colour changing.
    pub fn themes(&self) -> Store<Themes> {
        self.themes
    }

    /// Which scheme this provider was asked for.
    ///
    /// [`ColorScheme::System`] means *the desktop decides*, and it decides inside the style
    /// engine's `prefers-color-scheme` query — so this reports `System` rather than resolving it,
    /// because resolving it here would be a second, wrong answer.
    pub fn scheme(&self) -> Signal<ColorScheme, LocalStorage> {
        self.scheme
    }

    /// What this provider's tokens are declared on: `:root`, or a class for a themed region.
    pub fn selector(&self) -> &str {
        &self.selector
    }
}

/// The theme in force, from the nearest enclosing provider.
///
/// `None` outside one, which is an ordinary answer: a component reads tokens from CSS and works
/// perfectly well with no provider anywhere, falling back to whatever the cascade already had.
/// Only something that has to read a token *in Rust* — a chart choosing a series colour, a
/// motion-aware transition — needs this at all.
pub fn use_theme() -> Option<ThemeContext> {
    use_local_context::<ThemeContext>()
}

/// How many providers this document has already mounted.
///
/// Shared through context rather than kept in a global, for the reason every other per-window
/// thing is: two windows in one process each get their own numbering, and a test that opens three
/// is not renumbering the second one's sheets.
#[derive(Clone)]
pub(crate) struct ThemeCounter(Rc<Cell<usize>>);

impl ThemeCounter {
    /// A counter that has handed out nothing.
    pub(crate) fn new() -> Self {
        Self(Rc::new(Cell::new(0)))
    }

    /// The next number, and the one after it kept for whoever asks next.
    pub(crate) fn take(&self) -> usize {
        let next = self.0.get();
        self.0.set(next + 1);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::{ThemeCounter, Themes};
    use crate::scheme::ColorScheme;

    #[test]
    fn the_two_sets_are_the_two_schemes() {
        let themes = Themes::defaults();
        assert_ne!(themes.light, themes.dark);
        assert!(
            themes
                .sheet(":root", ColorScheme::System)
                .contains("@media (prefers-color-scheme: dark)")
        );
    }

    #[test]
    fn a_counter_never_hands_the_same_number_out_twice() {
        let counter = ThemeCounter::new();
        let taken: Vec<usize> = (0..4).map(|_| counter.take()).collect();
        assert_eq!(taken, [0, 1, 2, 3]);
    }

    #[test]
    fn two_counters_number_independently() {
        let first = ThemeCounter::new();
        let second = ThemeCounter::new();
        assert_eq!(first.take(), 0);
        assert_eq!(first.take(), 1);
        assert_eq!(second.take(), 0, "a second window starts its own numbering");
    }
}
