//! A colour transition drawn through paint slots its box owns shows, on every frame, what a box
//! built still at the same colour shows.
//!
//! The transitions run in four steps, so each frame holds one of five colours, and a still box of
//! each colour stands in the same window. Read off the composed target with the real damage set,
//! which is what a surface shows: the test device otherwise redraws the whole window every frame.

mod desktop;
mod device;
mod painted;

use zgui::geom::{DevicePx, Edges};
use zgui::prelude::*;
use zgui::view;
use zgui::view::{AnyView, NodeRef};

use crate::painted::stage::Stage;

/// A swatch whose background and an edge whose border go from transparent to a colour in four
/// steps, each beside a still box at every step. A slide moves its background with its box.
const SHEET: &str = ":root { background-color: #ffffff }
     .stage { display: flex; flex-direction: row; gap: 10px; padding: 24px }
     .swatch, .edge, .slide { width: 48px; height: 20px; border-radius: 6px }
     .swatch { background-color: transparent;
               transition: background-color 400ms steps(4, jump-end) }
     .swatch.on { background-color: #ff0000 }
     .swatch.pulse { animation: pulse 400ms steps(4, jump-end) infinite }
     .swatch.once { animation: pulse 400ms steps(4, jump-end) }
     @keyframes pulse { from { background-color: transparent } to { background-color: #ff0000 } }
     .s1 { background-color: rgba(255, 0, 0, 0.25) }
     .s2 { background-color: rgba(255, 0, 0, 0.5) }
     .s3 { background-color: rgba(255, 0, 0, 0.75) }
     .s4 { background-color: #ff0000 }
     .slide { background-color: transparent;
              transition: background-color 400ms steps(4, jump-end),
                          transform 400ms steps(4, jump-end) }
     .slide.on { background-color: #ff0000; transform: translateY(40px) }
     .edge { border: 4px solid transparent;
             transition: border-color 400ms steps(4, jump-end) }
     .edge.on { border-color: #0000ff }
     .e1 { border-color: rgba(0, 0, 255, 0.25) }
     .e2 { border-color: rgba(0, 0, 255, 0.5) }
     .e3 { border-color: rgba(0, 0, 255, 0.75) }
     .e4 { border-color: #0000ff }";

/// How many output frames each direction is read for, with room past its 400ms.
const FRAMES: usize = 36;

/// The still classes of one kind, from transparent to the full colour.
const SWATCHES: [&str; 5] = ["swatch", "swatch s1", "swatch s2", "swatch s3", "swatch s4"];

/// The same, for the border.
const EDGES: [&str; 5] = ["edge", "edge e1", "edge e2", "edge e3", "edge e4"];

/// What switches the animated box's classes.
#[derive(Clone, Copy)]
struct Switches {
    /// `.on`, the colour a transition runs to.
    on: RwSignal<bool>,
    /// `.pulse`, an animation that runs for as long as it holds.
    pulse: RwSignal<bool>,
    /// `.once`, the same animation for one iteration.
    once: RwSignal<bool>,
}

impl Switches {
    /// Every class off.
    fn off() -> Self {
        Self {
            on: RwSignal::new(false),
            pulse: RwSignal::new(false),
            once: RwSignal::new(false),
        }
    }
}

/// Opens the animated box of `kind` beside its five stills, its classes switched by `switches`.
fn open(kind: &'static str, stills: [&'static str; 5], switches: Switches) -> Option<Opened> {
    let Switches { on, pulse, once } = switches;
    let moving = NodeRef::new();
    let refs: [NodeRef; 5] = std::array::from_fn(|_| NodeRef::new());
    let held = refs;
    let stage = Stage::open(SHEET, move || {
        let mut boxes = vec![AnyView::new(view! {
            box(
                class = kind,
                class:on = move || on.get(),
                class:pulse = move || pulse.get(),
                class:once = move || once.get(),
                node_ref = moving,
            )
        })];
        for (class, still) in stills.iter().zip(held.iter()) {
            let (class, still) = (*class, *still);
            boxes.push(AnyView::new(view! { box(class = class, node_ref = still) }));
        }
        AnyView::new(view! { row(class = "stage") { {boxes} } })
    })?;
    Some(Opened {
        stage,
        moving,
        stills: refs,
    })
}

/// A stage and the boxes on it.
struct Opened {
    stage: Stage,
    moving: NodeRef,
    stills: [NodeRef; 5],
}

impl Opened {
    /// The pixels of `node`'s border box and two pixels round it, on the composed target.
    fn pixels(&self, node: NodeRef) -> Vec<(u8, u8, u8)> {
        let rect = self
            .stage
            .rect_of(node.get_untracked().expect("the box was built"));
        let around = rect.outset(Edges {
            top: DevicePx(2.0),
            right: DevicePx(2.0),
            bottom: DevicePx(2.0),
            left: DevicePx(2.0),
        });
        self.stage.composed_colours_in(around)
    }

    /// Which still the animated box shows exactly, or a panic naming the nearest one.
    fn step(&self, frame: usize) -> usize {
        let moving = self.pixels(self.moving);
        let mut nearest = (u8::MAX, 0);
        for (index, still) in self.stills.iter().enumerate() {
            let still = self.pixels(*still);
            assert_eq!(still.len(), moving.len(), "the boxes are one size");
            let worst = moving
                .iter()
                .zip(&still)
                .map(|(a, b)| {
                    a.0.abs_diff(b.0)
                        .max(a.1.abs_diff(b.1))
                        .max(a.2.abs_diff(b.2))
                })
                .max()
                .unwrap_or(0);
            if worst == 0 {
                return index;
            }
            nearest = nearest.min((worst, index));
        }
        panic!(
            "frame {frame}: the animated box matches no still box; the nearest is step {} at {} \
             levels",
            nearest.1, nearest.0
        );
    }

    /// The still step the animated box shows on each of the next [`FRAMES`] frames.
    fn steps(&mut self) -> Vec<usize> {
        (0..FRAMES)
            .map(|frame| {
                self.stage.tick();
                self.step(frame)
            })
            .collect()
    }
}

/// Runs the transition of `kind` to its colour and back, checking every frame against the stills.
fn there_and_back(kind: &'static str, stills: [&'static str; 5]) {
    crate::device::use_real_damage();
    let switches = Switches::off();
    let on = switches.on;
    let Some(mut opened) = open(kind, stills, switches) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    assert_eq!(opened.step(0), 0, "{kind}: the box starts transparent");

    on.set(true);
    let up = opened.steps();
    on.set(false);
    let down = opened.steps();

    let distinct = |steps: &[usize]| {
        let mut seen = steps.to_vec();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    assert!(
        distinct(&up) >= 4,
        "{kind}: the way in showed too few steps: {up:?}"
    );
    assert!(
        up.windows(2).all(|pair| pair[1] >= pair[0]),
        "{kind}: the way in went back: {up:?}"
    );
    assert_eq!(up.last(), Some(&4), "{kind}: the way in ended at {up:?}");
    assert!(
        distinct(&down) >= 4,
        "{kind}: the way out showed too few steps: {down:?}"
    );
    assert!(
        down.windows(2).all(|pair| pair[1] <= pair[0]),
        "{kind}: the way out went back: {down:?}"
    );
    assert_eq!(
        down.last(),
        Some(&0),
        "{kind}: the way out ended at {down:?}"
    );
}

#[test]
fn a_background_from_transparent_and_back_matches_a_still_box_every_frame() {
    there_and_back("swatch", SWATCHES);
}

#[test]
fn a_border_fading_in_and_out_matches_a_still_box_every_frame() {
    there_and_back("edge", EDGES);
}

#[test]
fn a_background_moving_with_its_box_matches_a_still_box_every_frame() {
    // The transform moves the box by writing its coordinate system, so its chunk replays at the
    // new place. The colour reaches the screen only through the slot the box owns.
    there_and_back("slide", SWATCHES);
}

#[test]
fn a_background_whose_animation_is_dropped_goes_back_to_its_own_style() {
    // The box's own style paints nothing, so the repaint marked when its override goes is one the
    // fragment pass damages nothing for. The pixels the slot drew stay unless the animation stage
    // damages them itself.
    crate::device::use_real_damage();
    let switches = Switches::off();
    let pulse = switches.pulse;
    pulse.set(true);
    let Some(mut opened) = open("swatch", SWATCHES, switches) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let shown = opened.steps();
    let at = *shown.last().expect("frames were read");
    assert!(
        shown.iter().any(|step| *step >= 2),
        "the pulse never showed its colour: {shown:?}"
    );
    if at == 0 {
        // Dropped where the pulse shows nothing, which proves nothing; one more frame moves on.
        opened.stage.tick();
    }
    assert_ne!(
        opened.step(0),
        0,
        "the pulse is showing a colour when it is dropped"
    );
    pulse.set(false);
    for _ in 0..3 {
        opened.stage.tick();
    }
    assert_eq!(
        opened.step(0),
        0,
        "the box kept the colour its dropped animation last drew"
    );
}

#[test]
fn a_background_whose_animation_ends_goes_back_to_its_own_style() {
    // The animation ends by itself on a colour, with no fill and no restyle. The repaint its end
    // marks is one the fragment pass damages nothing for, because the box's own style paints
    // nothing.
    crate::device::use_real_damage();
    let switches = Switches::off();
    let Some(mut opened) = open("swatch", SWATCHES, switches) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    switches.once.set(true);
    let shown = opened.steps();
    assert!(
        shown.contains(&3),
        "the animation never showed its last step: {shown:?}"
    );
    assert_eq!(
        shown.last(),
        Some(&0),
        "the box kept the colour its ended animation last drew: {shown:?}"
    );
}
