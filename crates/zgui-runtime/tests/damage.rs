//! What a real document damages when one node in it changes.
//!
//! Every other damage assertion in this workspace hands a renderer a [`DamageSet`] built by hand
//! and checks the renderer honours it. That tests the consumer and never the producer, so a frame
//! loop that reported the whole window on every keystroke would leave all of them green. The
//! assertions here build no damage at all: they mount a document, change exactly one node in it,
//! and ask what the frame that followed was drawn against.
//!
//! The damage has to be read *inside* `draw`. A window retires its damage as soon as the frame is
//! submitted, so a test reading it afterwards sees an empty set and passes whatever the frame did
//! — which is why the renderer here records every set it is handed.
//!
//! [`DamageSet`]: zgui_bits::DamageSet

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_platform::Surface;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::Scene;
use zgui_view::{BuildCx, IntoView, View};

/// A window far larger than the one node that changes in it.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 100px }
                   text { display: block; width: 40px; height: 20px }
                   .swatch { display: block; width: 30px; height: 20px; background-color: #101010 }
                   .swatch.lit { background-color: #f0f0f0 }";

/// The same window, with the text sized by its content so that a run growing moves what follows it.
///
/// [`CSS`] gives every text node a fixed forty pixels, which is what makes it useless for the one
/// question below: a run whose width cannot change cannot push anything.
const FLOW_CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 100px }
                        row { display: flex }
                        text { display: block }";

/// The one character placed after the growing run, and drawn nowhere else in that document.
///
/// Being unique in the document is what lets a frame's display list be asked where it went without
/// the test knowing anything about glyph identifiers: it is the sprite whose tile no other sprite
/// in the frame has.
const CARET: &str = "|";

/// The window's own area in device pixels, which is what a full repaint costs.
const SURFACE_AREA: i64 = 400 * 300;

/// What one frame was drawn against.
#[derive(Debug, Clone)]
struct Drawn {
    /// Whether the set was the whole surface rather than a list of rectangles.
    full: bool,
    /// The area the rectangles cover, or `None` for a full-surface set.
    area: Option<i64>,
    /// The rectangle the set's own rectangles are bounded by, or `None` when it is empty.
    bounds: Option<Rect<i32, Device>>,
    /// How many glyph sprites this frame put in the display list.
    glyphs: usize,
    /// The solid fills of every quad in the display list.
    fills: Vec<zgui_color::Color>,
    /// Every glyph sprite: the atlas tile it sampled, and where it was drawn.
    ///
    /// The tile is what says *which* glyph was drawn — two digits of the same width differ in
    /// nothing else. The position is what says whether the layout that placed it was recomputed,
    /// which no damage rectangle and no tile can show.
    sprites: Vec<Sprite>,
}

/// One glyph in a frame's display list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sprite {
    /// The atlas texture and tile it sampled.
    tile: (u32, u32),
    /// The left edge of its box, in whole device pixels.
    x: i32,
}

/// The frames a run was drawn against, in order.
type Log = Rc<RefCell<Vec<Drawn>>>;

/// A renderer that records the damage of every frame and draws nothing.
struct Recorder {
    /// Where the frames go.
    log: Log,
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go into plain memory, so the upload path still runs.
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for Recorder {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        self.log.borrow_mut().push(Drawn {
            full: damage.is_full(),
            area: damage.area(),
            bounds: damage.bounds(),
            glyphs: scene.primitives.mono_sprites.len(),
            fills: scene
                .primitives
                .quads
                .iter()
                .filter_map(|quad| match scene.paints.get(quad.fill.id()?) {
                    Some(zgui_scene::Paint::Solid(color)) => Some(*color),
                    _ => None,
                })
                .collect(),
            sprites: scene
                .primitives
                .mono_sprites
                .iter()
                .map(|sprite| Sprite {
                    tile: (sprite.tile.texture, sprite.tile.tile),
                    x: sprite.bounds[0].floor() as i32,
                })
                .collect(),
        });
        FrameOutcome::Presented(zgui_render::FrameStats {
            vector_passes: 0,
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: MemoryReport::ZERO,
        })
    }

    fn register_external(&mut self, _texture: ExternalTexture) -> TextureHandle {
        TextureHandle(0)
    }

    fn release_external(&mut self, _handle: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }
}

/// Mounts a document styled by `css`, recording every frame's damage into `log`.
///
/// The renderer, the shaper and the raster face are the same for every window in this file and
/// none of them is what any assertion here is about. What differs is the document.
fn mount(
    css: &'static str,
    log: &Log,
    view: impl FnMut(&mut BuildCx<'_>) -> Box<dyn zgui_view::Anchor> + 'static,
) -> zgui_platform_headless::Harness<Runtime> {
    let factory = Rc::clone(log);
    let handler = App::new()
        .with_title("damage")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Recorder {
                    log: Rc::clone(&factory),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| Arc::new(zgui_testkit_scene::MonoRaster::new())))
        .into_handler(view)
        .expect("the reactive runtime installs");
    zgui_platform_headless::Harness::new(handler)
}

/// A window whose single text node reads `count`, with every frame's damage recorded into `log`.
fn window_reading(count: RwSignal<i32>, log: &Log) -> zgui_platform_headless::Harness<Runtime> {
    mount(CSS, log, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child(move || count.get().to_string()))
                .into_view()
                .build(cx),
        )
    })
}

/// Whether `outer` covers every pixel of `inner`.
fn contains(outer: Rect<i32, Device>, inner: Rect<i32, Device>) -> bool {
    outer.origin.x <= inner.origin.x
        && outer.origin.y <= inner.origin.y
        && outer.origin.x + outer.size.width >= inner.origin.x + inner.size.width
        && outer.origin.y + outer.size.height >= inner.origin.y + inner.size.height
}

/// The border box of the only fragment the sheet gives this width, in device pixels.
///
/// Read out of the fragment tree rather than assumed, because that is what the damage is being
/// compared against: a constant here would be asserting the test's own arithmetic rather than the
/// frame's behaviour. The width is what identifies the element without the test knowing where
/// layout put it — the sheet gives the swatch thirty pixels and the text forty, and nothing else in
/// either document is either width.
fn box_of_width(window: &zgui_runtime::Window, width: f32) -> Rect<i32, Device> {
    let layout = window.layout().borrow();
    let mut found = None;
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            let box_of = fragment.border_box;
            if (box_of.size.width.0 - width).abs() < 0.5 {
                found = Some(box_of);
            }
        }
    }
    let box_of = found.unwrap_or_else(|| panic!("no fragment of the document is {width} wide"));
    Rect::new(
        Point::new(
            box_of.origin.x.0.floor() as i32,
            box_of.origin.y.0.floor() as i32,
        ),
        Size::new(
            box_of.size.width.0.ceil() as i32,
            box_of.size.height.0.ceil() as i32,
        ),
    )
}

/// Every box name in the window's tree, sorted, so two frames' trees can be compared.
fn box_names(window: &zgui_runtime::Window) -> Vec<zgui_layout::BoxKey> {
    let mut keys = window.layout().borrow().keys();
    keys.sort_by_key(|key| (key.index(), key.generation()));
    keys
}

/// Every fragment name in the window's tree, sorted, on the same terms.
fn fragment_names(window: &zgui_runtime::Window) -> Vec<zgui_layout::FragKey> {
    let layout = window.layout().borrow();
    let mut keys: Vec<zgui_layout::FragKey> = layout
        .keys()
        .iter()
        .flat_map(|key| layout.fragments_of_box(*key).to_vec())
        .collect();
    keys.sort_by_key(|key| (key.index(), key.generation()));
    keys
}

/// The union of the rectangles a burst of frames was drawn against, and the area they covered.
///
/// Both halves are needed and neither is derivable from the other: the union answers "did the frame
/// reach the pixels that had to change", and the sum answers "did it reach anything else". A change
/// is free to be spread over several frames, so what is asserted is what the burst did.
fn covered(frames: &[Drawn]) -> (Option<Rect<i32, Device>>, i64) {
    let mut bounds: Option<Rect<i32, Device>> = None;
    let mut area = 0;
    for frame in frames {
        assert!(
            !frame.full,
            "a frame in this burst was drawn against a full-surface damage set"
        );
        area += frame.area.expect("a set that is not full has an area");
        if let Some(rect) = frame.bounds {
            bounds = Some(bounds.map_or(rect, |held: Rect<i32, Device>| held.union(rect)));
        }
    }
    (bounds, area)
}

/// Fails unless the first frame of a run damaged the whole surface.
///
/// The control every upper bound below rests on. Without it, "smaller than the surface" is a claim
/// about a producer that might report nothing at all for every frame there is.
fn assert_mounted_fully(log: &Log) {
    let first = log
        .borrow()
        .first()
        .cloned()
        .expect("mounting a document drew at least one frame");
    assert!(
        first.full || first.area.is_some_and(|area| area >= SURFACE_AREA),
        "the first frame of a document did not damage the whole surface: {first:?}"
    );
}

/// A window whose text node reads `typed`, with every frame's damage recorded into `log`.
///
/// The same shape a text field has: a signal read into a text node, re-read whenever it is written.
fn window_typing(typed: RwSignal<String>, log: &Log) -> zgui_platform_headless::Harness<Runtime> {
    mount(CSS, log, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::column().class("swatch"))
                .child(zgui_elements::text().child(move || typed.get()))
                .into_view()
                .build(cx),
        )
    })
}

/// A window whose only element carries a class the signal decides.
///
/// The text beside the swatch is deliberately there and deliberately constant: it is what a frame
/// that repainted more than the swatch would have to reach through, so a bound that holds with it
/// present is a bound about the swatch and not about a document with one box in it.
fn window_classed(lit: RwSignal<bool>, log: &Log) -> zgui_platform_headless::Harness<Runtime> {
    mount(CSS, log, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child("hello"))
                .child(
                    zgui_elements::column()
                        .class("swatch")
                        .class_toggle(zgui_view::ClassName::new("lit"), move || lit.get()),
                )
                .into_view()
                .build(cx),
        )
    })
}

/// A window whose growing text run has a second text node placed after it in the same flow.
///
/// The shape of a caret beside a field, of a unit beside a number, of anything a layout puts to
/// the right of something that can change width. Nothing marks the second node when the first one's
/// characters change — it is the *flow* that moves it — which is exactly the case a pass that
/// decides what to recompose from invalidation marks alone gets wrong.
fn window_flowing(typed: RwSignal<String>, log: &Log) -> zgui_platform_headless::Harness<Runtime> {
    mount(FLOW_CSS, log, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .child(
                    zgui_elements::row()
                        .child(zgui_elements::text().child(move || typed.get()))
                        .child(zgui_elements::text().child(CARET)),
                )
                .into_view()
                .build(cx),
        )
    })
}

/// A change that repaints one node and moves nothing damages that node, and repaints it correctly.
///
/// This is the assertion the whole incremental architecture exists to satisfy, and the commonest
/// frame a real application draws: a hover changes one background colour. Box reuse, fragment
/// naming, geometry diffing and the emit walk's subtree skip are all conditioned on box identity
/// surviving a frame. If it does not, every one of them is dead code in a running application, and
/// nothing else in this workspace would notice — every other damage test in it builds a `DamageSet`
/// by hand and checks only that the renderer honours the set it was given.
///
/// The colour is asserted as well as the rectangle, and that half is not decoration. A box holds a
/// *clone* of the style it was built with and every stage after layout reads the box's copy, so a
/// frame that keeps the box across a restyle without refreshing that copy damages exactly the right
/// rectangle and redraws it in the colour that was already there. Every bound on a rectangle in
/// this file is blind to that.
#[test]
fn a_paint_only_change_damages_that_node_and_repaints_it_in_the_new_colour() {
    let lit = RwSignal::new(false);
    let log = Log::default();
    let mut app = window_classed(lit, &log);
    app.settle(8);
    assert_mounted_fully(&log);

    let dark: Vec<_> = log
        .borrow()
        .iter()
        .flat_map(|frame| frame.fills.clone())
        .collect();
    assert!(
        dark.iter().any(|fill| fill.components()[0] < 0.5),
        "the swatch did not reach the display list in its unlit colour at all, so the comparison \
         below would mean nothing: {dark:?}"
    );

    log.borrow_mut().clear();
    lit.set(true);
    app.settle(8);

    let frames = log.borrow().clone();
    assert!(
        !frames.is_empty(),
        "writing the signal produced no frame at all"
    );
    let (bounds, area) = covered(&frames);

    // The swatch's own pixels were in the set, and hardly anything else was. The lower bound is
    // what stops the upper bound passing by damaging nothing: a frame loop that skipped the repaint
    // entirely satisfies "smaller than the window" perfectly, and the colour never changes.
    let swatch = box_of_width(&app.app().windows()[0], 30.0);
    let bounds = bounds.expect("the frames that followed the change damaged nothing at all");
    assert!(
        contains(bounds, swatch),
        "the repainted swatch at {swatch:?} was not inside the damage {bounds:?}, so the frame \
         drew less than it had to"
    );
    let swatch_area = i64::from(swatch.size.width) * i64::from(swatch.size.height);
    assert!(
        area >= swatch_area,
        "the burst damaged {area} px, which is less than the {swatch_area} px the swatch occupies"
    );
    assert!(
        area <= SURFACE_AREA / 20,
        "one colour changed and the burst damaged {area} px of {SURFACE_AREA}, which is more than \
         a twentieth of the window"
    );

    // And the pixels that were redrawn were redrawn in the colour the cascade moved to. Nothing
    // above can see this: the damage is correct in both the working case and the broken one.
    let lit_fills: Vec<_> = frames
        .iter()
        .flat_map(|frame| frame.fills.clone())
        .collect();
    assert!(
        lit_fills.iter().any(|fill| fill.components()[0] > 0.5),
        "the damage was right and the display list still holds the old colour {lit_fills:?}: the \
         box is being painted from the cascade result it was built with"
    );
}

/// A counter incrementing damages the text that changed, not the window it sits in.
///
/// The frame a click on a counter produces, end to end. It used to cost the whole surface, because
/// a text node's characters are copied into the box that lays them out and the only way to get new
/// ones in was to rebuild the tree — which replaces every box, which makes every fragment new,
/// which makes every fragment compare as changed, which grows the damage to the root's ink.
///
/// So the box and fragment names are asserted beside the damage. They are the mechanism the bound
/// rests on, and a bound that held while the names churned would be holding for some other reason.
#[test]
fn a_counter_incrementing_damages_the_text_and_not_the_window() {
    let count = RwSignal::new(0);
    let log = Log::default();
    let mut app = window_reading(count, &log);
    app.settle(8);
    assert_mounted_fully(&log);

    let boxes = box_names(&app.app().windows()[0]);
    let fragments = fragment_names(&app.app().windows()[0]);
    assert!(!boxes.is_empty() && !fragments.is_empty());

    log.borrow_mut().clear();
    count.set(7);
    app.settle(8);

    let frames = log.borrow().clone();
    assert!(!frames.is_empty(), "the click produced no frame at all");
    let (bounds, area) = covered(&frames);

    let text = box_of_width(&app.app().windows()[0], 40.0);
    let bounds = bounds.expect("the frames that followed the click damaged nothing at all");
    assert!(
        contains(bounds, text),
        "the text at {text:?} was not inside the damage {bounds:?}, so the frame drew less than \
         it had to"
    );
    let text_area = i64::from(text.size.width) * i64::from(text.size.height);
    assert!(
        area >= text_area,
        "the burst damaged {area} px, which is less than the {text_area} px the text occupies"
    );
    assert!(
        area <= SURFACE_AREA / 20,
        "one digit changed and the burst damaged {area} px of {SURFACE_AREA}, which is more than \
         a twentieth of the window: the box tree is being rebuilt for a change of text"
    );

    assert_eq!(
        box_names(&app.app().windows()[0]),
        boxes,
        "the digit changed and every box in the document is a different box, so no fragment, no \
         paint record and no hit entry could have been reused"
    );
    assert_eq!(
        fragment_names(&app.app().windows()[0]),
        fragments,
        "every fragment in the document is a different fragment, so every one of them diffs as \
         changed whatever its geometry did"
    );
}

/// Typing one character rebuilds nothing above the run it was typed into.
///
/// A text field is a signal read into a text node, so this is the same path a keystroke takes: the
/// characters change and nothing else in the document does. What must survive is every box above
/// the run — the field, the row it sits in, the window's root — because a rebuild of any of them
/// takes their fragments, their paint records and their hit entries with it.
#[test]
fn typing_one_character_does_not_rebuild_the_box_tree_above_it() {
    let typed = RwSignal::new(String::from("ab"));
    let log = Log::default();
    let mut app = window_typing(typed, &log);
    app.settle(8);
    assert_mounted_fully(&log);

    let boxes = box_names(&app.app().windows()[0]);
    let fragments = fragment_names(&app.app().windows()[0]);
    let glyphs_before = log
        .borrow()
        .iter()
        .map(|frame| frame.glyphs)
        .max()
        .expect("mounting drew a frame");
    assert_eq!(glyphs_before, 2, "two characters did not draw two glyphs");

    log.borrow_mut().clear();
    typed.set(String::from("abc"));
    app.settle(8);

    let frames = log.borrow().clone();
    let (bounds, area) = covered(&frames);

    // The lower bound before the upper one, and it is containment rather than "more than nothing":
    // a burst that damaged one pixel somewhere satisfies both `area > 0` and every ceiling below,
    // while the characters that changed stay on the screen as they were.
    let text = box_of_width(&app.app().windows()[0], 40.0);
    let bounds = bounds.expect("the frames that followed the keystroke damaged nothing at all");
    assert!(
        contains(bounds, text),
        "the text at {text:?} was not inside the damage {bounds:?}, so the frame drew less than \
         it had to"
    );
    let text_area = i64::from(text.size.width) * i64::from(text.size.height);
    assert!(
        area >= text_area,
        "the burst damaged {area} px, which is less than the {text_area} px the text occupies"
    );
    assert!(
        area <= SURFACE_AREA / 20,
        "one character was typed and the burst damaged {area} px of {SURFACE_AREA}"
    );

    assert_eq!(
        box_names(&app.app().windows()[0]),
        boxes,
        "one character was typed and the box tree above it was rebuilt"
    );
    assert_eq!(
        fragment_names(&app.app().windows()[0]),
        fragments,
        "one character was typed and every fragment in the document was replaced"
    );

    // The oracle no damage assertion can be: the third glyph reached the display list. A frame that
    // kept the box and its characters would damage the right rectangle and draw two glyphs into it.
    let after = frames
        .iter()
        .map(|frame| frame.glyphs)
        .max()
        .expect("the keystroke produced no frame");
    assert_eq!(
        after, 3,
        "a third character was typed and no frame drew more than {after} glyph(s): the box kept \
         the string it was built with and the window is showing stale text"
    );
}

/// Text that changed is the text that reaches the display list.
///
/// The box tree holds a copy of every text node's content, and a box that is kept keeps the string
/// it was built with. So the frame loop has a choice to make for every change of text — rebuild the
/// tree, or leave boxes alone and scissor tightly — and taking the second without the first being
/// possible produces a window that damages exactly the right rectangle and paints the old string
/// into it. That failure is invisible to every damage assertion there is, which is why this is
/// asserted on the glyphs and not on the damage.
#[test]
fn text_that_changed_is_the_text_that_is_drawn() {
    let count = RwSignal::new(0);
    let log = Log::default();
    let mut app = window_reading(count, &log);
    app.settle(8);

    let before = log
        .borrow()
        .iter()
        .map(|frame| frame.glyphs)
        .max()
        .expect("mounting a document drew at least one frame");
    assert_eq!(
        before, 1,
        "the one-digit count did not draw exactly one glyph, so the count below means nothing"
    );

    // Two digits where there was one. The sprite count is the oracle because it cannot be satisfied
    // by re-drawing the string that was there before, whatever the damage set says. Taken as the
    // maximum over the burst: a frame loop is free to spread the work, and a later frame that
    // damaged nothing emits nothing.
    log.borrow_mut().clear();
    count.set(22);
    app.settle(8);

    let after = log
        .borrow()
        .iter()
        .map(|frame| frame.glyphs)
        .max()
        .expect("writing the signal produced no frame at all");
    assert_eq!(
        after, 2,
        "the count became a two-digit number and no frame drew more than {after} glyph(s): the \
         box tree kept the string it was built with and the window is painting stale text"
    );
}

/// A digit replaced by one of the same width still draws the new digit.
///
/// The case every other text assertion in this file misses. `0` becoming `22` changes the width of
/// the run, so the line's geometry moves and every comparison in the pipeline notices; `0` becoming
/// `7` changes nothing measurable about the line at all. What must notice it is the record of what
/// the fragment painted last time, and a record that compares only style, chain, transform and size
/// happily replays the previous digit into a perfectly damaged rectangle.
///
/// Asserted on which atlas tile the glyph sampled, because that is the only thing in the display
/// list that differs between the two.
#[test]
fn a_digit_replaced_by_one_of_the_same_width_draws_the_new_digit() {
    let count = RwSignal::new(1);
    let log = Log::default();
    let mut app = window_reading(count, &log);
    app.settle(8);

    let before: Vec<(u32, u32)> = log
        .borrow()
        .iter()
        .flat_map(|frame| {
            frame
                .sprites
                .iter()
                .map(|sprite| sprite.tile)
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        before.len(),
        1,
        "the one-digit count did not draw exactly one glyph, so the comparison below is not \
         between two digits"
    );

    log.borrow_mut().clear();
    count.set(7);
    app.settle(8);

    let after: Vec<(u32, u32)> = log
        .borrow()
        .iter()
        .flat_map(|frame| {
            frame
                .sprites
                .iter()
                .map(|sprite| sprite.tile)
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        !after.is_empty(),
        "the digit changed and no glyph reached the display list at all"
    );
    assert!(
        after.iter().all(|tile| !before.contains(tile)),
        "the digit changed for one of the same width and the glyph drawn is the one that was \
         drawn before: last frame's painting was replayed for a fragment whose content moved"
    );
}

/// A run that grows moves what the flow places after it, and the frame both moves and repaints it.
///
/// The failure this exists for produces a window that is *wrong*, not merely one that is slow, and
/// nothing else in this workspace can see it. The pass that composes fragments descends only what
/// is marked, and what changes a text node's characters marks that node — never the node beside it.
/// But a run that grows pushes everything the flow places after it, so the node beside it moves
/// while staying perfectly clean. Skipping it leaves its fragment composed where it stood last
/// frame: on screen, a caret frozen in the middle of the word being typed through it, with the
/// characters drawn over the top.
///
/// Every other assertion in this file is blind to that, and so is every damage bound anywhere: the
/// document's own boxes are all still the right size, the marks are all correct, the damage covers
/// the run that changed, and the glyph that changed is the glyph that reaches the display list. The
/// only thing that is wrong is where one untouched sprite was drawn — so that is what is asserted,
/// beside the damage that has to have covered both the place it left and the place it went.
#[test]
fn a_run_that_grows_moves_the_node_the_flow_puts_after_it() {
    // A run of one repeated character, so that the caret is the one glyph in the document that is
    // drawn once. That is what makes it findable in a display list without this test knowing how
    // the shaper names glyphs, and it costs nothing: what is being asked is where a sprite went.
    let typed = RwSignal::new(String::from("aa"));
    let log = Log::default();
    let mut app = window_flowing(typed, &log);
    app.settle(8);
    assert_mounted_fully(&log);

    // The caret is the sprite no other sprite shares a tile with, which is what makes it findable
    // without this test knowing how the shaper names glyphs.
    let mounted: Vec<Sprite> = log
        .borrow()
        .iter()
        .flat_map(|frame| frame.sprites.clone())
        .collect();
    let before = sole_caret(&mounted);
    assert_eq!(
        mounted.iter().filter(|sprite| sprite.x == before.x).count(),
        1,
        "two sprites were drawn at the caret's position before anything changed, so the assertion \
         below could not tell a caret that moved from one that did not: {mounted:?}"
    );

    log.borrow_mut().clear();
    typed.set(String::from("aaa"));
    app.settle(8);

    let frames = log.borrow().clone();
    let drawn: Vec<Sprite> = frames
        .iter()
        .flat_map(|frame| frame.sprites.clone())
        .collect();
    let after = sole_caret(&drawn);

    // Exactly the width of the character that was inserted before it, rather than "moved at all":
    // a caret that moved by the wrong amount is the same defect with an arithmetic error in front
    // of it. The width is read out of the mounted frame — the step between the two characters that
    // were already there — so it is the shaper's own answer and not a number written down here.
    let advance = run_advance(&mounted, before);
    assert_eq!(
        after.x,
        before.x + advance,
        "a character was inserted before the caret and the caret was drawn at {} where it was \
         drawn at {} before: the pass that composes fragments skipped it because its own node was \
         never marked, and a run growing does not mark what stands beside it",
        after.x,
        before.x
    );

    // And the frame paid for the move. Both the pixels it left and the pixels it arrived at have
    // to be in the damage, or one of the two is stale on the screen whatever the display list says.
    let (bounds, area) = covered(&frames);
    let bounds = bounds.expect("the frames that followed the keystroke damaged nothing at all");
    assert!(
        bounds.origin.x <= before.x && bounds.origin.x + bounds.size.width >= after.x + advance,
        "the caret moved from {} to {} and the damage {bounds:?} does not span both, so one of \
         the two is on the screen from last frame",
        before.x,
        after.x
    );
    assert!(
        area <= SURFACE_AREA / 20,
        "one character was typed and the burst damaged {area} px of {SURFACE_AREA}"
    );
}

/// The one sprite in `drawn` whose tile no other sprite there shares, which is the caret.
///
/// The run beside it is a repeated character, so every glyph of the run has a twin and the caret
/// does not.
///
/// # Panics
///
/// Panics unless exactly one sprite qualifies, because an assertion about "the caret" made against
/// a frame that drew none, or several, would be an assertion about whichever one came first.
fn sole_caret(drawn: &[Sprite]) -> Sprite {
    let mut unique: Vec<Sprite> = Vec::new();
    for sprite in drawn {
        let shared = drawn
            .iter()
            .filter(|other| other.tile == sprite.tile)
            .count();
        if shared == 1 {
            unique.push(*sprite);
        }
    }
    assert_eq!(
        unique.len(),
        1,
        "the frames drew {} glyphs no other glyph in them repeats, and the caret is supposed to \
         be the only one: {drawn:?}",
        unique.len()
    );
    unique[0]
}

/// The step between two adjacent characters of the run, read out of the frame that drew them.
///
/// # Panics
///
/// Panics unless the run drew at least two characters at distinct positions, because the step is
/// what everything about the caret's new position is measured in and a guess at it would make the
/// assertion pass for a caret that moved by the wrong distance.
fn run_advance(drawn: &[Sprite], caret: Sprite) -> i32 {
    let mut positions: Vec<i32> = drawn
        .iter()
        .filter(|sprite| sprite.tile != caret.tile)
        .map(|sprite| sprite.x)
        .collect();
    positions.sort_unstable();
    positions.dedup();
    assert!(
        positions.len() >= 2,
        "the run drew {} distinct positions, so the step between two of its characters cannot be \
         read from it: {drawn:?}",
        positions.len()
    );
    positions[1] - positions[0]
}

/// A field carried away from where it is laid out, holding one character.
const MOVED_FIELD_CSS: &str = "root { display: block; width: 400px; height: 300px }
                               editor { display: block; width: 200px; height: 40px;
                                        transform: translate(120px, 60px) }";

/// How far that field is carried, in device pixels.
const FIELD_AT: i32 = 120;

#[test]
fn a_blinking_caret_in_a_moved_field_damages_the_pixels_it_is_drawn_on() {
    // A caret is planned against the line's own fragment, whose rectangle is in the space the
    // paragraph was laid out in. Damage is measured in real pixels and in nothing else, so a plan
    // damaged in the paragraph's space clears pixels where no caret is and leaves the caret's own
    // pixels behind — a row of stationary insertion points across a panel that slid, which no
    // border box, no transcript and no display list disagrees with.
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let mut harness = mount(MOVED_FIELD_CSS, &log, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::editor().child("ab"))
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    harness.deliver_to_first(zgui_platform::SurfaceEvent::Key {
        state: zgui_vocab::KeyState::Pressed,
        event: zgui_vocab::KeyEvent::named(
            zgui_vocab::NamedKey::Tab,
            zgui_vocab::PhysicalKey::Code(zgui_vocab::KeyCode::Tab),
        ),
        modifiers: zgui_vocab::Modifiers::NONE,
        timestamp: zgui_vocab::Timestamp::ORIGIN,
    });
    harness.settle(8);

    // Everything from here on is the blink and nothing else: no node changed, no box moved, and
    // the only pixels any frame owes are the ones the caret is drawn on.
    log.borrow_mut().clear();
    for _ in 0..4 {
        harness.advance(core::time::Duration::from_millis(500));
        harness.settle(4);
    }
    let flips: Vec<Rect<i32, Device>> = log
        .borrow()
        .iter()
        .filter(|drawn| !drawn.full)
        .filter_map(|drawn| drawn.bounds)
        .collect();
    assert!(
        !flips.is_empty(),
        "the blink drew no frame with a damage set at all"
    );
    for bounds in &flips {
        assert!(
            bounds.origin.x >= FIELD_AT - 8,
            "the caret is drawn at x={FIELD_AT} and this frame cleared pixels at {bounds:?}, \
             which is where the field would be if the transform above it did not exist"
        );
    }
    harness.shut_down();
}
