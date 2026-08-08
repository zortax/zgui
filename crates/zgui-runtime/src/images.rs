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
//!    intrinsics table — known and unsized — and kicks a decode: a task that awaits the blocking
//!    pool and pushes the result into the completion queue. The task's wake is what asks for the
//!    frame that will apply it.
//! 3. A later `settle` drains the completion, files the real intrinsic, attaches the texels to
//!    every element showing that source, and marks each so the box is rebuilt with the natural
//!    size. Layout and paint then treat it like any other replaced content.
//!
//! # What eviction means here
//!
//! The loader is the one owner of decoded texels that *can* produce them again, so unlike the old
//! budget adapter's answer, these bytes are honestly evictable: an entry no live element shows is
//! dropped whole, and a forgotten entry that is still shown is re-kicked from its source on the
//! next settle. The intrinsics survive eviction on purpose — a page must not reflow because a
//! cache was trimmed.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::host::replaced::{Intrinsic, ReplacedId};
use zgui_dom::{Document, NodeKey};
use zgui_geom::{CssPx, Size};
use zgui_paint::ContentCache;

use crate::replaced::IntrinsicTable;

/// What one `src` string names.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum SourceKey {
    /// A file on disk, decoded through [`zgui_image::decode_file`].
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

/// Where one source is on its way to being shown.
enum State {
    /// The decode is in flight.
    Pending,
    /// Decoded, held, and attachable.
    Ready(zgui_image::Decoded),
    /// The decode failed; the memo is what stops a broken path being retried every frame.
    Failed,
    /// Was `Ready`; the texels were dropped by the budget and come back on demand.
    Evicted,
}

/// One source and everything showing it.
struct Entry {
    /// The atlas handle every node of this entry shares.
    handle: u64,
    /// Where the decode is.
    state: State,
    /// The nodes currently showing this source.
    nodes: FxHashSet<NodeKey>,
    /// Whether the texels still have to be attached to some of `nodes`.
    attach_owed: bool,
}

impl Entry {
    /// How many decoded bytes this entry holds.
    fn held_bytes(&self) -> u64 {
        match &self.state {
            State::Ready(decoded) => decoded.texels.len() as u64,
            _ => 0,
        }
    }
}

/// The `src` writes the attribute hook heard, waiting for the settle that applies them.
///
/// Shared rather than owned because the hook that fills it and the loader that drains it are
/// installed at different moments and neither outlives the other.
type SourceQueue = Rc<RefCell<Vec<(NodeKey, Option<String>)>>>;

/// The decodes that finished, waiting for the same settle.
type DecodeQueue = Rc<
    RefCell<
        Vec<(
            SourceKey,
            Result<zgui_image::Decoded, zgui_image::DecodeError>,
        )>,
    >,
>;

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
    /// What the decode tasks produced since the last settle.
    completed: DecodeQueue,
    /// The bound every decode is held to, from the atlas's own limit.
    limits: zgui_image::Limits,
    /// The next atlas handle, allocated per source and never reused.
    next_handle: u64,
    /// How many decode tasks have ever been kicked; a settle that moved it owes a frame.
    kicked: u64,
    /// Decoded bytes held across all sources.
    ///
    /// Maintained on state transitions so the per-frame budget report is constant-time.
    held_bytes: u64,
    /// The subset of [`ImageLoader::held_bytes`] belonging to sources no node shows.
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
            held_bytes: 0,
            evictable_bytes: 0,
        }
    }

    /// The queue the attribute hook pushes `src` changes into.
    pub(crate) fn source_queue(&self) -> SourceQueue {
        Rc::clone(&self.pending)
    }

    /// Applies everything that arrived since the last frame: `src` writes and finished decodes.
    ///
    /// Runs in the frame after the reactive flush and before the restyle, which is what lets a
    /// decode that landed during the flush be *shown* by the same frame. Every node whose content
    /// or intrinsic changed is marked through
    /// [`Document::replaced_content_changed`], so the steps below this one rebuild exactly the
    /// boxes that need it.
    ///
    /// Returns whether it spawned decode tasks. The caller owes those a frame: a task spawned
    /// *here* runs after the frame's own flush, so nothing has polled it yet, and a future that
    /// has never been polled has registered no waker for its completion to fire. One more frame
    /// polls it; from then on the wake edge carries it.
    #[must_use = "a settle that kicked decodes is owed the frame that will poll them"]
    pub(crate) fn settle(
        &mut self,
        document: &Rc<RefCell<Document>>,
        content: &mut ContentCache,
    ) -> bool {
        let kicked_before = self.kicked;
        let mut touched: Vec<NodeKey> = Vec::new();

        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for (node, src) in pending {
            self.set_source(node, src.as_deref(), content, &mut touched);
        }

        let completed = std::mem::take(&mut *self.completed.borrow_mut());
        for (key, result) in completed {
            self.land(key, result, &mut touched);
        }

        // Attach after landing, and also for entries whose texels arrived in an earlier frame but
        // whose nodes changed in this one; `attach_owed` is what remembers either.
        for (key, entry) in &mut self.entries {
            if !entry.attach_owed {
                continue;
            }
            match &entry.state {
                State::Ready(decoded) => {
                    for &node in &entry.nodes {
                        let id = ReplacedId::new(node);
                        if content
                            .set_image_shared(
                                id,
                                entry.handle,
                                decoded.size,
                                Arc::clone(&decoded.texels),
                            )
                            .is_err()
                        {
                            debug_assert!(false, "a decode checked its own byte count");
                        }
                        self.intrinsics.set(id, intrinsic_of(decoded));
                        touched.push(node);
                    }
                    entry.attach_owed = false;
                }
                // Dropped by the budget while still shown: decode it again from the source.
                State::Evicted => {
                    entry.state = State::Pending;
                    self.kicked += 1;
                    kick(key.clone(), Rc::clone(&self.completed), self.limits);
                }
                State::Pending | State::Failed => {}
            }
        }

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
                let became_orphaned = entry.nodes.len() == 1 && entry.nodes.contains(&node);
                entry.nodes.remove(&node);
                if became_orphaned {
                    self.evictable_bytes += entry.held_bytes();
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
            self.next_handle += 1;
            self.kicked += 1;
            let handle = self.next_handle;
            kick(key.clone(), Rc::clone(&self.completed), self.limits);
            Entry {
                handle,
                state: State::Pending,
                nodes: FxHashSet::default(),
                attach_owed: false,
            }
        });
        let was_orphaned = entry.nodes.is_empty();
        entry.nodes.insert(node);
        if was_orphaned {
            self.evictable_bytes = self.evictable_bytes.saturating_sub(entry.held_bytes());
        }
        match &entry.state {
            // Known and unsized, which is what keeps the box replaced while the decode runs.
            State::Pending | State::Failed | State::Evicted => {
                self.intrinsics.set(id, Intrinsic::default());
                entry.attach_owed = matches!(entry.state, State::Evicted) || entry.attach_owed;
                touched.push(node);
            }
            // Already decoded for some other node: this one only needs attaching.
            State::Ready(_) => entry.attach_owed = true,
        }
    }

    /// Files what one decode produced.
    fn land(
        &mut self,
        key: SourceKey,
        result: Result<zgui_image::Decoded, zgui_image::DecodeError>,
        touched: &mut Vec<NodeKey>,
    ) {
        let Some(entry) = self.entries.get_mut(&key) else {
            return;
        };
        let before = entry.held_bytes();
        self.held_bytes = self.held_bytes.saturating_sub(before);
        if entry.nodes.is_empty() {
            self.evictable_bytes = self.evictable_bytes.saturating_sub(before);
        }
        match result {
            Ok(decoded) => {
                entry.state = State::Ready(decoded);
                entry.attach_owed = true;
                let after = entry.held_bytes();
                self.held_bytes += after;
                if entry.nodes.is_empty() {
                    self.evictable_bytes += after;
                }
            }
            Err(error) => {
                tracing::warn!(target: "zgui::images", src = ?key, "an image failed to decode: {error}");
                entry.state = State::Failed;
                // The nodes stay claimed and unsized: a broken picture is a blank box, not a
                // relayout.
                touched.extend(entry.nodes.iter().copied());
            }
        }
    }

    /// Forgets the nodes that are gone, and the entries nothing shows any more.
    ///
    /// The frame-end half, beside the vector cache's own `retain`. Entry removal here is what
    /// bounds the map: an entry's texels are separately the budget's to trim.
    pub(crate) fn retain(&mut self, live: impl Fn(NodeKey) -> bool, content: &mut ContentCache) {
        let dead: Vec<NodeKey> = self
            .by_node
            .keys()
            .copied()
            .filter(|&node| !live(node))
            .collect();
        for node in dead {
            let key = self.by_node.remove(&node).expect("was just iterated");
            if let Some(entry) = self.entries.get_mut(&key) {
                let became_orphaned = entry.nodes.len() == 1 && entry.nodes.contains(&node);
                entry.nodes.remove(&node);
                if became_orphaned {
                    self.evictable_bytes += entry.held_bytes();
                }
            }
            let id = ReplacedId::new(node);
            content.remove_image(id);
            self.intrinsics.remove(id);
        }
        // An orphaned entry that holds texels is a cache — a list re-showing the same picture
        // skips the decode — and bounding it is the budget's job, by bytes. An orphaned entry
        // holding nothing is not even that.
        self.entries
            .retain(|_, entry| !entry.nodes.is_empty() || entry.held_bytes() > 0);
    }

    /// How many decoded bytes the loader holds, over every entry.
    pub(crate) fn held_bytes(&self) -> u64 {
        self.held_bytes
    }

    /// How many decoded bytes are held for sources nothing currently shows.
    pub(crate) fn evictable_bytes(&self) -> u64 {
        self.evictable_bytes
    }

    /// Drops decoded texels until `want` bytes have been freed, coldest first.
    ///
    /// Only sources nothing shows are touched — trimming a picture that is on the screen is
    /// [`forget`](ImageLoader::forget)'s business, not a budget's.
    pub(crate) fn evict(&mut self, want: u64) -> u64 {
        let mut freed = 0;
        self.entries.retain(|_, entry| {
            if freed >= want || !entry.nodes.is_empty() {
                return true;
            }
            let held = entry.held_bytes();
            if held == 0 {
                return true;
            }
            freed += held;
            false
        });
        self.held_bytes = self.held_bytes.saturating_sub(freed);
        self.evictable_bytes = self.evictable_bytes.saturating_sub(freed);
        freed
    }

    /// Drops every decoded byte, shown or not.
    ///
    /// The shown ones come back: their entries move to `Evicted` and the next settle re-decodes
    /// from the source. What this costs is the decode, which is the honest price of "a window
    /// with every cache empty".
    pub(crate) fn forget(&mut self, content: &mut ContentCache) {
        let mut freed = 0;
        let mut freed_evictable = 0;
        self.entries.retain(|_, entry| {
            let held = entry.held_bytes();
            if entry.nodes.is_empty() {
                freed += held;
                freed_evictable += held;
                return false;
            }
            for &node in &entry.nodes {
                content.remove_image(ReplacedId::new(node));
            }
            entry.state = State::Evicted;
            entry.attach_owed = true;
            freed += held;
            true
        });
        self.held_bytes = self.held_bytes.saturating_sub(freed);
        self.evictable_bytes = self.evictable_bytes.saturating_sub(freed_evictable);
    }
}

#[cfg(test)]
impl ImageLoader {
    /// Files a source as already decoded and shown by `nodes`, bypassing the async path.
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
            let held = previous.held_bytes();
            self.held_bytes = self.held_bytes.saturating_sub(held);
            if previous.nodes.is_empty() {
                self.evictable_bytes = self.evictable_bytes.saturating_sub(held);
            }
            for node in previous.nodes {
                self.by_node.remove(&node);
            }
        }
        self.next_handle += 1;
        let entry = Entry {
            handle: self.next_handle,
            state: State::Ready(decoded),
            nodes: nodes.iter().copied().collect(),
            attach_owed: false,
        };
        let held = entry.held_bytes();
        self.held_bytes += held;
        if entry.nodes.is_empty() {
            self.evictable_bytes += held;
        }
        for &node in nodes {
            self.by_node.insert(node, key.clone());
        }
        self.entries.insert(key, entry);
    }

    /// Whether `src`'s entry currently holds texels.
    pub(crate) fn holds_texels_for_tests(&self, src: &str) -> bool {
        self.entries
            .get(&SourceKey::of(src))
            .is_some_and(|entry| entry.held_bytes() > 0)
    }
}

/// The intrinsic one decode reports: its pixel count, read as CSS pixels.
///
/// The 1× reading is deliberate and documented on the element: density descriptors are a
/// vocabulary this framework does not have yet, and guessing from the window's scale would make
/// an image's layout size depend on which monitor it first decoded on.
fn intrinsic_of(decoded: &zgui_image::Decoded) -> Intrinsic {
    let size = Size::new(
        CssPx(decoded.size.width as f32),
        CssPx(decoded.size.height as f32),
    );
    Intrinsic {
        size: Some(size),
        ratio: (decoded.size.height != 0)
            .then(|| decoded.size.width as f32 / decoded.size.height as f32),
        baseline: None,
    }
}

/// Starts one decode off the frame thread, landing its result in `completed`.
///
/// The task is spawned on the UI thread and owns nothing but the queue; the decode itself runs on
/// the blocking pool. The wake edge when it finishes is what requests the frame whose settle will
/// apply the result.
fn kick(key: SourceKey, completed: DecodeQueue, limits: zgui_image::Limits) {
    let work = match &key {
        SourceKey::Path(path) => {
            let path = PathBuf::from(path);
            zgui_reactive::blocking(move || zgui_image::decode_file(&path, limits))
        }
        SourceKey::Bytes(url) => {
            let bytes = zgui_image::bytes_for_url(url);
            zgui_reactive::blocking(move || match bytes {
                Some(bytes) => zgui_image::decode(&bytes, limits),
                None => Err(zgui_image::DecodeError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "the ImageBytes handle was dropped before the decode ran",
                ))),
            })
        }
    };
    zgui_reactive::spawn_local(async move {
        let result = work.await;
        completed.borrow_mut().push((key, result));
    });
}
