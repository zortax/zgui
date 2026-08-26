//! Persistent chunk storage: per-kind element arenas, residence, and deferred reclamation.
//!
//! A chunk the paint cache encoded is uploaded once and stays resident; a frame that replays it
//! in place points its draws at the resident bytes through the remap list and uploads nothing
//! for it. What a frame does upload is its transient content — fresh encodings arrive as chunk
//! insertions, and offset replays, outlines and unresolved sprites travel as per-frame copies —
//! plus the resolved remap lists themselves.
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

use zgui_profile::{Counter, counter};
use zgui_scene::{ChunkPrims, PrimitiveKind, Scene};

use crate::buffer::upload::UploadBelt;
use crate::gpu::device::Gpu;

/// The instanced kinds with persistent storage, in lane order.
pub(crate) const LANES: [PrimitiveKind; 6] = [
    PrimitiveKind::Quad,
    PrimitiveKind::Shadow,
    PrimitiveKind::Decoration,
    PrimitiveKind::MonoSprite,
    PrimitiveKind::SubpixelSprite,
    PrimitiveKind::ColorSprite,
];

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

/// One resident chunk: its bytes, and where each lane's elements sit in the arenas.
#[derive(Debug)]
struct Resident {
    /// The bytes, shared with the paint cache's record — also the rebuild source.
    prims: Arc<ChunkPrims>,
    /// The element range each lane occupies, where the chunk has elements of that kind.
    ranges: [Option<Range<u32>>; 6],
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
    fn reclaim(&mut self, arenas: &mut [Arena; 6]) {
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
    /// One arena per lane.
    arenas: [Arena; 6],
    /// Every chunk the arenas hold, by revision.
    residence: HashMap<u64, Resident>,
    /// Ranges awaiting their submission's completion.
    ledger: RetireLedger,
    /// Per-frame scratch: the resolved remap for each lane.
    resolved: [Vec<u32>; 6],
    /// Per-frame scratch: gathered transient element bytes for each lane.
    gathered: [Vec<u8>; 6],
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
            ],
            residence: HashMap::new(),
            ledger: RetireLedger::new(),
            resolved: Default::default(),
            gathered: Default::default(),
            frame_offsets: vec![[0.0, 0.0]],
            offset_of: HashMap::new(),
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

        for &revision in scene.chunk_retired() {
            if let Some(resident) = self.residence.remove(&revision) {
                for (lane, range) in resident.ranges.into_iter().enumerate() {
                    if let Some(range) = range {
                        self.ledger.retire(lane, range);
                    }
                }
            }
        }

        for upload in scene.chunk_inserted() {
            uploaded += self.insert(gpu, belt, encoder, upload.revision, &upload.prims);
        }

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
        let lanes: [&[u8]; 6] = [
            bytemuck::cast_slice(&prims.quads),
            bytemuck::cast_slice(&prims.shadows),
            bytemuck::cast_slice(&prims.decorations),
            bytemuck::cast_slice(&prims.mono_sprites),
            bytemuck::cast_slice(&prims.subpixel_sprites),
            bytemuck::cast_slice(&prims.color_sprites),
        ];
        let counts = [
            prims.quads.len() as u32,
            prims.shadows.len() as u32,
            prims.decorations.len() as u32,
            prims.mono_sprites.len() as u32,
            prims.subpixel_sprites.len() as u32,
            prims.color_sprites.len() as u32,
        ];
        let mut ranges: [Option<Range<u32>>; 6] = Default::default();
        let mut uploaded = 0;
        for lane in 0..6 {
            if counts[lane] == 0 {
                continue;
            }
            let range = match self.arenas[lane].alloc(counts[lane]) {
                Some(range) => range,
                None => {
                    // Grow the lane and settle every resident chunk into the new buffer, then
                    // take the range that now must fit.
                    uploaded += self.grow(gpu, belt, encoder, lane, counts[lane]);
                    self.arenas[lane]
                        .alloc(counts[lane])
                        .expect("the arena was grown for exactly this request")
                }
            };
            uploaded += self.arenas[lane].upload(gpu, belt, encoder, range.start, lanes[lane]);
            ranges[lane] = Some(range);
        }
        self.residence.insert(
            revision,
            Resident {
                prims: Arc::clone(prims),
                ranges,
            },
        );
        uploaded
    }

    /// Replaces `lane`'s buffer with one that fits everything resident plus `incoming`, and
    /// re-uploads every resident chunk's lane.
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
        let live: u32 = self
            .residence
            .values()
            .map(|resident| {
                resident.ranges[lane]
                    .as_ref()
                    .map_or(0, |range| range.end - range.start)
            })
            .sum();
        // Double past the need, so a growth is rare rather than per-insert.
        let needed = (live + incoming).saturating_mul(2);
        self.arenas[lane].reset_with_capacity(gpu, needed);
        // Every ledger bucket may name ranges in the replaced buffer of this lane; forgetting
        // them all over-forgets other lanes' pending ranges, which costs those elements until
        // their own lanes grow. Rare enough to prefer over per-lane bookkeeping.
        self.ledger.forget();
        let mut uploaded = 0;
        let revisions: Vec<u64> = self.residence.keys().copied().collect();
        for revision in revisions {
            let (bytes, count) = {
                let resident = &self.residence[&revision];
                let bytes: Vec<u8> = match lane {
                    0 => bytemuck::cast_slice(&resident.prims.quads).to_vec(),
                    1 => bytemuck::cast_slice(&resident.prims.shadows).to_vec(),
                    2 => bytemuck::cast_slice(&resident.prims.decorations).to_vec(),
                    3 => bytemuck::cast_slice(&resident.prims.mono_sprites).to_vec(),
                    4 => bytemuck::cast_slice(&resident.prims.subpixel_sprites).to_vec(),
                    _ => bytemuck::cast_slice(&resident.prims.color_sprites).to_vec(),
                };
                let count = (bytes.len() as u32) / self.arenas[lane].element;
                (bytes, count)
            };
            if count == 0 {
                continue;
            }
            let range = self.arenas[lane]
                .alloc(count)
                .expect("the arena was sized for everything resident");
            uploaded += self.arenas[lane].upload(gpu, belt, encoder, range.start, &bytes);
            self.residence
                .get_mut(&revision)
                .expect("iterating known keys")
                .ranges[lane] = Some(range);
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
    match lane {
        0 => bytemuck::cast_slice(&scene.primitives.quads),
        1 => bytemuck::cast_slice(&scene.primitives.shadows),
        2 => bytemuck::cast_slice(&scene.primitives.decorations),
        3 => bytemuck::cast_slice(&scene.primitives.mono_sprites),
        4 => bytemuck::cast_slice(&scene.primitives.subpixel_sprites),
        _ => bytemuck::cast_slice(&scene.primitives.color_sprites),
    }
}
