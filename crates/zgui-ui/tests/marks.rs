//! Where the mark inside a control lands, read off the display list the device was handed.
//!
//! A tick is a drawing in a small square, and a drawing is the one kind of content a picture of the
//! window cannot be asked about on its own: an outline that never reached the frame and one whose
//! curves happen to land on pixels the colour of the box behind them are the same photograph. So
//! the question is asked of the item the frame carried — where its ink was claimed to land — and it
//! is asked as the question somebody looking at the control asks: is the mark in the middle of the
//! box, or off to one side of it?
//!
//! # Why a mark can be off-centre while every rule about it is right
//!
//! A checkbox shows a tick or a dash depending on its state, and both are built once and revealed
//! by state so that ticking one does not build a subtree. `opacity: 0` hides the one that is not
//! showing — and hides it from the eye only. It is still an item of the box's layout, so a box laid
//! out in a row puts *two* twelve-pixel marks side by side in fourteen pixels of content and
//! centres the pair: the tick lands five pixels left of the middle and the dash five pixels right
//! of it, and whichever one is showing is visibly off to one side. Nothing about the drawing, the
//! icon or the fit is wrong, which is why the assertion has to be about the placement.
//!
//! # Why each state is measured on its own
//!
//! The hidden mark is faded to nothing, and a primitive faded to nothing is not put in the display
//! list at all — so the frame carries the mark the control is showing and only that one. A box in
//! each state is therefore what asks the question about both marks: the ticked ones answer for the
//! tick, the part-way ones for the dash, and a control that centred one by pushing the other out of
//! the way still fails in the state that shows the other.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Rect};
use zgui::view;
use zgui::view::{AnyView, NodeId, NodeRef};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;

/// The page the fixtures are laid out on.
///
/// The padding is load-bearing: with a control at the window's corner, a mark drawn at the origin
/// and a mark drawn where it belongs are hard to tell apart, and every assertion below would hold
/// for an emitter that ignored the box altogether.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 40px; gap: 24px; align-items: flex-start }
                     .page .big { width: 28px; height: 28px }
                     .page .big > .zui-icon { --zui-icon-size: 20px }";

/// How far a mark's middle may be from its box's middle before the control looks wrong.
///
/// A tick is not symmetric about its own bounding box — the stroke that goes up is longer than the
/// one that comes down — so a whole pixel of slack is what "in the middle" means for a drawing at
/// this size. The defect this measures is five pixels.
const SLACK: f32 = 1.5;

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// Where a fixture leaves the references it built, so a test can find the elements it made.
///
/// A checkbox says nothing at all, so there is no text to find it by.
#[derive(Clone, Default)]
struct Built(Rc<RefCell<Vec<NodeRef>>>);

impl Built {
    /// Records the references this build produced, replacing whatever the last one left.
    fn keep(&self, refs: &[NodeRef]) {
        *self.0.borrow_mut() = refs.to_vec();
    }

    /// The node the `which`th reference was bound to.
    ///
    /// # Panics
    ///
    /// Panics before the view has been built, and for a control that never bound its reference.
    fn node(&self, which: usize) -> NodeId {
        self.0.borrow()[which]
            .get_untracked()
            .expect("the control bound its reference when it was built")
    }
}

/// The middle of a rectangle.
fn middle(rect: Rect<DevicePx, Device>) -> (f32, f32) {
    (
        rect.origin.x.0 + rect.size.width.0 / 2.0,
        rect.origin.y.0 + rect.size.height.0 / 2.0,
    )
}

/// Asserts that the mark the box is showing is in the middle of it, and inside it.
///
/// Exactly one drawing, because the mark that is not showing is faded to nothing and a primitive
/// faded to nothing is never pushed. Two would mean the hidden mark is being drawn over the shown
/// one, which is what a control with a mark visibly off to one side looks like from the outside.
fn assert_centred(stage: &Stage, node: NodeId, what: &str) {
    let box_ = stage.rect_of(node);
    let drawings = stage.drawings_in(box_);
    assert_eq!(
        drawings.len(),
        1,
        "{what}: a control shows one mark, and the frame carried {} inside {box_:?}: {drawings:?}",
        drawings.len()
    );
    let (want_x, want_y) = middle(box_);
    for drawing in drawings {
        let ink = drawing.ink;
        let (got_x, got_y) = middle(ink);
        assert!(
            (got_x - want_x).abs() <= SLACK,
            "{what}: a mark's middle is {got_x} across a box whose middle is {want_x} ({ink:?} in \
             {box_:?})"
        );
        assert!(
            (got_y - want_y).abs() <= SLACK,
            "{what}: a mark's middle is {got_y} down a box whose middle is {want_y} ({ink:?} in \
             {box_:?})"
        );
        assert!(
            ink.origin.x.0 >= box_.origin.x.0 && ink.origin.y.0 >= box_.origin.y.0,
            "{what}: a mark starts outside its box ({ink:?} in {box_:?})"
        );
        assert!(
            ink.origin.x.0 + ink.size.width.0 <= box_.origin.x.0 + box_.size.width.0
                && ink.origin.y.0 + ink.size.height.0 <= box_.origin.y.0 + box_.size.height.0,
            "{what}: a mark runs past its box ({ink:?} in {box_:?})"
        );
    }
}

/// Four checkboxes: each state at the usual size, and each of them again drawn larger.
fn boxes(built: &Built) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let ticked = NodeRef::new();
        let mixed = NodeRef::new();
        let large = NodeRef::new();
        let large_mixed = NodeRef::new();
        built.keep(&[ticked, mixed, large, large_mixed]);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Checkbox(node_ref = ticked, default_checked = Checked::Yes, a11y:label = "Ticked")
                    Checkbox(node_ref = mixed, default_checked = Checked::Mixed, a11y:label = "Mixed")
                    Checkbox(
                        node_ref = large,
                        class = "big",
                        default_checked = Checked::Yes,
                        a11y:label = "Large"
                    )
                    Checkbox(
                        node_ref = large_mixed,
                        class = "big",
                        default_checked = Checked::Mixed,
                        a11y:label = "Large and part-way"
                    )
                }
            }
        })
    }
}

#[test]
fn a_ticked_box_draws_its_tick_in_the_middle_of_itself() {
    let built = Built::default();
    let stage = staged!(boxes(&built));
    stage.capture("checkbox-marks");
    assert_centred(&stage, built.node(0), "a ticked box");
}

#[test]
fn a_part_way_box_draws_its_dash_in_the_middle_of_itself() {
    let built = Built::default();
    let stage = staged!(boxes(&built));
    assert_centred(&stage, built.node(1), "a part-way box");
}

/// The box and its mark are both sizes a caller can set, and a mark placed by anything other than
/// the box's own middle is off by an amount that moves with them. One size passing is the whole of
/// what a fixture that measured the default alone would prove.
#[test]
fn a_larger_box_with_a_larger_mark_still_centres_it() {
    let built = Built::default();
    let stage = staged!(boxes(&built));
    assert_centred(&stage, built.node(2), "a larger box");
}

/// The dash at the other size, for the reason the tick is measured at two: the mark that is not a
/// tick is placed by the same rules and is the one a fixture is most likely to leave out.
#[test]
fn a_larger_part_way_box_still_centres_its_dash() {
    let built = Built::default();
    let stage = staged!(boxes(&built));
    assert_centred(&stage, built.node(3), "a larger part-way box");
}
