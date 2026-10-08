//! A disabled button draws its label at every size and in every variant.
//!
//! `:disabled` takes the button to half opacity. A button that turns disabled after it was drawn
//! opens a group target over a label that was last encoded for the opaque window, so this is
//! also the assertion that a recorded line is not replayed into a target of another kind.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
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

const SIZES: [ButtonSize; 4] = [
    ButtonSize::Xs,
    ButtonSize::Sm,
    ButtonSize::Md,
    ButtonSize::Lg,
];

const VARIANTS: [ButtonVariant; 3] = [
    ButtonVariant::Outline,
    ButtonVariant::Default,
    ButtonVariant::Ghost,
];

#[derive(Clone, Default)]
struct Built(Rc<RefCell<Vec<(String, NodeRef)>>>);

impl Built {
    fn nodes(&self) -> Vec<(String, NodeId)> {
        self.0
            .borrow()
            .iter()
            .map(|(name, node)| {
                (
                    name.clone(),
                    node.get_untracked()
                        .expect("the button bound its reference"),
                )
            })
            .collect()
    }
}

fn ink_pixels(stage: &Stage, node: NodeId) -> usize {
    let colours = stage.colours_in(stage.rect_of(node));
    let mut counts: std::collections::HashMap<(u8, u8, u8), usize> =
        std::collections::HashMap::new();
    for colour in &colours {
        *counts.entry(*colour).or_default() += 1;
    }
    let dominant = counts.values().copied().max().unwrap_or(0);
    colours.len() - dominant
}

fn grid(built: &Built, disabled: Signal<bool, LocalStorage>) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let mut rows = Vec::new();
        built.0.borrow_mut().clear();
        for variant in VARIANTS {
            for size in SIZES {
                let node = NodeRef::new();
                built
                    .0
                    .borrow_mut()
                    .push((format!("{variant:?} {size:?}"), node));
                rows.push(AnyView::new(view! {
                    Button(node_ref = node, variant = variant, size = size, disabled = disabled) {"Open in editor"}
                }));
            }
        }
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") { {rows} }
            }
        })
    }
}

fn assert_labels(stage: &Stage, built: &Built, when: &str) {
    let inks: Vec<(String, usize)> = built
        .nodes()
        .into_iter()
        .map(|(name, node)| (name, ink_pixels(stage, node)))
        .collect();
    let blank: Vec<&(String, usize)> = inks.iter().filter(|(_, ink)| *ink <= 20).collect();
    assert!(blank.is_empty(), "labels missing {when}: {blank:?}");
}

#[test]
fn a_button_built_disabled_draws_its_label() {
    let built = Built::default();
    let mut stage = staged!(grid(&built, Signal::stored_local(true)));
    stage.settle();
    stage.repaint();
    assert_labels(&stage, &built, "when built disabled");
}

#[test]
fn a_button_that_turns_disabled_keeps_its_label() {
    let built = Built::default();
    let disabled = RwSignal::new_local(false);
    let mut stage = staged!(grid(&built, disabled.into()));
    stage.settle();
    assert_labels(&stage, &built, "while enabled");
    disabled.set(true);
    stage.wait(crate::painted::stage::SETTLED);
    assert_labels(&stage, &built, "after turning disabled");
    disabled.set(false);
    stage.wait(crate::painted::stage::SETTLED);
    assert_labels(&stage, &built, "after turning enabled again");
}
