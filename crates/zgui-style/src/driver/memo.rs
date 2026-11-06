//! Cascade results kept for one traversal, so that an element whose cascade would read the same
//! inputs as an earlier one takes the earlier result.
//!
//! A change that reaches many elements cascades each of them alone, and elements of one row or list
//! hold the same rule chain under equal parents. Selector matching is already skipped there; this
//! skips the cascade too. [`finish_restyle`](style::matching::MatchMethods::finish_restyle) still
//! runs per element, so damage, transitions and keyframes see an ordinary restyle.
//!
//! # What a cascade reads
//!
//! An element's result depends on its rule chain and on these inputs, all of them in the key or a
//! reason to stay out of the table:
//!
//! - The cascade input flags and the local name.
//! - The parent's inherited groups, its flags that pass to children, its zoom, its writing mode and
//!   both of its custom property maps.
//! - The parent's `display`, which sets `DISPLAY_CONTENTS_IN_ITEM_CONTAINER`.
//! - The layout parent's `display` and writing mode. An element under a `display: contents` parent
//!   stays out, so the layout parent is the parent.
//! - The parent's reset groups, through `inherit`. Such a result sets `INHERITS_RESET_STYLE` and is
//!   not kept.
//! - The element's attributes, through `attr()`, and ancestors, through container units. Such a
//!   result is not kept.
//!
//! - Each eager pseudo-element's rule chain and cascade input flags. Its result reads the element's
//!   primary style, which a hit proves equal. An element with generated content stays out, because
//!   that content lays out text and keeps a text group of its own. `::selection` lays out none.
//!
//! The table lives for one traversal, so the device, the root font size and the rule tree cannot
//! change under it. A result is only given after a real cascade of the same inputs ran in the same
//! traversal, and that cascade already recorded what it used on the device.
//!
//! # The text group
//!
//! A run of text claims its brush slot against the identity of its text group, as the
//! [`share`](super::share) module describes. A hit gives the identity a cascade would: the current
//! parent's group when the kept result borrowed its parent's, and a new allocation otherwise.
//! An element takes the kept style whole only when that group is the one the kept style holds.

use std::sync::{Mutex, OnceLock};

use rustc_hash::FxHashMap;
use selectors::Element as _;
use servo_arc::Arc as ServoArc;
use smallvec::SmallVec;
use style::LocalName;
use style::computed_value_flags::ComputedValueFlags;
use style::context::{ElementCascadeInputs, SharedStyleContext};
use style::data::{EagerPseudoStyles, ElementData};
use style::dom::TElement;
use style::properties::ComputedValues;
use style::rule_tree::StrongRuleNode;
use style::selector_parser::PseudoElement;
use style::style_resolver::{PrimaryStyle, ResolvedElementStyles, ResolvedStyle};
use zgui_css::MAX_STYLE_THREADS;
use zgui_dom::Node;

use super::recalc::inherited_groups_equal;
use super::share::{same_values, with_text};

/// How many results are kept per rule chain.
///
/// Each element compares itself with every one of them, so the bound is also the most comparisons
/// a miss pays for.
const PER_CHAIN: usize = 4;

/// Flags on a result that say it read something the key does not hold.
const NOT_KEPT: ComputedValueFlags = ComputedValueFlags::INHERITS_RESET_STYLE
    .union(ComputedValueFlags::USES_CONTAINER_UNITS)
    .union(ComputedValueFlags::IS_RELEVANT_LINK_VISITED);

/// Whether a traversal uses the table.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Mode {
    /// Every element cascades.
    Off,
    /// Every element cascades, and a result the table would have given must equal the cascade's.
    Shadow,
    /// A hit skips the cascade.
    On,
}

/// The mode, from `ZGUI_STYLE_MEMO` (`off`, `shadow` or `on`).
pub(super) fn mode() -> Mode {
    static MODE: OnceLock<Mode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("ZGUI_STYLE_MEMO").as_deref() {
        Ok("off") => Mode::Off,
        Ok("shadow") => Mode::Shadow,
        _ => Mode::On,
    })
}

/// One element's inputs to its cascade, for an element the table can answer.
pub(super) struct Inputs {
    /// The element's rule chain and cascade input flags.
    key: Key,
    /// The element's local name.
    name: LocalName,
    /// The parent's style.
    parent: ServoArc<ComputedValues>,
    /// The element's pseudo-element styles before this restyle, for their rule chains.
    pseudos: EagerPseudoStyles,
}

/// The part of the inputs that is compared by identity.
type Key = (StrongRuleNode, u32);

/// One kept result.
struct Held {
    /// The local name of the element it came from.
    name: LocalName,
    /// The parent it was cascaded under.
    parent: ServoArc<ComputedValues>,
    /// The element's pseudo-element styles before its cascade.
    pseudos_before: EagerPseudoStyles,
    /// The result.
    style: ServoArc<ComputedValues>,
    /// The pseudo-element styles of the result.
    pseudos: EagerPseudoStyles,
    /// Whether the result holds its parent's text group.
    borrowed_text: bool,
}

/// One worker's kept results.
type Table = FxHashMap<Key, SmallVec<[Held; 1]>>;

/// The kept results, one table per worker so that no two workers wait on the same lock.
pub(crate) struct CascadeMemo {
    /// The tables, one per worker index the pool can produce plus one for the calling thread.
    workers: Vec<Mutex<Table>>,
}

/// What the table gave an element.
pub(super) struct Found {
    /// The style.
    pub(super) style: ServoArc<ComputedValues>,
    /// Whether the element holds the kept style itself.
    pub(super) whole: bool,
    /// The pseudo-element styles.
    pub(super) pseudos: EagerPseudoStyles,
}

impl Found {
    /// The element's styles.
    pub(super) fn into_resolved(self) -> ResolvedElementStyles {
        ResolvedElementStyles {
            primary: PrimaryStyle {
                style: ResolvedStyle(self.style),
                // The cousin-sharing guarantee, as the share module describes.
                reused_via_rule_node: self.whole,
            },
            pseudos: self.pseudos,
        }
    }
}

impl Inputs {
    /// The inputs of `element`, or `None` when the table cannot answer for it.
    pub(super) fn of(
        element: Node<'_>,
        context: &SharedStyleContext<'_>,
        data: &ElementData,
        inputs: &ElementCascadeInputs,
    ) -> Option<Self> {
        if context.traversal_flags.for_animation_only()
            || element.implemented_pseudo_element().is_some()
            || element.style_attribute().is_some()
            || !element.matches_user_and_content_rules()
            || element.is_link()
            || inputs.primary.visited_rules.is_some()
            || !inputs.primary.included_cascade_flags.is_empty()
        {
            return None;
        }
        if data
            .styles
            .pseudos
            .as_array()
            .iter()
            .enumerate()
            .any(|(index, style)| {
                style.as_ref().is_some_and(|style| {
                    PseudoElement::from_eager_index(index).is_before_or_after()
                        || style.visited_style().is_some()
                })
            })
        {
            return None;
        }
        let ui = data.styles.get_primary()?.get_ui();
        if (ui.specifies_animations() || ui.specifies_transitions())
            && element.has_animations(context)
        {
            return None;
        }
        let rules = inputs.primary.rules.clone()?;
        let parent = element.inheritance_parent()?;
        let parent = parent.borrow_data()?.styles.get_primary()?.clone();
        if parent.is_display_contents() || parent.visited_style().is_some() {
            return None;
        }
        Some(Self {
            key: (rules, inputs.primary.flags.bits()),
            name: style::values::GenericAtomIdent(element.local_name().clone()),
            parent,
            pseudos: data.styles.pseudos.clone(),
        })
    }
}

impl Held {
    /// Whether a cascade for `inputs` would read what this result's cascade read.
    fn answers(&self, inputs: &Inputs) -> bool {
        self.name == inputs.name
            && same_pseudo_inputs(&self.pseudos_before, &inputs.pseudos)
            && same_parent(&self.parent, &inputs.parent)
    }

    /// The style for an element under `parent`.
    fn give(&self, parent: &ComputedValues) -> Found {
        let text = if self.borrowed_text {
            parent.clone_inherited_text()
        } else {
            ServoArc::new(self.style.get_inherited_text().clone())
        };
        let whole = !self.style.is_display_contents()
            && core::ptr::eq(&*text, self.style.get_inherited_text());
        Found {
            style: if whole {
                ServoArc::clone(&self.style)
            } else {
                with_text(&self.style, text)
            },
            whole,
            pseudos: self.pseudos.clone(),
        }
    }
}

/// Whether a child cascades alike under `a` and under `b`.
fn same_parent(a: &ServoArc<ComputedValues>, b: &ServoArc<ComputedValues>) -> bool {
    ServoArc::ptr_eq(a, b)
        || (inherited_groups_equal(a, b)
            && a.get_box().clone_display() == b.get_box().clone_display()
            && a.custom_properties() == b.custom_properties())
}

/// Whether two elements' pseudo-elements cascade from the same rule chains and flags.
fn same_pseudo_inputs(a: &EagerPseudoStyles, b: &EagerPseudoStyles) -> bool {
    a.as_array()
        .iter()
        .zip(b.as_array())
        .all(|pair| match pair {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.rules == b.rules && a.flags.for_cascade_inputs() == b.flags.for_cascade_inputs()
            }
            _ => false,
        })
}

/// Whether a style read nothing the key leaves out.
fn keepable(style: &ComputedValues) -> bool {
    style.attribute_references.is_none()
        && style.visited_style().is_none()
        && !style.flags.intersects(NOT_KEPT)
}

impl Default for CascadeMemo {
    fn default() -> Self {
        Self {
            workers: (0..=MAX_STYLE_THREADS)
                .map(|_| Mutex::new(Table::default()))
                .collect(),
        }
    }
}

impl CascadeMemo {
    /// Runs `f` on the calling worker's table.
    fn with<R>(&self, f: impl FnOnce(&mut Table) -> R) -> R {
        let slot = rayon::current_thread_index()
            .map_or(MAX_STYLE_THREADS, |index| index.min(MAX_STYLE_THREADS));
        f(&mut self.workers[slot]
            .lock()
            .unwrap_or_else(|held| held.into_inner()))
    }

    /// The style a cascade for `inputs` would produce, if the table holds it.
    pub(super) fn find(&self, inputs: &Inputs) -> Option<Found> {
        self.with(|table| {
            table
                .get(&inputs.key)?
                .iter()
                .find(|held| held.answers(inputs))
                .map(|held| held.give(&inputs.parent))
        })
    }

    /// Keeps `styles`, the result of a cascade for `inputs`, if it read nothing the key leaves out.
    pub(super) fn keep(&self, inputs: Inputs, styles: &ResolvedElementStyles) {
        let style = styles.primary_style();
        let pseudos = styles.pseudos.as_array().iter().flatten();
        if !keepable(style) || !pseudos.into_iter().all(|style| keepable(style)) {
            return;
        }
        let borrowed_text = core::ptr::eq(
            style.get_inherited_text(),
            inputs.parent.get_inherited_text(),
        );
        let held = Held {
            name: inputs.name,
            parent: inputs.parent,
            pseudos_before: inputs.pseudos,
            style: ServoArc::clone(style),
            pseudos: styles.pseudos.clone(),
            borrowed_text,
        };
        self.with(|table| {
            let chain = table.entry(inputs.key).or_default();
            if chain.len() >= PER_CHAIN {
                chain.remove(0);
            }
            chain.push(held);
        });
    }
}

/// Stops the process when `found` is not what the cascade produced.
pub(super) fn check(found: &Found, real: &ResolvedElementStyles, inputs: &Inputs) {
    let primary = real.primary_style();
    assert!(
        same_values(&found.style, primary),
        "the cascade memo gave <{:?}> a style its cascade did not produce",
        inputs.name,
    );
    let parent = inputs.parent.get_inherited_text();
    assert_eq!(
        core::ptr::eq(found.style.get_inherited_text(), parent),
        core::ptr::eq(primary.get_inherited_text(), parent),
        "the cascade memo gave <{:?}> another text group identity",
        inputs.name,
    );
    for (found, real) in found.pseudos.as_array().iter().zip(real.pseudos.as_array()) {
        let same = match (found, real) {
            (None, None) => true,
            (Some(found), Some(real)) => same_values(found, real),
            _ => false,
        };
        assert!(
            same,
            "the cascade memo gave <{:?}> a pseudo-element style its cascade did not produce",
            inputs.name,
        );
    }
}
