//! Whether the frames an incremental engine produces are the frames a thorough one would.
//!
//! Every saving in this area is the engine declining to do something, and declining is how a
//! window comes to show what the previous frame showed. So a saving is checked against the only
//! thing that settles it: the same document, in the same state, laid out by an engine that was
//! given no opportunity to hold anything.
//!
//! # The two documents
//!
//! Two windows are opened on the same view at the same size and fed **the same input, event for
//! event**. One is left alone. The other has every held layout result thrown away immediately
//! before each turn of the loop — [`mark_all_dirty`](zgui_layout::tree::dirty::mark_all_dirty)
//! empties the per-box caches, the baselines and the resolved inline lines, and forgets which
//! viewport the store was last laid out for — so every frame it runs recomputes from the root and
//! it can hold nothing. It is not asked for extra frames: it runs exactly the frames the first one
//! runs, and does all the work inside each.
//!
//! After every step the two are compared on two things:
//!
//! * **geometry** — every fragment's border box, in tree order, to a bit;
//! * **the display list** — the finished scene rendered as a transcript, which is every primitive
//!   in draw order with its paint, clip, transform and geometry resolved through the side tables.
//!
//! A difference in the first is a stale layout; a difference in the second is a stale pixel. The
//! damage is deliberately left out of the transcript, because the two windows are entitled to
//! differ on it: the thorough one repaints what it recomputed.
//!
//! # What the two windows are *not* compared on
//!
//! Two things reach a transcript that are not the picture, and each is dealt with in a module of its
//! own rather than absorbed into a tolerance:
//!
//! * where in the texture atlas a glyph was rasterised, and the sequence that puts the sprites of
//!   one painting order in — [`canon`];
//! * the last few bits of a float, which two engines that reach one position by different routes
//!   never agree on — [`apart`], which classifies every disagreement by what differs and how far,
//!   so a rounding and a stale pixel are never counted with one word.
//!
//! Neither is a threshold on the comparison. Every primitive, paint, clip, transform and position in
//! the painting order is compared exactly.

mod apart;
mod canon;
pub(crate) mod gradient;

pub(crate) use crate::verify::apart::{Apart, GRID};

use zgui::bits::DamageSet;
use zgui::geom::{Device, DevicePx, Rect};
use zgui_platform_headless::Harness;

/// Every fragment's border box, in the store's own order, where the frame put it on the screen.
///
/// Resolved rather than as measured. A fragment's own rectangle is expressed in the coordinate
/// system it was laid out in, and a coordinate system is named by the box that establishes it — so
/// a subtree that is moved, scaled or turned by a transform leaves every rectangle inside it
/// exactly where it was and changes only the matrix that name resolves to. Two windows compared on
/// the unresolved rectangles therefore agree about a document one of them is drawing somewhere
/// else entirely, which is the one thing this half exists to refuse.
pub(crate) fn geometry(window: &zgui::runtime::Window) -> Vec<[u32; 4]> {
    let layout = window.layout().borrow();
    let placements = window.placements();
    resolved(&layout, &placements)
}

/// The same, over a store and a set of answers held apart from any window.
pub(crate) fn resolved(
    layout: &zgui_layout::LayoutStore,
    placements: &zgui::scene::Placements,
) -> Vec<[u32; 4]> {
    let mut out = Vec::new();
    for key in layout.keys() {
        for fragment in layout.fragments_of_box(key) {
            let Some(held) = layout.fragment(*fragment) else {
                continue;
            };
            let border: Rect<DevicePx, Device> =
                zgui_layout::fragment::transform::placed::onto_device(
                    held.border_box,
                    held.transform,
                    placements,
                );
            out.push([
                border.origin.x.0.to_bits(),
                border.origin.y.0.to_bits(),
                border.size.width.0.to_bits(),
                border.size.height.0.to_bits(),
            ]);
        }
    }
    out
}

/// The finished display list as text, with the damage left out.
pub(crate) fn transcript(window: &zgui::runtime::Window) -> String {
    zgui_testkit_scene::transcript::of(window.scene(), &DamageSet::default()).into_string()
}

/// Throws away everything a window is holding about where things are.
///
/// Two things, and the second one is not an extra: every held layout result, so the next frame
/// recomputes from the root — and the index that answers what is under a point, which is kept up to
/// date one entry at a time by the pass that writes the fragments and would otherwise be the one
/// thing the thorough window carries over from every frame it has ever run. A comparison whose cold
/// side holds an index built incrementally since the window opened is comparing two incremental
/// indexes.
pub(crate) fn forget(harness: &mut Harness<zgui::runtime::Runtime>) -> u32 {
    let window = &mut harness.app_mut().windows_mut()[0];
    window.forget_hit_index();
    zgui_layout::tree::dirty::mark_all_dirty(&mut window.layout().borrow_mut())
}

/// Pumps a harness to quiet, forgetting every held result before each turn.
///
/// Forgetting once and settling would leave every frame after the first one incremental, inside a
/// comparison that claims to have none. Nothing extra is *asked* for: the loop still decides when
/// to draw, so the thorough window runs the frames the other one runs and does all the work inside
/// each. Quiet is two consecutive turns that drew nothing, because a turn that draws nothing may
/// still have queued the wake that the next one draws for.
pub(crate) fn settle_cold(harness: &mut Harness<zgui::runtime::Runtime>, turns: u32) -> u64 {
    let mut frames = 0;
    let mut quiet = 0;
    for _ in 0..turns {
        forget(harness);
        let ran = harness.pump();
        frames += ran;
        quiet = if ran == 0 { quiet + 1 } else { 0 };
        if quiet == 2 {
            break;
        }
    }
    frames
}

/// Carries the clock forward to the moment the window owes nothing at all.
///
/// # What "settled" has to mean for a comparison
///
/// Pumping to quiet is not enough, because a window at quiet can still be *owed*: a transition
/// half-way through its duration and a caret half-way through a blink both leave a deadline behind
/// and no work to do until it arrives. Two windows compared there are two windows caught at
/// whatever point of a movement each of them reached, and the difference reads as a colour that is
/// wrong rather than as one still on its way.
///
/// A caret makes that worse than a knife-edge: it flips every half second for ten seconds and then
/// settles on, so any fixed number of frames either lands on a flip — where the answer is decided
/// by which side of the boundary each window's blink started on — or stops before the settling and
/// leaves the answer to the phase. So the clock is carried to each owed moment in turn until none
/// is owed, which is the one state both windows can be in at once.
pub(crate) fn run_down(harness: &mut Harness<zgui::runtime::Runtime>, cold: bool) {
    // One half period of the blink per turn, for longer than the blinking lasts: every flip is an
    // edge the clock crosses, and after the last of them the caret is on for good in both windows.
    // Longer than every transition the tokens declare, by a wide margin, as a side effect.
    let trace = std::env::var_os("REPAINT_TRACE").is_some();
    let mut was = trace.then(|| geometry(&harness.app().windows()[0]));
    for turn in 0..RUN_DOWN_TURNS {
        harness.advance(zgui::runtime::caret::blink::HALF_PERIOD);
        if cold {
            forget(harness);
        }
        harness.pump();
        if let Some(before) = &was {
            let now = geometry(&harness.app().windows()[0]);
            let moved = before
                .iter()
                .zip(now.iter())
                .filter(|(one, two)| one != two)
                .count()
                + before.len().abs_diff(now.len());
            if moved != 0 {
                eprintln!(
                    "    RUN_DOWN turn {turn}: {moved} moved, {} fragments",
                    now.len()
                );
            }
            was = Some(now);
        }
    }
}

/// How many blink half-periods [`run_down`] carries the clock through.
///
/// The caret blinks for a fixed number of phases and then settles, so this is that number with room
/// to spare rather than a duration chosen to look long.
const RUN_DOWN_TURNS: u32 = zgui::runtime::caret::blink::PHASES + 8;

/// Damages the whole surface without changing a thing in the document, and settles what follows.
///
/// # Why a comparison cannot be taken without this
///
/// The emit walk is gated on damage: a frame emits what the damage reaches and nothing else, and
/// the renderer keeps the rest of the target from the frame before. So a window's display list is
/// not *what is on the screen* — it is what the last frame it happened to run redrew. Two windows
/// that ran a different number of frames therefore hold different fractions of the same picture,
/// and comparing those fractions compares which frames ran rather than what they drew.
///
/// The two windows here *always* run a different number of frames: the thorough one recomputes its
/// layout before every turn, which buys frames the incremental one never asks for. Left alone, that
/// makes the display-list half of the comparison read whichever window last happened to redraw
/// something — and, when the last frame on both sides drew nothing at all, makes it compare two
/// empty lists and report agreement.
///
/// Coming back from occlusion is the one event that damages every pixel while changing no input to
/// any of them: nothing reflows, no string is shaped again, and every glyph is drawn through the
/// slot it named when it was shaped. So both windows are put through it before every comparison,
/// and both then hold the whole document, emitted from whatever each of them is holding. The frame
/// that answers it is paced — offered once, declined while the reconfiguration it owes could not
/// yet have been seen, and asked for again by the deadline — so the clock is moved past that
/// deadline before the frame the comparison reads is expected to have run.
pub(crate) fn repaint_everything(harness: &mut Harness<zgui::runtime::Runtime>, cold: bool) {
    let settle = |harness: &mut Harness<zgui::runtime::Runtime>| {
        if cold {
            settle_cold(harness, 96);
        } else {
            harness.settle(96);
        }
    };
    let trace = std::env::var_os("REPAINT_TRACE").is_some();
    let mut was = geometry(&harness.app().windows()[0]);
    let mut note = |stage: &str, harness: &Harness<zgui::runtime::Runtime>| {
        if !trace {
            return;
        }
        let now = geometry(&harness.app().windows()[0]);
        let moved = was
            .iter()
            .zip(now.iter())
            .filter(|(one, two)| one != two)
            .count()
            + was.len().abs_diff(now.len());
        eprintln!(
            "    REPAINT TRACE {stage}: {moved} moved, {} fragments",
            now.len()
        );
        was = now;
    };
    run_down(harness, cold);
    note("run_down", harness);
    harness.deliver_to_first(zgui::platform::SurfaceEvent::Occluded(true));
    settle(harness);
    note("occluded", harness);
    harness.deliver_to_first(zgui::platform::SurfaceEvent::Occluded(false));
    settle(harness);
    note("shown", harness);
    harness.advance(std::time::Duration::from_millis(50));
    settle(harness);
    note("settled", harness);
}

/// The display list of the last frame one window drew against the whole surface.
///
/// A window's *current* scene is not this: it is whatever its last frame emitted, against whatever
/// damage that frame was built for, and the frame that answers a full repaint is routinely followed
/// by a small one that overwrites it. So the list is taken where it is drawn — inside the renderer,
/// after the scene is finished and before anything else can touch it — and only for the frames that
/// were drawn against every pixel, which are the only frames that hold the whole document.
#[derive(Default)]
pub(crate) struct FullFrame {
    /// Whether the renderer keeps the list. Off for the measuring phases, which must not pay for
    /// rendering a transcript on every frame they are timing.
    pub(crate) wanted: std::cell::Cell<bool>,
    /// The last one kept.
    pub(crate) last: std::cell::RefCell<Option<String>>,
}

impl FullFrame {
    /// Keeps `scene` if it was drawn against the whole surface and anyone asked for it.
    pub(crate) fn observe(&self, scene: &zgui::scene::Scene, damage: &DamageSet) {
        if self.wanted.get() && damage.is_full() {
            *self.last.borrow_mut() = Some(
                zgui_testkit_scene::transcript::of(scene, &DamageSet::default()).into_string(),
            );
        }
    }

    /// The list kept, and nothing if no such frame has been drawn since it was last emptied.
    pub(crate) fn taken(&self) -> Option<String> {
        self.last.borrow_mut().take()
    }
}

/// A renderer that keeps the display list of every frame drawn against the whole surface.
///
/// Wrapping rather than a field on each renderer, because a window is handed one renderer and a
/// comparison needs the list whichever renderer that is — the device's, the device's under a
/// readback, or the one that draws nothing at all.
pub(crate) struct Listed {
    /// The renderer that does the work.
    pub(crate) inner: Box<dyn zgui::render::Renderer>,
    /// Where a full frame's list is left.
    pub(crate) full: std::rc::Rc<FullFrame>,
}

impl zgui::render::Renderer for Listed {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: zgui::render::RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<zgui::render::RenderTarget> {
        self.inner.target()
    }

    fn draw(
        &mut self,
        scene: &zgui::scene::Scene,
        damage: &DamageSet,
    ) -> zgui::render::FrameOutcome {
        self.full.observe(scene, damage);
        self.inner.draw(scene, damage)
    }

    fn register_external(
        &mut self,
        texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: zgui::render::TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> zgui::render::MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.inner.texture_sink()
    }
}

/// What one comparison found.
pub(crate) struct Disagreement {
    /// Which step of the script it was.
    pub(crate) step: usize,
    /// What the step was.
    pub(crate) what: String,
    /// How many fragment rectangles differ.
    pub(crate) geometry: usize,
    /// The first differing rectangle, spelled out.
    pub(crate) first_box: Option<String>,
    /// How many transcript lines differ.
    pub(crate) lines: usize,
    /// The first differing line pair.
    pub(crate) first_line: Option<String>,
    /// What separates the two: a distance, or the picture itself.
    pub(crate) apart: Apart,
}

impl Disagreement {
    /// Whether this is float arithmetic taking two routes to one place rather than a difference.
    ///
    /// See [`GRID`] for the threshold and for what it is a threshold on.
    pub(crate) fn is_rounding(&self) -> bool {
        self.apart.is_rounding()
    }
}

impl std::fmt::Display for Disagreement {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "step {} ({}): apart by {}, {} rects differ [{}], {} transcript lines differ [{}]",
            self.step,
            self.what,
            self.apart,
            self.geometry,
            self.first_box.as_deref().unwrap_or("-"),
            self.lines,
            self.first_line.as_deref().unwrap_or("-"),
        )
    }
}

/// Compares one step of the two documents, reporting nothing when they agree.
///
/// `lists` are the two display lists to hold against each other, which are the ones the windows
/// drew when they were last asked for every pixel rather than whatever each is holding now.
pub(crate) fn compare(
    step: usize,
    what: &str,
    live: &zgui::runtime::Window,
    cold: &zgui::runtime::Window,
    lists: (&str, &str),
) -> Option<Disagreement> {
    compare_readings(step, what, &geometry(live), &geometry(cold), lists)
}

/// The same comparison, over readings taken elsewhere.
///
/// The two halves are separated here so that either of them can be held against a *deliberately*
/// wrong reading — an oracle whose subject has changed shape is an oracle that may have stopped
/// looking at anything, and the only way to find out is to break what it guards and watch it say
/// so.
pub(crate) fn compare_readings(
    step: usize,
    what: &str,
    left: &[[u32; 4]],
    right: &[[u32; 4]],
    lists: (&str, &str),
) -> Option<Disagreement> {
    let mut apart = apart::TOGETHER;
    let mut differing = left.len().abs_diff(right.len());
    let mut first_box = (left.len() != right.len())
        .then(|| format!("fragment count {} vs {}", left.len(), right.len()));
    if left.len() != right.len() {
        apart = Apart::Shape;
    }
    for (index, (one, two)) in left.iter().zip(right.iter()).enumerate() {
        if one != two {
            differing += 1;
            apart = apart.or_worse(apart::between_rects(one, two));
            if first_box.is_none() {
                first_box = Some(format!(
                    "#{index} live {:?} cold {:?}",
                    one.map(f32::from_bits),
                    two.map(f32::from_bits)
                ));
            }
        }
    }

    // Both lists in the one form two windows can be held against each other in: see [`canon`] for
    // what that takes out and why taking it out is not a tolerance.
    let (one, two) = (canon::of(lists.0), canon::of(lists.1));
    let (one, two) = (one.as_str(), two.as_str());
    let mut lines = 0;
    let mut first_line = None;
    for (index, pair) in one.lines().zip(two.lines()).enumerate() {
        if pair.0 != pair.1 {
            lines += 1;
            if first_line.is_none() {
                first_line = Some(format!("line {index}: live {:?} cold {:?}", pair.0, pair.1));
            }
        }
    }
    let (count_one, count_two) = (one.lines().count(), two.lines().count());
    if count_one != count_two {
        lines += count_one.abs_diff(count_two);
        first_line.get_or_insert(format!("line count {count_one} vs {count_two}"));
    }
    if lines != 0 {
        apart = apart.or_worse(apart::between(one, two));
    }

    // A first differing line names the primitive but not what surrounds it, and what surrounds it
    // is how the element it belongs to is identified. Setting `VERIFY_PAIR` to a directory leaves
    // both lists whole, in the canonical form they were compared in, so a disagreement can be read
    // with everything around it.
    if (differing != 0 || lines != 0)
        && let Some(directory) = std::env::var_os("VERIFY_PAIR")
    {
        let directory = std::path::PathBuf::from(directory);
        for (name, held) in [("live", one), ("cold", two)] {
            let path = directory.join(format!("step{step:02}-{name}.txt"));
            std::fs::write(&path, held).expect("the display list is written");
        }
    }

    (differing != 0 || lines != 0).then(|| Disagreement {
        step,
        what: what.to_owned(),
        geometry: differing,
        first_box,
        lines,
        first_line,
        apart,
    })
}

/// What one frame was drawn against: whether the whole surface, and the rectangles otherwise.
pub(crate) type Drawn = (bool, Vec<zgui::geom::Rect<i32, Device>>);

/// The pixels a window last presented, kept for a comparison to read.
///
/// A window owns its renderer, so the only place a readback can be taken from is inside the
/// renderer itself — after the draw and before anything else can touch the target.
#[derive(Default)]
pub(crate) struct Capture {
    /// Whether the next frame should be read back, which is off by default because a readback is
    /// a full device synchronisation and a measurement must not pay for one.
    pub(crate) want: std::cell::Cell<bool>,
    /// What the last frame that was asked for presented.
    pub(crate) last: std::cell::RefCell<Option<zgui_render_wgpu::Pixels>>,
    /// The persistent target that frame was composed into, which is the one a partial repaint
    /// accumulates in and the one a stale pixel would survive in.
    pub(crate) composed: std::cell::RefCell<Option<zgui_render_wgpu::Pixels>>,
    /// Every damage set drawn against since the last time the list was emptied.
    pub(crate) damages: std::cell::RefCell<Vec<Drawn>>,
    /// The display list of every frame drawn against the whole surface.
    pub(crate) full_lists: std::cell::RefCell<Vec<String>>,
    /// Whether the list of *every* frame is kept, not only the ones drawn against every pixel.
    ///
    /// A step is a sequence of frames and a displacement is introduced by one of them, so which one
    /// is the question a full-damage frame on its own cannot answer. Off by default because it
    /// renders a transcript per frame.
    pub(crate) transcribe: std::cell::Cell<bool>,
    /// Every frame's list since the run was last emptied, with what it was drawn against.
    pub(crate) every_list: std::cell::RefCell<Vec<(String, String)>>,
}

/// This machine's device, with a readback taken whenever one is asked for.
pub(crate) struct Recorded {
    /// The renderer that does the work.
    pub(crate) inner: zgui_render_wgpu::WgpuRenderer,
    /// Where a readback is left.
    pub(crate) capture: std::rc::Rc<Capture>,
}

impl zgui::render::Renderer for Recorded {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: zgui::render::RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<zgui::render::RenderTarget> {
        self.inner.target()
    }

    fn draw(
        &mut self,
        scene: &zgui::scene::Scene,
        damage: &DamageSet,
    ) -> zgui::render::FrameOutcome {
        self.capture
            .damages
            .borrow_mut()
            .push((damage.is_full(), damage.rects().to_vec()));
        if damage.is_full() {
            self.capture
                .full_lists
                .borrow_mut()
                .push(zgui_testkit_scene::transcript::of(scene, damage).into_string());
        }
        if self.capture.transcribe.get() {
            let against = if damage.is_full() {
                "full".to_owned()
            } else {
                format!("{:?}", damage.rects())
            };
            self.capture.every_list.borrow_mut().push((
                against,
                zgui_testkit_scene::transcript::of(scene, &DamageSet::default()).into_string(),
            ));
        }
        let outcome = self.inner.draw(scene, damage);
        if self.capture.want.get() {
            *self.capture.last.borrow_mut() = self.inner.read_presented();
            *self.capture.composed.borrow_mut() = Some(self.inner.read_composed());
        }
        outcome
    }

    fn register_external(
        &mut self,
        texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: zgui::render::TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> zgui::render::MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.inner.texture_sink()
    }
}

#[cfg(test)]
mod tests {
    use zgui::geom::Matrix4;
    use zgui::scene::{Placements, PropertyOwner, SpatialId, SpatialTree};

    use super::{compare_readings, resolved};

    /// A tree occupying the same slots as `tree`, with the same node under each of them.
    ///
    /// Built so that a coordinate system can be moved without moving the window's own: a slot
    /// number and an occupancy counter are what a reading resolves a name through, so a copy that
    /// fills the same slots in the same order answers every name the drawn frame's fragments
    /// carry. The assertion inside says so rather than assuming it.
    fn mirror(tree: &SpatialTree) -> SpatialTree {
        let mut out = SpatialTree::new(tree.domain());
        for (slot, held) in tree.slots().enumerate() {
            let id = held.expect("every slot of a settled window's tree is occupied");
            let node = *tree.get(id).expect("a live name resolves to a node");
            let owner = PropertyOwner::new(slot as u64 + 1).expect("a word that is not empty");
            assert_eq!(
                out.establish(owner, node),
                id,
                "the copy takes the slot and the counter of what it copies",
            );
        }
        out
    }

    /// How many coordinate systems `id` is expressed through, itself included.
    fn depth(tree: &SpatialTree, id: SpatialId) -> usize {
        tree.fold_up(id, 0, |so_far, _| so_far + 1)
            .expect("a live name resolves to the root")
    }

    /// Every fragment's rectangle as the store holds it, which is what the geometry half read
    /// before it resolved anything.
    fn as_measured(layout: &zgui_layout::LayoutStore) -> Vec<[u32; 4]> {
        resolved(layout, &Placements::EMPTY)
    }

    #[test]
    fn a_perturbed_spatial_node_fails_the_geometry_half() {
        // What this is for. The geometry half compares a rectangle per fragment, and the fragments
        // themselves stopped being where a person sees them the moment a subtree could be moved by
        // rewriting a matrix under a name. A half that went on reading the store would agree about
        // two documents drawn a pixel apart — and would agree about every document, for ever,
        // without a single assertion failing.
        let harness = crate::drive::opened("s0");
        let window = &harness.app().windows()[0];
        let layout = window.layout().borrow();
        let live = {
            let placements = window.placements();
            resolved(&layout, &placements)
        };
        assert!(!live.is_empty(), "the window laid out something to compare");

        // The control: a copy of the drawn frame's coordinate systems answers exactly what the
        // drawn frame's own answers do. Without it a difference below could be the copy rather
        // than the perturbation.
        let mut moved = mirror(&window.scene().spatial);
        assert_eq!(
            resolved(&layout, &Placements::of(&moved)),
            live,
            "the copy resolves every name the window's own answers resolve",
        );

        // One coordinate system, moved one device pixel sideways: the deepest, so the mutation is
        // the smallest one the tree admits rather than the whole document at once.
        let systems = moved.len();
        let deepest = moved
            .ids()
            .max_by_key(|id| (depth(&moved, *id), id.index()))
            .expect("a settled window has at least the coordinate system it draws in");
        let under = depth(&moved, deepest);
        let node = moved.get_mut(deepest).expect("a live name");
        node.local = node.local.then(&Matrix4::translation(1.0, 0.0, 0.0));

        let before = as_measured(&layout);
        let perturbed = resolved(&layout, &Placements::of(&moved));
        assert_eq!(
            as_measured(&layout),
            before,
            "moving a coordinate system moves no fragment's own rectangle, which is exactly why a \
             half that reads those rectangles cannot see this",
        );

        let found = compare_readings(
            0,
            "one coordinate system moved by a pixel",
            &live,
            &perturbed,
            ("", ""),
        )
        .unwrap_or_else(|| {
            panic!(
                "a coordinate system {under} deep, of {systems} in the document, was moved one \
                 device pixel and the comparison saw nothing at all across {} fragments — the \
                 geometry half is not resolving anything and would agree with a window drawing \
                 the whole document somewhere else",
                live.len(),
            )
        });
        assert!(
            found.geometry > 0,
            "the geometry half is what has to see this: {found}",
        );
        assert_eq!(
            found.lines, 0,
            "and it is the geometry half rather than the display list that saw it: {found}",
        );
    }
}
