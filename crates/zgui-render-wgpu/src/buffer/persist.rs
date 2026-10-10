//! Persistent chunk storage: per-kind element arenas, residence, and deferred reclamation.
//!
//! A chunk the paint cache encoded is uploaded once and stays resident; a frame that replays it
//! in place points its draws at the resident bytes through the remap list and uploads nothing
//! for it. What a frame does upload is its transient content — fresh encodings arrive as chunk
//! insertions, and offset replays, outlines and unresolved sprites travel as per-frame copies —
//! plus the resolved remap lists themselves.
//!
//! Mark payloads are resident once per payload allocation, whatever chunks hold them. A canvas
//! panned by its view re-encodes its chunk with the same payload, and the new chunk takes a hold on
//! the payload the old one held, so the pan uploads no payload.
//!
//! Ranges are reclaimed through a ledger keyed by submission: a replaced or retired chunk's
//! ranges, and every frame's transient ranges, go into the current bucket, and the bucket is
//! offered back to the arenas once the device reports the submission that could still read them
//! complete. The same channel discipline as the upload belt.

use std::collections::{HashMap, VecDeque};

/// Ranges one submission may still read: which lane, and where in it.
type RetiredRanges = Vec<(usize, Range<u32>)>;
use std::ops::Range;
use std::sync::Arc;
use std::sync::mpsc;

use rustc_hash::FxHashMap;
use zgui_profile::{Counter, counter};
use zgui_scene::{ChunkPrims, PrimitiveKind, Scene};

use crate::buffer::upload::UploadBelt;
use crate::gpu::device::Gpu;

/// The instanced kinds with persistent storage, in lane order.
pub(crate) const LANES: [PrimitiveKind; 8] = [
    PrimitiveKind::Quad,
    PrimitiveKind::Shadow,
    PrimitiveKind::Decoration,
    PrimitiveKind::MonoSprite,
    PrimitiveKind::SubpixelSprite,
    PrimitiveKind::ColorSprite,
    PrimitiveKind::Shaded,
    PrimitiveKind::Marks,
];

/// The lane mark items live in.
pub(crate) const MARKS_LANE: usize = 7;

/// How many payload kinds a mark has: discs, boxes, polyline vertices and glyph words.
pub(crate) const PAYLOAD_LANES: usize = 4;

/// Every arena: one per lane, then one per payload kind.
const ARENAS: usize = LANES.len() + PAYLOAD_LANES;

/// The arena a payload kind lives in.
const fn payload_arena(kind: usize) -> usize {
    LANES.len() + kind
}

/// How many low bits of a resolved remap entry name the arena slot.
///
/// The bits above name an entry in the frame's offset table, which is how a chunk that merely
/// moved keeps its residence: the resident bytes stay where they are and the shader adds the
/// chunk's offset to what it reads. Twenty-four bits is sixteen million elements a lane — over a
/// gigabyte of quads — and a resident slot past it falls back to the transient gather, which is
/// correct and merely copies.
pub(crate) const SLOT_BITS: u32 = 24;

/// The mask that keeps a resolved remap entry's arena slot.
pub(crate) const SLOT_MASK: u32 = (1 << SLOT_BITS) - 1;

/// The bytes one kind holds in a chunk or in a frame's arrays, which share their field names.
///
/// One exhaustive match for every reader, so a new kind cannot be read through another kind's
/// array. A kind with no lane holds no bytes here.
macro_rules! lane_slice {
    ($prims:expr, $kind:expr) => {{
        let prims = &$prims;
        let bytes: &[u8] = match $kind {
            PrimitiveKind::Quad => bytemuck::cast_slice(&prims.quads),
            PrimitiveKind::Shadow => bytemuck::cast_slice(&prims.shadows),
            PrimitiveKind::Decoration => bytemuck::cast_slice(&prims.decorations),
            PrimitiveKind::MonoSprite => bytemuck::cast_slice(&prims.mono_sprites),
            PrimitiveKind::SubpixelSprite => bytemuck::cast_slice(&prims.subpixel_sprites),
            PrimitiveKind::ColorSprite => bytemuck::cast_slice(&prims.color_sprites),
            PrimitiveKind::Shaded => bytemuck::cast_slice(&prims.shaded),
            PrimitiveKind::Marks => bytemuck::cast_slice(&prims.marks),
            PrimitiveKind::GroupStart
            | PrimitiveKind::GroupEnd
            | PrimitiveKind::Vector
            | PrimitiveKind::External
            | PrimitiveKind::Backdrop => &[],
        };
        bytes
    }};
}

/// One resident chunk: its bytes, and where each lane's elements sit in the arenas.
#[derive(Debug)]
struct Resident {
    /// The bytes, shared with the paint cache's record — also the rebuild source.
    prims: Arc<ChunkPrims>,
    /// The element range each lane holds of the chunk, where it has elements of that kind.
    ranges: [Option<Range<u32>>; LANES.len()],
    /// The key of each mark's payload in [`ChunkStore::shared`], one per mark.
    payloads: Box<[usize]>,
}

/// One resident mark payload, held by every resident mark that draws it.
#[derive(Debug)]
struct Shared {
    /// The payload. Held so no other payload takes its address while the key lives.
    payload: Arc<zgui_scene::MarkPayload>,
    /// Where each payload kind sits in its arena, where the payload has elements of that kind.
    ranges: [Option<Range<u32>>; PAYLOAD_LANES],
    /// How many resident marks draw it.
    holders: u32,
}

/// The key a payload is resident under: the address of its allocation.
fn payload_key(payload: &Arc<zgui_scene::MarkPayload>) -> usize {
    Arc::as_ptr(payload) as usize
}

/// One payload kind of one mark, as bytes.
fn payload_slice(payload: &zgui_scene::MarkPayload, kind: usize) -> &[u8] {
    match kind {
        0 => bytemuck::cast_slice(&payload.discs),
        1 => bytemuck::cast_slice(&payload.boxes),
        2 => bytemuck::cast_slice(&payload.vertices),
        _ => bytemuck::cast_slice(&payload.glyphs),
    }
}

/// One persistent element arena: a buffer, a bump tail, and ranges given back by the ledger.
#[derive(Debug)]
struct Arena {
    /// The buffer, bound as the pipeline's instance storage.
    buffer: wgpu::Buffer,
    /// What it is called, so a driver message names it.
    label: &'static str,
    /// One element's size in bytes.
    element: u32,
    /// How many elements the buffer holds.
    capacity: u32,
    /// Changes whenever `buffer` changes identity, for bind-group cache invalidation.
    generation: u64,
    /// The first element never allocated.
    tail: u32,
    /// Reclaimed ranges, coalesced on insert, first-fit on allocation.
    free: Vec<Range<u32>>,
}

impl Arena {
    /// The smallest allocation, in bytes — a bind group has to name a buffer either way.
    const MINIMUM_BYTES: u64 = 256;

    /// An empty arena for elements of `element` bytes.
    fn new(gpu: &Gpu, label: &'static str, element: u32) -> Self {
        let capacity = (Self::MINIMUM_BYTES as u32 / element).max(1);
        Self {
            buffer: allocate(gpu, label, u64::from(capacity) * u64::from(element)),
            label,
            element,
            capacity,
            generation: 1,
            tail: 0,
            free: Vec::new(),
        }
    }

    /// Takes `len` contiguous elements, first from the free list, else from the tail.
    fn alloc(&mut self, len: u32) -> Option<Range<u32>> {
        if len == 0 {
            return Some(0..0);
        }
        if let Some(at) = self
            .free
            .iter()
            .position(|range| range.end - range.start >= len)
        {
            let range = self.free[at].clone();
            let taken = range.start..range.start + len;
            if range.end - range.start == len {
                // An ordered remove, because the order is what `free` merges by.
                self.free.remove(at);
            } else {
                self.free[at] = range.start + len..range.end;
            }
            return Some(taken);
        }
        if self.tail + len <= self.capacity {
            let taken = self.tail..self.tail + len;
            self.tail += len;
            return Some(taken);
        }
        None
    }

    /// Gives a range back, merging every neighbour it touches and retracting the tail.
    ///
    /// The list is kept sorted by start, which is what lets a return merge *both* of its
    /// neighbours: a drag frees and re-takes differently sized ranges every frame, and a list
    /// that merged only one side fragmented toward one entry per chunk — with every allocation
    /// scanning all of them.
    fn free(&mut self, range: Range<u32>) {
        if range.is_empty() {
            return;
        }
        let at = self.free.partition_point(|held| held.start < range.start);
        let merges_left = at > 0 && self.free[at - 1].end == range.start;
        let merges_right = at < self.free.len() && range.end == self.free[at].start;
        match (merges_left, merges_right) {
            (true, true) => {
                self.free[at - 1].end = self.free[at].end;
                self.free.remove(at);
            }
            (true, false) => self.free[at - 1].end = range.end,
            (false, true) => self.free[at].start = range.start,
            (false, false) => self.free.insert(at, range),
        }
        // A range that has come to abut the tail is not a fragment at all: giving it back to the
        // tail is what lets a fully drained arena allocate from zero again.
        while let Some(last) = self.free.last() {
            if last.end != self.tail {
                break;
            }
            self.tail = last.start;
            self.free.pop();
        }
    }

    /// Copies `bytes` over the elements starting at `start`.
    fn upload(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        start: u32,
        bytes: &[u8],
    ) -> u64 {
        if bytes.is_empty() {
            return 0;
        }
        belt.write(
            gpu,
            encoder,
            &self.buffer,
            u64::from(start) * u64::from(self.element),
            bytes,
        )
    }

    /// Replaces the buffer with one holding at least `needed` elements, forgetting every range.
    ///
    /// The caller re-uploads every resident chunk afterwards: this is the growth path, the
    /// idle-release path and the device-loss path, and they are deliberately one code path.
    fn reset_with_capacity(&mut self, gpu: &Gpu, needed: u32) {
        let bytes = (u64::from(needed) * u64::from(self.element))
            .next_power_of_two()
            .max(Self::MINIMUM_BYTES);
        self.capacity = (bytes / u64::from(self.element)) as u32;
        self.buffer = allocate(gpu, self.label, bytes);
        self.generation = self.generation.wrapping_add(1);
        self.tail = 0;
        self.free.clear();
    }
}

/// Allocates a storage buffer of `size` bytes.
fn allocate(gpu: &Gpu, label: &'static str, size: u64) -> wgpu::Buffer {
    gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Ranges awaiting reclamation, bucketed by the submission that could still read them.
#[derive(Debug)]
struct RetireLedger {
    /// The sequence number the next submission takes.
    seq: u64,
    /// The current frame's retirements, moved into `pending` at submission.
    current: RetiredRanges,
    /// Buckets not yet reported complete.
    pending: VecDeque<(u64, RetiredRanges)>,
    /// Where completions arrive from the device's callback thread.
    sender: mpsc::Sender<u64>,
    /// The receiving half, drained at reclamation.
    receiver: mpsc::Receiver<u64>,
}

impl RetireLedger {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            seq: 0,
            current: Vec::new(),
            pending: VecDeque::new(),
            sender,
            receiver,
        }
    }

    /// Notes ranges the frame being built stops using — reclaimable once this frame's
    /// submission completes.
    fn retire(&mut self, lane: usize, range: Range<u32>) {
        if !range.is_empty() {
            self.current.push((lane, range));
        }
    }

    /// Moves the frame's retirements into a bucket tied to the submission just made.
    fn submitted(&mut self, gpu: &Gpu) {
        if self.current.is_empty() {
            return;
        }
        self.seq += 1;
        let seq = self.seq;
        self.pending
            .push_back((seq, core::mem::take(&mut self.current)));
        let sender = self.sender.clone();
        gpu.queue().on_submitted_work_done(move || {
            let _ = sender.send(seq);
        });
    }

    /// Offers every bucket the device has finished with back to `arenas`.
    fn reclaim(&mut self, arenas: &mut [Arena; ARENAS]) {
        let mut completed = 0;
        while let Ok(seq) = self.receiver.try_recv() {
            completed = completed.max(seq);
        }
        if completed == 0 {
            return;
        }
        while let Some((seq, _)) = self.pending.front() {
            if *seq > completed {
                break;
            }
            let (_, ranges) = self.pending.pop_front().expect("front just answered");
            for (lane, range) in ranges {
                arenas[lane].free(range);
            }
        }
    }

    /// Forgets everything in flight — for the paths that replace the arenas wholesale.
    fn forget(&mut self) {
        self.current.clear();
        self.pending.clear();
        while self.receiver.try_recv().is_ok() {}
        self.seq = 0;
    }
}

/// The persistent halves of the six instanced kinds, and the residence over them.
#[derive(Debug)]
pub struct ChunkStore {
    /// One arena per lane, then one per mark payload kind.
    arenas: [Arena; ARENAS],
    /// Every chunk the arenas hold, by revision.
    residence: HashMap<u64, Resident>,
    /// Every mark payload the arenas hold, by [`payload_key`].
    shared: FxHashMap<usize, Shared>,
    /// Per-frame scratch: the payload keys of the chunks retired this frame, one per mark.
    released: Vec<usize>,
    /// Ranges awaiting their submission's completion.
    ledger: RetireLedger,
    /// Per-frame scratch: the resolved remap for each lane.
    resolved: [Vec<u32>; LANES.len()],
    /// Per-frame scratch: gathered transient element bytes for each lane.
    gathered: [Vec<u8>; LANES.len()],
    /// The frame's chunk offsets, indexed by the high bits of a resolved remap entry.
    ///
    /// Entry zero is the zero offset every unmoved element names, so a frame with nothing moved
    /// carries one entry and every remap entry's high bits are clear.
    frame_offsets: Vec<[f32; 2]>,
    /// Per-frame scratch: each moved revision's offset index, or the spill marker.
    ///
    /// A frame can name at most as many offsets as the remap's high bits can count. A revision
    /// past that spills to [`u32::MAX`] and is served transiently — the frame arrays hold its
    /// translated bytes — which is correct and merely copies.
    offset_of: HashMap<u64, u32>,
    /// Per-frame scratch: each mark's payload ranges by remap position, one per payload kind.
    mark_payload: Vec<[Range<u32>; PAYLOAD_LANES]>,
}

impl ChunkStore {
    /// Empty storage on `gpu`.
    pub fn new(gpu: &Gpu) -> Self {
        Self {
            arenas: [
                Arena::new(
                    gpu,
                    "zgui.arena.quads",
                    size_of::<zgui_scene::Quad>() as u32,
                ),
                Arena::new(
                    gpu,
                    "zgui.arena.shadows",
                    size_of::<zgui_scene::Shadow>() as u32,
                ),
                Arena::new(
                    gpu,
                    "zgui.arena.decorations",
                    size_of::<zgui_scene::Decoration>() as u32,
                ),
                Arena::new(
                    gpu,
                    "zgui.arena.mono_sprites",
                    size_of::<zgui_scene::MonoSprite>() as u32,
                ),
                Arena::new(
                    gpu,
                    "zgui.arena.subpixel_sprites",
                    size_of::<zgui_scene::SubpixelSprite>() as u32,
                ),
                Arena::new(
                    gpu,
                    "zgui.arena.color_sprites",
                    size_of::<zgui_scene::ColorSprite>() as u32,
                ),
                Arena::new(
                    gpu,
                    "zgui.arena.shaded",
                    size_of::<zgui_scene::ShadedQuad>() as u32,
                ),
                Arena::new(
                    gpu,
                    "zgui.arena.marks",
                    size_of::<zgui_scene::MarkItem>() as u32,
                ),
                Arena::new(gpu, "zgui.arena.mark_discs", size_of::<[f32; 4]>() as u32),
                Arena::new(
                    gpu,
                    "zgui.arena.mark_boxes",
                    size_of::<zgui_scene::MarkBox>() as u32,
                ),
                Arena::new(
                    gpu,
                    "zgui.arena.mark_vertices",
                    size_of::<[f32; 2]>() as u32,
                ),
                Arena::new(gpu, "zgui.arena.mark_glyphs", size_of::<[u32; 4]>() as u32),
            ],
            residence: HashMap::new(),
            shared: FxHashMap::default(),
            released: Vec::new(),
            ledger: RetireLedger::new(),
            resolved: Default::default(),
            gathered: Default::default(),
            frame_offsets: vec![[0.0, 0.0]],
            offset_of: HashMap::new(),
            mark_payload: Vec::new(),
        }
    }

    /// The buffer a pipeline binds as its instance storage for `lane`.
    pub fn binding(&self, lane: usize) -> wgpu::BindingResource<'_> {
        self.arenas[lane].buffer.as_entire_binding()
    }

    /// The allocation epoch of `lane`'s buffer, for bind-group cache keys.
    pub fn generation(&self, lane: usize) -> u64 {
        self.arenas[lane].generation
    }

    /// The buffer a mark draw binds as the payload of `kind`.
    pub fn payload_binding(&self, kind: usize) -> wgpu::BindingResource<'_> {
        self.arenas[payload_arena(kind)].buffer.as_entire_binding()
    }

    /// The allocation epoch of the payload buffer of `kind`.
    pub fn payload_generation(&self, kind: usize) -> u64 {
        self.arenas[payload_arena(kind)].generation
    }

    /// The payload ranges of the mark at draw-order `position`, one per payload kind.
    ///
    /// Empty ranges past the frame's marks.
    pub fn mark_payload(&self, position: usize) -> [Range<u32>; PAYLOAD_LANES] {
        self.mark_payload
            .get(position)
            .cloned()
            .unwrap_or([0..0, 0..0, 0..0, 0..0])
    }

    /// How many bytes the arenas hold.
    pub fn bytes(&self) -> u64 {
        self.arenas
            .iter()
            .map(|arena| u64::from(arena.capacity) * u64::from(arena.element))
            .sum()
    }

    /// Uploads the frame's chunk changes and transient content, and resolves each lane's remap
    /// into arena slots. Returns the bytes copied; the resolved remaps are in
    /// [`ChunkStore::resolved_remap`] afterwards.
    pub fn upload_frame(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
    ) -> u64 {
        self.ledger.reclaim(&mut self.arenas);
        let mut uploaded = 0;

        self.released.clear();
        for &revision in scene.chunk_retired() {
            if let Some(resident) = self.residence.remove(&revision) {
                for (lane, range) in resident.ranges.into_iter().enumerate() {
                    if let Some(range) = range {
                        self.ledger.retire(lane, range);
                    }
                }
                self.released.extend_from_slice(&resident.payloads);
            }
        }

        // The new chunks take their holds before the retired ones drop theirs, so a payload that
        // passes from one revision to the next stays resident.
        for upload in scene.chunk_inserted() {
            uploaded += self.insert(gpu, belt, encoder, upload.revision, &upload.prims);
        }
        let released = core::mem::take(&mut self.released);
        for key in &released {
            self.drop_hold(*key);
        }
        self.released = released;

        uploaded += self.resolve_and_gather(gpu, belt, encoder, scene);
        counter::set(Counter::ChunksResident, self.residence.len() as u64);
        counter::add(Counter::ChunkBytesUploaded, uploaded);
        uploaded
    }

    /// Uploads one chunk's lanes into the arenas, making it resident.
    fn insert(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        revision: u64,
        prims: &Arc<ChunkPrims>,
    ) -> u64 {
        if self.residence.contains_key(&revision) {
            return 0;
        }
        // A sprite still carrying a name rather than a placement is patched in the frame's own
        // arrays by resolution, and a resident copy would keep the placeholder. The chunk stays
        // transient; its fragment re-encodes when the content lands.
        if prims.mono_sprites.iter().any(|s| s.tile.is_unresolved())
            || prims
                .subpixel_sprites
                .iter()
                .any(|s| s.tile.is_unresolved())
            || prims.color_sprites.iter().any(|s| s.tile.is_unresolved())
        {
            return 0;
        }
        let mut ranges: [Option<Range<u32>>; LANES.len()] = Default::default();
        let mut uploaded = 0;
        for (lane, held) in ranges.iter_mut().enumerate() {
            let bytes = lane_slice!(prims, LANES[lane]);
            let count = (bytes.len() / self.arenas[lane].element as usize) as u32;
            if count == 0 {
                continue;
            }
            let range = self.alloc(gpu, belt, encoder, lane, count, &mut uploaded);
            uploaded += self.arenas[lane].upload(gpu, belt, encoder, range.start, bytes);
            *held = Some(range);
        }
        // The payloads no resident mark holds yet are placed one after another in one range per
        // payload kind, and uploaded in one write each.
        let mut payloads = Vec::with_capacity(prims.mark_payloads.len());
        let mut fresh = Vec::new();
        let mut totals = [0u32; PAYLOAD_LANES];
        for payload in &prims.mark_payloads {
            let key = payload_key(payload);
            payloads.push(key);
            if let Some(shared) = self.shared.get_mut(&key) {
                shared.holders += 1;
                continue;
            }
            self.shared.insert(
                key,
                Shared {
                    payload: Arc::clone(payload),
                    ranges: Default::default(),
                    holders: 1,
                },
            );
            fresh.push(key);
            for (kind, count) in payload.counts().into_iter().enumerate() {
                totals[kind] += count;
            }
        }
        for (kind, &total) in totals.iter().enumerate() {
            if total == 0 {
                continue;
            }
            let arena = payload_arena(kind);
            let range = self.alloc(gpu, belt, encoder, arena, total, &mut uploaded);
            let mut bytes =
                Vec::with_capacity(total as usize * self.arenas[arena].element as usize);
            let mut start = range.start;
            for key in &fresh {
                let shared = self.shared.get_mut(key).expect("inserted above");
                let slice = payload_slice(&shared.payload, kind);
                let count = (slice.len() / self.arenas[arena].element as usize) as u32;
                if count == 0 {
                    continue;
                }
                bytes.extend_from_slice(slice);
                shared.ranges[kind] = Some(start..start + count);
                start += count;
            }
            let written = self.arenas[arena].upload(gpu, belt, encoder, range.start, &bytes);
            counter::add(Counter::MarksPayloadBytes, written);
            uploaded += written;
        }
        self.residence.insert(
            revision,
            Resident {
                prims: Arc::clone(prims),
                ranges,
                payloads: payloads.into_boxed_slice(),
            },
        );
        uploaded
    }

    /// Takes `count` elements of `arena`, growing it when they do not fit, and adds what the
    /// growth uploaded to `uploaded`.
    fn alloc(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        arena: usize,
        count: u32,
        uploaded: &mut u64,
    ) -> Range<u32> {
        if let Some(range) = self.arenas[arena].alloc(count) {
            return range;
        }
        // Grow the arena and settle everything resident into the new buffer, then take the range
        // that now must fit.
        *uploaded += self.grow(gpu, belt, encoder, arena, count);
        self.arenas[arena]
            .alloc(count)
            .expect("the arena was grown for exactly this request")
    }

    /// Drops one hold on the payload under `key`, and retires its ranges when none is left.
    fn drop_hold(&mut self, key: usize) {
        let Some(shared) = self.shared.get_mut(&key) else {
            return;
        };
        shared.holders = shared.holders.saturating_sub(1);
        if shared.holders > 0 {
            return;
        }
        let shared = self.shared.remove(&key).expect("just found");
        for (kind, range) in shared.ranges.into_iter().enumerate() {
            if let Some(range) = range {
                self.ledger.retire(payload_arena(kind), range);
            }
        }
    }

    /// Replaces `lane`'s buffer with one that fits everything resident plus `incoming`, and
    /// re-uploads every resident chunk's lane, or every resident payload of a payload arena.
    ///
    /// Nothing in flight can be corrupted: the old buffer is dropped, and the device keeps it
    /// alive until the submissions reading it complete. The ledger's claims on the old buffer
    /// are meaningless afterwards, so they are forgotten with it.
    fn grow(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        lane: usize,
        incoming: u32,
    ) -> u64 {
        let length = |range: &Option<Range<u32>>| range.as_ref().map_or(0, |r| r.end - r.start);
        let live: u32 = match lane.checked_sub(LANES.len()) {
            None => self
                .residence
                .values()
                .map(|resident| length(&resident.ranges[lane]))
                .sum(),
            Some(kind) => self
                .shared
                .values()
                .map(|shared| length(&shared.ranges[kind]))
                .sum(),
        };
        // Double past the need, so a growth is rare rather than per-insert. Ranges still in flight
        // count toward no need, so a growth also at least doubles what the arena held: a lane of a
        // few transient elements would otherwise grow to the same size every few frames.
        let needed = (live + incoming)
            .saturating_mul(2)
            .max(self.arenas[lane].capacity.saturating_mul(2));
        self.arenas[lane].reset_with_capacity(gpu, needed);
        // Every ledger bucket may name ranges in the replaced buffer of this lane; forgetting
        // them all over-forgets other lanes' pending ranges, which costs those elements until
        // their own lanes grow. Rare enough to prefer over per-lane bookkeeping.
        self.ledger.forget();
        let mut uploaded = 0;
        if let Some(kind) = lane.checked_sub(LANES.len()) {
            let arena = &mut self.arenas[lane];
            for shared in self.shared.values_mut() {
                let bytes = payload_slice(&shared.payload, kind);
                let count = (bytes.len() as u32) / arena.element;
                // A payload being inserted has no range yet, and takes one after the growth.
                if count == 0 || shared.ranges[kind].is_none() {
                    continue;
                }
                let range = arena
                    .alloc(count)
                    .expect("the arena was sized for everything resident");
                let written = arena.upload(gpu, belt, encoder, range.start, bytes);
                counter::add(Counter::MarksPayloadBytes, written);
                uploaded += written;
                shared.ranges[kind] = Some(range);
            }
            return uploaded;
        }
        let arena = &mut self.arenas[lane];
        for resident in self.residence.values_mut() {
            let bytes = lane_slice!(resident.prims, LANES[lane]);
            let count = (bytes.len() as u32) / arena.element;
            if count == 0 {
                continue;
            }
            let range = arena
                .alloc(count)
                .expect("the arena was sized for everything resident");
            uploaded += arena.upload(gpu, belt, encoder, range.start, bytes);
            resident.ranges[lane] = Some(range);
        }
        uploaded
    }

    /// Builds each lane's resolved remap — arena slots in draw order — gathering transient
    /// content into per-frame ranges, and uploads the gathered bytes.
    fn resolve_and_gather(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
    ) -> u64 {
        let mut uploaded = 0;
        self.frame_offsets.clear();
        self.frame_offsets.push([0.0, 0.0]);
        self.offset_of.clear();
        for (&revision, &offset) in scene.chunk_offsets() {
            let index = if self.frame_offsets.len() <= (u32::MAX >> SLOT_BITS) as usize {
                let index = self.frame_offsets.len() as u32;
                self.frame_offsets.push(offset);
                index
            } else {
                u32::MAX
            };
            self.offset_of.insert(revision, index);
        }
        for (lane, &kind) in LANES.iter().enumerate() {
            let remap = scene.remap(kind);
            let provenance = scene.provenance(kind);
            let element = self.arenas[lane].element as usize;
            self.resolved[lane].clear();
            self.gathered[lane].clear();

            // First pass: how many positions cannot be served from a resident chunk. The test is
            // the same call the second pass resolves with, so the two can never disagree.
            let transients = remap
                .iter()
                .filter(|&&index| {
                    resident_slot(
                        &self.residence,
                        &self.offset_of,
                        lane,
                        &provenance[index as usize],
                    )
                    .is_none()
                })
                .count() as u32;
            let transient_range = if transients > 0 {
                match self.arenas[lane].alloc(transients) {
                    Some(range) => range,
                    None => {
                        uploaded += self.grow(gpu, belt, encoder, lane, transients);
                        self.arenas[lane]
                            .alloc(transients)
                            .expect("the arena was grown for exactly this request")
                    }
                }
            } else {
                0..0
            };
            debug_assert!(
                transient_range.end <= SLOT_MASK,
                "a lane's transient range left the slot bits; see SLOT_BITS"
            );

            let bytes = lane_bytes(scene, lane);
            let mut placed = 0;
            for &index in remap {
                let slot = provenance[index as usize];
                match resident_slot(&self.residence, &self.offset_of, lane, &slot) {
                    Some(at) => self.resolved[lane].push(at),
                    None => {
                        let at = index as usize * element;
                        self.gathered[lane].extend_from_slice(&bytes[at..at + element]);
                        self.resolved[lane].push(transient_range.start + placed);
                        placed += 1;
                    }
                }
            }
            debug_assert_eq!(placed, transients);
            if !self.gathered[lane].is_empty() {
                let gathered = core::mem::take(&mut self.gathered[lane]);
                uploaded +=
                    self.arenas[lane].upload(gpu, belt, encoder, transient_range.start, &gathered);
                self.gathered[lane] = gathered;
            }
            // This frame's transient elements are reclaimable once its submission completes.
            self.ledger.retire(lane, transient_range);
        }
        uploaded += self.resolve_payloads(gpu, belt, encoder, scene);
        uploaded
    }

    /// Finds each mark's payload ranges, by remap position: in the resident payload its resident
    /// chunk holds, in a resident payload a transient mark draws as well, or gathered into
    /// per-frame ranges.
    ///
    /// The test is the one the lane's own resolution made, so a resident item always reads the
    /// payload its chunk holds.
    fn resolve_payloads(
        &mut self,
        gpu: &Gpu,
        belt: &mut UploadBelt,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
    ) -> u64 {
        self.mark_payload.clear();
        let remap = scene.remap(PrimitiveKind::Marks);
        if remap.is_empty() {
            return 0;
        }
        let provenance = scene.provenance(PrimitiveKind::Marks);
        let payloads = &scene.primitives.mark_payloads;
        let resident = |store: &Self, index: u32| {
            resident_slot(
                &store.residence,
                &store.offset_of,
                MARKS_LANE,
                &provenance[index as usize],
            )
            .is_some()
        };
        // A mark served transiently gathers its payload only when no resident payload is it.
        let gathers = |store: &Self, index: u32| {
            !resident(store, index)
                && !store
                    .shared
                    .contains_key(&payload_key(&payloads[index as usize]))
        };
        let mut transient = [0u32; PAYLOAD_LANES];
        for &index in remap {
            if gathers(self, index) {
                for (kind, count) in payloads[index as usize].counts().into_iter().enumerate() {
                    transient[kind] += count;
                }
            }
        }
        let mut uploaded = 0;
        let mut ranges: [Range<u32>; PAYLOAD_LANES] = [0..0, 0..0, 0..0, 0..0];
        for (kind, range) in ranges.iter_mut().enumerate() {
            if transient[kind] == 0 {
                continue;
            }
            let arena = payload_arena(kind);
            *range = match self.arenas[arena].alloc(transient[kind]) {
                Some(range) => range,
                None => {
                    uploaded += self.grow(gpu, belt, encoder, arena, transient[kind]);
                    self.arenas[arena]
                        .alloc(transient[kind])
                        .expect("the arena was grown for exactly this request")
                }
            };
        }
        let mut gathered: [Vec<u8>; PAYLOAD_LANES] = Default::default();
        let mut placed = [0u32; PAYLOAD_LANES];
        for &index in remap {
            let payload = &payloads[index as usize];
            let counts = payload.counts();
            let slot = provenance[index as usize];
            let key = if resident(self, index) {
                Some(self.residence[&slot.revision].payloads[slot.index as usize])
            } else {
                let key = payload_key(payload);
                self.shared.contains_key(&key).then_some(key)
            };
            let at = if let Some(key) = key {
                let held = &self.shared[&key].ranges;
                core::array::from_fn(|kind| held[kind].as_ref().map_or(0, |range| range.start))
            } else {
                let at: [u32; PAYLOAD_LANES] =
                    core::array::from_fn(|kind| ranges[kind].start + placed[kind]);
                for kind in 0..PAYLOAD_LANES {
                    gathered[kind].extend_from_slice(payload_slice(payload, kind));
                    placed[kind] += counts[kind];
                }
                at
            };
            self.mark_payload.push(core::array::from_fn(|kind| {
                at[kind]..at[kind] + counts[kind]
            }));
        }
        for (kind, bytes) in gathered.iter().enumerate() {
            let arena = payload_arena(kind);
            let written = self.arenas[arena].upload(gpu, belt, encoder, ranges[kind].start, bytes);
            counter::add(Counter::MarksPayloadBytes, written);
            uploaded += written;
            self.ledger.retire(arena, ranges[kind].clone());
        }
        uploaded
    }

    /// The resolved remap for `lane`: packed offset-and-slot entries, in draw order.
    pub fn resolved_remap(&self, lane: usize) -> &[u32] {
        &self.resolved[lane]
    }

    /// The frame's chunk offsets, indexed by the high bits of a resolved remap entry.
    pub fn frame_offsets(&self) -> &[[f32; 2]] {
        &self.frame_offsets
    }

    /// Ties the frame's retirements to the submission just made.
    pub fn submitted(&mut self, gpu: &Gpu) {
        self.ledger.submitted(gpu);
    }

    /// Drops everything resident and shrinks the arenas to their minimum.
    ///
    /// The next frames serve every chunk transiently until its fragment encodes again, which is
    /// correct and slower — the price of giving the memory back.
    pub fn release(&mut self, gpu: &Gpu) -> u64 {
        let before = self.bytes();
        self.residence.clear();
        self.shared.clear();
        self.ledger.forget();
        for arena in &mut self.arenas {
            arena.reset_with_capacity(gpu, 0);
        }
        before.saturating_sub(self.bytes())
    }
}

/// The packed remap entry serving one primitive from a resident chunk, if one can.
///
/// `None` is the transient answer, for every reason there is: the primitive is transient by
/// provenance, its chunk is not resident, the chunk holds nothing in this lane, the resident
/// slot lies past what the slot bits can name, or the chunk moved this frame and the offset
/// table was already full.
fn resident_slot(
    residence: &HashMap<u64, Resident>,
    offset_of: &HashMap<u64, u32>,
    lane: usize,
    slot: &zgui_scene::ChunkSlot,
) -> Option<u32> {
    if slot.is_transient() {
        return None;
    }
    let offset = match offset_of.get(&slot.revision) {
        Some(&u32::MAX) => return None,
        Some(&index) => index,
        None => 0,
    };
    let range = residence.get(&slot.revision)?.ranges[lane].as_ref()?;
    let at = range.start + slot.index;
    (at <= SLOT_MASK).then_some((offset << SLOT_BITS) | at)
}

/// The frame array of `lane`, as bytes.
fn lane_bytes(scene: &Scene, lane: usize) -> &[u8] {
    lane_slice!(scene.primitives, LANES[lane])
}
