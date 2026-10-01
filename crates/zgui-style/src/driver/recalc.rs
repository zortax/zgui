//! The per-element step of the style traversal, owned here so the hint handed to an element's
//! children can be decided by this crate.
//!
//! DERIVED-FROM: stylo 0.19.0 `traversal.rs` (`recalc_style_at`, `compute_style`,
//! `note_children`), MPL-2.0. The engine's own copy computes the children's hint from the damage
//! it accumulated, which for a change to a custom property or to any reset property means "cascade
//! every descendant again". The copy here computes it from what the children can actually observe
//! of the element's new style — its inherited property groups — and so leaves a subtree alone when
//! nothing it inherits moved. See [`super::traversal`] for where it is called from.

use std::sync::{Arc, OnceLock};

use servo_arc::Arc as ServoArc;
use style::computed_values::display::T as Display;
use style::context::{ElementCascadeInputs, StyleContext};
use style::custom_properties::ComputedCustomProperties;
use style::custom_properties_map::CustomPropertiesMap;
use style::data::{ElementData, RestyleKind};
use style::dom::{TElement, TNode};
use style::invalidation::element::restyle_hints::RestyleHint;
use style::matching::MatchMethods;
use style::properties::{ComputedValues, ComputedValuesInner};
use style::sharing::StyleSharingTarget;
use style::style_resolver::{PseudoElementResolution, StyleResolverForElement};
use style::stylist::RuleInclusion;
use style::traversal::{DomTraversal, PerLevelTraversalData};
use style::traversal_flags::TraversalFlags;
use zgui_dom::Node;
use zgui_profile::{Counter, counter};

use crate::driver::traversal::RecalcStyle;

/// Which custom properties an element's inherited map changed by, for the elements below it.
#[derive(Debug, Default)]
pub(crate) struct ChangedNames {
    /// The names, without their `--` prefix.
    pub(crate) names: Vec<String>,
    /// Whether the change is too wide to name, so that every descendant counts as a reader.
    ///
    /// Also set when a name the framework reads on the element's behalf moved — a fill, a stroke,
    /// a shader parameter — since no declaration of the element mentions those.
    pub(crate) wildcard: bool,
    /// The Bloom set of `names`, or every name when the change is a wildcard.
    pub(crate) bloom: u64,
}

impl ChangedNames {
    /// A change too wide to name.
    fn wildcard() -> Self {
        Self {
            names: Vec::new(),
            wildcard: true,
            bloom: zgui_dom::side::readers::ALL,
        }
    }

    /// Whether a subtree whose readers union is `below` can hold a reader of this change.
    fn reaches(&self, below: u64) -> bool {
        self.wildcard || below & self.bloom != 0
    }
}

/// How many names a change may carry before every descendant is treated as a reader.
const NAMED_AT_MOST: usize = 16;

/// The prefix of the custom properties the framework reads on an element's behalf.
const FRAMEWORK_PREFIX: &str = "zgui-";

impl ChangedNames {
    /// The names whose values differ between the two maps, in either direction.
    fn between(old: &CustomPropertiesMap, new: &CustomPropertiesMap) -> Self {
        let mut names = Vec::new();
        for (name, value) in new.iter() {
            if old.get(name) != value.as_ref() {
                names.push(name.to_string());
            }
        }
        for (name, _) in old.iter() {
            if new.get(name).is_none() {
                names.push(name.to_string());
            }
        }
        let wildcard = names.len() > NAMED_AT_MOST
            || names.iter().any(|name| name.starts_with(FRAMEWORK_PREFIX));
        let bloom = if wildcard {
            zgui_dom::side::readers::ALL
        } else {
            names.iter().fold(0, |bits, name| {
                bits | zgui_dom::side::readers::name_bits(name)
            })
        };
        Self {
            names,
            wildcard,
            bloom,
        }
    }
}

/// The element type this copy is written for.
///
/// The engine's copy is generic and reaches the element through `unsafe` trait methods; this one
/// serves one document type, whose flags and data have safe methods of their own.
type E<'doc> = Node<'doc>;

/// Calculates the style for a single element, and notes which children the traversal descends to.
pub(super) fn recalc_style_at<'doc, F>(
    traversal: &RecalcStyle<'_>,
    traversal_data: &PerLevelTraversalData,
    context: &mut StyleContext<E<'doc>>,
    element: E<'doc>,
    data: &mut ElementData,
    note_child: F,
) where
    F: FnMut(E<'doc>),
{
    let flags = context.shared.traversal_flags;
    let is_initial_style = !data.has_styles();

    context.thread_local.statistics.elements_traversed += 1;
    counter::bump(Counter::ElementsTraversed);

    // The parent's visit raised the bit when the map this element inherits from moved. A style
    // built against the old map reads stale values through `var()`; whether that matters is
    // decided here, before the element's own kind of restyle is asked for, because the answer
    // may be to cascade after all.
    let mut custom_changed: Option<Arc<ChangedNames>> = None;
    let map_changed = !flags.for_animation_only() && element.take_custom_map_changed();
    if map_changed
        && data.has_styles()
        && data.restyle_kind(context.shared).is_none()
        && let Some(parent_style) = parent_style(element)
    {
        // The names come from the parent's visit this frame. Without them — a subtree the
        // change skipped because nothing in it read the names, entered on a later frame for a
        // reason of its own — they are read off the two maps themselves.
        let names = TElement::traversal_parent(&element)
            .and_then(|parent| traversal.changed_names_of(parent.index()))
            .unwrap_or_else(|| {
                data.styles.get_primary().map_or_else(
                    || Arc::new(ChangedNames::wildcard()),
                    |old| {
                        Arc::new(ChangedNames::between(
                            &old.custom_properties().inherited,
                            &parent_style.custom_properties().inherited,
                        ))
                    },
                )
            });
        if names.names.is_empty() && !names.wildcard {
            // Equal maps under different identities: the element takes the parent's and its
            // children keep theirs, which hold the same values.
            if let Some(old) = data.styles.get_primary() {
                data.styles.primary = Some(with_inherited_map(old, &parent_style));
                counter::bump(Counter::CustomMapsRefreshed);
            }
        } else if reads_custom_properties(traversal, data, &names) {
            data.hint.insert(RestyleHint::RECASCADE_SELF);
        } else if let Some(old) = data.styles.get_primary() {
            data.styles.primary = Some(with_inherited_map(old, &parent_style));
            counter::bump(Counter::CustomMapsRefreshed);
            custom_changed = Some(names);
        }
    }

    let restyle_kind = data.restyle_kind(context.shared);
    let mut child_restyle_hint = RestyleHint::empty();

    if let Some(restyle_kind) = restyle_kind {
        let (hint, changed) = compute_style(traversal_data, context, element, data, restyle_kind);
        child_restyle_hint = hint;
        if let Some(changed) = changed {
            custom_changed = Some(changed);
        }

        if !element.matches_user_and_content_rules() {
            // A native anonymous subtree is always cascaded: it may hold pseudo-elements that
            // inherit from the closest ordinary ancestor rather than from this element.
            child_restyle_hint |= RestyleHint::RECASCADE_SELF;
        }

        // An element restyled to `display: none` takes every style below it with it, and the
        // traversal stops here.
        if data.styles.is_display_none() {
            element.clear_style_data_below();
        }
    } else {
        debug_assert!(data.has_styles());
        data.set_traversed_without_styling();
    }
    if let Some(changed) = &custom_changed {
        traversal.note_changed_names(element.index(), Arc::clone(changed));
    }

    debug_assert!(
        flags.for_animation_only() || !data.hint.has_animation_hint(),
        "animation restyle hint should be handled during animation-only restyles"
    );
    let mut propagated_hint = data.hint.propagate(&flags);
    propagated_hint |= child_restyle_hint;

    let has_dirty_descendants_for_this_restyle = if flags.for_animation_only() {
        element.has_animation_only_dirty_descendants()
    } else {
        element.has_dirty_descendants()
    };

    let mut traverse_children = has_dirty_descendants_for_this_restyle
        || !propagated_hint.is_empty()
        || custom_changed.is_some();
    traverse_children = traverse_children && !data.styles.is_display_none();

    if traverse_children {
        note_children(
            context,
            element,
            data,
            propagated_hint,
            is_initial_style,
            custom_changed.as_deref(),
            note_child,
        );
    }

    clear_state_after_traversing(element, data, flags);
}

fn clear_state_after_traversing(element: E<'_>, data: &mut ElementData, flags: TraversalFlags) {
    if flags.intersects(TraversalFlags::FinalAnimationTraversal) {
        debug_assert!(flags.for_animation_only());
        data.clear_restyle_flags_and_damage();
        element.clear_animation_work_below();
    }
}

/// The parent's style, which holds the map the element inherits from.
fn parent_style(element: E<'_>) -> Option<ServoArc<ComputedValues>> {
    let parent = TElement::traversal_parent(&element)?;
    let data = parent.borrow_data()?;
    data.styles.get_primary().cloned()
}

/// Whether the element's own declarations read any of `changed`, or declare custom properties.
///
/// Generated content cascades on rules of its own, which are scanned the same way.
fn reads_custom_properties(
    traversal: &RecalcStyle<'_>,
    data: &ElementData,
    changed: &ChangedNames,
) -> bool {
    if changed.wildcard {
        return true;
    }
    let Some(style) = data.styles.get_primary() else {
        return true;
    };
    if declares_any(style, &traversal.wildcard_names) {
        return true;
    }
    let pseudos = data
        .styles
        .pseudos
        .as_optional_array()
        .into_iter()
        .flat_map(|array| array.iter().flatten());
    std::iter::once(style).chain(pseudos).any(|style| {
        style.rules.as_ref().is_some_and(|rules| {
            let refs = traversal.refs_of(rules);
            refs.declares || refs.reads_any(&changed.names)
        })
    })
}

/// Whether `style` holds any of `names` in either of its custom property maps.
pub(super) fn declares_any(
    style: &ComputedValues,
    names: &[style::custom_properties::Name],
) -> bool {
    let maps = style.custom_properties();
    names
        .iter()
        .any(|name| maps.inherited.get(name).is_some() || maps.non_inherited.get(name).is_some())
}

/// The element's style again, holding the map its parent holds now and everything else it held.
///
/// What a cascade would rebuild for an element none of whose declarations read the map: every
/// group stays the allocation it was, so nothing keyed on those addresses moves, and only the
/// map — which its own children inherit from — is the parent's current one.
fn with_inherited_map(old: &ComputedValues, parent: &ComputedValues) -> ServoArc<ComputedValues> {
    ComputedValues::new(
        old.pseudo().as_ref(),
        ComputedCustomProperties {
            inherited: parent.custom_properties().inherited.clone(),
            non_inherited: old.custom_properties().non_inherited.clone(),
        },
        old.attribute_references.clone(),
        old.writing_mode,
        old.effective_zoom,
        old.flags,
        old.rules.clone(),
        old.visited_style()
            .map(|visited| ServoArc::new(visited.clone())),
        old.clone_background(),
        old.clone_border(),
        old.clone_box(),
        old.clone_column(),
        old.clone_counters(),
        old.clone_effects(),
        old.clone_font(),
        old.clone_inherited_box(),
        old.clone_inherited_table(),
        old.clone_inherited_text(),
        old.clone_inherited_ui(),
        old.clone_list(),
        old.clone_margin(),
        old.clone_outline(),
        old.clone_padding(),
        ComputedValuesInner::clone_position(old),
        old.clone_svg(),
        old.clone_table(),
        old.clone_text(),
        old.clone_ui(),
    )
}

fn compute_style<'doc>(
    traversal_data: &PerLevelTraversalData,
    context: &mut StyleContext<E<'doc>>,
    element: E<'doc>,
    data: &mut ElementData,
    kind: RestyleKind,
) -> (RestyleHint, Option<Arc<ChangedNames>>) {
    context.thread_local.statistics.elements_styled += 1;

    if data.has_styles() {
        data.set_restyled();
    }

    let mut important_rules_changed = false;
    let new_styles = match kind {
        RestyleKind::MatchAndCascade => {
            debug_assert!(
                !context.shared.traversal_flags.for_animation_only() || !data.has_styles(),
                "MatchAndCascade shouldn't normally be processed during animation-only traversal"
            );
            context
                .thread_local
                .bloom_filter
                .insert_parents_recovering(element, traversal_data.current_dom_depth);
            context.thread_local.bloom_filter.assert_complete(element);
            debug_assert_eq!(
                context.thread_local.bloom_filter.matching_depth(),
                traversal_data.current_dom_depth
            );

            important_rules_changed = true;

            let mut target = StyleSharingTarget::new(element);
            match target.share_style_if_possible(context) {
                Some(shared_styles) => {
                    context.thread_local.statistics.styles_shared += 1;
                    shared_styles
                }
                None => {
                    context.thread_local.statistics.elements_matched += 1;
                    let new_styles = {
                        let mut resolver = StyleResolverForElement::new(
                            element,
                            context,
                            RuleInclusion::All,
                            PseudoElementResolution::IfApplicable,
                        );
                        resolver.resolve_style_with_default_parents()
                    };
                    context.thread_local.sharing_cache.insert_if_possible(
                        &element,
                        &new_styles.primary,
                        Some(&mut target),
                        traversal_data.current_dom_depth,
                        context.shared,
                    );
                    new_styles
                }
            }
        }
        RestyleKind::CascadeWithReplacements(flags) => {
            let mut cascade_inputs = ElementCascadeInputs::new_from_element_data(data);
            important_rules_changed = element.replace_rules(flags, context, &mut cascade_inputs);
            let mut resolver = StyleResolverForElement::new(
                element,
                context,
                RuleInclusion::All,
                PseudoElementResolution::IfApplicable,
            );
            resolver.cascade_styles_with_default_parents(cascade_inputs)
        }
        RestyleKind::CascadeOnly => {
            let cascade_inputs = ElementCascadeInputs::new_from_element_data(data);
            let new_styles = {
                let mut resolver = StyleResolverForElement::new(
                    element,
                    context,
                    RuleInclusion::All,
                    PseudoElementResolution::IfApplicable,
                );
                resolver.cascade_styles_with_default_parents(cascade_inputs)
            };
            if !new_styles.primary.reused_via_rule_node {
                context.thread_local.sharing_cache.insert_if_possible(
                    &element,
                    &new_styles.primary,
                    None,
                    traversal_data.current_dom_depth,
                    context.shared,
                );
            }
            new_styles
        }
    };

    // Kept across the call that replaces it: the children's hint is decided by what they can
    // observe of the change, which is the difference between the style they inherited from and
    // the one they will.
    let old_primary = data.styles.primary.clone();
    // The restyle removes every finished animation. The hold keeps the ones that fill forwards.
    let held = crate::driver::animations::hold::hold(element, context.shared);
    let hint = element.finish_restyle(context, data, new_styles, important_rules_changed);
    crate::driver::animations::hold::release(context.shared, held);
    match (old_primary, data.styles.get_primary()) {
        (Some(old), Some(new)) if !engine_hints() => {
            let narrowed = narrowed_children_hint(&old, new, hint);
            let changed = (old.custom_properties().inherited != new.custom_properties().inherited)
                .then(|| {
                    Arc::new(ChangedNames::between(
                        &old.custom_properties().inherited,
                        &new.custom_properties().inherited,
                    ))
                });
            (narrowed, changed)
        }
        _ => (hint, None),
    }
}

/// Whether the children's hint is left as the engine computed it.
///
/// `ZGUI_STYLE_ENGINE_HINTS=1` restores the engine's rule for an A/B run: every restyle that
/// changed anything hands its children a recascade.
fn engine_hints() -> bool {
    static ENGINE: OnceLock<bool> = OnceLock::new();
    *ENGINE.get_or_init(|| std::env::var_os("ZGUI_STYLE_ENGINE_HINTS").is_some_and(|v| v == "1"))
}

/// The hint an element's children take from its restyle, narrowed to what they can observe.
///
/// The engine hands every child `RECASCADE_SELF` whenever anything about the element changed —
/// a background colour, a border — because its difference does not tell reset properties from
/// inherited ones. A child's cascade reads only the element's inherited groups and its custom
/// property map, so when those stand the recascade is dropped, and a reset change is answered by
/// the engine's own narrower flag for children that inherit reset properties explicitly.
fn narrowed_children_hint(
    old: &ComputedValues,
    new: &ComputedValues,
    engine: RestyleHint,
) -> RestyleHint {
    if !engine.contains(RestyleHint::RECASCADE_SELF) {
        return engine;
    }
    if !inherited_groups_equal(old, new) || unconditional_recascade_for_reset_change(old, new) {
        return engine;
    }
    let mut hint = engine;
    hint.remove(RestyleHint::RECASCADE_SELF);
    if reset_groups_differ(old, new) {
        hint |= RestyleHint::RECASCADE_SELF_IF_INHERIT_RESET_STYLE;
    }
    // A custom property map that moved is handed down by name rather than as a recascade: each
    // child decides at its own visit whether it reads what moved. The engine also tells the
    // children in case a container style query reads the map; this document rejects `@container`
    // (see `sheets::errors`), so there is no such query, and a hint left on the children would
    // make the parent note style work below it that no dirty-child record can reach.
    hint.remove(
        RestyleHint::RESTYLE_IF_AFFECTED_BY_STYLE_QUERIES
            | RestyleHint::RESTYLE_IF_AFFECTED_BY_NAMED_STYLE_CONTAINER,
    );
    hint
}

/// Whether every group a child inherits from is the same, by pointer or by value.
fn inherited_groups_equal(old: &ComputedValues, new: &ComputedValues) -> bool {
    macro_rules! same {
        ($get:ident) => {
            core::ptr::eq(old.$get(), new.$get()) || old.$get() == new.$get()
        };
    }
    same!(get_font)
        && same!(get_inherited_box)
        && same!(get_inherited_table)
        && same!(get_inherited_text)
        && same!(get_inherited_ui)
        && same!(get_list)
        && old.flags.maybe_inherited() == new.flags.maybe_inherited()
        && old.effective_zoom == new.effective_zoom
        && old.writing_mode == new.writing_mode
}

/// Whether any reset group differs, by pointer and by value.
fn reset_groups_differ(old: &ComputedValues, new: &ComputedValues) -> bool {
    macro_rules! differs {
        ($get:ident) => {
            !core::ptr::eq(old.$get(), new.$get()) && old.$get() != new.$get()
        };
    }
    differs!(get_background)
        || differs!(get_border)
        || differs!(get_box)
        || differs!(get_column)
        || differs!(get_counters)
        || differs!(get_effects)
        || differs!(get_margin)
        || differs!(get_outline)
        || differs!(get_padding)
        || differs!(get_position)
        || differs!(get_svg)
        || differs!(get_table)
        || differs!(get_text)
        || differs!(get_ui)
}

/// The engine's own reasons to recascade children for a reset change, copied from
/// `need_to_unconditionally_recascade_for_reset_change` in stylo `matching.rs`.
fn unconditional_recascade_for_reset_change(old: &ComputedValues, new: &ComputedValues) -> bool {
    let old_display = old.clone_display();
    let new_display = new.clone_display();
    if old_display != new_display {
        if old_display == Display::None {
            return true;
        }
        if old_display.is_item_container() != new_display.is_item_container() {
            return true;
        }
        if old_display.is_contents() || new_display.is_contents() {
            return true;
        }
    }
    false
}

fn note_children<'doc, F>(
    context: &mut StyleContext<E<'doc>>,
    element: E<'doc>,
    data: &ElementData,
    propagated_hint: RestyleHint,
    is_initial_style: bool,
    custom_changed: Option<&ChangedNames>,
    mut note_child: F,
) where
    F: FnMut(E<'doc>),
{
    let flags = context.shared.traversal_flags;

    for child_node in element.traversal_children() {
        let child = match child_node.as_element() {
            Some(el) => el,
            None => {
                if <RecalcStyle<'_> as DomTraversal<E<'doc>>>::text_node_needs_traversal(
                    child_node, data,
                ) {
                    note_child(child_node);
                }
                continue;
            }
        };

        // Every child's map moved, and every child is told. Whether the child is *visited* for
        // it is decided by what its subtree reads: a subtree with no reader of the changed names
        // keeps the bit and is refreshed the next time something else brings the traversal in.
        let mut reached = false;
        if let Some(names) = custom_changed {
            child.note_custom_map_changed();
            let below = zgui_dom::side::readers::below(child.store(), child.index());
            reached = names.reaches(below);
        }
        let mut child_data = child.mutate_data();
        let mut child_data = child_data.as_deref_mut();

        if let Some(ref mut child_data) = child_data {
            child_data.hint.insert(propagated_hint);
            // A reset-only change above asks a child to cascade again only if it inherits reset
            // properties, which is answered off the child's own style right here. Left on the
            // child, the hint alone is a reason to visit it — and a background moving on every
            // row of a list visited every cell of every row for a question with a known answer.
            if child_data.hint.bits() == RestyleHint::RECASCADE_SELF_IF_INHERIT_RESET_STYLE.bits()
                && child_data.styles.get_primary().is_some_and(|style| {
                    !style.flags.contains(
                        style::computed_value_flags::ComputedValueFlags::INHERITS_RESET_STYLE,
                    )
                })
            {
                child_data.hint = RestyleHint::empty();
            }
            child_data.invalidate_style_if_needed(
                child,
                context.shared,
                Some(&context.thread_local.stack_limit_checker),
                &mut context.thread_local.selector_caches,
            );
        }

        // The engine's reasons to visit the child are noted below the parent, where the document's
        // retirement walk finds them through the dirty-child records the same marks widened. A
        // child visited only because its inherited map moved is found through its own bit: noting
        // it below the parent would raise a subtree word the walk never reaches, and every later
        // mark climbing through it would stop there.
        let engine_needs =
            super::traversal::engine_needs_traversal(child, flags, child_data.as_deref());
        if !engine_needs && custom_changed.is_some() && !reached {
            counter::bump(Counter::CustomSubtreesSkipped);
        }
        if engine_needs || reached {
            note_child(child_node);
            if !is_initial_style && engine_needs {
                if flags.for_animation_only() {
                    element.note_animation_work_below();
                } else {
                    element.note_style_work_below();
                }
            }
        }
    }
}
