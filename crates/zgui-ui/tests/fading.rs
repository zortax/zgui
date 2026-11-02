//! A panel that paints nothing of its own fades its children on the screen.
//!
//! Read off the composed target with the real damage set, which is what a surface shows: the
//! test device otherwise redraws the whole window every frame.

mod desktop;
mod device;
mod painted;

use zgui::geom::{DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::{AnyView, NodeRef};

use crate::painted::stage::Stage;

/// A panel with no background, holding black swatches on white.
///
/// `.fading` runs the panel's opacity from 1 to its own value. `.flash` runs it from 1 to 0.2
/// and then lets it go back to 1. `.half` holds it at 0.6, and `.dim` or `.lowered`, which win,
/// at 0.2.
const SHEET: &str = ":root { background-color: #ffffff }
     .stage { display: block; padding: 24px }
     .panel { display: flex; flex-direction: row; gap: 6px; width: 600px }
     .half { opacity: 0.6 }
     .dim, .lowered { opacity: 0.2 }
     .fading { animation-name: fade; animation-duration: 400ms; animation-timing-function: linear }
     .flash { animation-name: flash; animation-duration: 400ms; animation-timing-function: linear }
     @keyframes fade { from { opacity: 1 } }
     @keyframes flash { from { opacity: 1 } to { opacity: 0.2 } }
     .swatch { width: 48px; height: 20px; background-color: #000000 }
     .overlap > .swatch { margin-left: -20px }";

/// How many swatches the panel holds.
const SWATCHES: usize = 6;

/// How many output frames the fade runs for, with room to spare.
const FRAMES: usize = 30;

/// Opens a panel with `class`, and with `.lowered` while `dim` holds, and the first swatch's
/// reference.
fn open(class: &'static str, dim: RwSignal<bool>) -> Option<(Stage, NodeRef)> {
    let first = NodeRef::new();
    let stage = Stage::open(SHEET, move || {
        let swatches: Vec<AnyView> = (0..SWATCHES)
            .map(|index| {
                if index == 0 {
                    AnyView::new(view! { box(class = "swatch", node_ref = first) })
                } else {
                    AnyView::new(view! { box(class = "swatch") })
                }
            })
            .collect();
        AnyView::new(view! {
            column(class = "stage") {
                box(class = class, class:lowered = move || dim.get()) { {swatches} }
            }
        })
    })?;
    Some((stage, first))
}

/// The red level at the middle of the first swatch, read off the composed target.
fn level(stage: &Stage, first: NodeRef) -> u8 {
    let rect = stage.rect_of(first.get_untracked().expect("the swatch was built"));
    let middle = Rect::new(
        Point::new(
            DevicePx(rect.origin.x.0 + 12.0),
            DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
        ),
        Size::new(DevicePx(1.0), DevicePx(1.0)),
    );
    stage.composed_colours_in(middle)[0].0
}

/// The level a freshly built panel with `class` shows, with no animation and no change.
fn still(class: &'static str) -> Option<u8> {
    let (stage, first) = open(class, RwSignal::new(false))?;
    Some(level(&stage, first))
}

/// The level at each frame of the animation on a panel with `class`, the first one included.
fn frames(class: &'static str) -> Option<Vec<u8>> {
    let (mut stage, first) = open(class, RwSignal::new(false))?;
    let mut levels = vec![level(&stage, first)];
    for _ in 0..FRAMES {
        stage.tick();
        levels.push(level(&stage, first));
    }
    Some(levels)
}

/// How many frames in a row, from the first, changed the level.
fn changing(levels: &[u8]) -> usize {
    levels
        .windows(2)
        .take_while(|pair| pair[1] != pair[0])
        .count()
}

/// Fades the panel with `class` from 1 to its own opacity, checking every frame.
fn fades(class: &'static str, rest: &'static str) {
    crate::device::use_real_damage();
    let Some(levels) = frames(class) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let Some(end) = still(rest) else {
        return;
    };
    // Black at full opacity over white, lighter every frame until the fade ends.
    assert!(levels[0] < 40, "{class}: first frame {levels:?}");
    assert!(
        changing(&levels) >= 15,
        "{class}: the fade changed {} frames in a row: {levels:?}",
        changing(&levels)
    );
    assert!(
        levels.windows(2).all(|pair| pair[1] >= pair[0]),
        "{class}: the fade went back: {levels:?}"
    );
    let last = *levels.last().expect("levels were read");
    assert!(
        last.abs_diff(end) <= 1,
        "{class}: the fade ended at {last}, a still panel shows {end}: {levels:?}"
    );
}

#[test]
fn a_paintless_panel_fades_its_disjoint_children() {
    fades("panel dim fading", "panel dim");
}

#[test]
fn a_paintless_panel_fades_its_overlapping_children() {
    fades("panel overlap dim fading", "panel overlap dim");
}

/// The animation alone makes the panel translucent, and its end puts the panel back to opaque.
#[test]
fn a_paintless_panel_comes_back_when_its_fade_ends() {
    crate::device::use_real_damage();
    let Some(levels) = frames("panel flash") else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let Some(end) = still("panel") else {
        return;
    };
    assert!(
        changing(&levels) >= 15,
        "the fade changed {} frames in a row: {levels:?}",
        changing(&levels)
    );
    let lightest = *levels.iter().max().expect("levels were read");
    assert!(lightest > 150, "the fade reached {lightest}: {levels:?}");
    let last = *levels.last().expect("levels were read");
    assert!(
        last.abs_diff(end) <= 1,
        "the panel ended at {last}, a still opaque panel shows {end}: {levels:?}"
    );
}

/// A restyle that moves the opacity of a translucent panel, with no animation.
#[test]
fn a_paintless_panel_shows_a_restyled_opacity() {
    crate::device::use_real_damage();
    let dim = RwSignal::new(false);
    let Some((mut stage, first)) = open("panel half", dim) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let before = level(&stage, first);
    dim.set(true);
    stage.settle();
    stage.tick();
    let after = level(&stage, first);
    drop(stage);
    let Some(end) = still("panel dim") else {
        return;
    };
    assert!(
        after.abs_diff(end) <= 1 && before.abs_diff(end) > 40,
        "the panel showed {before}, then {after}; a still panel at the new opacity shows {end}"
    );
}
