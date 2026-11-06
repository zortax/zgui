//! A frame of colours served without composing, and everything that must not be.
//!
//! Each case changes one element's style through the real restyle and patch. A case that is served
//! must damage what a composition would. A case that is refused must leave the fragment as a
//! composition makes it, so a shadow run of this file proves the walk against the full pass too.

mod support;

use std::sync::{Mutex, MutexGuard};

use support::{Element, Fixture, Frame, fragments, lay_out, measurer};
use zgui_bits::{DamageSet, Dirty};
use zgui_layout::LayoutStore;
use zgui_layout::fragment::FragmentFlags;
use zgui_layout::fragment::diff::FrameDirty;
use zgui_profile::{Counter, counter};

/// Held for the whole of a case, because the counters are process-global.
static COUNTERS: Mutex<()> = Mutex::new(());

/// Takes the counter lock.
fn exclusive() -> MutexGuard<'static, ()> {
    COUNTERS.lock().unwrap_or_else(|held| held.into_inner())
}

/// Repaint marks on some elements, as a document's marks read after a style change.
struct Repaints {
    /// The elements that owe a repaint.
    nodes: Vec<Option<zgui_dom::NodeKey>>,
    /// What they owe beside it.
    also: Dirty,
}

impl FrameDirty for Repaints {
    fn own(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        if self.nodes.contains(&node) {
            Dirty::REPAINT | self.also
        } else {
            Dirty::empty()
        }
    }

    fn subtree(&self, _node: Option<zgui_dom::NodeKey>) -> Dirty {
        Dirty::REPAINT
    }

    fn mark(&mut self, _node: Option<zgui_dom::NodeKey>, _bits: Dirty) {}

    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: Dirty) {}
}

/// What one frame did.
struct Outcome {
    /// Whether the walk served it.
    served: bool,
    /// The store after it.
    store: LayoutStore,
    /// The frame, with the damage.
    frame: Frame,
}

impl Outcome {
    /// The flags of the first piece of the document's second box.
    fn flags(&self) -> FragmentFlags {
        let root = self.store.root().expect("a root");
        let frag = self
            .store
            .fragments_of_box(self.store.node(root).children[0])[0];
        self.store.fragment(frag).expect("live").flags
    }
}

/// Lays `tree` out under `css`, gives the element with class `target` the class `to`, and runs one
/// fragment pass over what that owes. `marked_too` owes a repaint without a new style.
fn change(tree: Element, css: &str, marked_too: Option<&str>) -> Outcome {
    change_owing(tree, css, marked_too, Dirty::empty())
}

/// [`change`], with the restyled elements owing `also` beside the repaint.
fn change_owing(tree: Element, css: &str, marked_too: Option<&str>, also: Dirty) -> Outcome {
    let mut fixture = Fixture::new(tree, css);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 200.0);

    let target = support::find::with_class(&fixture, "target");
    let other = marked_too.map(|class| support::find::with_class(&fixture, class));
    fixture.edit_restyle_and_patch(&mut store, |edit| {
        edit.set_classes(target, &["target".into(), "to".into()]);
    });
    let keys = [Some(target), other]
        .into_iter()
        .flatten()
        .map(|index| Some(fixture.document.store().key_of(index)))
        .collect();

    frame.damage = DamageSet::new();
    let root = store.root().expect("a root");
    counter::reset();
    let mut dirty = Repaints { nodes: keys, also };
    fragments(&mut frame, &mut store, root, &mut dirty);
    Outcome {
        served: counter::get(Counter::RepaintWalks) == 1,
        store,
        frame,
    }
}

/// One block child that holds the class the cases restyle.
fn block() -> Element {
    Element::new("root").children(vec![Element::new("a").classes(&["target"])])
}

/// The block's sheet, with the rules of one case after it.
fn sheet(rules: &str) -> String {
    format!(
        "root {{ display: block; width: 200px }}
         a {{ display: block; height: 30px }}
         {rules}"
    )
}

#[test]
fn a_new_background_colour_is_served_and_damages_the_box() {
    let _guard = exclusive();
    let out = change(
        block(),
        &sheet(".target { background-color: #246 } .to { background-color: #642 }"),
        None,
    );
    assert!(out.served);
    let bounds = out.frame.damage.bounds().expect("the colour is owed");
    assert_eq!((bounds.size.width, bounds.size.height), (200, 30));
}

#[test]
fn a_new_text_colour_is_served() {
    let _guard = exclusive();
    let tree =
        Element::new("root").children(vec![Element::new("a").classes(&["target"]).text("x")]);
    let out = change(
        tree,
        &sheet(".target { color: #246 } .to { color: #642 }"),
        None,
    );
    assert!(out.served);
}

#[test]
fn a_gradient_with_a_new_colour_is_served() {
    let _guard = exclusive();
    let out = change(
        block(),
        &sheet(
            ".target { background-image: linear-gradient(#246, #246) }
             .to { background-image: linear-gradient(#642, #642) }",
        ),
        None,
    );
    assert!(out.served);
}

#[test]
fn a_background_that_starts_painting_is_composed() {
    let _guard = exclusive();
    let out = change(
        block(),
        &sheet(".target { background-color: transparent } .to { background-color: #246 }"),
        None,
    );
    assert!(!out.served);
    assert!(!out.flags().contains(FragmentFlags::PAINTS_NOTHING));
    assert!(out.frame.damage.bounds().is_some());
}

#[test]
fn a_background_image_that_starts_painting_is_composed() {
    let _guard = exclusive();
    let out = change(
        block(),
        &sheet(
            ".target { background-image: none }
             .to { background-image: linear-gradient(#246, #246) }",
        ),
        None,
    );
    assert!(!out.served);
    assert!(!out.flags().contains(FragmentFlags::PAINTS_NOTHING));
}

#[test]
fn a_shadow_is_composed_and_takes_its_reach() {
    let _guard = exclusive();
    let out = change(
        block(),
        &sheet(".target { background-color: #246 } .to { box-shadow: 0 0 20px #000 }"),
        None,
    );
    assert!(!out.served);
    let bounds = out.frame.damage.bounds().expect("the shadow is owed");
    assert!(bounds.size.width > 200, "the ink grew with the shadow");
}

#[test]
fn an_opacity_is_composed_and_groups_the_subtree() {
    let _guard = exclusive();
    let out = change(
        block(),
        &sheet(".target { background-color: #246 } .to { opacity: 0.5 }"),
        None,
    );
    assert!(!out.served);
    assert!(out.flags().contains(FragmentFlags::GROUPS_SUBTREE));
}

#[test]
fn a_new_radius_is_composed() {
    let _guard = exclusive();
    let out = change(
        block(),
        &sheet(".target { background-color: #246 } .to { border-radius: 9px }"),
        None,
    );
    assert!(!out.served);
}

#[test]
fn a_framework_custom_property_is_composed() {
    let _guard = exclusive();
    let name = zgui_css::values::custom::WILDCARD_DECLARERS[0];
    let out = change(
        block(),
        &sheet(&format!(
            ".target {{ --{name}: a; background-color: #246 }} .to {{ background-color: #642 }}"
        )),
        None,
    );
    assert!(!out.served);
}

#[test]
fn a_nested_inline_element_is_composed() {
    let _guard = exclusive();
    let tree = Element::new("root").children(vec![
        Element::new("a").children(vec![Element::new("span").classes(&["target"]).text("x")]),
    ]);
    let out = change(
        tree,
        &sheet(
            "span { display: inline }
             .target { background-color: #246 } .to { background-color: #642 }",
        ),
        None,
    );
    assert!(!out.served);
}

/// An animation or a text edit marks a repaint with no new style, and the proof of one element
/// says nothing about another.
#[test]
fn a_repaint_with_no_new_style_is_composed() {
    let _guard = exclusive();
    let tree = Element::new("root").children(vec![
        Element::new("a").classes(&["target"]),
        Element::new("a").classes(&["other"]),
    ]);
    let out = change(
        tree,
        &sheet(".target { background-color: #246 } .to { background-color: #642 }"),
        Some("other"),
    );
    assert!(!out.served);
}

/// The proof covers a colour. Any other mark on the same element is work the walk does not do.
#[test]
fn a_mark_beside_the_repaint_is_composed() {
    let _guard = exclusive();
    let out = change_owing(
        block(),
        &sheet(".target { background-color: #246 } .to { background-color: #642 }"),
        None,
        Dirty::REHIT,
    );
    assert!(!out.served);
}

/// A transform in flight owes a bit no pass enters for, and its fragments change all the same.
#[test]
fn an_animation_below_the_repaint_is_composed() {
    let _guard = exclusive();
    let out = change_owing(
        block(),
        &sheet(".target { background-color: #246 } .to { background-color: #642 }"),
        None,
        Dirty::ANIMATING,
    );
    assert!(!out.served);
}
