//! Deciding that a change cannot affect any computed style, and skipping the engine entirely.
//!
//! Most of what a running interface writes to a document cannot change a single computed value. A
//! component library styles `:hover` and `:focus-visible`; nothing in it styles `:read-only`,
//! `:in-range` or `:indeterminate`, and nothing matches the data attributes its variants are
//! driven by. A change to something no selector depends on needs no record of the previous value,
//! no ancestor marking and no traversal.
//!
//! # The schedule, which is two phases and not one
//!
//! The answers are built from the rule set's dependency index, and that index is repopulated only
//! when the rule set is flushed. So:
//!
//! * **The frame in which the sheet set changed disables the filter.** Every mutation in that one
//!   frame takes the full path, which is the bounded once-per-sheet-change cost that makes the
//!   staleness harmless rather than merely rare.
//! * **The tail of that same frame rebuilds it**, after the restyle has flushed the rule set and
//!   therefore after the index describes the sheets that are actually installed.
//!
//! Rebuilding at the start of the frame instead cannot work, and does not fail loudly. At that
//! point the index still describes the *previous* sheet set; worse, the flag that would trigger a
//! rebuild is cleared by the flush in the same frame, so a rebuild scheduled there would never run
//! again after the first one and the filter would answer from the first sheet set for ever.
//!
//! | Module | Contents |
//! |---|---|
//! | [`state_mask`] | which state bits can matter for one element |
//! | [`class_set`] | which class names any selector mentions |
//! | [`attr_set`] | which attribute names any selector mentions |

pub mod attr_set;
pub mod class_set;
pub mod state_mask;

use rustc_hash::FxHashSet;
use style::stylist::Stylist;
use stylo_dom::ElementState;
use zgui_dom::{Node, StyleFilter};
use zgui_interned::{AttrName, ClassName};

/// What the installed stylesheets can actually be affected by.
///
/// The per-element state answer is deliberately *not* a field: it is a lookup into the rule set's
/// index narrowed by the element asking, so it is answered on demand and cached by the document
/// against the element it was narrowed for.
pub struct StyleDependencies {
    /// Every class name mentioned by any selector.
    classes: FxHashSet<ClassName>,
    /// Every attribute name mentioned by any selector.
    attrs: FxHashSet<AttrName>,
    /// Whether these answers describe sheets that are no longer the installed ones.
    disabled: bool,
}

impl StyleDependencies {
    /// Answers that narrow nothing, which is where a rule set with no flush behind it starts.
    pub(crate) fn unusable() -> Self {
        Self {
            classes: FxHashSet::default(),
            attrs: FxHashSet::default(),
            disabled: true,
        }
    }

    /// Rebuilds the answers from `stylist`, which must have been flushed.
    pub(crate) fn rebuild(&mut self, stylist: &Stylist) {
        self.classes = class_set::build(stylist);
        self.attrs = attr_set::build(stylist);
        self.disabled = false;
    }

    /// Marks the answers as describing sheets that are no longer installed.
    pub(crate) fn disable(&mut self) {
        self.disabled = true;
    }

    /// Whether the answers are currently unusable.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// How many distinct class names any selector mentions.
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// How many distinct attribute names any selector mentions.
    pub fn attr_count(&self) -> usize {
        self.attrs.len()
    }
}

/// The dependency answers together with the rule set they were built from.
///
/// This is what the document's mutation API is handed. It borrows both halves because the
/// per-element state answer is a live lookup: caching one on this side would be a second copy of
/// something the document already caches, retired on a different schedule.
pub struct StyleFilterView<'a> {
    /// Where the per-element state answer comes from.
    stylist: &'a Stylist,
    /// The answers that do not depend on which element is asking.
    deps: &'a StyleDependencies,
}

impl<'a> StyleFilterView<'a> {
    /// A view over `deps`, answering per-element questions from `stylist`.
    pub(crate) fn new(stylist: &'a Stylist, deps: &'a StyleDependencies) -> Self {
        Self { stylist, deps }
    }
}

impl StyleFilter for StyleFilterView<'_> {
    fn states_for(&self, element: Node<'_>) -> ElementState {
        if self.deps.disabled {
            return ElementState::all();
        }
        state_mask::states_for(self.stylist, element)
    }

    fn names_class(&self, class: ClassName) -> bool {
        self.deps.disabled || self.deps.classes.contains(&class)
    }

    fn names_attr(&self, attr: AttrName) -> bool {
        self.deps.disabled || self.deps.attrs.contains(&attr)
    }

    fn is_disabled(&self) -> bool {
        self.deps.disabled
    }
}
