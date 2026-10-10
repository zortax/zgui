//! A button's label stays painted while its background moves under the pointer.
//!
//! Read off the composed target with the real damage set, which is what a surface shows: the
//! test device otherwise redraws the whole window every frame.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::view;
use zgui::view::{AnyView, NodeId, NodeRef};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 40px; gap: 12px; align-items: flex-start }";

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

#[derive(Clone, Default)]
struct Built(Rc<RefCell<Vec<NodeRef>>>);

impl Built {
    fn keep(&self, refs: &[NodeRef]) {
        *self.0.borrow_mut() = refs.to_vec();
    }

    fn node(&self, which: usize) -> NodeId {
        self.0.borrow()[which]
            .get_untracked()
            .expect("the control bound its reference when it was built")
    }

    fn len(&self) -> usize {
        self.0.borrow().len()
    }
}

fn buttons(built: &Built) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let refs: Vec<NodeRef> = (0..6).map(|_| NodeRef::new()).collect();
        built.keep(&refs);
        AnyView::new(view! {
            ThemeProvider {
            column(class = "page") {
                Button(node_ref = refs[0], variant = ButtonVariant::Default) {"Default action"}
                Button(node_ref = refs[1], variant = ButtonVariant::Destructive) {"Delete it"}
                Button(node_ref = refs[2], variant = ButtonVariant::Outline) {"Outline one"}
                Button(node_ref = refs[3], variant = ButtonVariant::Secondary) {"Secondary"}
                Button(node_ref = refs[4], variant = ButtonVariant::Ghost) {"Ghost button"}
                Button(node_ref = refs[5], variant = ButtonVariant::Link) {"Link button"}
            }
            }
        })
    }
}

/// How many pixels inside `node`'s box are not its dominant colour: the label's ink.
fn ink_pixels(stage: &Stage, node: NodeId) -> usize {
    let colours = stage.composed_colours_in(stage.rect_of(node));
    let mut counts: std::collections::HashMap<(u8, u8, u8), usize> =
        std::collections::HashMap::new();
    for colour in &colours {
        *counts.entry(*colour).or_default() += 1;
    }
    let dominant = counts.values().copied().max().unwrap_or(0);
    colours.len() - dominant
}

#[test]
fn every_button_keeps_its_label_through_hover_and_leave() {
    crate::device::use_real_damage();
    let built = Built::default();
    let mut stage = staged!(buttons(&built));
    stage.settle();
    let at_rest: Vec<usize> = (0..built.len())
        .map(|which| ink_pixels(&stage, built.node(which)))
        .collect();
    assert!(
        at_rest.iter().all(|ink| *ink > 20),
        "labels at rest: {at_rest:?}"
    );
    let check = |stage: &Stage, when: &str| {
        for (which, rest) in at_rest.iter().enumerate() {
            let ink = ink_pixels(stage, built.node(which));
            assert!(
                ink * 10 >= rest * 7,
                "button {which} lost its label {when}: {ink} ink pixels, {rest} at rest"
            );
        }
    };
    for round in 0..2 {
        for which in 0..built.len() {
            let at = stage.centre_of(built.node(which));
            stage.move_to(at);
            stage.settle();
            check(
                &stage,
                &format!("while button {which} is hovered (round {round})"),
            );
            // Every frame of the transition, with no repair in between.
            for frame in 0..24 {
                stage.tick();
                check(
                    &stage,
                    &format!("frame {frame} after hovering button {which} (round {round})"),
                );
            }
        }
        // Back and forth between the outline and ghost buttons, faster than the transition.
        for flip in 0..12 {
            let which = if flip % 2 == 0 { 2 } else { 4 };
            stage.move_to(stage.centre_of(built.node(which)));
            stage.settle();
            stage.tick();
            check(
                &stage,
                &format!("flip {flip} onto button {which} (round {round})"),
            );
            stage.tick();
            check(
                &stage,
                &format!("flip {flip} onto button {which}, next frame (round {round})"),
            );
        }
        stage.leave();
        stage.settle();
        for frame in 0..24 {
            stage.tick();
            check(
                &stage,
                &format!("frame {frame} after the pointer left (round {round})"),
            );
        }
    }
}
