//! Do the hooks a downstream consumer implements actually reach the engine?
//!
//! A seam nothing calls is a promise rather than a mechanism, and the failure mode of getting one
//! wrong is silence: a consumer implements the trait, installs it, and nothing happens. So each case
//! here installs a hook and reads the consequence off the far side — a computed value, a matched
//! state pseudo-class, an intrinsic size.

use std::sync::Arc;

use selectors::matching::VisitedHandlingMode;
use selectors::sink::Push;
use servo_arc::Arc as ServoArc;
use style::applicable_declarations::ApplicableDeclarationBlock;
use style::properties::PropertyDeclarationBlock;
use style::properties::declaration_block::parse_style_attribute;
use style::rule_tree::{CascadeLevel, CascadeOrigin};
use style::shared_lock::{Locked, SharedRwLock};
use style::stylesheets::CssRuleType;
use style::stylesheets::layer_rule::LayerOrder;
use stylo_dom::ElementState;
use zgui_dom::{
    Document, Intrinsic, LinkResolver, Node, NodeFlags, NodeKind, PresentationalHints,
    ReplacedContent, ReplacedId, SheetRequest,
};
use zgui_geom::{CssPx, Size};
use zgui_interned::{AttrName, ElementName};
use zgui_vocab::SharedString;

use crate::support::engine::Engine;
use crate::support::fixture;
use crate::support::read::{color, radius};
use crate::support::sheets::base_url;

/// A hint source that gives every element carrying a `width` attribute a corner radius.
///
/// A radius rather than a width only because a reset property with a numeric computed value is the
/// easiest thing to read back; the point is that the declaration reaches the cascade at all.
struct WidthAttributeHints {
    /// The block every matching element is given.
    block: ServoArc<Locked<PropertyDeclarationBlock>>,
}

impl WidthAttributeHints {
    /// A source whose declarations are locked with `lock`, which must be the document's.
    fn new(lock: &SharedRwLock) -> Self {
        let block = parse_style_attribute(
            "border-top-left-radius: 11px",
            &base_url(),
            None,
            selectors::matching::QuirksMode::NoQuirks,
            CssRuleType::Style,
        );
        Self {
            block: ServoArc::new(lock.wrap(block)),
        }
    }
}

impl PresentationalHints for WidthAttributeHints {
    fn hints_for(
        &self,
        element: Node<'_>,
        _visited: VisitedHandlingMode,
        out: &mut dyn Push<ApplicableDeclarationBlock>,
    ) {
        if element.attr("width").is_some() {
            out.push(ApplicableDeclarationBlock::from_declarations(
                self.block.clone(),
                CascadeLevel::new(CascadeOrigin::PresHints),
                LayerOrder::root(),
            ));
        }
    }
}

/// A replaced-content source that reports one fixed size for everything.
struct FixedIntrinsic;

impl ReplacedContent for FixedIntrinsic {
    fn intrinsic(&self, _id: ReplacedId) -> Intrinsic {
        Intrinsic {
            size: Some(Size::new(CssPx(320.0), CssPx(180.0))),
            ratio: Some(16.0 / 9.0),
            baseline: None,
        }
    }
}

/// A link resolver for which nothing is a link, installed to prove the default is not accidental.
struct NothingIsALink;

impl LinkResolver for NothingIsALink {
    fn is_link(&self, _element: Node<'_>) -> bool {
        false
    }
}

#[test]
fn a_presentational_hint_reaches_the_cascade_and_loses_to_an_author_rule() {
    let mut tree = fixture::page();
    let target = tree.at("badge");
    tree.document.set_attribute(
        target,
        AttrName::new("width"),
        Some(SharedString::from("40")),
    );
    let hints = WidthAttributeHints::new(tree.document.store().lock());
    tree.document.install_presentational_hints(Arc::new(hints));

    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(".title { border-top-left-radius: 2px }");
    engine.restyle(&mut tree.document, None);

    assert_eq!(
        radius(&tree.document, target),
        11.0,
        "the hook's declaration has to reach the element it named"
    );
    assert_eq!(
        radius(&tree.document, tree.at("i1")),
        0.0,
        "and only that one"
    );
    assert_eq!(
        radius(&tree.document, tree.at("title")),
        2.0,
        "an author rule of any specificity outranks a presentational hint"
    );
}

#[test]
fn the_default_hint_source_contributes_nothing() {
    let mut tree = fixture::page();
    let target = tree.at("badge");
    tree.document.set_attribute(
        target,
        AttrName::new("width"),
        Some(SharedString::from("40")),
    );

    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet("* { color: rgb(1, 1, 1) }");
    engine.restyle(&mut tree.document, None);
    assert_eq!(radius(&tree.document, target), 0.0);
}

#[test]
fn the_link_resolvers_answer_reaches_the_link_pseudo_classes() {
    let mut tree = fixture::page();
    let mut engine = Engine::new(&tree.document);
    engine.add_author_sheet(
        "a          { color: rgb(1, 1, 1) }
         :any-link  { border-top-left-radius: 1px }
         :link      { color: rgb(2, 0, 0) }
         :visited   { color: rgb(3, 0, 0) }",
    );
    engine.restyle(&mut tree.document, None);

    assert_eq!(radius(&tree.document, tree.at("linkA")), 1.0);
    assert_eq!(radius(&tree.document, tree.at("linkB")), 1.0);
    assert_eq!(
        radius(&tree.document, tree.at("title")),
        0.0,
        "an element with no `href` is not a link"
    );
    assert_eq!(color(&tree.document, tree.at("linkA")), (2, 0, 0));
    assert!(
        tree.document
            .node(tree.at("linkB"))
            .element_state()
            .contains(ElementState::VISITED),
        "the resolver said this one had been visited, and that has to be in the state word where \
         the engine's own invalidation can see it"
    );
}

#[test]
fn installing_a_resolver_that_says_no_takes_the_link_state_away_again() {
    let mut tree = fixture::page();
    assert!(
        tree.document
            .node(tree.at("linkA"))
            .element_state()
            .intersects(ElementState::VISITED_OR_UNVISITED)
    );
    tree.document
        .install_link_resolver(Arc::new(NothingIsALink));
    assert!(
        !tree
            .document
            .node(tree.at("linkA"))
            .element_state()
            .intersects(ElementState::VISITED_OR_UNVISITED),
        "installing a resolver has to reach the elements that already exist"
    );
}

#[test]
fn intrinsic_sizing_is_asked_only_of_nodes_flagged_replaced() {
    let mut document = Document::new();
    let root = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("root"),
    );
    let image = document.append(root, NodeKind::Element, ElementName::new("image"));
    document.install_replaced_content(Arc::new(FixedIntrinsic));

    assert_eq!(
        document.intrinsic_of(image),
        None,
        "a node nothing flagged replaced has no outside content to ask about"
    );
    document.set_flags(image, NodeFlags::IN_DOCUMENT | NodeFlags::IS_REPLACED);
    let intrinsic = document.intrinsic_of(image).expect("the node is replaced");
    assert_eq!(intrinsic.size, Some(Size::new(CssPx(320.0), CssPx(180.0))));
    assert_eq!(document.intrinsic_of(root), None);
}

#[test]
fn without_a_loader_every_stylesheet_request_is_refused() {
    let document = Document::new();
    assert!(matches!(
        document
            .store()
            .host()
            .sheets()
            .load("zgui:///tests", "theme.css"),
        SheetRequest::Rejected
    ));
}
