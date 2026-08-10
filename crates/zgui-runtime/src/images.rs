//! Loading the pictures `image` elements name, and owning what was decoded.
//!
//! This is the wiring the content cache deliberately does not have: it hears `src` change through
//! the backend's attribute hook, decodes off the frame thread through
//! [`blocking`](zgui_reactive::blocking), and lands the result in the one frame step that may
//! mutate the document — [`ImageLoader::settle`]. Everything is keyed by *source*, not by node:
//! ten elements showing one file cost one decode, one buffer and one atlas tile.
//!
//! # How a picture travels
//!
//! 1. A view writes `src`. The attribute hook queues `(node, value)`; nothing else happens where
//!    the write happened.
//! 2. `settle` drains the queue. A source seen for the first time claims the node in the
//!    intrinsics table — known and unsized — and kicks a *probe*: a header read that reports the
//!    extent without decoding a pixel. The probe's completion files the intrinsic, and layout
//!    sizes the box from it.
//! 3. After layout, [`ImageLoader::observe_demand`] reads each shown node's content box and
//!    records how many device pixels the picture actually needs. A source whose demand has no
//!    decoded variant yet kicks a decode *at that size*: a photo shown as a thumbnail is decoded
//!    to the thumbnail. The decode does not wait for the probe — a box with a styled size has a
//!    demand before the extent is known, and the two reads run side by side.
//! 4. A later `settle` lands the decode and attaches the texels to every element showing the
//!    source. Layout and paint then treat it like any other replaced content.
//!
//! # Variants
//!
//! One source may be decoded at several sizes over its life — a panel grew, the window moved to a
//! denser display. Each decode target is a *class*: a long-edge figure from a small ladder, so a
//! box oscillating by a pixel does not oscillate the decode. A grown demand decodes a larger
//! class while the smaller stays attached, and the swap happens when the better one lands; a
//! shrunken demand re-attaches a smaller class that still exists and decodes nothing.
//!
//! # What eviction means here
//!
//! The loader is the one owner of decoded texels that *can* produce them again, so these bytes
//! are honestly evictable: variants no element is attached to go first, then entries no live
//! element shows, and a forgotten entry that is still shown is re-kicked from its source on the
//! next settle. The intrinsics survive eviction on purpose — a page must not reflow because a
//! cache was trimmed.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::host::replaced::{Intrinsic, ReplacedId};
use zgui_dom::{Document, NodeKey};
use zgui_geom::{CssPx, Device, Size};
use zgui_layout::tree::store::LayoutStore;
use zgui_paint::ContentCache;

use crate::replaced::IntrinsicTable;

/// What one `src` string names.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum SourceKey {
    /// A file on disk, decoded through [`zgui_image::decode_file_scaled`].
    Path(String),
    /// Registered in-memory bytes, resolved through [`zgui_image::bytes_for_url`].
    Bytes(String),
}

impl SourceKey {
    /// Classifies one `src` value.
    fn of(src: &str) -> Self {
        if src.starts_with("zgui-bytes:") {
            Self::Bytes(src.to_owned())
        } else {
            Self::Path(src.to_owned())
        }
    }
}

/// A decode target: the long edge a variant is decoded to, in device pixels.
type Class = u32;

/// The classes a demand is rounded up to.
///
/// Steps rather than exact sizes, so a box that grows by a pixel reuses the variant it has. A
/// demand over the last step decodes at the source's own long edge, capped by the decode limit.
const LADDER: [Class; 6] = [64, 128, 256, 512, 1024, 2048];

/// The long edge above which a variant gets a texture of its own, with levels of detail.
///
/// Below it, a variant shares an atlas page and its shrink stays within the band bilinear
/// filtering handles well, because the decode was sized by the box. Above it, the tile would
/// dominate a shared page anyway, and a page cannot carry levels of detail — so the variant is
/// cached standalone and its mip chain is what keeps a further shrink alias-free.
const STANDALONE_LONG_EDGE: u32 = 512;

/// One finished decode: the texels, and their levels of detail when the variant is large.
struct DecodedVariant {
    /// The texels at the variant's own size.
    decoded: zgui_image::Decoded,
    /// Levels of detail below them; empty for a variant small enough to share an atlas page.
    mips: Vec<zgui_image::Decoded>,
}

impl DecodedVariant {
    /// How many decoded bytes this holds, levels included.
    fn held_bytes(&self) -> u64 {
        self.decoded.texels.len() as u64
            + self
                .mips
                .iter()
                .map(|level| level.texels.len() as u64)
                .sum::<u64>()
    }
}

/// Texels below this threshold keep their host copy after the tile is resident.
///
/// A small picture costs almost nothing to hold and a visible blink to re-decode after a lost
/// device; a large one is the other way round. Sixty-four kibibytes is a 128-square RGBA image.
pub(crate) const RETAIN_SMALL_BYTES: u64 = 64 * 1024;

/// Where one variant's decode is.
enum VariantState {
    /// The decode is in flight.
    Pending,
    /// Decoded, held, and attachable.
    Ready(DecodedVariant),
    /// Decoded, uploaded, and the host copy given back; the tile alone serves it.
    ///
    /// The steady state of a large picture. A frame that finds the tile gone reports it, and
    /// [`ImageLoader::redecode_missing`] runs the decode again from the source.
    Uploaded {
        /// The extent the variant was decoded at.
        size: Size<u32, Device>,
    },
    /// The decode failed; the memo is what stops a broken source being retried every frame.
    Failed,
}

/// One decoded size of one source.
struct Variant {
    /// The atlas handle every node showing this variant shares.
    handle: u64,
    /// Where the decode is.
    state: VariantState,
}

impl Variant {
    /// How many decoded bytes this variant holds.
    fn held_bytes(&self) -> u64 {
        match &self.state {
            VariantState::Ready(decoded) => decoded.held_bytes(),
            _ => 0,
        }
    }
}

/// Where one source's header probe is.
enum Probe {
    /// The header read is in flight.
    Pending,
    /// The extent is known and filed.
    Done,
    /// The probe failed; the nodes stay blank and unsized.
    Failed,
}

/// One source and everything showing it.
struct Entry {
    /// The source's own extent, once the probe has reported it.
    intrinsic: Option<Size<u32, Device>>,
    /// Where the header probe is.
    probe: Probe,
    /// Every size this source has been decoded at.
    variants: FxHashMap<Class, Variant>,
    /// The class currently attached to the nodes.
    active: Option<Class>,
    /// The long edge layout most recently asked for, in device pixels.
    demanded: Option<u32>,
    /// The nodes currently showing this source.
    nodes: FxHashSet<NodeKey>,
    /// Whether some of `nodes` still have to be attached to the best variant.
    attach_owed: bool,
    /// When the last node left, from the loader's clock; meaningful while `nodes` is empty.
    ///
    /// What makes "coldest first" true rather than a doc comment about hash order: eviction sorts
    /// the orphans by it and takes the oldest.
    orphaned_at: u64,
}

impl Entry {
    /// A fresh entry with its probe in flight.
    fn probing() -> Self {
        Self {
            intrinsic: None,
            probe: Probe::Pending,
            variants: FxHashMap::default(),
            active: None,
            demanded: None,
            nodes: FxHashSet::default(),
            attach_owed: false,
            orphaned_at: 0,
        }
    }

    /// How many decoded bytes this entry holds, over every variant.
    fn held_bytes(&self) -> u64 {
        self.variants.values().map(Variant::held_bytes).sum()
    }

    /// The class the current demand asks to have decoded, or `None` before there is a demand.
    ///
    /// The extent caps the class once the probe has reported it, so a huge box over a small
    /// picture does not name a class the source cannot fill. Before the probe answers, the
    /// demand alone decides — the decode itself never upscales, so an uncapped class costs
    /// nothing beyond a key naming more than the texels deliver, and waiting for the probe would
    /// serialise two reads that can run side by side.
    fn target_class(&self, max_dimension: u32) -> Option<Class> {
        let demand = self.demanded?;
        let step = LADDER
            .into_iter()
            .find(|step| *step >= demand)
            .unwrap_or(max_dimension);
        let mut target = step.min(max_dimension);
        if let Some(native) = self.intrinsic {
            target = target.min(native.width.max(native.height).max(1).min(max_dimension));
        }
        Some(target)
    }

    /// The class that should be attached: the smallest attachable one that satisfies the target,
    /// or the largest attachable one when none does.
    ///
    /// Attachable means decoded and held, or uploaded with the tile standing in for the texels.
    fn best_attachable(&self, max_dimension: u32) -> Option<Class> {
        let attachable = |class: &Class| {
            matches!(
                self.variants.get(class).map(|variant| &variant.state),
                Some(VariantState::Ready(_) | VariantState::Uploaded { .. })
            )
        };
        let target = self.target_class(max_dimension);
        let sufficient = self
            .variants
            .keys()
            .filter(|class| attachable(class))
            .filter(|&&class| target.is_some_and(|target| class >= target))
            .min()
            .copied();
        sufficient.or_else(|| {
            self.variants
                .keys()
                .filter(|class| attachable(class))
                .max()
                .copied()
        })
    }
}

/// What one background task delivered.
enum Arrival {
    /// A header probe's answer.
    Probe(Result<Size<u32, Device>, zgui_image::DecodeError>),
    /// One variant's decode.
    Variant(Class, Result<DecodedVariant, zgui_image::DecodeError>),
}

/// The `src` writes the attribute hook heard, waiting for the settle that applies them.
///
/// Shared rather than owned because the hook that fills it and the loader that drains it are
/// installed at different moments and neither outlives the other.
type SourceQueue = Rc<RefCell<Vec<(NodeKey, Option<String>)>>>;

/// The probes and decodes that finished, waiting for the same settle.
type DecodeQueue = Rc<RefCell<Vec<(SourceKey, Arrival)>>>;

/// The loader: every source an `image` element of one window has named, and what became of it.
pub(crate) struct ImageLoader {
    /// Everything by source.
    entries: FxHashMap<SourceKey, Entry>,
    /// Which source each node shows.
    by_node: FxHashMap<NodeKey, SourceKey>,
    /// The images slot of the window's replaced-content mux.
    intrinsics: Arc<IntrinsicTable>,
    /// What the attribute hook heard since the last settle.
    pending: SourceQueue,
    /// What the background tasks produced since the last settle.
    completed: DecodeQueue,
    /// The bound every decode is held to, from the atlas's own limit.
    limits: zgui_image::Limits,
    /// The next atlas handle, allocated per variant and never reused.
    next_handle: u64,
    /// How many background tasks have ever been kicked; a step that moved it owes a frame.
    kicked: u64,
    /// A monotonic tick, moved by every settle and retain, for stamping when an entry orphaned.
    clock: u64,
    /// Decoded bytes held across all sources, recounted whenever a state changes.
    held_bytes: u64,
    /// The subset of [`ImageLoader::held_bytes`] nothing pins: variants that are not attached,
    /// and every variant of a source no node shows.
    evictable_bytes: u64,
}

impl ImageLoader {
    /// A loader writing intrinsics into `intrinsics`, decoding no larger than `max_dimension`.
    pub(crate) fn new(intrinsics: Arc<IntrinsicTable>, max_dimension: u32) -> Self {
        Self {
            entries: FxHashMap::default(),
            by_node: FxHashMap::default(),
            intrinsics,
            pending: Rc::new(RefCell::new(Vec::new())),
            completed: Rc::new(RefCell::new(Vec::new())),
            limits: zgui_image::Limits { max_dimension },
            next_handle: 0,
            kicked: 0,
            clock: 0,
            held_bytes: 0,
            evictable_bytes: 0,
        }
    }

    /// The queue the attribute hook pushes `src` changes into.
    pub(crate) fn source_queue(&self) -> SourceQueue {
        Rc::clone(&self.pending)
    }

    /// Whether anything is waiting for a settle: `src` writes, or finished background work.
    pub(crate) fn has_arrivals(&self) -> bool {
        !self.pending.borrow().is_empty() || !self.completed.borrow().is_empty()
    }

    /// Applies everything that arrived since the last frame: `src` writes, finished probes and
    /// finished decodes.
    ///
    /// Runs in the frame after the reactive flush and before the restyle, which is what lets a
    /// decode that landed during the flush be *shown* by the same frame. Every node whose content
    /// or intrinsic changed is marked through
    /// [`Document::replaced_content_changed`], so the steps below this one rebuild exactly the
    /// boxes that need it.
    ///
    /// Returns whether it spawned background tasks. The caller owes those a frame: a task spawned
    /// *here* runs after the frame's own flush, so nothing has polled it yet, and a future that
    /// has never been polled has registered no waker for its completion to fire. One more frame
    /// polls it; from then on the wake edge carries it.
    #[must_use = "a settle that kicked work is owed the frame that will poll it"]
    pub(crate) fn settle(
        &mut self,
        document: &Rc<RefCell<Document>>,
        content: &mut ContentCache,
    ) -> bool {
        let kicked_before = self.kicked;
        self.clock += 1;
        let mut touched: Vec<NodeKey> = Vec::new();

        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for (node, src) in pending {
            self.set_source(node, src.as_deref(), content, &mut touched);
        }

        let completed = std::mem::take(&mut *self.completed.borrow_mut());
        for (key, arrival) in completed {
            self.land(key, arrival, &mut touched);
        }

        self.ensure_and_attach(content, &mut touched);
        self.recount();

        if !touched.is_empty() {
            let mut document = document.borrow_mut();
            for node in touched {
                if let Some(index) = document.store().index_of(node) {
                    document.replaced_content_changed(index);
                }
            }
        }
        self.kicked != kicked_before
    }

    /// Reads what layout decided each picture's box is, and kicks the decodes those sizes need.
    ///
    /// Runs after layout, which is the first moment the demand exists. A node with no fragment —
    /// culled, or not laid out yet — keeps its last demand rather than forgetting it.
    ///
    /// Returns whether it spawned decode tasks, under the same contract as
    /// [`ImageLoader::settle`]: the caller owes them a frame.
    #[must_use = "a demand that kicked decodes is owed the frame that will poll them"]
    pub(crate) fn observe_demand(&mut self, layout: &LayoutStore, scale: f32) -> bool {
        let kicked_before = self.kicked;
        let mut demands: FxHashMap<SourceKey, u32> = FxHashMap::default();
        for (node, key) in &self.by_node {
            let native = self.entries.get(key).and_then(|entry| entry.intrinsic);
            let demand = demand_of(layout, *node, native, scale);
            if demand > 0 {
                let entry = demands.entry(key.clone()).or_insert(0);
                *entry = (*entry).max(demand);
            }
        }
        for (key, demand) in demands {
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            entry.demanded = Some(demand);
            ensure_variant(
                entry,
                &key,
                &self.completed,
                self.limits,
                &mut self.next_handle,
                &mut self.kicked,
            );
        }
        self.kicked != kicked_before
    }

    /// Points `node` at `src`, or at nothing.
    fn set_source(
        &mut self,
        node: NodeKey,
        src: Option<&str>,
        content: &mut ContentCache,
        touched: &mut Vec<NodeKey>,
    ) {
        let next = src.map(SourceKey::of);
        let previous = self.by_node.get(&node).cloned();
        if next == previous {
            return;
        }

        if let Some(previous) = previous {
            if let Some(entry) = self.entries.get_mut(&previous) {
                entry.nodes.remove(&node);
                if entry.nodes.is_empty() {
                    entry.orphaned_at = self.clock;
                }
            }
            let id = ReplacedId::new(node);
            content.remove_image(id);
            self.intrinsics.remove(id);
            self.by_node.remove(&node);
            touched.push(node);
        }

        let Some(key) = next else {
            return;
        };
        self.by_node.insert(node, key.clone());
        let id = ReplacedId::new(node);

        let entry = self.entries.entry(key.clone()).or_insert_with(|| {
            self.kicked += 1;
            kick_probe(key.clone(), Rc::clone(&self.completed));
            Entry::probing()
        });
        entry.nodes.insert(node);
        // The intrinsic the node gets right now: the real one when the probe has answered for an
        // earlier node, otherwise known-and-unsized, which is what keeps the box replaced while
        // the probe runs.
        match entry.intrinsic {
            Some(native) => self.intrinsics.set(id, intrinsic_of(native)),
            None => self.intrinsics.set(id, Intrinsic::default()),
        }
        entry.attach_owed = true;
        touched.push(node);
    }

    /// Files what one background task produced.
    fn land(&mut self, key: SourceKey, arrival: Arrival, touched: &mut Vec<NodeKey>) {
        let Some(entry) = self.entries.get_mut(&key) else {
            return;
        };
        match arrival {
            Arrival::Probe(Ok(native)) => {
                entry.probe = Probe::Done;
                entry.intrinsic = Some(native);
                for &node in &entry.nodes {
                    self.intrinsics
                        .set(ReplacedId::new(node), intrinsic_of(native));
                    touched.push(node);
                }
            }
            Arrival::Probe(Err(error)) => {
                tracing::warn!(target: "zgui::images", src = ?key, "an image failed to probe: {error}");
                entry.probe = Probe::Failed;
                // The nodes stay claimed and unsized: a broken picture is a blank box, not a
                // relayout.
                touched.extend(entry.nodes.iter().copied());
            }
            Arrival::Variant(class, Ok(decoded)) => {
                if let Some(variant) = entry.variants.get_mut(&class) {
                    variant.state = VariantState::Ready(decoded);
                    entry.attach_owed = true;
                }
            }
            Arrival::Variant(class, Err(error)) => {
                tracing::warn!(target: "zgui::images", src = ?key, "an image failed to decode: {error}");
                if let Some(variant) = entry.variants.get_mut(&class) {
                    variant.state = VariantState::Failed;
                }
                touched.extend(entry.nodes.iter().copied());
            }
        }
    }

    /// Kicks the decodes current demands are missing, and attaches the best decoded variant
    /// wherever attachment is owed or a better one has landed.
    fn ensure_and_attach(&mut self, content: &mut ContentCache, touched: &mut Vec<NodeKey>) {
        let Self {
            entries,
            completed,
            limits,
            next_handle,
            kicked,
            intrinsics,
            ..
        } = self;
        for (key, entry) in entries.iter_mut() {
            if entry.nodes.is_empty() {
                continue;
            }
            ensure_variant(entry, key, completed, *limits, next_handle, kicked);
            let Some(best) = entry.best_attachable(limits.max_dimension) else {
                continue;
            };
            if !entry.attach_owed && entry.active == Some(best) {
                continue;
            }
            let variant = entry.variants.get(&best).expect("best_attachable chose it");
            for &node in &entry.nodes {
                let id = ReplacedId::new(node);
                match &variant.state {
                    VariantState::Ready(ready) => {
                        let mips = ready
                            .mips
                            .iter()
                            .map(|level| zgui_paint::MipLevel {
                                size: level.size,
                                texels: Arc::clone(&level.texels),
                            })
                            .collect();
                        if content
                            .set_image_shared_mipped(
                                id,
                                variant.handle,
                                ready.decoded.size,
                                entry.intrinsic.unwrap_or(ready.decoded.size),
                                Arc::clone(&ready.decoded.texels),
                                mips,
                            )
                            .is_err()
                        {
                            debug_assert!(false, "a decode checked its own byte count");
                        }
                    }
                    // The tile stands in for the texels: a node joining a settled picture costs
                    // no host bytes at all.
                    VariantState::Uploaded { size } => {
                        content.set_image_uploaded(
                            id,
                            variant.handle,
                            entry.intrinsic.unwrap_or(*size),
                        );
                    }
                    VariantState::Pending | VariantState::Failed => {
                        unreachable!("best_attachable checked the state")
                    }
                }
                if let Some(native) = entry.intrinsic {
                    intrinsics.set(id, intrinsic_of(native));
                }
                touched.push(node);
            }
            entry.active = Some(best);
            entry.attach_owed = false;
        }
    }

    /// Gives back the host texels of every variant whose tile `resident` vouches for.
    ///
    /// Call after the frame's uploads have flushed, with `resident` answering from the content
    /// cache's atlas by shared handle. Variants at or under [`RETAIN_SMALL_BYTES`] keep their
    /// texels — see the constant. Returns how many bytes were given back.
    pub(crate) fn release_uploaded(&mut self, resident: impl Fn(u64) -> bool) -> u64 {
        let mut freed = 0;
        for entry in self.entries.values_mut() {
            for variant in entry.variants.values_mut() {
                let held = variant.held_bytes();
                if held <= RETAIN_SMALL_BYTES || !resident(variant.handle) {
                    continue;
                }
                let VariantState::Ready(ready) = &variant.state else {
                    continue;
                };
                let size = ready.decoded.size;
                variant.state = VariantState::Uploaded { size };
                freed += held;
            }
        }
        if freed > 0 {
            self.recount();
        }
        freed
    }

    /// Decodes again the pictures whose tiles a frame found gone.
    ///
    /// `missing` is what [`ContentCache::take_missing_images`] drained: uploaded attachments
    /// whose tile was evicted or lost with the device. Each one's attached variant goes back to
    /// pending and its decode is kicked from the source the loader kept.
    ///
    /// Returns whether it spawned decode tasks, under the same contract as
    /// [`ImageLoader::settle`]: the caller owes them a frame.
    #[must_use = "a re-decode that was kicked is owed the frame that will poll it"]
    pub(crate) fn redecode_missing(&mut self, missing: &[ReplacedId]) -> bool {
        let kicked_before = self.kicked;
        let Self {
            entries,
            by_node,
            completed,
            limits,
            kicked,
            ..
        } = self;
        for id in missing {
            let Some(key) = by_node.get(&id.node()) else {
                continue;
            };
            let Some(entry) = entries.get_mut(key) else {
                continue;
            };
            let Some(active) = entry.active else {
                continue;
            };
            let Some(variant) = entry.variants.get_mut(&active) else {
                continue;
            };
            if matches!(variant.state, VariantState::Uploaded { .. }) {
                variant.state = VariantState::Pending;
                entry.attach_owed = true;
                *kicked += 1;
                kick_decode(key.clone(), active, Rc::clone(completed), *limits);
            }
        }
        self.kicked != kicked_before
    }

    /// Forgets the nodes that are gone, and the entries nothing shows any more.
    ///
    /// The frame-end half, beside the vector cache's own `retain`. Entry removal here is what
    /// bounds the map: an entry's texels are separately the budget's to trim.
    pub(crate) fn retain(&mut self, live: impl Fn(NodeKey) -> bool, content: &mut ContentCache) {
        self.clock += 1;
        let dead: Vec<NodeKey> = self
            .by_node
            .keys()
            .copied()
            .filter(|&node| !live(node))
            .collect();
        for node in dead {
            let key = self.by_node.remove(&node).expect("was just iterated");
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.nodes.remove(&node);
                if entry.nodes.is_empty() {
                    entry.orphaned_at = self.clock;
                }
            }
            let id = ReplacedId::new(node);
            content.remove_image(id);
            self.intrinsics.remove(id);
        }
        // An orphaned entry is a cache while anything of it remains: held texels a re-shown
        // picture attaches without decoding, or a resident tile it attaches without even an
        // upload. One with neither goes, and any tiles its handles still name go with it,
        // because the handles leave with the entry and nothing can ever ask for them again.
        // The atlas budget bounds the tiles, and through the residency check here, the tiles
        // bound this map.
        self.entries.retain(|_, entry| {
            if !entry.nodes.is_empty() {
                return true;
            }
            let worth_keeping = entry.variants.values().any(|variant| {
                variant.held_bytes() > 0
                    || (matches!(variant.state, VariantState::Uploaded { .. })
                        && content.image_tile_resident(variant.handle))
            });
            if !worth_keeping {
                for variant in entry.variants.values() {
                    content.remove_shared_tile(variant.handle);
                }
            }
            worth_keeping
        });
        self.recount();
    }

    /// How many decoded bytes the loader holds, over every entry.
    pub(crate) fn held_bytes(&self) -> u64 {
        self.held_bytes
    }

    /// How many decoded bytes are held with nothing pinning them.
    pub(crate) fn evictable_bytes(&self) -> u64 {
        self.evictable_bytes
    }

    /// Drops decoded texels until `want` bytes have been freed.
    ///
    /// Unattached variants of shown sources go first — nothing draws them right now — and then
    /// sources nothing shows, whole, coldest first by when their last node left. The attached
    /// variant of a shown source is never touched: trimming a picture that is on the screen is
    /// [`forget`](ImageLoader::forget)'s business, not a budget's. A dropped source's tiles are
    /// returned with it, unless a replayed range still holds one.
    pub(crate) fn evict(&mut self, want: u64, content: &mut ContentCache) -> u64 {
        let mut freed = 0;
        for entry in self.entries.values_mut() {
            if freed >= want {
                break;
            }
            if entry.nodes.is_empty() {
                continue;
            }
            let active = entry.active;
            entry.variants.retain(|class, variant| {
                if freed >= want || Some(*class) == active {
                    return true;
                }
                let held = variant.held_bytes();
                if held == 0 {
                    // A pending decode is about to be worth what it cost, and a failure memo is
                    // what stops a broken source being retried every frame.
                    return true;
                }
                freed += held;
                content.remove_shared_tile(variant.handle);
                false
            });
        }
        if freed < want {
            let mut orphans: Vec<(SourceKey, u64)> = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.nodes.is_empty() && entry.held_bytes() > 0)
                .map(|(key, entry)| (key.clone(), entry.orphaned_at))
                .collect();
            orphans.sort_unstable_by_key(|(_, orphaned_at)| *orphaned_at);
            for (key, _) in orphans {
                if freed >= want {
                    break;
                }
                if let Some(entry) = self.entries.remove(&key) {
                    freed += entry.held_bytes();
                    for variant in entry.variants.values() {
                        content.remove_shared_tile(variant.handle);
                    }
                }
            }
        }
        self.recount();
        freed
    }

    /// Drops every decoded byte, shown or not.
    ///
    /// The shown ones come back: their entries keep their names and demands, and the next settle
    /// re-decodes from the source. What this costs is the decode, which is the honest price of "a
    /// window with every cache empty".
    pub(crate) fn forget(&mut self, content: &mut ContentCache) {
        self.entries.retain(|_, entry| {
            for variant in entry.variants.values() {
                content.remove_shared_tile(variant.handle);
            }
            if entry.nodes.is_empty() {
                return false;
            }
            for &node in &entry.nodes {
                content.remove_image(ReplacedId::new(node));
            }
            entry.variants.clear();
            entry.active = None;
            entry.attach_owed = true;
            true
        });
        self.recount();
    }

    /// Recomputes the byte totals from the entries.
    ///
    /// One walk at each state change rather than increments threaded through every transition,
    /// because the walk is over a handful of sources and the increments were the part that could
    /// silently drift.
    fn recount(&mut self) {
        let mut held = 0;
        let mut evictable = 0;
        for entry in self.entries.values() {
            for (class, variant) in &entry.variants {
                let bytes = variant.held_bytes();
                held += bytes;
                let pinned = !entry.nodes.is_empty() && entry.active == Some(*class);
                if !pinned {
                    evictable += bytes;
                }
            }
        }
        self.held_bytes = held;
        self.evictable_bytes = evictable;
    }
}

#[cfg(test)]
impl ImageLoader {
    /// Files a source as already probed, decoded at its own size, shown by `nodes` and attached,
    /// bypassing the async path.
    ///
    /// The budget tests need entries in known states without an executor to run decodes through;
    /// everything downstream of the state — reports, eviction, forget — is what they assert.
    pub(crate) fn insert_ready_for_tests(
        &mut self,
        src: &str,
        nodes: &[NodeKey],
        decoded: zgui_image::Decoded,
    ) {
        let key = SourceKey::of(src);
        if let Some(previous) = self.entries.remove(&key) {
            for node in previous.nodes {
                self.by_node.remove(&node);
            }
        }
        self.next_handle += 1;
        let class = decoded.size.width.max(decoded.size.height).max(1);
        let mut variants = FxHashMap::default();
        variants.insert(
            class,
            Variant {
                handle: self.next_handle,
                state: VariantState::Ready(DecodedVariant {
                    decoded: decoded.clone(),
                    mips: Vec::new(),
                }),
            },
        );
        self.clock += 1;
        let entry = Entry {
            intrinsic: Some(decoded.size),
            probe: Probe::Done,
            variants,
            active: Some(class),
            demanded: Some(class),
            nodes: nodes.iter().copied().collect(),
            attach_owed: false,
            orphaned_at: if nodes.is_empty() { self.clock } else { 0 },
        };
        for &node in nodes {
            self.by_node.insert(node, key.clone());
        }
        self.entries.insert(key, entry);
        self.recount();
    }

    /// Whether `src`'s entry currently holds texels.
    pub(crate) fn holds_texels_for_tests(&self, src: &str) -> bool {
        self.entries
            .get(&SourceKey::of(src))
            .is_some_and(|entry| entry.held_bytes() > 0)
    }
}

/// The class `entry`'s demand asks to have decoded, or `None` when nothing has to happen.
///
/// A variant at or above the target, decoded or in flight, satisfies the demand: a bigger picture
/// downscales in the sampler, and decoding a smaller one to save memory is eviction's decision
/// rather than a reflex. This is also the hysteresis — a box oscillating across a class boundary
/// keeps both variants and decodes nothing. A target that already failed stays failed; without
/// the memo a broken source would decode once per frame for ever.
fn wanted_kick(entry: &Entry, max_dimension: u32) -> Option<Class> {
    let target = entry.target_class(max_dimension)?;
    let satisfied = entry.variants.iter().any(|(class, variant)| {
        *class >= target
            && matches!(
                variant.state,
                VariantState::Ready(_) | VariantState::Pending | VariantState::Uploaded { .. }
            )
    });
    if satisfied {
        return None;
    }
    if matches!(
        entry.variants.get(&target).map(|variant| &variant.state),
        Some(VariantState::Failed)
    ) {
        return None;
    }
    Some(target)
}

/// Kicks the decode `entry`'s demand asks for, when [`wanted_kick`] says one is missing.
///
/// Free-standing rather than a method so the loader can call it while iterating its own entries.
fn ensure_variant(
    entry: &mut Entry,
    key: &SourceKey,
    completed: &DecodeQueue,
    limits: zgui_image::Limits,
    next_handle: &mut u64,
    kicked: &mut u64,
) {
    let Some(target) = wanted_kick(entry, limits.max_dimension) else {
        return;
    };
    *next_handle += 1;
    *kicked += 1;
    entry.variants.insert(
        target,
        Variant {
            handle: *next_handle,
            state: VariantState::Pending,
        },
    );
    kick_decode(key.clone(), target, Rc::clone(completed), limits);
}

/// The longest edge any of `node`'s fragments asks the picture to have, in whole device pixels.
///
/// The box's `object-fit` is part of the question: `fill`, `contain` and `scale-down` show no
/// more texels than the box has pixels, `cover` shows the whole scaled picture's worth even
/// though the box crops it, and `none` shows source texels one-to-one, so its demand is the
/// source itself at the device's scale.
fn demand_of(
    layout: &LayoutStore,
    node: NodeKey,
    native: Option<Size<u32, Device>>,
    scale: f32,
) -> u32 {
    use zgui_css::values::size::ObjectFitValue;
    let native_long = |native: Size<u32, Device>| native.width.max(native.height).max(1) as f32;
    let mut long = 0.0f32;
    for box_ in layout.boxes_of(node) {
        for fragment in layout.fragments_of_box(*box_) {
            let Some(fragment) = layout.fragment(*fragment) else {
                continue;
            };
            let size = fragment.content_box.size;
            let (width, height) = (size.width.0, size.height.0);
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            let fit = layout.node(*box_).style.get_position().object_fit;
            let demanded = match (fit, native) {
                (ObjectFitValue::Cover, Some(native)) if native.width > 0 && native.height > 0 => {
                    // The box's device pixels over the source's own, so the factor carries the
                    // display scale with it; the crop shows the scaled picture's long edge.
                    let factor = (width / native.width as f32).max(height / native.height as f32);
                    native_long(native) * factor
                }
                (ObjectFitValue::None, Some(native)) => native_long(native) * scale,
                _ => width.max(height),
            };
            long = long.max(demanded);
        }
    }
    long.max(0.0).ceil() as u32
}

/// The intrinsic one probe reports: the source's pixel count, read as CSS pixels.
///
/// The 1× reading is deliberate and documented on the element: density descriptors are a
/// vocabulary this framework does not have yet, and guessing from the window's scale would make
/// an image's layout size depend on which monitor it first decoded on.
fn intrinsic_of(native: Size<u32, Device>) -> Intrinsic {
    let size = Size::new(CssPx(native.width as f32), CssPx(native.height as f32));
    Intrinsic {
        size: Some(size),
        ratio: (native.height != 0).then(|| native.width as f32 / native.height as f32),
        baseline: None,
    }
}

/// Starts one header probe off the frame thread, landing its result in `completed`.
///
/// The task is spawned on the UI thread and owns nothing but the queue; the read itself runs on
/// the blocking pool. The wake edge when it finishes is what requests the frame whose settle will
/// apply the result.
fn kick_probe(key: SourceKey, completed: DecodeQueue) {
    let work = match &key {
        SourceKey::Path(path) => {
            let path = PathBuf::from(path);
            zgui_reactive::blocking(move || zgui_image::probe_file(&path))
        }
        SourceKey::Bytes(url) => {
            let bytes = zgui_image::bytes_for_url(url);
            zgui_reactive::blocking(move || match bytes {
                Some(bytes) => zgui_image::probe(&bytes),
                None => Err(dropped_handle()),
            })
        }
    };
    zgui_reactive::spawn_local(async move {
        let result = work.await;
        completed.borrow_mut().push((key, Arrival::Probe(result)));
    });
}

/// Starts one decode off the frame thread, landing its result in `completed`.
///
/// A variant whose long edge lands over [`STANDALONE_LONG_EDGE`] gets its levels of detail built
/// on the same worker, so the frame thread never runs the filter.
fn kick_decode(key: SourceKey, class: Class, completed: DecodeQueue, limits: zgui_image::Limits) {
    let with_levels = |decoded: zgui_image::Decoded| {
        let mips = if decoded.size.width.max(decoded.size.height) > STANDALONE_LONG_EDGE {
            zgui_image::mip_chain(&decoded)
        } else {
            Vec::new()
        };
        DecodedVariant { decoded, mips }
    };
    let work = match &key {
        SourceKey::Path(path) => {
            let path = PathBuf::from(path);
            zgui_reactive::blocking(move || {
                zgui_image::decode_file_scaled(&path, class, limits).map(with_levels)
            })
        }
        SourceKey::Bytes(url) => {
            let bytes = zgui_image::bytes_for_url(url);
            zgui_reactive::blocking(move || match bytes {
                Some(bytes) => zgui_image::decode_scaled(&bytes, class, limits).map(with_levels),
                None => Err(dropped_handle()),
            })
        }
    };
    zgui_reactive::spawn_local(async move {
        let result = work.await;
        completed
            .borrow_mut()
            .push((key, Arrival::Variant(class, result)));
    });
}

/// The error a `zgui-bytes:` URL resolves to once its handle is gone.
fn dropped_handle() -> zgui_image::DecodeError {
    zgui_image::DecodeError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "the ImageBytes handle was dropped before the work ran",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ready variant of `long` texels on its long edge, one texel high.
    fn ready(long: u32) -> Variant {
        Variant {
            handle: u64::from(long),
            state: VariantState::Ready(DecodedVariant {
                decoded: zgui_image::Decoded {
                    size: Size::new(long, 1),
                    texels: Arc::new(vec![0; (long * 4) as usize]),
                },
                mips: Vec::new(),
            }),
        }
    }

    /// An entry for a 3000×2000 source with `demand` device pixels of long edge.
    fn entry(demand: u32) -> Entry {
        let mut entry = Entry::probing();
        entry.probe = Probe::Done;
        entry.intrinsic = Some(Size::new(3000, 2000));
        entry.demanded = Some(demand);
        entry
    }

    #[test]
    fn a_thumbnail_demand_decodes_the_thumbnail_class_and_never_the_source() {
        assert_eq!(entry(64).target_class(4096), Some(64));
        assert_eq!(entry(65).target_class(4096), Some(128));
        assert_eq!(
            entry(2500).target_class(4096),
            Some(3000),
            "a demand past the ladder decodes the source's own long edge"
        );
        assert_eq!(
            entry(2500).target_class(2048),
            Some(2048),
            "and the decode limit caps it"
        );
        let mut small = entry(500);
        small.intrinsic = Some(Size::new(96, 64));
        assert_eq!(
            small.target_class(4096),
            Some(96),
            "a source smaller than its box is decoded once, at its own size"
        );
    }

    #[test]
    fn a_demand_kicks_before_the_probe_answers_and_no_layout_kicks_nothing() {
        let mut unprobed = entry(64);
        unprobed.intrinsic = None;
        assert_eq!(
            wanted_kick(&unprobed, 4096),
            Some(64),
            "the probe and the decode run side by side: a decode never upscales, so the \
             demanded class is safe to kick before the extent is known"
        );
        let mut unmeasured = entry(64);
        unmeasured.demanded = None;
        assert_eq!(
            wanted_kick(&unmeasured, 4096),
            None,
            "the box sizes the class"
        );
    }

    #[test]
    fn a_larger_variant_satisfies_a_smaller_demand_so_oscillation_decodes_nothing() {
        let mut entry = entry(129);
        entry.variants.insert(128, ready(128));
        entry.variants.insert(256, ready(256));
        assert_eq!(
            wanted_kick(&entry, 4096),
            None,
            "256 covers a demand of 129"
        );
        entry.demanded = Some(128);
        assert_eq!(wanted_kick(&entry, 4096), None, "and 128 covers 128 again");
        entry.demanded = Some(300);
        assert_eq!(
            wanted_kick(&entry, 4096),
            Some(512),
            "only a demand nothing covers decodes"
        );
    }

    #[test]
    fn an_in_flight_or_failed_target_is_not_kicked_again() {
        let mut waiting = entry(100);
        waiting.variants.insert(
            128,
            Variant {
                handle: 1,
                state: VariantState::Pending,
            },
        );
        assert_eq!(wanted_kick(&waiting, 4096), None, "the decode is coming");
        let mut broken = entry(100);
        broken.variants.insert(
            128,
            Variant {
                handle: 1,
                state: VariantState::Failed,
            },
        );
        assert_eq!(wanted_kick(&broken, 4096), None, "a failure is a memo");
    }

    #[test]
    fn the_attached_variant_is_the_smallest_sufficient_one_or_the_largest_there_is() {
        let mut entry = entry(200);
        entry.variants.insert(128, ready(128));
        entry.variants.insert(256, ready(256));
        entry.variants.insert(1024, ready(1024));
        assert_eq!(
            entry.best_attachable(4096),
            Some(256),
            "256 is the smallest that covers a demand of 200"
        );
        entry.demanded = Some(2000);
        assert_eq!(
            entry.best_attachable(4096),
            Some(1024),
            "nothing covers 2000, so the largest stands in while the decode runs"
        );
    }

    #[test]
    fn release_gives_back_large_resident_texels_and_keeps_small_or_unuploaded_ones() {
        let node = zgui_dom::NodeKey::new(
            7,
            zgui_arena::Generation::FIRST,
            zgui_arena::DomainId::FIRST,
        );
        let mut loader = ImageLoader::new(IntrinsicTable::new(), 4096);

        // A large picture whose tile is resident, and a small one likewise.
        loader.insert_ready_for_tests(
            "large.png",
            &[node],
            zgui_image::Decoded {
                size: Size::new(512, 512),
                texels: Arc::new(vec![0; 512 * 512 * 4]),
            },
        );
        loader.insert_ready_for_tests(
            "small.png",
            &[],
            zgui_image::Decoded {
                size: Size::new(4, 4),
                texels: Arc::new(vec![0; 64]),
            },
        );

        let freed = loader.release_uploaded(|_| false);
        assert_eq!(freed, 0, "a tile nobody vouches for keeps its texels");

        let freed = loader.release_uploaded(|_| true);
        assert_eq!(
            freed,
            512 * 512 * 4,
            "the large picture settles into its tile"
        );
        assert!(
            !loader.holds_texels_for_tests("large.png"),
            "its host copy is gone"
        );
        assert!(
            loader.holds_texels_for_tests("small.png"),
            "a small picture keeps its copy: re-decoding it costs more than holding it"
        );
    }

    #[test]
    fn eviction_takes_unattached_variants_first_and_never_the_attached_one() {
        let node = zgui_dom::NodeKey::new(
            7,
            zgui_arena::Generation::FIRST,
            zgui_arena::DomainId::FIRST,
        );
        let mut loader = ImageLoader::new(IntrinsicTable::new(), 4096);
        let key = SourceKey::of("photo.png");
        let mut shown = entry(200);
        shown.variants.insert(128, ready(128));
        shown.variants.insert(1024, ready(1024));
        shown.active = Some(1024);
        shown.nodes.insert(node);
        loader.by_node.insert(node, key.clone());
        loader.entries.insert(key.clone(), shown);
        loader.recount();
        assert_eq!(loader.held_bytes(), (128 + 1024) * 4);
        assert_eq!(
            loader.evictable_bytes(),
            128 * 4,
            "the unattached variant is the evictable part"
        );

        let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
        let freed = loader.evict(u64::MAX, &mut content);
        assert_eq!(freed, 128 * 4, "the attached variant is never touched");
        assert_eq!(loader.held_bytes(), 1024 * 4);
        assert_eq!(loader.evictable_bytes(), 0);
    }

    #[test]
    fn orphans_are_evicted_coldest_first() {
        let mut loader = ImageLoader::new(IntrinsicTable::new(), 4096);
        let picture = |side: u32| zgui_image::Decoded {
            size: Size::new(side, side),
            texels: Arc::new(vec![0; (side * side * 4) as usize]),
        };
        // Orphaned in this order, so "coldest" has a defined meaning.
        loader.insert_ready_for_tests("first.png", &[], picture(4));
        loader.insert_ready_for_tests("second.png", &[], picture(4));
        loader.insert_ready_for_tests("third.png", &[], picture(4));

        let mut content = ContentCache::new(zgui_atlas::AtlasLimits::default());
        let freed = loader.evict(1, &mut content);
        assert_eq!(freed, 4 * 4 * 4, "one entry was enough for one byte");
        assert!(
            !loader.holds_texels_for_tests("first.png"),
            "the coldest orphan goes first"
        );
        assert!(loader.holds_texels_for_tests("second.png"));
        assert!(loader.holds_texels_for_tests("third.png"));

        loader.evict(1, &mut content);
        assert!(
            !loader.holds_texels_for_tests("second.png"),
            "and then the next coldest"
        );
        assert!(loader.holds_texels_for_tests("third.png"));
    }
}
