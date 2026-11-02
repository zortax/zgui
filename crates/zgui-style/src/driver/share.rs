//! Computed styles kept by rule chain, so that an element whose cascade result equals a kept one
//! takes the groups of the kept one.
//!
//! The engine shares styles through its sharing cache, which holds the last few elements of one
//! tree level, and through a lookup by rule node, which needs the same parent style. Neither
//! reaches cells under rows of different styles when the rows have more cells than the cache
//! holds: each cell cascades to a style of its own, and each of those holds its own copy of every
//! group its rules wrote. Keyed by rule chain and kept from one restyle to the next, this table
//! finds those cells again, and it finds rows mounted on a later frame too.
//!
//! Only the first cascade of an element is looked up. A restyle keeps the engine's result: a
//! restyle often touches every element of a list, and a comparison for each of them costs more
//! time than the sharing gives back.
//!
//! An element takes kept groups only when the two styles are equal in every field, so the swap
//! changes identities and nothing else. Equal groups paint, lay out and inherit the same, so every
//! key that is read from group identities stays a proof of equality.
//!
//! # A style given whole marks the element
//!
//! The engine shares a style between cousins when their parents hold one style, on the guarantee
//! that every class up both parent chains is the same. A primary style that the table gives whole
//! can be held by parents whose classes differ. Such an element is marked as reused through its
//! rule node, as the engine marks its own lookup by rule node, and the engine then shares no style
//! between cousins under it. A `display: contents` element is never given a style whole: its
//! children take their layout parent from further up, and the engine's lookup by rule node reads
//! only the parent style.
//!
//! # The text group keeps its identity
//!
//! The group that holds the text colour is the one exception. A run of text claims its brush slot
//! against the identity of that group, and a shaped paragraph is keyed by its brush slot. Two
//! paragraphs with one key share one shaped form, and a shaped form holds the lines of one width
//! at a time. So a style that can hold text keeps its own text group: two equal labels in boxes of
//! different widths then keep two paragraphs, as they did before they were shared. Such a style
//! takes the kept style whole only when the text group is already the same allocation, and a new
//! style made of the kept groups and its own text group otherwise.

use std::sync::{Mutex, MutexGuard};

use rustc_hash::FxHashMap;
use selectors::Element as _;
use servo_arc::Arc as ServoArc;
use smallvec::SmallVec;
use style::dom::TElement;
use style::properties::{ComputedValues, ComputedValuesInner};
use style::rule_tree::StrongRuleNode;
use style::selector_parser::{EAGER_PSEUDO_COUNT, PseudoElement};
use style::style_resolver::ResolvedElementStyles;
use zgui_css::MAX_STYLE_THREADS;
use zgui_dom::Node;

/// How many distinct styles are kept per rule chain.
///
/// One chain cascades to several styles when the elements that match it inherit different values.
/// Each style that arrives compares itself with every kept one, so the bound is also the most
/// comparisons a cascade result pays for.
const PER_CHAIN: usize = 4;

/// The least number of kept styles at which a sweep runs.
const SWEEP_FLOOR: usize = 256;

/// The styles kept for one worker.
#[derive(Default)]
struct Kept {
    /// The styles, by the rule chain they were cascaded from.
    by_rules: FxHashMap<StrongRuleNode, SmallVec<[ServoArc<ComputedValues>; 1]>>,
    /// How many styles the map holds.
    len: usize,
}

/// What an element took from a kept style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Took {
    /// The kept style itself.
    Whole,
    /// A new style made of the kept groups and the element's own text group.
    Groups,
}

impl Kept {
    /// The groups of a kept style equal to `style`, or `None` when `style` stays as it is.
    ///
    /// `holds_text` keeps the identity of the text group, as the module documentation describes.
    /// A kept style equal to `style` that only the table holds is replaced with `style`: taking
    /// its groups frees nothing, and the elements after this one that hold `style` already then
    /// find it as it is.
    fn share(
        &mut self,
        style: &ServoArc<ComputedValues>,
        holds_text: bool,
    ) -> Option<(ServoArc<ComputedValues>, Took)> {
        if style.visited_style().is_some() || style.attribute_references.is_some() {
            return None;
        }
        let rules = style.rules.as_ref()?;
        let Some(chain) = self.by_rules.get_mut(rules) else {
            self.by_rules
                .insert(rules.clone(), smallvec::smallvec![ServoArc::clone(style)]);
            self.len += 1;
            return None;
        };
        if let Some(found) = chain.iter_mut().find(|held| same_values(held, style)) {
            if ServoArc::ptr_eq(found, style) {
                return None;
            }
            if found.is_unique() {
                *found = ServoArc::clone(style);
                return None;
            }
            if !holds_text || core::ptr::eq(found.get_inherited_text(), style.get_inherited_text())
            {
                return Some((ServoArc::clone(found), Took::Whole));
            }
            return Some((with_own_text(found, style), Took::Groups));
        }
        if chain.len() >= PER_CHAIN {
            chain.remove(0);
        } else {
            self.len += 1;
        }
        chain.push(ServoArc::clone(style));
        None
    }
}

/// The kept styles, one table per worker so that no two workers wait on the same lock.
///
/// A style one worker keeps is not found by another. A traversal that splits a list across
/// workers keeps at most one copy per worker of each style.
pub(crate) struct SharedStyles {
    /// The tables, one per worker index the pool can produce plus one for the calling thread.
    workers: Vec<Mutex<Kept>>,
    /// How many styles were kept after the last sweep.
    swept_to: usize,
}

impl Default for SharedStyles {
    fn default() -> Self {
        Self {
            workers: (0..=MAX_STYLE_THREADS)
                .map(|_| Mutex::new(Kept::default()))
                .collect(),
            swept_to: 0,
        }
    }
}

impl SharedStyles {
    /// Gives each style of `styles`, the first cascade of `element`, the groups of a kept style
    /// equal to it, or keeps it for the elements after this one, and returns how many it replaced.
    ///
    /// The primary style and each eager pseudo-element style are looked up on their own. A
    /// primary style the engine already took from another element through its rule node is left
    /// as it is.
    ///
    /// An element that holds an inline style, runs an animation, is a link, or is not matched
    /// against author rules is left alone: its chain or its style is its own, and keeping it would
    /// only fill the table.
    pub(crate) fn share(
        &self,
        element: Node<'_>,
        context: &style::context::SharedStyleContext<'_>,
        styles: &mut ResolvedElementStyles,
    ) -> usize {
        if element.implemented_pseudo_element().is_some()
            || element.style_attribute().is_some()
            || !element.matches_user_and_content_rules()
            || element.is_link()
        {
            return 0;
        }
        let ui = styles.primary.style().get_ui();
        if (ui.specifies_animations() || ui.specifies_transitions())
            && element.has_animations(context)
        {
            return 0;
        }
        let mut kept = None;
        let mut replaced = 0;

        let primary = &mut styles.primary;
        if !primary.reused_via_rule_node
            && !primary.style().get_box().clone_display().is_contents()
            && let Some((style, took)) = self.kept(&mut kept).share(&primary.style.0, true)
        {
            primary.style.0 = style;
            // The cousin-sharing guarantee, as the module documentation describes.
            primary.reused_via_rule_node = took == Took::Whole;
            replaced += 1;
        }

        for index in 0..EAGER_PSEUDO_COUNT {
            let pseudo = PseudoElement::from_eager_index(index);
            // Only generated content lays out text of its own.
            let holds_text = matches!(pseudo, PseudoElement::Before | PseudoElement::After);
            let Some(style) = styles.pseudos.as_array()[index].as_ref() else {
                continue;
            };
            if let Some((style, _)) = self.kept(&mut kept).share(style, holds_text) {
                styles.pseudos.set(&pseudo, style);
                replaced += 1;
            }
        }
        replaced
    }

    /// The table of the calling worker, locked on first use.
    fn kept<'s, 'h>(&'s self, held: &'h mut Option<MutexGuard<'s, Kept>>) -> &'h mut Kept {
        held.get_or_insert_with(|| {
            let slot = rayon::current_thread_index()
                .map_or(MAX_STYLE_THREADS, |index| index.min(MAX_STYLE_THREADS));
            self.workers[slot]
                .lock()
                .unwrap_or_else(|held| held.into_inner())
        })
    }

    /// Gives back the styles that nothing else holds, once the table has doubled since the last
    /// time it did.
    pub(crate) fn sweep(&mut self) {
        let len: usize = self
            .workers
            .iter_mut()
            .map(|kept| kept.get_mut().unwrap_or_else(|held| held.into_inner()).len)
            .sum();
        if len < SWEEP_FLOOR.max(self.swept_to * 2) {
            return;
        }
        let mut left = 0;
        for kept in &mut self.workers {
            let kept = kept.get_mut().unwrap_or_else(|held| held.into_inner());
            kept.by_rules.retain(|_, chain| {
                chain.retain(|style| !style.is_unique());
                !chain.is_empty()
            });
            kept.len = kept.by_rules.values().map(SmallVec::len).sum();
            left += kept.len;
        }
        self.swept_to = left;
    }

    /// How many styles are kept.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.workers
            .iter()
            .map(|kept| kept.lock().unwrap_or_else(|held| held.into_inner()).len)
            .sum()
    }

    /// Gives back every kept style.
    pub(crate) fn clear(&mut self) {
        for kept in &mut self.workers {
            *kept.get_mut().unwrap_or_else(|held| held.into_inner()) = Kept::default();
        }
        self.swept_to = 0;
    }
}

/// A style that holds the groups of `kept` and the text group of `own`.
///
/// The two are equal in every field, so the result is equal to both.
fn with_own_text(kept: &ComputedValues, own: &ComputedValues) -> ServoArc<ComputedValues> {
    ComputedValues::new(
        kept.pseudo().as_ref(),
        kept.custom_properties().clone(),
        kept.attribute_references.clone(),
        kept.writing_mode,
        kept.effective_zoom,
        kept.flags,
        kept.rules.clone(),
        None,
        kept.clone_background(),
        kept.clone_border(),
        kept.clone_box(),
        kept.clone_column(),
        kept.clone_counters(),
        kept.clone_effects(),
        kept.clone_font(),
        kept.clone_inherited_box(),
        kept.clone_inherited_table(),
        own.clone_inherited_text(),
        kept.clone_inherited_ui(),
        kept.clone_list(),
        kept.clone_margin(),
        kept.clone_outline(),
        kept.clone_padding(),
        ComputedValuesInner::clone_position(kept),
        kept.clone_svg(),
        kept.clone_table(),
        kept.clone_text(),
        kept.clone_ui(),
    )
}

/// Whether two computed styles are equal in every field.
///
/// Each group is compared by address first and by value after, so styles that already share
/// their groups compare in a few instructions.
fn same_values(a: &ServoArc<ComputedValues>, b: &ServoArc<ComputedValues>) -> bool {
    if ServoArc::ptr_eq(a, b) {
        return true;
    }
    macro_rules! same {
        ($get:ident) => {
            core::ptr::eq(a.$get(), b.$get()) || a.$get() == b.$get()
        };
    }
    a.rules == b.rules
        && a.pseudo() == b.pseudo()
        && a.flags == b.flags
        && a.writing_mode == b.writing_mode
        && a.effective_zoom == b.effective_zoom
        && a.visited_style().is_none()
        && b.visited_style().is_none()
        && a.attribute_references.is_none()
        && b.attribute_references.is_none()
        && a.custom_properties() == b.custom_properties()
        && same!(get_background)
        && same!(get_border)
        && same!(get_box)
        && same!(get_column)
        && same!(get_counters)
        && same!(get_effects)
        && same!(get_font)
        && same!(get_inherited_box)
        && same!(get_inherited_table)
        && same!(get_inherited_text)
        && same!(get_inherited_ui)
        && same!(get_list)
        && same!(get_margin)
        && same!(get_outline)
        && same!(get_padding)
        && same!(get_position)
        && same!(get_svg)
        && same!(get_table)
        && same!(get_text)
        && same!(get_ui)
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::{same_values, with_own_text};

    /// A style whose text colour and font size are its own.
    fn styled() -> zgui_css::ComputedStyle {
        let mut draft = StyleDraft::initial().with_font_size(zgui_geom::CssPx(23.0));
        draft.inherited_text().color =
            zgui_css::values::color::AbsoluteColor::srgb_legacy(10, 20, 30, 1.0);
        draft.build()
    }

    #[test]
    fn two_styles_built_alike_have_the_same_values() {
        let (one, two) = (styled(), styled());
        assert!(!core::ptr::eq(one.get_font(), two.get_font()));
        assert!(same_values(&one, &two));
    }

    #[test]
    fn a_style_that_differs_in_one_group_is_not_the_same() {
        let one = styled();
        let mut draft = StyleDraft::from_style(&one);
        draft.inherited_text().color =
            zgui_css::values::color::AbsoluteColor::srgb_legacy(10, 20, 31, 1.0);
        assert!(!same_values(&one, &draft.build()));
    }

    #[test]
    fn a_style_with_its_own_text_holds_every_other_group_of_the_kept_one() {
        let (kept, own) = (styled(), styled());
        let made = with_own_text(&kept, &own);

        assert!(core::ptr::eq(
            made.get_inherited_text(),
            own.get_inherited_text()
        ));
        assert!(core::ptr::eq(made.get_font(), kept.get_font()));
        assert!(!core::ptr::eq(made.get_font(), own.get_font()));
        assert!(same_values(&made, &kept) && same_values(&made, &own));
    }
}
