//! The cascade's view of an element.
//!
//! One trait, and it is the largest surface the style engine asks for: the tree it descends, the
//! declarations it cascades, the state it matches, the data it writes back and the bookkeeping it
//! keeps while several workers run at once. A trait implementation cannot be split across files, so
//! what is here is the surface itself — every method one line of delegation — and the answers live
//! beside their reasons in the modules below.
//!
//! | Module | Answers |
//! |---|---|
//! | [`tree`] | the chain the traversal descends, and the names an element is known by |
//! | [`data`] | establishing, clearing and borrowing the engine's per-element data |
//! | [`state`] | interaction state, language and custom states |
//! | [`pseudo`] | which pseudo-elements are worth resolving a style for |
//! | [`animation`] | why the engine owns none of this framework's animations |
//! | [`damage`] | the pipeline stages a layout-affecting change costs |
//!
//! # The methods that are not the engine's default
//!
//! Four, and each is a decision rather than a stub. Which pseudo-elements may be generated, because
//! the default resolves a first-letter style nothing reads. What a layout-affecting change costs,
//! because the default is "nothing" and every incremental relayout would be skipped. The subtree
//! hash summary, pinned to "unfiltered" so a future release cannot change it silently. And the
//! animation-only descent flag, which is the one descent flag this document stores rather than
//! derives.
//!
//! Two more are deliberately *left* at the default and it is worth saying why, because both existed
//! to serve a design where a pseudo-element was a node. There is no such node here, so asking an
//! element which pseudo-element it implements correctly answers "none", and iterating an element's
//! anonymous content correctly iterates nothing.

pub mod animation;
pub mod damage;
pub mod data;
pub mod pseudo;
pub mod state;
pub mod tree;

use selectors::matching::{ElementSelectorFlags, VisitedHandlingMode};
use selectors::sink::Push;
use servo_arc::{Arc as ServoArc, ArcBorrow};
use style::applicable_declarations::ApplicableDeclarationBlock;
use style::context::SharedStyleContext;
use style::data::{ElementDataMut, ElementDataRef};
use style::dom::{LayoutIterator, TElement};
use style::properties::{ComputedValues, PropertyDeclarationBlock};
use style::selector_parser::{AttrValue, Lang, PseudoElement, RestyleDamage};
use style::shared_lock::Locked;
use style::values::AtomIdent;
use stylo_atoms::Atom;
use stylo_dom::ElementState;

use crate::node::handle::Node;
use crate::stylo::bloom::SUBTREE_FILTER_UNFILTERED;
use crate::stylo::element::tree::TraversalChildren;

impl<'doc> TElement for Node<'doc> {
    type ConcreteNode = Node<'doc>;
    type TraversalChildrenIterator = TraversalChildren<'doc>;

    fn as_node(&self) -> Self::ConcreteNode {
        *self
    }

    fn traversal_children(&self) -> LayoutIterator<Self::TraversalChildrenIterator> {
        LayoutIterator(TraversalChildren::of(*self))
    }

    /// Not an HTML element, because this document has no document language.
    fn is_html_element(&self) -> bool {
        false
    }

    /// Not a MathML element, for the same reason.
    fn is_mathml_element(&self) -> bool {
        false
    }

    /// Not an SVG element, for the same reason.
    ///
    /// Vector content is painted from a node's own computed properties rather than from an SVG
    /// element tree, so nothing here needs the engine's SVG element handling.
    fn is_svg_element(&self) -> bool {
        false
    }

    fn style_attribute(&self) -> Option<ArcBorrow<'_, Locked<PropertyDeclarationBlock>>> {
        self.store()
            .columns()
            .inline_style
            .get(self.key())
            .and_then(Option::as_ref)
            .map(ServoArc::borrow_arc)
    }

    fn animation_rule(
        &self,
        context: &SharedStyleContext,
    ) -> Option<ServoArc<Locked<PropertyDeclarationBlock>>> {
        self.engine_animation_rule(context)
    }

    fn transition_rule(
        &self,
        context: &SharedStyleContext,
    ) -> Option<ServoArc<Locked<PropertyDeclarationBlock>>> {
        self.engine_transition_rule(context)
    }

    fn state(&self) -> ElementState {
        self.element_state()
    }

    /// No shadow parts, because there are no shadow trees.
    fn has_part_attr(&self) -> bool {
        false
    }

    /// No shadow parts, because there are no shadow trees.
    fn exports_any_part(&self) -> bool {
        false
    }

    fn id(&self) -> Option<&Atom> {
        self.record()
            .id_attr()
            .and_then(|ident| self.store().idents().resolve(ident))
    }

    fn each_class<F>(&self, mut callback: F)
    where
        F: FnMut(&AtomIdent),
    {
        for class in self.store().classes_of(self.index()) {
            callback(class);
        }
    }

    fn each_custom_state<F>(&self, callback: F)
    where
        F: FnMut(&AtomIdent),
    {
        Node::each_custom_state(*self, callback);
    }

    fn each_attr_name<F>(&self, mut callback: F)
    where
        F: FnMut(&style::LocalName),
    {
        for attr in self.attrs() {
            callback(&style::values::GenericAtomIdent(
                web_atoms::LocalName::from(attr.name.as_str()),
            ));
        }
    }

    fn has_dirty_descendants(&self) -> bool {
        self.has_style_work_below()
    }

    fn has_snapshot(&self) -> bool {
        Node::has_snapshot(*self)
    }

    fn handled_snapshot(&self) -> bool {
        Node::handled_snapshot(*self)
    }

    unsafe fn set_handled_snapshot(&self) {
        Node::set_handled_snapshot(*self);
    }

    unsafe fn set_dirty_descendants(&self) {
        self.note_style_work_below();
    }

    /// Deliberately does nothing.
    ///
    /// The answer to "is there work below me" is a view of this node's invalidation word, and that
    /// word is retired exactly once per frame, by the walk that also retires the obligations it
    /// summarises. Clearing it here would retire it on a second schedule, and a mark taken between
    /// the two would be dropped silently.
    unsafe fn unset_dirty_descendants(&self) {}

    fn has_animation_only_dirty_descendants(&self) -> bool {
        self.has_animation_work_below()
    }

    unsafe fn set_animation_only_dirty_descendants(&self) {
        self.note_animation_work_below();
    }

    unsafe fn unset_animation_only_dirty_descendants(&self) {
        self.clear_animation_work_below();
    }

    fn store_children_to_process(&self, n: isize) {
        self.record().store_children_to_process(n as i32);
    }

    fn did_process_child(&self) -> isize {
        self.record().did_process_child() as isize
    }

    unsafe fn ensure_data(&self) -> ElementDataMut<'_> {
        self.ensure_style_data()
    }

    unsafe fn clear_data(&self) {
        self.clear_style_data();
    }

    fn has_data(&self) -> bool {
        self.is_styled()
    }

    fn borrow_data(&self) -> Option<ElementDataRef<'_>> {
        self.borrow_style_data()
    }

    fn mutate_data(&self) -> Option<ElementDataMut<'_>> {
        self.mutate_style_data()
    }

    /// Every element takes the display fixup a flex or grid parent imposes on its children.
    ///
    /// Opting out exists for engine-internal anonymous content, and this document has none.
    fn skip_item_display_fixup(&self) -> bool {
        false
    }

    /// Every element may: an animation is recorded in a table keyed by slot number, not on the
    /// node, so there is no per-node fact to answer from. The lookups this admits are a hash probe
    /// each, and they are skipped outright while the table is empty, which is every frame of a
    /// document that is not animating.
    fn may_have_animations(&self) -> bool {
        true
    }

    fn has_animations(&self, context: &SharedStyleContext) -> bool {
        self.has_engine_animations(context)
    }

    fn has_css_animations(
        &self,
        context: &SharedStyleContext,
        pseudo: Option<PseudoElement>,
    ) -> bool {
        self.has_engine_css_animations(context, pseudo)
    }

    fn has_css_transitions(
        &self,
        context: &SharedStyleContext,
        pseudo: Option<PseudoElement>,
    ) -> bool {
        self.has_engine_css_transitions(context, pseudo)
    }

    /// No shadow root, because this document has no shadow trees.
    fn shadow_root(&self) -> Option<Node<'doc>> {
        None
    }

    /// No containing shadow root, for the same reason.
    fn containing_shadow(&self) -> Option<Node<'doc>> {
        None
    }

    fn lang_attr(&self) -> Option<AttrValue> {
        self.lang_attribute()
    }

    fn match_element_lang(&self, override_lang: Option<Option<AttrValue>>, value: &Lang) -> bool {
        self.matches_lang(override_lang, value)
    }

    /// Not a document body, because there is no document language to have one.
    fn is_html_document_body_element(&self) -> bool {
        false
    }

    /// Contributes whatever the installed hint source says this element's attributes mean.
    ///
    /// The document core contributes nothing itself: attributes with a presentational meaning are a
    /// document-language notion, and a core that knew about them would carry that language forever.
    /// The installed source is where a consumer puts them, and by default there is none.
    fn synthesize_presentational_hints_for_legacy_attributes<V>(
        &self,
        visited_handling: VisitedHandlingMode,
        hints: &mut V,
    ) where
        V: Push<ApplicableDeclarationBlock>,
    {
        self.store()
            .host()
            .hints()
            .hints_for(*self, visited_handling, hints);
    }

    fn local_name(&self) -> &web_atoms::LocalName {
        &self.tag_name().0
    }

    fn namespace(&self) -> &web_atoms::Namespace {
        self.namespace_uri()
    }

    /// No container size, because container queries resolve against boxes and boxes are built after
    /// styling.
    ///
    /// Answering with nothing makes every container query fall back to its "no container" branch,
    /// which is the same answer an element outside any container gets.
    fn query_container_size(
        &self,
        _display: &style::values::computed::Display,
    ) -> euclid::default::Size2D<Option<app_units::Au>> {
        euclid::default::Size2D::new(None, None)
    }

    fn has_selector_flags(&self, flags: ElementSelectorFlags) -> bool {
        self.record().selector_flags().contains(flags)
    }

    fn relative_selector_search_direction(&self) -> ElementSelectorFlags {
        self.record().selector_flags()
            & ElementSelectorFlags::RELATIVE_SELECTOR_SEARCH_DIRECTION_ANCESTOR_SIBLING
    }

    /// Pinned to "anything may be below here" rather than left at the engine's default.
    ///
    /// A real subtree summary would let a query skip a whole subtree without descending, and
    /// maintaining one costs an update on every ancestor of every insertion. Nothing here is bounded
    /// by the walk it would save, so the summary admits everything — stated explicitly, so that a
    /// release changing the default cannot change what this document does without changing this
    /// line.
    fn subtree_bloom_filter(&self) -> u64 {
        SUBTREE_FILTER_UNFILTERED
    }

    fn may_generate_pseudo(&self, pseudo: &PseudoElement, _primary_style: &ComputedValues) -> bool {
        self.may_generate(pseudo)
    }

    fn compute_layout_damage(old: &ComputedValues, new: &ComputedValues) -> RestyleDamage {
        Node::layout_damage(old, new)
    }
}
