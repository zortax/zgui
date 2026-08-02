//! Does a cached state mask that outlives a class change lose a restyle?
//!
//! The narrowed answer to "which interaction-state bits can matter for this element" is what makes a
//! hover write cheap: a bit outside the mask changes nothing that matches, so the write skips the
//! snapshot, the ancestor marking and the traversal entirely. The answer is bucketed by the
//! element's own identity, so adding a class can *widen* it — and a cache that survives the class
//! change reports that hover cannot matter, the write is skipped, and the element keeps the colour
//! it had.
//!
//! The failure is a wrong colour on the screen, not a slow frame and not a counter that moves, which
//! is why this case asserts the computed value rather than a count. Both documents below are
//! identical and both take the cheap path when the mask says they may; the only difference is
//! whether the mask was read before or after the class change.

use stylo_dom::ElementState;
use zgui_dom::Document;
use zgui_interned::ClassName;

use crate::support::edit;
use crate::support::engine::Engine;
use crate::support::filter::SheetFilter;
use crate::support::fixture;
use crate::support::read::color;

/// The sheet both documents are styled with. Nothing styles `.badge`, and `.chosen` is hoverable.
const SHEET: &str = ".chosen { color: rgb(1, 1, 1) } .chosen:hover { color: rgb(9, 0, 0) }";

/// Styles `document` once and hands back the engine that did it.
fn prepared(document: &mut Document) -> Engine {
    let mut engine = Engine::new(document);
    engine.add_author_sheet(SHEET);
    engine.restyle(document, None);
    edit::retire(document);
    engine
}

#[test]
fn a_state_mask_cached_across_a_class_change_loses_a_restyle() {
    let filter = SheetFilter::new(&[SHEET]);

    // The stale document. The mask is taken in one frame, the class is added in the next, and the
    // hover write in the frame after that consults the mask that was never dropped.
    let mut stale = fixture::page();
    let mut engine = prepared(&mut stale.document);
    let target = stale.at("badge");
    let taken_before = stale.document.store_mut().states_for(target, &filter);
    assert_eq!(
        taken_before,
        ElementState::empty(),
        "nothing in the sheet can match this element as it is now"
    );

    edit::set_classes(&stale.document, target, &[ClassName::new("chosen")]);
    engine.restyle(&mut stale.document, None);
    edit::retire(&mut stale.document);
    assert_eq!(color(&stale.document, target), (1, 1, 1));

    let full_path =
        edit::set_state_filtered(&stale.document, target, ElementState::HOVER, taken_before);
    engine.restyle(&mut stale.document, None);
    assert!(
        !full_path,
        "the stale mask says a hover on this element cannot matter"
    );
    assert_eq!(
        color(&stale.document, target),
        (1, 1, 1),
        "so the element ends the frame showing the wrong colour, with nothing to notice it by"
    );

    // The correct document runs the identical three frames. The only difference is that the class
    // write dropped the cached answer, so the third frame asks again.
    let mut fresh = fixture::page();
    let mut engine = prepared(&mut fresh.document);
    let target = fresh.at("badge");
    fresh.document.store_mut().states_for(target, &filter);

    edit::set_classes(&fresh.document, target, &[ClassName::new("chosen")]);
    engine.restyle(&mut fresh.document, None);
    edit::retire(&mut fresh.document);

    let taken_after = fresh.document.store_mut().states_for(target, &filter);
    assert_eq!(
        taken_after,
        ElementState::HOVER,
        "the class change widened the mask, which is the fact the cache has to be dropped for"
    );
    let full_path =
        edit::set_state_filtered(&fresh.document, target, ElementState::HOVER, taken_after);
    engine.restyle(&mut fresh.document, None);
    assert!(full_path);
    assert_eq!(color(&fresh.document, target), (9, 0, 0));
}

#[test]
fn the_class_write_is_what_drops_the_cache_and_it_drops_it_there_and_then() {
    let filter = SheetFilter::new(&[SHEET]);
    let mut tree = fixture::page();
    let target = tree.at("badge");

    assert_eq!(
        tree.document.store_mut().states_for(target, &filter),
        ElementState::empty()
    );
    tree.document
        .set_classes(target, &[ClassName::new("chosen")]);
    assert_eq!(
        tree.document.store_mut().states_for(target, &filter),
        ElementState::HOVER,
        "the answer is recomputed because the write that changed the identity dropped it"
    );
}

#[test]
fn an_identifier_write_drops_the_cache_too() {
    const BY_ID: &str = "#picked:hover { color: rgb(9, 0, 0) }";
    let filter = SheetFilter::new(&[BY_ID]);
    let mut tree = fixture::page();
    let target = tree.at("badge");

    assert_eq!(
        tree.document.store_mut().states_for(target, &filter),
        ElementState::empty()
    );
    tree.document
        .set_id(target, Some(zgui_interned::Ident::new("picked")));
    assert_eq!(
        tree.document.store_mut().states_for(target, &filter),
        ElementState::HOVER
    );
}

#[test]
fn a_stylesheet_set_change_drops_every_cached_answer_at_once() {
    let filter = SheetFilter::new(&[SHEET]);
    let mut tree = fixture::page();
    let first = tree.at("badge");
    let second = tree.at("title");
    tree.document.store_mut().states_for(first, &filter);
    tree.document.store_mut().states_for(second, &filter);

    tree.document.store_mut().invalidate_all_state_masks();

    // Recomputing against a filter that answers differently proves the cache is gone rather than
    // merely agreeing with itself.
    let wider = SheetFilter::new(&[".badge:hover { color: rgb(9, 0, 0) }"]);
    assert_eq!(
        tree.document.store_mut().states_for(first, &wider),
        ElementState::HOVER
    );
}

/// The third bucket the answer is narrowed by, and the one that had no writer until a node could be
/// moved: an element becomes the document's root by being linked under the document node, which
/// changes what `:root` matches without touching the element at all.
///
/// The failure this guards against is the same shape as the class one — a mask that outlives the
/// move says a hover on this element cannot matter, the write is skipped, and the element ends the
/// frame showing the wrong colour — so this case reads the colour rather than the cache.
#[test]
fn a_move_that_makes_an_element_the_root_drops_its_cached_state_mask() {
    const ROOTED: &str = ".badge { color: rgb(1, 1, 1) } :root:hover { color: rgb(9, 0, 0) }";

    /// `document > wrapper > badge`, styled once, with the engine that did it.
    fn prepared_pair() -> (Document, Engine, zgui_dom::NodeIndex, zgui_dom::NodeIndex) {
        use zgui_dom::NodeKind;
        use zgui_interned::ElementName;

        let mut document = Document::new();
        let wrapper = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("wrapper"),
        );
        let badge = document.append(wrapper, NodeKind::Element, ElementName::new("span"));
        document.set_classes(badge, &[ClassName::new("badge")]);

        let mut engine = Engine::new(&document);
        engine.add_author_sheet(ROOTED);
        engine.restyle(&mut document, None);
        edit::retire(&mut document);
        (document, engine, wrapper, badge)
    }

    /// Promotes `badge` to be the document's root, in one batch.
    fn promote(document: &Document, wrapper: zgui_dom::NodeIndex, badge: zgui_dom::NodeIndex) {
        let document_index = document.document_index();
        document
            .edit(&zgui_dom::EverythingMatters, |batch| {
                batch.insert_before(document_index, badge, Some(wrapper));
                batch.remove(wrapper);
            })
            .expect("the document is not poisoned");
    }

    let filter = SheetFilter::new(&[ROOTED]);
    let (mut document, mut engine, wrapper, badge) = prepared_pair();
    let taken_before = document.store_mut().states_for(badge, &filter);
    assert_eq!(
        taken_before,
        ElementState::empty(),
        "nothing can match this element on a state bit while it is not the root"
    );
    assert_eq!(color(&document, badge), (1, 1, 1));

    promote(&document, wrapper, badge);
    assert_eq!(document.root_index(), Some(badge));
    engine.restyle(&mut document, None);
    edit::retire(&mut document);
    assert_eq!(
        document.store_mut().states_for(badge, &filter),
        ElementState::HOVER,
        "the move widened the mask, which is the fact the cache has to be dropped for"
    );

    edit::set_state(&document, badge, ElementState::HOVER, true);
    engine.restyle(&mut document, None);
    assert_eq!(color(&document, badge), (9, 0, 0));

    // The negative control runs the identical frames against the mask taken before the move, which
    // is exactly what a cache that survived it would have supplied.
    let (mut stale, mut engine, wrapper, badge) = prepared_pair();
    let taken_before = stale.store_mut().states_for(badge, &filter);
    promote(&stale, wrapper, badge);
    engine.restyle(&mut stale, None);
    edit::retire(&mut stale);

    let full_path = edit::set_state_filtered(&stale, badge, ElementState::HOVER, taken_before);
    engine.restyle(&mut stale, None);
    assert!(!full_path);
    assert_eq!(
        color(&stale, badge),
        (1, 1, 1),
        "the stale mask skipped the hover, and the element shows the wrong colour with nothing to \
         notice it by"
    );
}
