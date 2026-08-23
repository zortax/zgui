//! The emit walk: the document in painting order, gated on damage.
//!
//! # What gates a fragment, and what does not
//!
//! Not a dirty bit. The renderer clears each damage rectangle before redrawing it, so everything
//! intersecting one has to be emitted whether it changed or not — a clean row sitting under a
//! repainting tooltip is emitted, and a dirty fragment nowhere near any rectangle cannot exist,
//! because whatever made it dirty put its ink in the set.
//!
//! So the test is intersection, at two granularities. A whole subtree whose union of ink misses the
//! damage is skipped in constant time, which is what makes a hover on a thousand-row table cost the
//! rows near the pointer; and a fragment whose own cull rectangle misses it is skipped while its
//! children are still visited, because a child can paint outside its parent.
//!
//! Two things are never skipped. A group's markers are matched pairs — dropping one leaves a target
//! open, or composites one that was never begun — and neither is anything when the damage says the
//! whole surface is being redrawn.

pub mod decorate;
pub mod fill;
pub mod order;
pub mod replay;
pub mod stacking;

use std::borrow::Cow;

use zgui_atlas::AtlasKey;
use zgui_bits::DamageSet;
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_layout::fragment::FragmentFlags;
use zgui_layout::fragment::diff::pixels;
use zgui_layout::{FragKey, Fragment, FragmentKind, LayoutStore};
use zgui_profile::{Counter, counter};
use zgui_render::RenderCapabilities;
use zgui_scene::{GroupBoundary, Scene};

use crate::content::vectors::{NoVectorMasks, NoVectors, VectorMaskSource, VectorSource};
use crate::emit::group::{self, Isolation};
use crate::emit::highlight::{HighlightSource, NoHighlights};
use crate::emit::scrollbar::ScrollbarPaint;
use crate::emit::text::{GlyphPlacementSource, GlyphSource, TextPlacement};
use crate::emit::{BoxPlacement, NoGlyphs};
use zgui_dom::NodeKey;
use zgui_dom::side::AnimOverride;

use crate::lower::anim::{AnimOverrides, NoAnim};
use crate::lower::cache::{PaintStyleCache, PaintStyleRef};
use crate::walk::decorate::Decorations;
use crate::walk::fill::TextFills;
use crate::walk::order::{Emission, NoReplaced, ReplacedSource};
use crate::walk::replay::hold::{NoResources, ResourceOwner};
use crate::walk::replay::{Encoding, PaintCache, Painted, Reuse};

pub use crate::walk::order::{NoReplaced as NoReplacedContent, ReplacedSource as ReplacedContent};

/// Everything one emit walk reads.
pub struct PaintInput<'a> {
    /// The fragment tree, read and never written: this walk is a pure reader.
    pub store: &'a LayoutStore,
    /// The frozen damage set the walk is gated on.
    pub damage: &'a DamageSet,
    /// Where glyphs come from.
    pub glyphs: &'a dyn GlyphSource,
    /// Where a run a custom element shaped itself gets its tiles.
    pub glyph_placements: &'a dyn GlyphPlacementSource,
    /// Where the caret and the selection bands drawn with a line come from.
    pub highlights: &'a dyn HighlightSource,
    /// Where replaced content comes from.
    pub replaced: &'a dyn ReplacedSource,
    /// Where the outlines an element draws come from.
    pub vectors: &'a dyn VectorSource,
    /// Where eligible small solid vector shapes get their monochrome coverage masks.
    pub vector_masks: &'a dyn VectorMaskSource,
    /// Where a custom element's painting comes from.
    pub custom: &'a dyn crate::content::custom::CustomPaintSource,
    /// The cache a recorded range's rasters live in, which the record holds them in.
    ///
    /// Separate from [`PaintInput::glyphs`] and [`PaintInput::replaced`] even though one object
    /// usually answers all three: those two are asked *for* content while the walk runs, and this
    /// one is told what to keep afterwards. A source that has no atlas behind it — a fixture, a
    /// document with no text — passes [`NoResources`] and records nothing to keep.
    pub resources: &'a dyn ResourceOwner,
    /// What each node's running animations are currently overriding.
    ///
    /// Asked per box rather than folded into the lowering, because a lowering is shared between
    /// every element that cascaded to the same style and an animated value is not.
    pub anim: &'a dyn AnimOverrides,
    /// What the device can do, which decides whether text can be antialiased per colour channel.
    pub capabilities: RenderCapabilities,
    /// Whether the surface being drawn into is opaque.
    pub opaque_surface: bool,
    /// How many device pixels one CSS pixel is.
    pub scale: f32,
    /// How a scrollbar is painted.
    pub scrollbars: ScrollbarPaint,
    /// Whether to record which fragments were emitted, for the completeness oracle.
    ///
    /// Off by default, because a frame that is not being audited should not pay a push per
    /// fragment. A caller that wants [`PaintReport::assert_emission_complete`] turns it on, and
    /// that assertion refuses to run when it is off rather than passing over an empty list.
    ///
    /// It is deliberately not derived from the build profile. A recording that vanished in release
    /// would take the oracle with it, so the release tree would be asserting nothing where the
    /// debug tree asserts the whole thing.
    pub record_emitted: bool,
    /// Whether every replayed range is checked against the cache its rasters live in.
    ///
    /// The check is what catches a range drawing a raster that has been freed underneath it — see
    /// [`replay::hold`]. It costs a lookup per distinct raster per replayed fragment, which is a
    /// per-frame cost proportional to what the frame *saved*, so it is off unless something asks
    /// for it: [`PaintInput::new`] takes its default from
    /// [`zgui_layout::invariants::enabled`], and a test that is about the check sets it directly.
    pub verify_replays: bool,
    /// The drawn frame's resolved matrices, for putting a line's marks where they are drawn.
    ///
    /// The caret and selection rectangles a line reports are measured in the line's own space,
    /// while the damage they are culled against is measured on the device — see
    /// [`marks::extent`](crate::damage::marks::extent). A caller with no composed frame yet passes
    /// nothing and the marks stay untransformed, which is exact for everything upright.
    pub placements: Option<&'a zgui_scene::Placements>,
}

impl<'a> PaintInput<'a> {
    /// An input over `store` and `damage`, with no text, no content of any kind and a minimal
    /// device.
    ///
    /// This is what a caller that is not testing text, images or drawings wants: everything a walk
    /// needs, and nothing it has to pretend to have.
    pub fn new(store: &'a LayoutStore, damage: &'a DamageSet) -> Self {
        Self {
            store,
            damage,
            glyphs: &NoGlyphs,
            glyph_placements: &crate::emit::text::NoGlyphPlacements,
            highlights: &NoHighlights,
            replaced: &NoReplaced,
            vectors: &NoVectors,
            vector_masks: &NoVectorMasks,
            custom: &crate::content::custom::NoCustom,
            resources: &NoResources,
            anim: &NoAnim,
            capabilities: RenderCapabilities::MINIMAL,
            opaque_surface: true,
            scale: 1.0,
            scrollbars: crate::emit::scrollbar::default_paint(),
            record_emitted: false,
            verify_replays: zgui_layout::invariants::enabled(),
            placements: None,
        }
    }
}

/// What one emit walk did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaintReport {
    /// How many primitives reached the display list.
    pub primitives: usize,
    /// Which fragments contributed, when the walk was asked to record them.
    pub emitted: Vec<FragKey>,
    /// How many groups were opened.
    pub groups: usize,
    /// How many subtrees the damage test skipped whole.
    pub skipped_subtrees: usize,
    /// Whether [`PaintInput::record_emitted`] was on, so [`PaintReport::emitted`] is the whole list
    /// rather than an empty one nobody filled.
    pub recorded: bool,
    /// Vector routes freshly encoded for elements this frame.
    ///
    /// A route-less entry is meaningful: a vector or custom element was encoded and emitted no
    /// vector shape, so a retained diagnostic for its previous content must be cleared.
    pub vector_routes: Vec<VectorRouteReport>,
}

/// The vector raster paths selected for one freshly encoded element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VectorRouteReport {
    /// The element whose own fragment selected the routes.
    pub node: NodeKey,
    /// All routes selected by its shapes.
    pub routes: crate::emit::vector::VectorRoutes,
}

impl PaintReport {
    /// Whether `frag` contributed to this frame.
    pub fn emitted(&self, frag: FragKey) -> bool {
        self.emitted.contains(&frag)
    }

    /// Fails if any fragment whose ink meets the damage was not emitted.
    ///
    /// This is the direct test of the expansion's placement, and a damage-rectangle assertion
    /// structurally cannot make it: moving the expansion after this walk leaves the damage set
    /// looking exactly as it should while the fragments inside the newly added region were never
    /// visited. It compares the two answers instead.
    ///
    /// # Panics
    ///
    /// Panics naming the first fragment the damage reaches that nothing emitted for, and panics
    /// outright if the walk was not asked to record what it emitted — an oracle run against a list
    /// nobody filled would pass over every document there is.
    pub fn assert_emission_complete(&self, store: &LayoutStore, damage: &DamageSet) {
        assert!(
            self.recorded,
            "the walk was not asked to record what it emitted, so this would assert nothing: set \
             `PaintInput::record_emitted`"
        );
        for key in store.keys() {
            for frag in store.fragments_of_box(key) {
                let Some(fragment) = store.fragment(*frag) else {
                    continue;
                };
                if fragment.ink.is_empty() {
                    continue;
                }
                let reached = damage.is_full() || damage.intersects(pixels(fragment.ink));
                assert!(
                    !reached || self.emitted(*frag),
                    "the damage reaches {frag:?} at {:?} and nothing emitted for it",
                    fragment.ink
                );
            }
        }
    }
}

/// The paint stage's state between frames: the lowered styles, and what each fragment painted.
#[derive(Debug, Default)]
pub struct Painter {
    /// Lowered styles, held so a thousand identical buttons lower a handful of them.
    styles: PaintStyleCache,
    /// What each fragment painted last time, held so an unchanged one replays instead of encoding.
    cache: PaintCache,
}

impl Painter {
    /// A painter with nothing lowered and nothing recorded.
    pub fn new() -> Self {
        Self::default()
    }

    /// The lowered styles, for a caller asking how many distinct ones a document has.
    pub fn styles(&self) -> &PaintStyleCache {
        &self.styles
    }

    /// The per-fragment record, for a caller asking how much of a frame was replayed.
    pub fn cache(&self) -> &PaintCache {
        &self.cache
    }

    /// Discards everything held between frames.
    ///
    /// Legal only beside emptying the caches the records hold into — a lost device, a forgotten
    /// window. The records' atlas holds die with the atlas rather than being released one by one,
    /// and the next frame must redraw the whole surface, because nothing recorded survives.
    pub fn reset(&mut self) {
        self.styles.clear();
        self.cache.clear();
    }

    /// The report a document with nothing in it produces.
    ///
    /// Stated rather than left to be inferred from an empty walk, so that a caller can tell "there
    /// was nothing to draw" from "the walk did not run".
    pub fn report_of_nothing(&self) -> PaintReport {
        PaintReport::default()
    }

    /// Walks the document in painting order and emits everything the damage reaches.
    pub fn emit(&mut self, input: &PaintInput<'_>, scene: &mut Scene) -> PaintReport {
        let Some(root) = input.store.root() else {
            return PaintReport {
                recorded: input.record_emitted,
                ..self.report_of_nothing()
            };
        };
        self.cache.begin_frame();
        let mut pass = Pass {
            painter: self,
            input,
            scene,
            report: PaintReport {
                recorded: input.record_emitted,
                ..PaintReport::default()
            },
            open: Vec::new(),
            bands: Vec::new(),
            alpha: Vec::new(),
            opaque: Vec::new(),
            decorations: Decorations::new(),
            text_fills: TextFills::new(),
            named: Vec::new(),
        };
        stacking::walk(input.store, root, &mut pass);
        let report = pass.report;
        self.cache.end_frame();
        report
    }

    /// Drops the records of fragments the layout store destroyed, releasing what they held.
    ///
    /// Called once per painted frame, before [`Painter::emit`], with the drained retirement list.
    /// Together with the budget's eviction this is what removes a record: a fragment's painting
    /// lives as long as the fragment, or until memory pressure takes it as a clean miss.
    pub fn retire(
        &mut self,
        keys: &[zgui_layout::FragKey],
        scene: &mut Scene,
        owner: &dyn ResourceOwner,
    ) {
        self.cache.retire(keys, scene, owner);
    }

    /// Notes every record's chunk into the scene again, for a renderer that lost its residence.
    pub fn renote_chunks(&self, scene: &mut Scene) {
        self.cache.renote_chunks(scene);
    }

    /// Drops the coldest records until `bytes` chunk bytes have gone, and reports how many went.
    pub fn evict_cold_chunks(
        &mut self,
        bytes: u64,
        scene: &mut Scene,
        owner: &dyn ResourceOwner,
    ) -> u64 {
        self.cache.evict_cold(bytes, scene, owner)
    }

    /// Drops every record, releasing everything each one held into caches that survive.
    pub fn clear_records(&mut self, scene: &mut Scene, owner: &dyn ResourceOwner) {
        self.cache.clear_releasing(scene, owner);
    }
}

/// One emit walk in progress.
struct Pass<'a, 'b> {
    /// The state that outlives the frame.
    painter: &'a mut Painter,
    /// What the walk reads.
    input: &'a PaintInput<'b>,
    /// What it writes.
    scene: &'a mut Scene,
    /// What it did.
    report: PaintReport,
    /// The groups currently open, innermost last, with the box that opened each.
    open: Vec<(BoxKey, GroupBoundary)>,
    /// The boxes whose order band is open, innermost last.
    ///
    /// A box that is going to be moved by writing its coordinate system is ordered against the
    /// whole region its movement visits rather than against where it happens to be, because the
    /// write happens after this walk has assigned every order. Opened as the box is entered and
    /// closed as it is left, like the group stack beside it and for the same reason.
    bands: Vec<BoxKey>,
    /// The folded alpha in force, innermost last.
    alpha: Vec<f32>,
    /// Whether the target in force is opaque, innermost last.
    opaque: Vec<bool>,
    /// The text decorations contributed by the boxes still on the walk.
    decorations: Decorations,
    /// The ramps contributed by the boxes still on the walk, for the text inside them.
    text_fills: TextFills,
    /// The rasters the fragment currently being encoded has named, reused between fragments.
    named: Vec<AtlasKey>,
}

impl Pass<'_, '_> {
    /// The alpha every colour is multiplied by right now.
    fn alpha(&self) -> f32 {
        self.alpha.iter().product::<f32>().clamp(0.0, 1.0)
    }

    /// Whether the target being drawn into right now is opaque.
    fn opaque(&self) -> bool {
        *self.opaque.last().unwrap_or(&self.input.opaque_surface)
    }

    /// The union of every fragment of `key`'s subtree ink, which is what the subtree skip tests.
    ///
    /// The marks drawn *with* a line are part of that union, for the reason they are part of the
    /// per-fragment cull below: a field somebody has just emptied has one line box of no width, so
    /// the ink of its whole subtree is a rectangle no damage can ever intersect. Everything under it
    /// is then skipped in one step — before the per-fragment cull that does know about the caret is
    /// reached — and the field has no insertion point in it until something else damages the same
    /// pixels. Which is what typing does, and is why such a field looks like one whose caret comes
    /// back the moment a letter goes in.
    fn subtree_ink(&self, key: BoxKey) -> Option<Rect<DevicePx, Device>> {
        let mut held: Option<Rect<DevicePx, Device>> = None;
        for frag in self.input.store.fragments_of_box(key) {
            let Some(fragment) = self.input.store.fragment(*frag) else {
                continue;
            };
            let mut ink = self.where_drawn(fragment.subtree_ink, fragment.transform);
            if let Some(marks) = crate::damage::marks::extent(
                fragment,
                self.input.highlights,
                self.input.scale,
                self.input.placements,
            ) {
                ink = ink.union(marks);
            }
            held = Some(match held {
                Some(union) => union.union(ink),
                None => ink,
            });
        }
        held
    }

    /// What `key`'s element is currently animating, if anything.
    ///
    /// An anonymous box has no element and therefore no animation of its own; it inherits the
    /// composited alpha of the box above it exactly as it inherits everything else.
    fn anim_of(&self, store: &LayoutStore, key: BoxKey) -> Option<&AnimOverride> {
        self.input.anim.get(store.get(key)?.source?)
    }

    /// Whether the damage reaches `rect`.
    fn reaches(&self, rect: Rect<DevicePx, Device>) -> bool {
        self.input.damage.is_full() || self.input.damage.intersects(pixels(rect))
    }

    /// A rectangle widened to cover where its fragment is actually drawn.
    ///
    /// A fragment's ink is put on the device by the matrix the *build* resolved, and an animation
    /// that moves a box without refragmenting it — a dialog holding the placement its entrance
    /// settled on — moves the drawing without moving the ink. The cull then reads rectangles from
    /// where the box was built while the damage holds rectangles from where it is drawn, and a
    /// line whose two readings drift apart is erased by the clear and skipped by the walk: a
    /// dialog whose text vanishes under the pointer and comes back with every resize. The union
    /// of both readings is taken rather than a choice between them, because for a transform the
    /// build did resolve the ink is already on the device and re-placing it would move it away —
    /// a union only ever grows, and a cull too large is a fragment emitted for nothing.
    fn where_drawn(
        &self,
        rect: Rect<DevicePx, Device>,
        transform: Option<zgui_scene::SpatialId>,
    ) -> Rect<DevicePx, Device> {
        match self.input.placements {
            Some(placements) => rect.union(zgui_layout::fragment::transform::placed::onto_device(
                rect, transform, placements,
            )),
            None => rect,
        }
    }

    /// Everything one fragment needs in order to be emitted.
    fn emission<'e>(
        &self,
        fragment: &Fragment,
        style: &'e crate::lower::PaintStyle,
        decorations: &'e [crate::emit::text::DecorationStyle],
        text_fill: Option<&'e crate::lower::background::GradientSpec>,
    ) -> Emission<'e>
    where
        Self: 'e,
    {
        let radii = crate::lower::border::radii_of(
            &self.input.store.node(fragment.box_).style,
            fragment.border_box,
            self.input.scale,
        );
        Emission {
            style,
            box_placement: BoxPlacement {
                border_box: fragment.border_box,
                border: fragment.border,
                radii,
                clip: fragment.clip,
                transform: crate::lower::transform::of(fragment),
                scale: self.input.scale,
            },
            text_placement: TextPlacement {
                line: fragment.border_box,
                clip: fragment.clip,
                transform: crate::lower::transform::of(fragment),
                opaque_target: self.opaque(),
                subpixel_capable: self.input.capabilities.subpixel_text,
                // A fragment with no matrix over it is drawn on the pixels it was measured for;
                // one with any matrix at all is resampled on its way to the surface, which is
                // what per-channel coverage does not survive.
                upright: fragment.transform.is_none(),
                scale: self.input.scale,
                ellipsis: self.ellipsis(fragment),
            },
            alpha: self.alpha(),
            decorations,
            text_fill,
            glyphs: self.input.glyphs,
            glyph_placements: self.input.glyph_placements,
            highlights: self.input.highlights,
            replaced: self.input.replaced,
            vectors: self.input.vectors,
            vector_masks: self.input.vector_masks,
            custom: self.input.custom,
            custom_reference: self.input.store.custom_reference(fragment.box_),
            scale: self.input.scale,
            scrollbars: self.input.scrollbars,
        }
    }

    /// Where a line fragment was cut off by `text-overflow`, if it was.
    ///
    /// The cut is a coordinate in the context's own content box, decided while the line was laid
    /// out; what is needed here is the same coordinate on the device, which is the content box's own
    /// origin plus it. Nothing but a line can be cut, so everything else answers nothing.
    fn ellipsis(&self, fragment: &Fragment) -> Option<crate::emit::text::EllipsisPaint> {
        let zgui_layout::FragmentKind::Line { line, .. } = fragment.kind else {
            return None;
        };
        let resolution = self.input.store.inline_resolution(fragment.box_)?;
        let cut = resolution.lines.get(line as usize)?.ellipsis?;
        let mark = resolution.ellipsis.mark(cut.at_start)?;
        // The line fragment's own rectangle is the line box, and the cut was measured from the
        // context's content box start — which is where the line box's own start edge is, less
        // whatever the line was indented or aligned by. Both were composed into the fragment, so
        // the content box's origin is recovered from the box rather than from the line.
        let content = self.input.store.fragments_of_box(fragment.box_).first()?;
        let origin = self.input.store.fragment(*content)?.content_box.origin.x.0;
        Some(crate::emit::text::EllipsisPaint {
            paragraph: mark.paragraph,
            cutoff: origin + cut.cutoff,
            width: mark.width,
            at_start: cut.at_start,
        })
    }

    /// The fingerprint of what is drawn with a line fragment besides its glyphs, and zero for
    /// anything else.
    ///
    /// Read for the replay record, and it is the only thing in that record that a blinking caret
    /// moves.
    fn highlight_signature(&self, fragment: &Fragment) -> u64 {
        match fragment.kind {
            zgui_layout::FragmentKind::Line { paragraph, line } => {
                self.input.highlights.fingerprint(paragraph, line)
            }
            _ => 0,
        }
    }

    /// Fails if the range about to be replayed draws a raster that has been freed underneath it.
    ///
    /// # Panics
    ///
    /// Naming the fragment and the keys, when [`PaintInput::verify_replays`] is set and the record
    /// holds a key its cache no longer has. Nothing further along can produce a better message:
    /// the display list is exactly what it was last frame, the geometry never moved, and the only
    /// evidence downstream is a letter that is the wrong letter.
    fn verify_replay(&self, fragment: &Fragment) {
        if !self.input.verify_replays {
            return;
        }
        let stale = self
            .painter
            .cache
            .stale_resources(fragment, self.input.resources);
        assert!(
            stale.is_empty(),
            "fragment {:?} is about to replay a range naming {} raster(s) its cache no longer \
             holds, the first of them {:?}: the rectangles they name are free to be handed to \
             other content, so the replay draws whatever took their place",
            fragment.key,
            stale.len(),
            stale.first(),
        );
    }

    /// Emits one fragment, replaying its recorded range when nothing about it changed.
    fn paint(&mut self, frag: FragKey, style_ref: PaintStyleRef, anim: u64) {
        // The store outlives the walk, so the fragment is *borrowed* for as long as this needs it
        // rather than copied out of the way of the mutable borrow below. Copying it was two hundred
        // bytes per piece of every box the damage reached, most of them for pieces the next line
        // then declined to paint at all.
        let store = self.input.store;
        let Some(fragment) = store.fragment(frag) else {
            return;
        };
        // The marks drawn *with* a line are part of what it paints, and one of them can be the
        // only thing it paints: an emptied field's line box has no width, so its ink alone is a
        // rectangle no damage can reach and the caret standing on it would be culled away.
        let mut cull = self.where_drawn(
            crate::damage::ink::cull_rect(store, fragment, self.input.scale),
            fragment.transform,
        );
        if let Some(marks) = crate::damage::marks::extent(
            fragment,
            self.input.highlights,
            self.input.scale,
            self.input.placements,
        ) {
            cull = cull.union(marks);
        }
        if !self.reaches(cull) {
            return;
        }
        let clip = fragment.clip;
        let transform = crate::lower::transform::of(fragment);
        let decorations = self.decorations.faded(self.alpha());
        let text_fill = self.text_fills.in_force().cloned();
        let painted = Painted {
            style: style_ref,
            clip,
            transform,
            transform_hash: fragment.transform_hash,
            custom: match fragment.kind {
                zgui_layout::FragmentKind::Custom => self
                    .input
                    .store
                    .custom_reference(fragment.box_)
                    .map_or(0, |(token, _, _)| self.input.custom.revision(token)),
                _ => 0,
            },
            content: match fragment.kind {
                zgui_layout::FragmentKind::Replaced { content } => {
                    self.input.replaced.revision(content)
                }
                zgui_layout::FragmentKind::Vector => fragment
                    .node
                    .map_or(0, |node| self.input.vectors.revision(node)),
                _ => 0,
            },
            cut: fragment.content_hash,
            scale: self.input.scale.to_bits(),
            decorations: decorate::signature(&decorations),
            text_fill: fill::signature(text_fill.as_ref()),
            anim,
            alpha: self.alpha().to_bits(),
            highlights: self.highlight_signature(fragment),
        };
        match self.painter.cache.reuse(self.scene, fragment, painted) {
            Reuse::Replay(offset) => {
                self.verify_replay(fragment);
                let (source, chunk) = self
                    .painter
                    .cache
                    .chunk(fragment.key)
                    .expect("a replayable fragment has a recorded chunk");
                let range = self.scene.replay_chunk(chunk, offset, source);
                let pushed = (range.end - range.start) as usize;
                self.report.primitives += pushed;
                self.painter.cache.replayed(fragment);
            }
            Reuse::Encode => {
                let Some(mut style) = self.painter.styles.get(style_ref).cloned() else {
                    return;
                };
                if let Some(over) = self.anim_of(self.input.store, fragment.box_) {
                    crate::lower::anim::compose(&mut style, over);
                }
                let emission = self.emission(fragment, &style, &decorations, text_fill.as_ref());
                // Emptied immediately before the encoding rather than after it, so that whatever a
                // source placed for somebody else — a run visited and then declined, a fragment
                // that returned early above — cannot be attributed to this fragment and held for
                // as long as this fragment lives.
                self.named.clear();
                self.input.resources.take_named(&mut self.named);
                // Everything the emitters push between here and the take is the fragment's own,
                // captured before the cull so the record is the painting and never one position's
                // clipped part of it.
                self.scene
                    .begin_chunk_capture(self.painter.cache.take_capture_scratch());
                let emitted = order::fragment_tracked(self.scene, fragment, &emission);
                let chunk = self.scene.take_chunk_capture();
                self.report.primitives += emitted.pushed;
                if matches!(fragment.kind, FragmentKind::Vector | FragmentKind::Custom)
                    && let Some(node) = fragment.node
                {
                    self.report.vector_routes.push(VectorRouteReport {
                        node,
                        routes: emitted.vector_routes,
                    });
                }
                self.named.clear();
                self.input.resources.take_named(&mut self.named);
                self.painter.cache.encoded(
                    self.scene,
                    fragment,
                    painted,
                    Encoding {
                        chunk,
                        resources: &self.named,
                    },
                    self.input.resources,
                );
            }
        }
        if self.input.record_emitted {
            self.report.emitted.push(frag);
        }
    }
}

impl stacking::Visitor for Pass<'_, '_> {
    fn enter(&mut self, store: &LayoutStore, key: BoxKey) -> bool {
        counter::bump(Counter::NodesVisited);
        // A box with no fragments generated no geometry, which is what `display: none` produces;
        // there is nothing below it either.
        let Some(ink) = self.subtree_ink(key) else {
            return false;
        };
        if !self.reaches(ink) {
            self.report.skipped_subtrees += 1;
            counter::add(Counter::PrimitivesCulled, 1);
            return false;
        }
        let Some(node) = store.get(key) else {
            return false;
        };
        let style_ref = self.painter.styles.lower(&node.style, self.input.scale);
        let entry = self
            .painter
            .styles
            .get(style_ref)
            .expect("a reference just handed out resolves");
        // Copied only where there is something to compose into it. The entry `style_ref` names is
        // shared with every element that cascaded to the same result, so an animated value written
        // into it would animate all of them — which is exactly what a row of identical buttons
        // looks like when one of them is hovered. That is a reason to copy the one box that is
        // animating, and every other box borrows: a `PaintStyle` is several hundred bytes with two
        // inline vectors and a path in it, and at most one box on a screen is being hovered.
        let anim = self.anim_of(store, key);
        let style = match anim {
            Some(over) => {
                let mut composed = entry.clone();
                crate::lower::anim::compose(&mut composed, over);
                Cow::Owned(composed)
            }
            None => Cow::Borrowed(entry),
        };
        let anim = anim.map_or(0, AnimOverride::signature);

        let fragments = store.fragments_of_box(key);
        let own = fragments.first().and_then(|frag| store.fragment(*frag));

        let isolation = own.map_or(Isolation::None, |fragment| {
            group::isolation(&style, fragment)
        });
        // A subtree at zero alpha composites nothing. Every fragment under it is already refused
        // one at a time by `order::vanished`, so what this saves is the walk itself: the ink
        // query, the lowered style and its copy, the animation lookup and the sorted child list,
        // for every box below a panel that is laid out and invisible. A closed disclosure costs
        // its own box rather than its contents.
        //
        // Refused before either stack is pushed, and before a band or a group target is opened, so
        // the leave that never comes has nothing to withdraw.
        if isolation.alpha() == 0.0 {
            self.report.skipped_subtrees += 1;
            counter::add(Counter::PrimitivesCulled, 1);
            return false;
        }
        // The two stacks are pushed once per entered box and popped once per leave, whatever the
        // box turned out to have: a push made conditionally and a pop made on a different condition
        // is how the alpha in force comes to belong to somebody else's subtree.
        if let Some(fragment) = own
            && fragment.flags.contains(FragmentFlags::HAS_TRANSFORM)
            && let Some(space) = fragment.transform
            && self.scene.open_place_band(space).is_some()
        {
            self.bands.push(key);
        }
        if let Some(fragment) = own {
            self.report.primitives += group::backdrop(self.scene, &style, fragment, fragment.clip);
            if isolation.needs_target() {
                let boundary = group::open(self.scene, &style, fragment, fragment.clip);
                self.open.push((key, boundary));
                self.report.groups += 1;
            }
        }
        // A group's target is transparent, so per-channel text coverage inside it is meaningless
        // whatever the device can do.
        self.opaque.push(!isolation.needs_target() && self.opaque());
        self.alpha.push(isolation.alpha());
        // Contributed on the way in and withdrawn on the way out, for the same reason the alpha is:
        // a decoration is drawn across the box's in-flow descendants, and the line boxes that draw
        // it belong to anonymous boxes generated below the element that declared it.
        self.decorations.enter(&node.style, &style.decoration);
        // The same propagation, for the same reason: the ramp that paints a heading's letters is
        // declared on the heading and the line boxes belong to an anonymous box under it.
        self.text_fills.enter(&node.style, style.text_fill.as_ref());

        for &frag in store.fragments_of_box(key) {
            self.paint(frag, style_ref, anim);
        }
        true
    }

    fn leave(&mut self, store: &LayoutStore, key: BoxKey) {
        // The outline is drawn after the box's descendants, which is Appendix E's step ten.
        if let Some(frag) = store.fragments_of_box(key).first().copied()
            && let Some(fragment) = store.fragment(frag)
        {
            let style_ref = self
                .painter
                .styles
                .lower(&store.node(key).style, self.input.scale);
            let mut lowered = self.painter.styles.get(style_ref).cloned();
            if let (Some(style), Some(over)) = (lowered.as_mut(), self.anim_of(store, key)) {
                crate::lower::anim::compose(style, over);
            }
            if let Some(style) = lowered
                && style.outline.is_some()
                && self.reaches(fragment.ink)
            {
                let emission = self.emission(fragment, &style, &[], None);
                self.report.primitives += order::outline(self.scene, &emission);
            }
        }
        self.alpha.pop();
        self.opaque.pop();
        self.decorations.leave();
        self.text_fills.leave();
        if self.open.last().is_some_and(|(opener, _)| *opener == key) {
            let (_, boundary) = self.open.pop().expect("just checked");
            group::close(self.scene, &boundary);
        }
        if self.bands.last() == Some(&key) {
            self.bands.pop();
            self.scene.close_place_band();
        }
    }
}
