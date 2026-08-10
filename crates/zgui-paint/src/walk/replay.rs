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
//! # Why a chunk with a vector item in it is not replayed where it paints
//!
//! A record's chunk is captured at the pushes, before the clip cull, so it is the fragment's
//! complete painting: a row arriving at the edge of a scroll port replays the whole of itself and
//! the cull admits the part that has come into view. One thing a replay still cannot reproduce is
//! a **vector item** — vector content is planned into rasterisation passes rather than re-emitted
//! as instances, and the replay skips it. A line of text drawn as outlines rather than as
//! coverage tiles — a display-size heading, a gradient-filled one — would therefore replay
//! without its letters.
//!
//! [`Record::whole`] is the bit that answers it. A record that is not whole is encoded again
//! everywhere the fragment paints anything — which is everywhere its ink reaches the clip.
//! Outside the clip it may stand: a drawing far below a scroll port paints nothing wherever it is
//! put, so replaying nothing is the whole of its painting there.
//!
//! # The invariant a replay depends on
//!
//! A replayed chunk carries the clip, paint and transform indices of the frame it was encoded in.
//! Those resolve because the side tables are kept across frames and an entry keeps its identity
//! for as long as anything refers to it — so the recorded content hashes are checked, in debug
//! builds, against what the indices resolve to now. A table rebuilt per frame would draw one
//! fragment with another's paint, with no error anywhere, and this is what fails instead.

pub mod hold;

use rustc_hash::FxHashMap;
use zgui_atlas::AtlasKey;
use zgui_geom::{Device, DevicePx, Rect, Size};
use zgui_layout::{FragKey, Fragment, FragmentKind};
use zgui_profile::{Counter, counter};
use zgui_scene::{ChunkPrims, ClipId, Scene, SpatialId, TableHolds};

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
    /// The revision of the outside content the fragment draws, and zero for every other kind.
    ///
    /// A replaced fragment names its node and a drawing fragment names its curves' source; both
    /// names stay put while what they resolve to changes. For a record dropped at the end of
    /// every unvisited frame the box rebuild that accompanies such a change was protection
    /// enough; a record kept across frames needs the change in its own signature.
    pub content: u64,
    /// The bits of the device scale the fragment was encoded at.
    ///
    /// The lowered style already moves on rescale — the style cache is invalidated wholesale —
    /// so this is the local statement of that invariant rather than the only guard.
    pub scale: u32,
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
    /// Whether replaying the chunk draws everything the fragment drew when it was recorded.
    ///
    /// The chunk is captured before the cull, so a refused primitive no longer cuts it — the one
    /// thing a replay still cannot reproduce is a vector item, which is planned into a
    /// rasterisation pass rather than re-emitted. False exactly when the chunk holds one, and
    /// then the record may only stand in for the fragment where the fragment paints nothing.
    pub whole: bool,
    /// What the clip resolved to when the range was recorded.
    pub clip_hash: Option<u64>,
    /// The frame this record was last selected in — encoded or replayed.
    ///
    /// What eviction orders by: a record selected this frame is the working set, and the oldest
    /// stamp is the coldest chunk.
    pub last_selected: u64,
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
    /// The fragment's complete painting, captured at the pushes before the cull and the order.
    pub chunk: ChunkPrims,
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
    /// Fragments seen this frame, so the retained rest can be counted when the frame ends.
    seen: Vec<FragKey>,
    /// The storage the next capture starts from — the arrays of the last record replaced.
    capture_scratch: ChunkPrims,
    /// The revision the next encoding is stamped with.
    next_revision: u64,
    /// The frame number selections are stamped with.
    epoch: u64,
    /// The owned bytes of every record's chunk, maintained as records come and go.
    bytes: usize,
    /// The owned bytes of records selected this frame, maintained as they are stamped.
    ///
    /// Maintained rather than summed on demand, because the budget reads it every frame and a
    /// sweep of the records would cost the large retained documents this cache now holds.
    selected_bytes: usize,
    /// How many selections — encodes and replays — the cache has answered, monotonic.
    selections: u64,
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
        self.epoch += 1;
        self.selected_bytes = 0;
    }

    /// The owned bytes of every record's chunk.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The owned bytes of records selected this frame, which is the working set a budget must
    /// not evict.
    pub fn selected_bytes(&self) -> usize {
        self.selected_bytes
    }

    /// How many selections — encodes and replays — the cache has answered, monotonic.
    pub fn selections(&self) -> u64 {
        self.selections
    }

    /// The storage the next capture starts from, emptied by the capture that takes it.
    ///
    /// Handing the last replaced record's arrays back is what keeps an animating fragment's
    /// re-encodes allocation-free once its painting has reached its size.
    pub fn take_capture_scratch(&mut self) -> ChunkPrims {
        core::mem::take(&mut self.capture_scratch)
    }

    /// Ends the frame, counting the records the frame kept without visiting their fragment.
    ///
    /// A record survives culling, clean frames and invisibility: it owns its primitives, and its
    /// atlas and table indices stay valid because those caches release nothing a record holds. It
    /// dies with its fragment, through [`PaintCache::retire`] — a destroyed fragment's key is
    /// generational, so a kept record can never be replayed for a successor in its slot.
    pub fn end_frame(&mut self) {
        let unvisited = self.records.len().saturating_sub(self.seen.len());
        counter::add(Counter::ChunksRetainedUnvisited, unvisited as u64);
        counter::set(Counter::PaintChunkBytes, self.bytes as u64);
    }

    /// Drops the records of fragments that ceased to exist, releasing what they held.
    ///
    /// `keys` is the layout store's account of every fragment destroyed since the last drain. A
    /// key with no record is the ordinary case — most fragments die without ever being painted —
    /// and costs one lookup.
    pub fn retire(&mut self, keys: &[FragKey], scene: &mut Scene, owner: &dyn ResourceOwner) {
        counter::add(Counter::FragmentsRetired, keys.len() as u64);
        let mut holds = TableHolds::default();
        for key in keys {
            if let Some(record) = self.records.remove(key) {
                self.drop_record(record, scene, owner, &mut holds);
            }
        }
    }

    /// Releases everything one removed record held, and keeps the byte account true.
    fn drop_record(
        &mut self,
        record: Record,
        scene: &mut Scene,
        owner: &dyn ResourceOwner,
        holds: &mut TableHolds,
    ) {
        self.bytes = self.bytes.saturating_sub(record.prims.bytes());
        record.prims.named_ids(holds);
        release_tables(scene, holds);
        release(owner, &record.resources);
    }

    /// Drops the coldest records until `bytes_to_free` chunk bytes have gone, and reports how
    /// many went.
    ///
    /// Records selected this frame are the working set and are never taken, so a frame whose own
    /// working set exceeds the budget stays over it rather than dropping what it is drawing.
    /// Eviction is a clean miss by construction: the next frame whose damage reaches an evicted
    /// fragment encodes it again, and the pixels on screen are untouched in the meantime.
    pub fn evict_cold(
        &mut self,
        bytes_to_free: u64,
        scene: &mut Scene,
        owner: &dyn ResourceOwner,
    ) -> u64 {
        let mut cold: Vec<(u64, FragKey)> = self
            .records
            .iter()
            .filter(|(_, record)| record.last_selected < self.epoch)
            .map(|(key, record)| (record.last_selected, *key))
            .collect();
        cold.sort_unstable();
        let mut freed = 0_u64;
        let mut holds = TableHolds::default();
        for (_, key) in cold {
            if freed >= bytes_to_free {
                break;
            }
            let Some(record) = self.records.remove(&key) else {
                continue;
            };
            freed += record.prims.bytes() as u64;
            counter::bump(Counter::ChunksEvicted);
            self.drop_record(record, scene, owner, &mut holds);
        }
        freed
    }

    /// Drops every record, releasing everything each one held.
    ///
    /// For a caller whose scene tables and atlas survive the painter — a budget's forget. Where
    /// they die together, [`PaintCache::clear`] is the cheaper spelling.
    pub fn clear_releasing(&mut self, scene: &mut Scene, owner: &dyn ResourceOwner) {
        let mut holds = TableHolds::default();
        let records: Vec<Record> = self.records.drain().map(|(_, record)| record).collect();
        for record in records {
            self.drop_record(record, scene, owner, &mut holds);
        }
        self.seen.clear();
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
        // The chain must still resolve to the content it was recorded against. Interned clip ids
        // are content-stable today, so this only misses once table eviction can reassign a slot —
        // at which point a miss here is what keeps the replay from drawing through another
        // fragment's chain.
        if record.clip_hash != scene.clips.content_hash(painted.clip) {
            return Reuse::Encode;
        }
        // A vector-bearing record may still stand in for a fragment that is *entirely* outside
        // the clip: a drawing below a scroll port paints nothing down there, however far it
        // moves. What must not happen is replaying it — minus the curves the replay skips — the
        // moment any part of the fragment reaches the clip.
        //
        // The *local* ink, against the clip as it was interned, because that is the comparison
        // the insert cull makes: a primitive is culled on its own recorded bounds, which inside a
        // transformed subtree are that subtree's coordinates, and the clip imposed there is
        // measured in the same coordinates. The device-space ink would cross spaces with the clip
        // and answer about pixels neither of them means.
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
        scene: &mut Scene,
        fragment: &Fragment,
        painted: Painted,
        encoding: Encoding<'_>,
        owner: &dyn ResourceOwner,
    ) {
        counter::bump(Counter::ChunksReencoded);
        counter::bump(Counter::Repaints);
        self.seen.push(fragment.key);
        self.selections += 1;
        let Encoding { chunk, resources } = encoding;
        // A chunk is complete except for its vector items, which a replay skips until they are
        // planned from the chunk. A chunk with none re-emits everything it captured.
        let whole = chunk.vectors.is_empty();
        let mut held: Vec<AtlasKey> = resources.to_vec();
        held.sort_unstable();
        held.dedup();
        for key in &held {
            owner.retain(*key);
        }
        counter::add(Counter::RecordTilesRetained, held.len() as u64);
        // The replaced record's arrays become the next capture's storage, so re-encoding a
        // fragment allocates only where its painting grew. Everything it held — atlas tiles and
        // table entries alike — is released only after the new holds are taken, so a fragment
        // re-encoding the letters it already had never lets its own tiles reach a refcount of
        // zero in between.
        let mut replaced_holds = TableHolds::default();
        let (prims, replaced_resources) = match self.records.remove(&fragment.key) {
            Some(replaced) => {
                self.bytes = self.bytes.saturating_sub(replaced.prims.bytes());
                replaced.prims.named_ids(&mut replaced_holds);
                self.capture_scratch = replaced.prims;
                (chunk, Some(replaced.resources))
            }
            None => (chunk, None),
        };
        let mut holds = TableHolds::default();
        prims.named_ids(&mut holds);
        for clip in &holds.clips {
            scene.clips.retain(*clip);
        }
        for paint in &holds.paints {
            scene.paints.retain(*paint);
        }
        let chunk_bytes = prims.bytes();
        self.bytes += chunk_bytes;
        // A fragment is selected at most once per frame, so nothing selected earlier this frame
        // is being replaced here and the sum only grows.
        self.selected_bytes += chunk_bytes;
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
                last_selected: self.epoch,
                resources: held,
            },
        );
        release_tables(scene, &replaced_holds);
        if let Some(resources) = replaced_resources {
            release(owner, &resources);
        }
    }

    /// Records that a fragment's chunk was replayed, and counts it as translated.
    ///
    /// The replay changes nothing about the record's completeness: the chunk was captured before
    /// the cull, so what a position's clip refuses of it is refused again at the next selection
    /// rather than lost. [`Record::whole`] moves only at an encoding.
    pub fn replayed(&mut self, fragment: &Fragment) {
        counter::bump(Counter::ChunksTranslated);
        self.seen.push(fragment.key);
        self.selections += 1;
        if let Some(record) = self.records.get_mut(&fragment.key) {
            record.border_box = fragment.border_box;
            if record.last_selected != self.epoch {
                record.last_selected = self.epoch;
                self.selected_bytes += record.prims.bytes();
            }
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

/// Gives up one hold on each table entry in `holds`.
fn release_tables(scene: &mut Scene, holds: &TableHolds) {
    for clip in &holds.clips {
        scene.clips.release(*clip);
    }
    for paint in &holds.paints {
        scene.paints.release(*paint);
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
