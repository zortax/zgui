//! A colour transition drawn through paint slots its box owns shows, on every frame, what a box
//! built still at the same colour shows.
//!
//! The transitions run in four steps, so each frame holds one of five colours, and a still box of
//! each colour stands in the same window. Read off the composed target with the real damage set,
//! which is what a surface shows: the test device otherwise redraws the whole window every frame.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Edges, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::{AnyView, NodeRef};

use crate::painted::stage::Stage;

/// A swatch whose background and an edge whose border go from transparent to a colour in four
/// steps, each beside a still box at every step. A slide moves its background with its box.
const SHEET: &str = ":root { background-color: #ffffff }
     .stage { display: flex; flex-direction: row; gap: 10px; padding: 24px }
     .swatch, .edge, .slide, .under { width: 48px; height: 20px; border-radius: 6px }
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
     .e4 { border-color: #0000ff }
     .under { border-style: solid; border-width: 0 0 4px 0;
              border-color: #00ff00 #00ff00 transparent #00ff00;
              transition: border-color 400ms steps(4, jump-end) }
     .under.on { border-bottom-color: #0000ff }
     .u1 { border-bottom-color: rgba(0, 0, 255, 0.25) }
     .u2 { border-bottom-color: rgba(0, 0, 255, 0.5) }
     .u3 { border-bottom-color: rgba(0, 0, 255, 0.75) }
     .u4 { border-bottom-color: #0000ff }
     .centred { display: flex; flex-direction: row; justify-content: center; gap: 10px;
                padding: 24px }
     .column { display: flex; flex-direction: column; gap: 10px }
     .slot { display: flex; flex-direction: row; justify-content: center; width: 300px;
             height: 20px }
     .slot.left { justify-content: flex-start }
     .holder { display: flex; flex-direction: row; justify-content: center; height: 20px }
     .held, .wide, .padded { height: 20px; background-color: transparent;
                             animation: held 100s linear }
     @keyframes held {
         from { background-color: transparent }
         1% { background-color: #ff0000 }
         to { background-color: #ff0000 }
     }
     .held { width: 48px }
     .wide { width: 12% }
     .padded { box-sizing: border-box; width: 64px; padding-left: 10% }";

/// How many output frames each direction is read for, with room past its 400ms.
const FRAMES: usize = 36;

/// The still classes of one kind, from transparent to the full colour.
const SWATCHES: [&str; 5] = ["swatch", "swatch s1", "swatch s2", "swatch s3", "swatch s4"];

/// The same, for the border.
const EDGES: [&str; 5] = ["edge", "edge e1", "edge e2", "edge e3", "edge e4"];

/// The same, for a border drawn on one side under three sides of another colour.
const UNDERS: [&str; 5] = ["under", "under u1", "under u2", "under u3", "under u4"];

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
fn a_border_on_one_side_matches_a_still_box_every_frame() {
    // Only the bottom side has a width, and the other three hold another colour. The still box
    // strokes the bottom colour alone, so the owned slot has to hold the same.
    there_and_back("under", UNDERS);
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

/// The whole window as the composed target holds it.
fn picture(stage: &Stage, width: f32, height: f32) -> Vec<(u8, u8, u8)> {
    stage.composed_colours_in(Rect::<DevicePx, Device>::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(width), DevicePx(height)),
    ))
}

/// How many pixels of the partial picture differ from a complete redraw, named by `step`.
fn against_a_redraw(stage: &mut Stage, width: f32, height: f32, step: &str) -> Option<String> {
    let partial = picture(stage, width, height);
    stage.repaint();
    let full = picture(stage, width, height);
    let wrong = partial
        .iter()
        .zip(&full)
        .filter(|(a, b)| {
            a.0.abs_diff(b.0)
                .max(a.1.abs_diff(b.1))
                .max(a.2.abs_diff(b.2))
                > 8
        })
        .count();
    (wrong > 0).then(|| format!("{step}: {wrong} pixels"))
}

#[test]
fn a_held_colour_on_a_transparent_box_moves_with_the_box() {
    // The animation starts transparent, so the boxes' own style paints nothing, and then holds
    // one colour, so no frame of it damages anything: what draws the boxes is the override alone.
    // Switches unmount two, a resize moves them, and a narrower holder moves one and changes its
    // padding. Each partial frame has to be what a complete redraw shows.
    crate::device::use_real_damage();
    let holder = RwSignal::new(200.0f32);
    let early = RwSignal::new(true);
    let late = RwSignal::new(true);
    let held = NodeRef::new();
    let Some(mut stage) = Stage::open(SHEET, move || {
        AnyView::new(view! {
            column(class = "column") {
                row(class = "centred") {
                    box(class = "held", node_ref = held)
                    box(class = "wide")
                }
                row(class = "slot left") {
                    row(class = "holder", style:width = move || Some(format!("{}px", holder.get()))) {
                        box(class = "padded")
                    }
                }
                row(class = "slot") {
                    if move || early.get() {
                        box(class = "held")
                    } else {}
                }
                row(class = "slot") {
                    if move || late.get() {
                        box(class = "held")
                    } else {}
                }
            }
        })
    }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    stage.wait_quietly(std::time::Duration::from_millis(1_500));
    let rect = stage.rect_of(held.get_untracked().expect("the box was built"));
    assert!(
        stage.composed_colours_in(rect).contains(&(255, 0, 0)),
        "the held colour draws the box"
    );

    let mut failures = Vec::new();
    // First, while no box has been composed again since its colour began.
    early.set(false);
    stage.tick();
    failures.extend(against_a_redraw(
        &mut stage,
        900.0,
        600.0,
        "unmounted-early",
    ));
    let sizes = [(860.0, 590.0), (700.0, 560.0), (900.0, 600.0)];
    for (index, (width, height)) in sizes.into_iter().enumerate() {
        stage.resize(width, height);
        stage.tick();
        failures.extend(against_a_redraw(
            &mut stage,
            width,
            height,
            &format!("size-{index}-{width}x{height}"),
        ));
    }
    for (index, width) in [120.0f32, 260.0, 160.0].into_iter().enumerate() {
        holder.set(width);
        stage.tick();
        failures.extend(against_a_redraw(
            &mut stage,
            900.0,
            600.0,
            &format!("holder-{index}-{width}"),
        ));
    }
    // Last, after the resizes composed every box again under its colour.
    late.set(false);
    stage.tick();
    failures.extend(against_a_redraw(&mut stage, 900.0, 600.0, "unmounted-late"));
    assert!(
        failures.is_empty(),
        "a box with a held colour left pixels behind: {failures:#?}"
    );
}
