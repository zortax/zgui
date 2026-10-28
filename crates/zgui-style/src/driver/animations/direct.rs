//! Animation values written into the element's own computed style, without a cascade.
//!
//! The animation-only traversal replaces an element's animation and transition declarations and
//! then cascades it again: every declaration of every matched rule, every `var()` substituted again,
//! and for an element with transitions a second cascade that leaves the transition values out. On a
//! frame that only advances the clock, all of that produces the style the element already holds with
//! the animated values moved. This module takes that frame's style from the held one in place: it
//! clones the held style, writes the interpolated values into the groups they belong to, and keeps
//! every other group as the allocation it was.
//!
//! The path is taken only when its result is the result of the cascade:
//!
//! - Every animated property is one whose computed value depends on no other property of the
//!   element, that the style adjuster does not change, and that sets no flag. See [`direct`].
//! - The element has no visited style, no zoom, is not the root, is not a container and is not
//!   `display: none`, and none of its generated content animates. Generated content that reads an
//!   inherited value which moved cascades again from its own rules; the rest keeps its style.
//! - Only the animation hints are pending on it, and its inherited custom property map did not move.
//! - Every animation and transition on it is running or waiting for its delay, the set is not dirty,
//!   and no animation of it crossed a lifecycle edge on the last tick. A crossing is where a value
//!   starts or stops being animated, and only the cascade puts the base value back.
//! - No property has a keyframe animation and a transition at the same time, and no generated
//!   content specifies transitions. In both cases the cascade can start or replace a transition.
//! - No `!important` declaration can override a keyframe value. Keyframe values are computed when
//!   the animation starts, so a declaration added later wins only in the cascade. Transition values
//!   override every declaration, so only keyframe animations need this check.
//!
//! The rule node is still replaced. A later cascade of the element (a hover, a theme change) reads
//! the animation declarations from it, so a node left behind would draw that frame from old values.

use rustc_hash::FxHashSet;
use servo_arc::Arc as ServoArc;
use style::animation::{AnimationSetKey, AnimationState, ElementAnimationSet};
use style::computed_value_flags::ComputedValueFlags;
use style::context::{ElementCascadeInputs, StyleContext};
use style::data::{EagerPseudoStyles, ElementData};
use style::invalidation::element::restyle_hints::RestyleHint;
use style::matching::{MatchMethods, StyleChange};
use style::properties::{ComputedValues, LonghandId, PropertyDeclarationId};
use style::rule_tree::{CascadeOrigin, StrongRuleNode};
use style::selector_parser::PseudoElement;
use style::style_resolver::{
    PrimaryStyle, PseudoElementResolution, ResolvedElementStyles, ResolvedStyle,
    StyleResolverForElement,
};
use style::stylist::RuleInclusion;
use style::traversal_flags::TraversalFlags;
use style::values::computed::Zoom;
use zgui_dom::{Node, NodeIndex};

use crate::driver::animations::set;

/// Whether the direct path may write `longhand`.
///
/// A property is here when its computed value is the interpolated value itself: no other property
/// of the element changes it (as `border-*-style` changes a border width), the style adjuster never
/// rewrites it (as it rewrites `display` and `overflow`), the cascade does not snap it to device
/// pixels (as it snaps `outline-offset`), and it sets no style flag (as `opacity: 0` does). Colours
/// keep `currentColor` unresolved in the computed value, so a colour reads the element's `color` at
/// use time on both paths.
fn direct(longhand: LonghandId) -> bool {
    matches!(
        longhand,
        LonghandId::Width
            | LonghandId::Height
            | LonghandId::MinWidth
            | LonghandId::MinHeight
            | LonghandId::MaxWidth
            | LonghandId::MaxHeight
            | LonghandId::PaddingTop
            | LonghandId::PaddingRight
            | LonghandId::PaddingBottom
            | LonghandId::PaddingLeft
            | LonghandId::MarginTop
            | LonghandId::MarginRight
            | LonghandId::MarginBottom
            | LonghandId::MarginLeft
            | LonghandId::Top
            | LonghandId::Right
            | LonghandId::Bottom
            | LonghandId::Left
            | LonghandId::RowGap
            | LonghandId::ColumnGap
            | LonghandId::FlexGrow
            | LonghandId::FlexShrink
            | LonghandId::FlexBasis
            | LonghandId::Color
            | LonghandId::BackgroundColor
            | LonghandId::BorderTopColor
            | LonghandId::BorderRightColor
            | LonghandId::BorderBottomColor
            | LonghandId::BorderLeftColor
            | LonghandId::OutlineColor
            | LonghandId::BorderTopLeftRadius
            | LonghandId::BorderTopRightRadius
            | LonghandId::BorderBottomRightRadius
            | LonghandId::BorderBottomLeftRadius
            | LonghandId::BoxShadow
            | LonghandId::TextShadow
            | LonghandId::LetterSpacing
            | LonghandId::WordSpacing
            | LonghandId::Transform
            | LonghandId::Translate
            | LonghandId::Rotate
            | LonghandId::Scale
            | LonghandId::TransformOrigin
            | LonghandId::Filter
    )
}

/// Whether the direct path may write the property a transition or keyframe moves.
fn direct_property(property: PropertyDeclarationId<'_>) -> bool {
    match property {
        PropertyDeclarationId::Longhand(longhand) => direct(longhand),
        PropertyDeclarationId::Custom(_) => false,
    }
}

/// Whether an animation in `state` is one the direct path may sample.
fn ticking(state: &AnimationState) -> bool {
    matches!(state, AnimationState::Running | AnimationState::Pending)
}

/// Styles `element` for its pending animation hints without a cascade, when the result is the
/// cascade's.
///
/// Returns the hint the element's children take, or `None` with nothing changed when the element
/// needs the cascade. `settling` holds the elements whose animations crossed a lifecycle edge since
/// their last cascade.
pub(crate) fn restyle<'doc>(
    context: &mut StyleContext<Node<'doc>>,
    element: Node<'doc>,
    data: &mut ElementData,
    replacements: RestyleHint,
    settling: &FxHashSet<NodeIndex>,
) -> Option<RestyleHint> {
    let flags = context.shared.traversal_flags;
    if !flags.for_animation_only() || flags.contains(TraversalFlags::FinalAnimationTraversal) {
        return None;
    }
    // Any other hint is work the cascade does, and a recascade hint means the parent moved.
    if !(data.hint & !RestyleHint::for_animations()).is_empty()
        || settling.contains(&element.index())
        || element.has_custom_map_changed()
    {
        return None;
    }
    let old = data.styles.primary.clone()?;
    let rules = old.rules.clone()?;
    if old.visited_style().is_some()
        || old.effective_zoom != Zoom::ONE
        || old
            .flags
            .contains(ComputedValueFlags::IS_ROOT_ELEMENT_STYLE)
        || old.get_box().clone_display().is_none()
        || !old.clone_container_type().is_normal()
    {
        return None;
    }

    let held_pseudos = data.styles.pseudos.clone();
    let pseudos = held_pseudos.as_array();
    // Generated content with transitions of its own starts them from the inherited values that
    // move, and only the cascade does that.
    if pseudos
        .iter()
        .flatten()
        .any(|pseudo| pseudo.visited_style().is_some() || pseudo.get_ui().specifies_transitions())
    {
        return None;
    }

    let animated = {
        let sets = context.shared.animations.sets.read();
        let key = set::key_of(element.index());
        // A generated-content animation is the cascade's: its style has no other source.
        let pseudo_animates = pseudos.iter().enumerate().any(|(index, style)| {
            style.is_some()
                && sets.contains_key(&AnimationSetKey::new(
                    key.node,
                    Some(PseudoElement::from_eager_index(index)),
                ))
        });
        if pseudo_animates {
            return None;
        }
        animated(sets.get(&key)?, context, &old, &rules)?
    };

    // The node the cascade would have built, so a later cascade reads this frame's values.
    let mut inputs = ElementCascadeInputs::new_from_element_data(data);
    element.replace_rules(replacements, context, &mut inputs);
    let mut style = animated;
    ServoArc::make_mut(&mut style).rules = inputs.primary.rules;

    let difference = element.compute_style_difference(&old, &style, None);
    data.damage |= difference.damage;
    // What the engine's own comparison hands the children. The flags, the zoom, the display and
    // the custom properties are the held style's, so only the change itself is left to read.
    let (hint, inherited) = match difference.change {
        StyleChange::Unchanged => (RestyleHint::empty(), false),
        StyleChange::Changed {
            reset_only: true, ..
        } => (RestyleHint::RECASCADE_SELF_IF_INHERIT_RESET_STYLE, false),
        StyleChange::Changed { .. } => (RestyleHint::RECASCADE_SELF, true),
    };

    let primary = PrimaryStyle {
        style: ResolvedStyle(style),
        reused_via_rule_node: false,
    };
    // A generated-content style inherits from the primary one. It keeps its allocation unless it
    // reads what moved: an inherited value, or a reset value it inherits explicitly.
    let mut generated = EagerPseudoStyles::default();
    let mut pseudo_inputs = inputs.pseudos.into_array();
    for (index, held) in pseudos.iter().enumerate() {
        let Some(held) = held else {
            continue;
        };
        let pseudo = PseudoElement::from_eager_index(index);
        let reads_change = inherited
            || (!matches!(difference.change, StyleChange::Unchanged)
                && held
                    .flags
                    .contains(ComputedValueFlags::INHERITS_RESET_STYLE));
        let restyled = match pseudo_inputs.as_mut().and_then(|array| array[index].take()) {
            Some(inputs) if reads_change => {
                let restyled = StyleResolverForElement::new(
                    element,
                    context,
                    RuleInclusion::All,
                    PseudoElementResolution::IfApplicable,
                )
                .cascade_style_and_visited_for_pseudo_with_default_parents(
                    inputs, &pseudo, &primary,
                )
                .0;
                data.damage |= element
                    .compute_style_difference(held, &restyled, Some(&pseudo))
                    .damage;
                restyled
            }
            _ => ServoArc::clone(held),
        };
        generated.set(&pseudo, restyled);
    }
    data.set_styles(ResolvedElementStyles {
        primary,
        pseudos: generated,
    });
    Some(hint)
}

/// The held style with the values of `animations` written into it, or `None` when one of them
/// needs the cascade.
///
/// Keyframe values are written first and transition values after them, the order of their cascade
/// levels, and a later transition of one property replaces an earlier one, as in the engine's map.
fn animated(
    animations: &ElementAnimationSet,
    context: &StyleContext<Node<'_>>,
    old: &ServoArc<ComputedValues>,
    rules: &StrongRuleNode,
) -> Option<ServoArc<ComputedValues>> {
    if animations.dirty
        || !animations.animations.iter().all(|one| ticking(&one.state))
        || !animations.transitions.iter().all(|one| ticking(&one.state))
        || !animations
            .transitions
            .iter()
            .all(|one| direct_property(one.property_animation.property_id()))
    {
        return None;
    }
    let now = context.shared.current_time_for_animations;
    let keyframes = animations.get_value_map_for_active_animations(now);
    if let Some(keyframes) = &keyframes {
        if !keyframes
            .keys()
            .all(|property| direct_property(property.as_borrowed()))
        {
            return None;
        }
        if !keyframes.is_empty() && overridden_by_important(rules) {
            return None;
        }
        // A keyframe moves the after-change value of a transition of the same property, and the
        // cascade decides whether that replaces the transition.
        if animations
            .transitions
            .iter()
            .any(|one| keyframes.contains_key(&one.property_animation.property_id().to_owned()))
        {
            return None;
        }
    }

    let mut style = ServoArc::clone(old);
    let values = ServoArc::make_mut(&mut style);
    for value in keyframes.iter().flat_map(|map| map.values()) {
        value.set_in_style_for_servo(values, context.shared);
    }
    for transition in &animations.transitions {
        transition
            .calculate_value(now)
            .set_in_style_for_servo(values, context.shared);
    }
    Some(style)
}

/// Whether the chain ending at `rules` holds an `!important` declaration above the keyframe level.
///
/// The chain runs from the most important level at its leaf down: transitions, then the important
/// levels, then keyframe animations and everything normal. So the walk stops at the first normal
/// level it meets.
fn overridden_by_important(rules: &StrongRuleNode) -> bool {
    for node in rules.self_and_ancestors() {
        let level = node.cascade_level();
        if level.origin() == CascadeOrigin::Transitions {
            continue;
        }
        return level.is_important() && node.style_source().is_some();
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use style::properties::ComputedValues;
    use style::selector_parser::{EAGER_PSEUDO_COUNT, PseudoElement};
    use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKind};
    use zgui_geom::CssPx;
    use zgui_interned::{ClassName, ElementName};
    use zgui_text::FixedMetrics;

    use crate::device::Viewport;
    use crate::engine::StyleEngine;
    use crate::sheets::SheetSource;
    use crate::sheets::origin::SheetOrigin;
    use crate::{AnimationTime, TextRun};

    /// How long one frame is, in seconds.
    const FRAME: f64 = 0.016;

    /// One document and the engine that styles it.
    struct Side {
        document: Document,
        engine: StyleEngine,
        /// The root element.
        root: NodeIndex,
        /// Every element below the root, in tree order.
        elements: Vec<NodeIndex>,
        /// The colour each text run is drawn in, as the text paint updates told it.
        drawn: HashMap<(usize, TextRun), String>,
    }

    impl Side {
        /// `rows` rows of a list, each holding one label, styled by `sheet`.
        fn new(sheet: &str, rows: usize, direct: bool) -> Self {
            let mut document = Document::new();
            let root = document.append(
                document.document_index(),
                NodeKind::Element,
                ElementName::new("root"),
            );
            let mut engine = StyleEngine::new(
                &document,
                Arc::new(FixedMetrics::new()),
                Viewport::new(CssPx(800.0), CssPx(600.0)),
            );
            if !direct {
                engine.animations_mut().cascade_every_tick();
            }
            let (_handle, dropped) =
                engine.add_sheet(&document, SheetOrigin::Author, SheetSource::Text(sheet));
            assert!(dropped.is_empty(), "{dropped:?}");
            std::mem::forget(_handle);
            let elements = document
                .edit(&EverythingMatters, |batch| {
                    let list = batch.create_element(ElementName::new("box"));
                    batch.set_classes(list, &[ClassName::new("list")]);
                    batch.insert_before(root, list, None);
                    let mut elements = vec![list];
                    for _ in 0..rows {
                        let row = batch.create_element(ElementName::new("box"));
                        batch.set_classes(row, &[ClassName::new("row")]);
                        batch.insert_before(list, row, None);
                        let label = batch.create_element(ElementName::new("box"));
                        batch.set_classes(label, &[ClassName::new("label")]);
                        batch.insert_before(row, label, None);
                        elements.extend([row, label]);
                    }
                    elements
                })
                .expect("an unpoisoned document");
            let mut side = Self {
                document,
                engine,
                root,
                elements,
                drawn: HashMap::new(),
            };
            side.engine.restyle(&mut side.document, None);
            side.draw();
            side
        }

        /// Applies the last restyle's text paint updates to the drawn colours, as a scene does.
        fn draw(&mut self) {
            for update in self.engine.text_paint_updates() {
                let element = std::iter::once(&self.root)
                    .chain(&self.elements)
                    .position(|&element| element == update.index)
                    .expect("an element of the document");
                self.drawn
                    .insert((element, update.run), format!("{:?}", update.paint.color));
            }
        }

        /// Sets the list's classes.
        fn list(&mut self, classes: &[&str]) {
            self.classes(self.elements[0], classes);
        }

        /// Sets the root's classes.
        fn root(&mut self, classes: &[&str]) {
            self.classes(self.root, classes);
        }

        /// Sets the classes of `element`.
        fn classes(&mut self, element: NodeIndex, classes: &[&str]) {
            let classes: Vec<ClassName> = classes.iter().copied().map(ClassName::new).collect();
            self.document
                .edit(&EverythingMatters, |batch| {
                    batch.set_classes(element, &classes)
                })
                .expect("an unpoisoned document");
        }

        /// One frame as a window runs it: the tick, the marks, the restyle. Returns the edges the
        /// tick crossed and what the restyle styled, as text to compare.
        fn frame(&mut self, now: f64) -> (Vec<String>, Vec<String>) {
            let report = self
                .engine
                .animation_tick(&self.document, AnimationTime(now));
            for element in &report.elements {
                if element.properties.is_paint_only() || !(element.advancing || element.crossed) {
                    continue;
                }
                self.engine
                    .mark_animation_restyle(&self.document, element.index);
            }
            let mut edges: Vec<String> = report
                .edges
                .iter()
                .map(|edge| format!("{edge:?}"))
                .collect();
            edges.sort();
            let pass = self.engine.restyle(&mut self.document, None);
            self.draw();
            let mut records: Vec<String> = pass
                .records
                .iter()
                .map(|record| {
                    format!(
                        "{:?} {:?} {} {}",
                        record.index, record.damage, record.readers, record.initial
                    )
                })
                .collect();
            records.sort();
            (edges, records)
        }

        /// The computed style of `element`.
        fn style(&self, element: NodeIndex) -> servo_arc::Arc<ComputedValues> {
            self.document
                .node(element)
                .primary_style()
                .expect("the element is styled")
        }
    }

    /// Whether two computed styles hold the same values.
    fn same(one: &ComputedValues, two: &ComputedValues) -> bool {
        macro_rules! groups {
            ($($get:ident),*) => { true $(&& one.$get() == two.$get())* };
        }
        groups!(
            get_background,
            get_border,
            get_box,
            get_column,
            get_counters,
            get_effects,
            get_font,
            get_inherited_box,
            get_inherited_table,
            get_inherited_text,
            get_inherited_ui,
            get_list,
            get_margin,
            get_outline,
            get_padding,
            get_position,
            get_svg,
            get_table,
            get_text,
            get_ui
        ) && one.custom_properties() == two.custom_properties()
            && one.flags == two.flags
            && one.writing_mode == two.writing_mode
            && one.effective_zoom == two.effective_zoom
    }

    /// Runs `frames` frames on a document styled by `sheet` with the direct path and without it,
    /// calling `between` with the frame number and each side before each frame, and asserts that
    /// every element holds the same style, every tick crosses the same edges, every restyle
    /// reports the same records and every text run is drawn in the same colour.
    ///
    /// Returns on how many frames a row kept its `ui` group allocation, which only the direct path
    /// does: a row declares its transitions, so a cascade builds that group again.
    fn compare(sheet: &str, frames: u32, mut between: impl FnMut(u32, &mut Side)) -> usize {
        let mut direct = Side::new(sheet, 4, true);
        let mut cascaded = Side::new(sheet, 4, false);
        let mut kept = 0;
        for frame in 0..frames {
            between(frame, &mut direct);
            between(frame, &mut cascaded);
            let row = direct.elements[1];
            let before = direct.style(row);
            let now = f64::from(frame + 1) * FRAME;
            let (edges, records) = direct.frame(now);
            let (cascaded_edges, cascaded_records) = cascaded.frame(now);
            assert_eq!(edges, cascaded_edges, "frame {frame}: the edges differ");
            assert_eq!(
                records, cascaded_records,
                "frame {frame}: the records differ"
            );
            // The direct path keeps a text group it did not move, so it reports no update for a
            // colour that stayed. The colours a scene draws must still be the same.
            assert_eq!(
                direct.drawn, cascaded.drawn,
                "frame {frame}: the drawn text colours differ"
            );
            for (&one, &two) in direct.elements.iter().zip(&cascaded.elements) {
                assert!(
                    same(&direct.style(one), &cascaded.style(two)),
                    "frame {frame}: element {one:?} differs:\n{:?}\n{:?}",
                    direct.style(one),
                    cascaded.style(two)
                );
                for index in 0..EAGER_PSEUDO_COUNT {
                    let pseudo = PseudoElement::from_eager_index(index);
                    let one = direct.document.node(one).pseudo_style(&pseudo);
                    let two = cascaded.document.node(two).pseudo_style(&pseudo);
                    assert_eq!(one.is_some(), two.is_some(), "frame {frame}: {pseudo:?}");
                    if let (Some(one), Some(two)) = (one, two) {
                        assert!(same(&one, &two), "frame {frame}: {pseudo:?} differs");
                    }
                }
            }
            let after = direct.style(row);
            if !servo_arc::Arc::ptr_eq(&before, &after)
                && std::ptr::eq(before.get_ui(), after.get_ui())
            {
                kept += 1;
            }
        }
        kept
    }

    /// Tokens a component library reads, so a cascade substitutes `var()` on every row.
    const TOKENS: &str = "
        root { --fg: rgb(10, 20, 30); --hot: rgb(200, 100, 50); --pad: 4px; --wide: 60px;
               --line: rgb(1, 2, 3); display: block; color: var(--fg) }
        .list { display: block }
        .label { display: block; padding-top: var(--pad) }
        .row { display: block; width: var(--wide); padding-left: var(--pad);
               border: 1px solid var(--line); background-color: var(--line);
               box-shadow: 0 0 0 1px var(--line) }";

    #[test]
    fn a_transition_tick_writes_what_the_cascade_writes() {
        let sheet = format!(
            "{TOKENS}
            .row {{ transition: padding-left 200ms linear, width 200ms ease-in,
                     color 200ms linear, box-shadow 200ms linear,
                     background-color 200ms linear 50ms }}
            .on > .row {{ padding-left: 20px; width: 90px; color: var(--hot);
                          box-shadow: 0 0 4px 2px var(--hot); background-color: var(--hot) }}"
        );
        // On, back off half way through, and on again after the reverse settled.
        let kept = compare(&sheet, 50, |frame, side| match frame {
            0 | 30 => side.list(&["list", "on"]),
            8 => side.list(&["list"]),
            _ => {}
        });
        assert!(
            kept > 10,
            "the direct path ran on {kept} frames, so the comparison measures the cascade alone"
        );
    }

    #[test]
    fn generated_content_follows_the_colour_it_inherits() {
        let sheet = format!(
            "{TOKENS}
            .row::before {{ content: \"x\"; display: block; border-top: 1px solid currentColor }}
            .row::after {{ content: \"y\"; display: block; padding: inherit }}
            .row {{ transition: color 200ms linear, padding-left 200ms linear }}
            .on > .row {{ color: var(--hot); padding-left: 24px }}"
        );
        let kept = compare(&sheet, 20, |frame, side| {
            if frame == 0 {
                side.list(&["list", "on"]);
            }
        });
        assert!(kept > 10, "the direct path ran on {kept} frames");
    }

    #[test]
    fn a_keyframe_tick_writes_what_the_cascade_writes() {
        let sheet = format!(
            "{TOKENS}
            @keyframes grow {{ from {{ width: 40px; color: rgb(0, 0, 0) }}
                               50% {{ width: 80px; color: rgb(250, 0, 0) }}
                               to {{ width: 40px; color: rgb(0, 0, 0) }} }}
            @keyframes once {{ from {{ padding-left: 0px }} to {{ padding-left: 30px }} }}
            .on > .row {{ animation: grow 160ms linear infinite, once 120ms linear 32ms }}"
        );
        let kept = compare(&sheet, 40, |frame, side| {
            if frame == 0 {
                side.list(&["list", "on"]);
            }
        });
        assert!(kept > 10, "the direct path ran on {kept} frames");
    }

    #[test]
    fn an_important_declaration_still_beats_a_keyframe() {
        // Keyframe values are computed when the animation starts, so an `!important` declaration
        // that arrives later is the cascade's to apply.
        let sheet = format!(
            "{TOKENS}
            @keyframes grow {{ from {{ width: 40px }} to {{ width: 80px }} }}
            .on > .row {{ animation: grow 200ms linear infinite }}
            .pinned > .row {{ width: 50px !important }}"
        );
        compare(&sheet, 20, |frame, side| match frame {
            0 => side.list(&["list", "on"]),
            4 => side.list(&["list", "on", "pinned"]),
            _ => {}
        });
    }

    #[test]
    fn a_property_outside_the_set_takes_the_cascade() {
        // `font-size` moves every `em` of the element, so the direct path refuses the row.
        let sheet = format!(
            "{TOKENS}
            .row {{ font-size: 10px; margin-left: 2em; transition: font-size 200ms linear,
                     padding-left 200ms linear }}
            .on > .row {{ font-size: 20px; padding-left: 20px }}"
        );
        let kept = compare(&sheet, 20, |frame, side| {
            if frame == 0 {
                side.list(&["list", "on"]);
            }
        });
        assert_eq!(
            kept, 0,
            "a row moving its font size was written without a cascade"
        );
    }

    #[test]
    fn every_property_group_the_path_writes_matches_the_cascade() {
        // One sample of each group the direct path may write, which the other pins leave out.
        let sheet = format!(
            "{TOKENS}
            .row {{ position: relative; display: flex; outline: 1px solid var(--line);
                     transition: transform 200ms linear, translate 200ms linear,
                     rotate 200ms linear, scale 200ms linear, transform-origin 200ms linear,
                     filter 200ms linear, border-top-left-radius 200ms linear,
                     letter-spacing 200ms linear, word-spacing 200ms linear,
                     text-shadow 200ms linear, left 200ms linear, row-gap 200ms linear,
                     flex-grow 200ms linear,
                     margin-left 200ms linear, min-height 200ms linear,
                     border-left-color 200ms linear, outline-color 200ms linear }}
            .on > .row {{ transform: rotate(20deg) translateX(4px); translate: 3px 5px;
                          rotate: 30deg; scale: 1.5; transform-origin: 10% 20%;
                          filter: blur(2px) brightness(1.4);
                          border-top-left-radius: 8px; letter-spacing: 2px; word-spacing: 3px;
                          text-shadow: 1px 1px 2px var(--hot); left: 12px; row-gap: 6px;
                          flex-grow: 2; margin-left: 7px;
                          min-height: 30px; border-left-color: var(--hot);
                          outline-color: var(--hot) }}"
        );
        let kept = compare(&sheet, 20, |frame, side| {
            if frame == 0 {
                side.list(&["list", "on"]);
            }
        });
        assert!(kept > 10, "the direct path ran on {kept} frames");
    }

    #[test]
    fn keyframes_and_transitions_on_one_element_match_the_cascade() {
        // Keyframes on one property and transitions on others are written in cascade order.
        let sheet = format!(
            "{TOKENS}
            @keyframes grow {{ from {{ width: 40px }} to {{ width: 80px }} }}
            .row {{ transition: padding-left 200ms linear, color 200ms linear }}
            .on > .row {{ animation: grow 160ms linear infinite; padding-left: 20px;
                          color: var(--hot) }}"
        );
        let kept = compare(&sheet, 20, |frame, side| {
            if frame == 0 {
                side.list(&["list", "on"]);
            }
        });
        assert!(kept > 10, "the direct path ran on {kept} frames");
    }

    #[test]
    fn a_keyframe_under_a_transition_of_its_property_matches_the_cascade() {
        // A keyframe animation starts under a running transition of its property, which the
        // cascade replaces.
        let sheet = format!(
            "{TOKENS}
            @keyframes slide {{ from {{ padding-left: 0px }} to {{ padding-left: 30px }} }}
            .row {{ transition: padding-left 300ms linear }}
            .on > .row {{ padding-left: 20px }}
            .spin > .row {{ animation: slide 160ms linear infinite }}"
        );
        compare(&sheet, 30, |frame, side| match frame {
            0 => side.list(&["list", "on"]),
            5 => side.list(&["list", "on", "spin"]),
            _ => {}
        });
    }

    #[test]
    fn generated_content_with_its_own_transition_takes_the_cascade() {
        // The cascade starts a transition on `::after` from the colour it inherits.
        let sheet = format!(
            "{TOKENS}
            .row::after {{ content: \"y\"; display: block; transition: color 100ms linear }}
            .row {{ transition: color 200ms linear }}
            .on > .row {{ color: var(--hot) }}"
        );
        compare(&sheet, 20, |frame, side| {
            if frame == 0 {
                side.list(&["list", "on"]);
            }
        });
    }

    #[test]
    fn a_token_change_during_a_transition_reads_the_tick_it_happened_on() {
        // A token moves under a running transition. The rows cascade again without matching, from
        // the rule node, so they read the animation declarations the last tick left on it.
        let sheet = format!(
            "{TOKENS}
            root.wide {{ --wide: 120px }}
            .row {{ transition: padding-left 300ms linear }}
            .on > .row {{ padding-left: 20px }}"
        );
        compare(&sheet, 30, |frame, side| match frame {
            0 => side.list(&["list", "on"]),
            6 => side.root(&["wide"]),
            _ => {}
        });
    }
}
