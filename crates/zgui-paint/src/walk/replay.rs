//! Painting a fragment again, or replaying what it painted last time.
//!
//! # Why the cache lives here and not on the fragment
//!
//! The emit walk is a pure reader of the fragment tree — that is what lets the fragment tree have
//! exactly one writer — so a fragment cannot be handed a range to remember. The record lives here
//! instead, keyed by fragment, and it carries enough of the fragment's own state to decide for
//! itself whether the record is still good: the lowered style it was painted with, the rectangle it
//! was painted at, and the chain and transform it was drawn through.
//!
//! That is stronger than being told. A record that decides for itself cannot be invalidated by the
//! wrong phase, or left valid by a phase that forgot.
//!
//! # Why the record holds what the fragment draws as well as where
//!
//! A fragment keeps its name for as long as it is the same piece of the same box, and *which
//! paragraph a line draws* is not part of being the same line — a paragraph is interned by the
//! shaping of its characters, so one changed character issues a new identifier for it while the
//! line stays exactly where it was. A record that compared only the style, the chain, the transform
//! and the size would find every one of those equal and replay the previous characters into a
//! perfectly damaged rectangle. A counter going from `1` to `7` is the whole failure: same width,
//! same line, same everything the geometry can see, and the old digit back on the screen.
//!
//! # Why a range that is less than the painting is never replayed
//!
//! A recorded range is a claim that replaying it draws what the fragment draws, and there are two
//! ways the claim can be false the moment it is made.
//!
//! A primitive whose ink misses the **clip** in force is refused by the scene rather than logged,
//! so what a fragment on the edge of a scroll port records is the part of itself that was inside
//! the port at that moment — and a fragment entirely outside the port records nothing at all.
//! Neither is the fragment's painting; both are the painting of one position. The record travels
//! with the fragment and the fragment moves: a panel below the port that scrolls into view keeps
//! the empty range it was given while it was hidden, replays it at every position it passes
//! through, and never appears. Nothing else in the record moves — same style, same clip, same
//! transform, same size — so nothing else can notice.
//!
//! A **vector item** is logged and then skipped by the replay, because vector content is planned
//! into rasterisation passes rather than re-emitted as instances. A line of text drawn as outlines
//! rather than as coverage tiles — which is what a display-size heading and a gradient-filled one
//! are — therefore records a range that holds none of its letters.
//!
//! [`Record::whole`] is the one bit that answers both. A record that is not whole is encoded again
//! everywhere the fragment paints anything — which is everywhere its ink reaches the clip. Outside
//! the clip it may stand: a row far below a scroll port paints nothing wherever it is put, so the
//! empty range is the whole of its painting there, and a list of a thousand rows is not re-encoded
//! for the sake of the two that are arriving.
//!
//! # The invariant a replay depends on
//!
//! A replayed chunk carries the clip, paint and transform indices of the frame it was encoded in.
//! Those resolve because the side tables are kept across frames and an entry keeps its identity
//! for as long as anything refers to it — so the recorded content hashes are checked, in debug
//! builds, against what the indices resolve to now. A table rebuilt per frame would draw one
//! fragment with another's paint, with no error anywhere, and this is what fails instead.

pub mod hold;

use core::ops::Range;

use rustc_hash::FxHashMap;
use zgui_atlas::AtlasKey;
use zgui_geom::{Device, DevicePx, Rect, Size};
use zgui_layout::{FragKey, Fragment, FragmentKind};
use zgui_profile::{Counter, counter};
use zgui_scene::{ChunkPrims, ClipId, Scene, SpatialId};

use crate::lower::cache::PaintStyleRef;
use crate::walk::replay::hold::ResourceOwner;

/// How a fragment is being painted this frame, and how it was painted last time.
///
/// Together they are what decides whether last frame's range can be replayed. They travel as one
/// value because they are one question — *is this the same painting?* — and because three of them
/// are indices into side tables that mean nothing apart from each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Painted {
    /// The lowered style it carries.
    pub style: PaintStyleRef,
    /// The chain it is drawn through.
    pub clip: ClipId,
    /// The coordinate system it is drawn in.
    pub transform: SpatialId,
    /// A fingerprint of the matrix that coordinate system resolved to.
    ///
    /// The name above is structural: it is the same name on the first frame of a movement and on
    /// the last, which is what makes moving something a write rather than a new identity. It is
    /// therefore not enough on its own to decide whether a recorded range may be replayed, because
    /// a range is a set of instances that were encoded against a particular matrix. This is the
    /// part of the comparison that a movement moves.
    pub transform_hash: u64,
    /// The revision of what a custom element paints, and zero for every other kind.
    ///
    /// The caret's pattern: nothing else in this record moves when a custom element redraws in
    /// place, so this is the one field that turns "the implementation says it changed" into a
    /// cache miss — and its absence into a replay.
    pub custom: u64,
    /// A fingerprint of the text decorations in force over it.
    ///
    /// Those come from the boxes *above* the fragment rather than from its own style, so nothing
    /// else here moves when they change. Without it, changing `text-decoration` on a paragraph
    /// would replay every line inside it exactly as it was.
    pub decorations: u64,
    /// A fingerprint of the ramp painting the text over it.
    ///
    /// Like the decorations, the ramp comes from a box *above* the fragment, so nothing else here
    /// moves when it changes. Without it, editing the gradient on a heading replays every line
    /// inside it exactly as it was.
    pub text_fill: u64,
    /// A fingerprint of what the box's running animations are overriding.
    ///
    /// The lowered style is *shared* and does not move while an animation runs, so without this the
    /// first frame of every animation is replayed for the whole of its length: the animation ticks,
    /// the damage is right, and the picture does not change.
    pub anim: u64,
    /// The bits of the alpha folded into every colour this fragment is emitted with.
    ///
    /// An ancestor's `opacity` is folded into its descendants' colours rather than composited
    /// through a target whenever their ink is disjoint, and none of the rest of this record moves
    /// when it changes: a descendant's own style, clip, transform and animation are all exactly
    /// what they were. Without it, a panel fading out is a panel whose *contents* never fade —
    /// which is the whole of what is on the screen, replayed at the alpha it had when the fade
    /// began.
    ///
    /// The bits rather than the number, because this record is compared for equality and `f32` is
    /// not `Eq`. Two alphas that differ only in the sign of a zero compare unequal here, and the
    /// cost of that is one re-encoded fragment.
    pub alpha: u32,
    /// A fingerprint of the caret and the selection bands drawn with a line fragment, and zero
    /// for a fragment that is not a line.
    ///
    /// A caret blinking moves nothing else in this record: same line, same style, same geometry,
    /// same clip. Without it the previous frame's range is replayed for ever and the caret is
    /// frozen in whichever phase it was first encoded in — which looks exactly like no caret at
    /// all half the time, and like one that never blinks the other half.
    pub highlights: u64,
}

/// What one fragment painted last time it was painted.
#[derive(Clone, Debug)]
pub struct Record {
    /// The fragment's compiled painting, owned here.
    ///
    /// Owned rather than a range of the scene's log, because the log is cleared every frame: a
    /// range names whatever occupies those positions now, and a record must stay replayable
    /// however many frames pass between the encoding and the next visit.
    pub prims: ChunkPrims,
    /// Which encoding produced [`Record::prims`], distinct per encoding for the life of the cache.
    ///
    /// A consumer that mirrors chunks elsewhere — a persistent GPU copy — keys its residence on
    /// the pair of the fragment and this, so a re-encoded chunk is a new identity rather than a
    /// mutation it has to detect.
    pub revision: u64,
    /// What the fragment was drawing when the range was recorded.
    pub kind: FragmentKind,
    /// How it was painted.
    pub painted: Painted,
    /// The border box it was painted at.
    pub border_box: Rect<DevicePx, Device>,
    /// Whether replaying the range draws everything the fragment drew when it was recorded.
    ///
    /// False when the clip refused a primitive, and false when one was pushed that the replay
    /// skips. Either way the range is less than the painting, so it may only stand in for the
    /// fragment where the fragment paints nothing at all.
    pub whole: bool,
    /// What the clip resolved to when the range was recorded.
    pub clip_hash: Option<u64>,
    /// The cached rasters the range draws, each held once for as long as the record stands.
    ///
    /// A replay re-emits instances that already carry the rectangle of the texture their pixels
    /// are in, so nothing on that path tells the cache holding those pixels that they are still on
    /// the screen. This is what does — see [`hold`].
    ///
    /// Distinct: a line drawing the same letter forty times holds it once, because a hold that
    /// counted repetitions would have to be given back exactly as many times and one miscount is
    /// either a tile that can never be freed or one freed while it is being drawn.
    pub resources: Vec<AtlasKey>,
}

/// What one fragment's encoding produced, as the record is told it.
///
/// The three travel together because they are one answer — *this is what the fragment just drew* —
/// and because two of them are only meaningful against the third: a range without the rasters it
/// names is a range nothing can keep alive, and a completeness bit about no range at all says
/// nothing.
#[derive(Clone, Debug)]
pub struct Encoding<'a> {
    /// The range of this frame's operation log the primitives occupied.
    ///
    /// Read once, to copy the pushed primitives into the record's own chunk. The range itself is
    /// not kept: the log is cleared every frame, and the chunk is what survives.
    pub ops: Range<u32>,
    /// Whether replaying the range would draw what the encoding drew.
    ///
    /// A caller answers it by reading [`Scene::unreplayable`] either side of the encoding, because
    /// the scene is the only place a refused primitive or a skipped one is known about.
    pub whole: bool,
    /// The rasters the encoding named, in the order it named them and with repeats.
    pub resources: &'a [AtlasKey],
}

/// What a fragment costs this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reuse {
    /// Nothing usable was recorded, so the fragment is encoded from its style and geometry.
    Encode,
    /// The record still stands, and is replayed with the fragment's movement applied.
    ///
    /// The offset is zero for a fragment that did not move at all, which is the commonest case and
    /// is still a replay: what it saves is the encoding, not the movement.
    Replay(Size<DevicePx, Device>),
}

/// Whether a fragment of this kind paints anything a recorded range can carry.
///
/// Everything a replay re-emits comes out of the operation log, and vector items are not in it:
/// they are planned into rasterisation passes out of a list the scene rebuilds from nothing every
/// frame. A drawing paints *nothing but* a vector item, so replaying one re-emits an empty range —
/// while the damage that reached the fragment has already cleared the pixels it drew last time.
/// The icon vanishes, and because the following frames no longer damage the hole it stays vanished
/// until something repaints the whole surface.
///
/// So a drawing is encoded every frame that reaches it. That costs a push per outline and no
/// re-placing at all: the curves are memoised by node and the same shared allocation is handed
/// back, which is what a rasteriser's encoding cache recognises.
fn replayable(kind: FragmentKind) -> bool {
    !matches!(kind, FragmentKind::Vector)
}

/// Every fragment's last painting, kept across frames.
#[derive(Debug, Default)]
pub struct PaintCache {
    /// The records, by fragment.
    records: FxHashMap<FragKey, Record>,
    /// Fragments seen this frame, so the rest can be dropped when the frame ends.
    seen: Vec<FragKey>,
    /// The revision the next encoding is stamped with.
    next_revision: u64,
}

impl PaintCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many fragments have a record.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether nothing is recorded.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Discards every record, which is what a frame drawn into a fresh scene needs.
    ///
    /// The holds the records were carrying go with them, so this may only be used where the cache
    /// they were taken in is emptied too — a lost device, a scene rebuilt from nothing. Anywhere
    /// else it would leave tiles held by records that no longer exist, which is a cache where
    /// nothing is ever evictable again.
    pub fn clear(&mut self) {
        self.records.clear();
        self.seen.clear();
    }

    /// Starts a frame, forgetting which fragments were seen in the last one.
    pub fn begin_frame(&mut self) {
        self.seen.clear();
    }

    /// Drops the record of every fragment this frame did not visit, releasing what it held.
    ///
    /// A fragment nobody visited is one that was culled or has ceased to exist. The records own
    /// their primitives, so keeping them across unvisited frames is safe — this drop is retention
    /// policy, kept until fragment-retirement events tell the cache which names are gone. A
    /// destroyed fragment's key is generational, so a kept record can never be replayed for a
    /// successor in its slot.
    pub fn end_frame(&mut self, owner: &dyn ResourceOwner) {
        if self.seen.len() == self.records.len() {
            return;
        }
        let seen: rustc_hash::FxHashSet<FragKey> = self.seen.iter().copied().collect();
        let mut dropped = Vec::new();
        self.records.retain(|key, record| {
            if seen.contains(key) {
                return true;
            }
            dropped.append(&mut record.resources);
            false
        });
        release(owner, &dropped);
    }

    /// What `fragment` costs this frame, given the style it lowers to now.
    ///
    /// A record is replayable when the fragment is still drawing what it was drawing, the style,
    /// the chain and the transform are the ones it was recorded with, and the fragment is the
    /// same size. Anything else is encoded again.
    pub fn reuse(&self, scene: &Scene, fragment: &Fragment, painted: Painted) -> Reuse {
        let Some(record) = self.records.get(&fragment.key) else {
            return Reuse::Encode;
        };
        if record.painted != painted {
            return Reuse::Encode;
        }
        if record.kind != fragment.kind {
            return Reuse::Encode;
        }
        if !replayable(fragment.kind) {
            return Reuse::Encode;
        }
        if record.border_box.size != fragment.border_box.size {
            return Reuse::Encode;
        }
        // A cut range may still stand in for a fragment that is *entirely* outside the clip, and
        // that is the case worth keeping: the rows of a long list below a scroll port are cut to
        // nothing, and nothing is exactly what they paint down there, however far they move. What
        // must not happen is replaying that emptiness the moment any part of the fragment reaches
        // the clip, which is what a row arriving at the edge of the port does.
        //
        // The *local* ink, against the clip as it was interned, because that is the comparison the
        // insert cull that cut the range made: a primitive is culled on its own recorded bounds,
        // which inside a transformed subtree are that subtree's coordinates, and the clip imposed
        // there is measured in the same coordinates. The device-space ink would cross spaces with
        // the clip and answer about pixels neither of them means.
        if !record.whole
            && fragment
                .local_ink
                .intersects(scene.clips.bounds(painted.clip))
        {
            return Reuse::Encode;
        }
        debug_assert!(
            self.indices_still_resolve(scene, record),
            "a replayed range's clip or transform no longer resolves to what it was recorded with"
        );
        Reuse::Replay(Size::new(
            DevicePx(fragment.border_box.origin.x.0 - record.border_box.origin.x.0),
            DevicePx(fragment.border_box.origin.y.0 - record.border_box.origin.y.0),
        ))
    }

    /// The chunk recorded for `fragment`, for a caller that has already decided to replay it.
    pub fn prims(&self, fragment: FragKey) -> Option<&ChunkPrims> {
        self.records.get(&fragment).map(|record| &record.prims)
    }

    /// Records what a fragment painted this frame, and counts it as encoded.
    ///
    /// `whole` says whether replaying the range would draw what the encoding drew. A caller
    /// answers it by reading [`Scene::unreplayable`] either side of the encoding, because the scene
    /// is the only place a refused primitive or a skipped one is known about.
    ///
    /// The record takes one hold on each distinct key `encoding` names and gives back whatever the
    /// record it replaced was holding. The two happen in that order, so a fragment re-encoding the
    /// letters it already had never lets its own tiles reach a refcount of zero in between.
    pub fn encoded(
        &mut self,
        scene: &Scene,
        fragment: &Fragment,
        painted: Painted,
        encoding: Encoding<'_>,
        owner: &dyn ResourceOwner,
    ) {
        counter::bump(Counter::ChunksReencoded);
        counter::bump(Counter::Repaints);
        self.seen.push(fragment.key);
        let Encoding {
            ops,
            whole,
            resources,
        } = encoding;
        let mut held: Vec<AtlasKey> = resources.to_vec();
        held.sort_unstable();
        held.dedup();
        for key in &held {
            owner.retain(*key);
        }
        counter::add(Counter::RecordTilesRetained, held.len() as u64);
        // The replaced record's arrays become the new chunk's storage, so re-encoding a fragment
        // allocates only where its painting grew. Its holds are released after the new ones are
        // taken, so a fragment re-encoding the letters it already had never lets its own tiles
        // reach a refcount of zero in between.
        let (mut prims, replaced_resources) = match self.records.remove(&fragment.key) {
            Some(replaced) => (replaced.prims, Some(replaced.resources)),
            None => (ChunkPrims::default(), None),
        };
        scene.extract_chunk(ops, &mut prims);
        self.next_revision += 1;
        self.records.insert(
            fragment.key,
            Record {
                prims,
                revision: self.next_revision,
                kind: fragment.kind,
                painted,
                whole,
                border_box: fragment.border_box,
                clip_hash: scene.clips.content_hash(painted.clip),
                resources: held,
            },
        );
        if let Some(resources) = replaced_resources {
            release(owner, &resources);
        }
    }

    /// Records that a fragment's range was replayed, and counts it as translated.
    ///
    /// # Why a replay may lower the bit and never raise it
    ///
    /// `whole` measured over the replay answers *did this pushing lose anything*, and that is the
    /// right question in one direction only. A fragment that was whole where it was encoded and is
    /// half out of the scroll port now leaves a shorter range behind it, and that shorter range is
    /// the one the next frame would be asked to replay — so a replay that loses something has to
    /// say so, and the measurement is how it does.
    ///
    /// The converse does not follow, because a replay pushes the range the record already holds
    /// rather than the fragment's painting. A range that was short when it was recorded is short
    /// again wherever it is replayed, and losing nothing out of a lossy range is not a claim that
    /// the range is the fragment's whole painting. The limiting case is the one
    /// [`Record::whole`] exists for: a fragment encoded entirely outside its clip records the
    /// empty range, replaying nothing refuses nothing, and a measurement taken over that reads
    /// `true` — which would write over the `false` the encoding stated honestly and put the record
    /// beyond the reach of the guard in [`PaintCache::reuse`] for as long as it lives.
    ///
    /// So the two are combined rather than replaced. It is self-healing rather than sticky: a
    /// record that is not whole is encoded again the moment its ink meets the clip, and *that*
    /// encoding is entitled to raise the bit because it measured the fragment's own painting.
    pub fn replayed(&mut self, fragment: &Fragment, whole: bool) {
        counter::bump(Counter::ChunksTranslated);
        self.seen.push(fragment.key);
        if let Some(record) = self.records.get_mut(&fragment.key) {
            record.border_box = fragment.border_box;
            record.whole &= whole;
        }
    }

    /// Whether a record's indices still resolve to the content they were recorded against.
    ///
    /// The coordinate system is checked for still being *there* rather than for still resolving to
    /// the matrix it did. A replayed instance carries the slot and its rectangle in that slot's own
    /// coordinates, so the matrix is applied when the frame is drawn and not when the range was
    /// recorded: a range replayed through a coordinate system that has since moved draws where the
    /// box is now, which is the whole of why moving a box is a write. What must not happen is the
    /// range being replayed through a *different box's* coordinate system, and that is the name's
    /// occupancy counter — compared where the record's whole painting is.
    fn indices_still_resolve(&self, scene: &Scene, record: &Record) -> bool {
        scene.clips.content_hash(record.painted.clip) == record.clip_hash
            && scene.spatial.contains(record.painted.transform)
    }

    /// Every raster every live record names, with repeats across records.
    ///
    /// What the cache holding those rasters must not free while these records stand. Repeats are
    /// kept rather than collapsed because they are the holds: two lines drawing the same letter
    /// are two records each holding it once.
    pub fn resources(&self) -> impl Iterator<Item = AtlasKey> + '_ {
        self.records
            .values()
            .flat_map(|record| record.resources.iter().copied())
    }

    /// The keys `fragment`'s record names that the cache no longer holds.
    ///
    /// Empty for a record that owns what it draws, and that is the whole assertion: a key in this
    /// list is a rectangle of a texture the replay is about to draw from and something else is now
    /// entitled to write into. Nothing downstream can notice — the display list says exactly what
    /// it said last frame, the geometry never moved, and the pixels are wrong only once the
    /// rectangle has been handed out and filled.
    ///
    /// Answering it costs one lookup per distinct raster the fragment draws, so it is asked behind
    /// a flag rather than on every frame of every window.
    pub fn stale_resources(&self, fragment: &Fragment, owner: &dyn ResourceOwner) -> Vec<AtlasKey> {
        let Some(record) = self.records.get(&fragment.key) else {
            return Vec::new();
        };
        record
            .resources
            .iter()
            .copied()
            .filter(|key| !owner.contains(*key))
            .collect()
    }
}

/// Gives up one hold on each of `keys`, and counts them.
fn release(owner: &dyn ResourceOwner, keys: &[AtlasKey]) {
    for key in keys {
        owner.release(*key);
    }
    counter::add(Counter::RecordTilesReleased, keys.len() as u64);
}

#[cfg(test)]
mod tests;
