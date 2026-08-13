//! Running one batch of independent child computations across the layout pool.
//!
//! The algorithms hand over a list of requests whose inputs are all known and whose results they
//! consume in order. The executor plans each worker's chunk up front — the boxes its subtrees
//! hold, the shaped paragraphs it will break — then splits the store, carves one exclusive
//! borrow per planned box out of the layout column, and lets the workers write results in place.
//! Nothing is copied in and nothing is committed out; what remains afterwards is re-interning
//! the paragraph identifiers workers recorded as markers, in request order, and reconstructing
//! the retain-and-release accounting from the snapshots taken before the batch.

use rustc_hash::FxHashMap;
use taffy::{ChildRequest, LayoutOutput, LayoutPartialTree};
use zgui_dom::side::BoxKey;
use zgui_profile::{Counter, counter};
use zgui_text::{ParagraphContent, ParagraphKey};

use crate::fragment::ParagraphId;
use crate::inline::atomic::AtomicMemo;
use crate::inline::content::styles::TextStyles;
use crate::key::from_node_id;
use crate::measure::{MeasureContent, WorkerMeasure};
use crate::tree::store::state::BoxLayout;
use crate::tree::view::{PROVISIONAL_PARAGRAPH, StoreView, WorkerStore};
use crate::tree::{LayoutTree, parallel::LayoutPool};

/// The fewest distinct cold nodes a batch distributes.
///
/// Cold, as in the cache holds no answer: a hot request costs a ring probe and nothing else, so
/// a batch of hot requests — which is what every steady frame produces — must run serially where
/// it stands, or the pool's own overhead is paid for work that does not exist. Counted in nodes
/// rather than requests because one site asks two questions per node, and a keystroke's short
/// dirty spine reached eight questions long before it held eight subtrees. Measured on the
/// kitchen keystroke, distributing those spines cost the frame a fifth extra.
const MIN_COLD_NODES: usize = 8;

/// The fewest planned boxes a batch distributes.
///
/// Even a batch of many cold nodes can hold almost no work — eight empty leaves plan eight
/// boxes. Waking the pool costs the frame tens of microseconds in spawn latency and worker
/// spin, so a batch below this floor runs serially where it stands. The floor is checked after
/// planning, which costs a walk of the cold subtrees only — a price only cold frames pay.
///
/// The number comes from the `batch_scale` probe: batches near 500 boxes run at half their
/// serial time, and the kitchen document's widest batch — 72 boxes across 24 items — measured
/// as a pure loss at every worker count. The floor sits between the two.
const MIN_BATCH_BOXES: usize = 256;

/// What one worker's chunk was planned to touch.
#[derive(Debug, Default)]
struct ChunkPlan {
    /// Every box in the chunk's subtrees, hot and cold alike: state access covers cache hits.
    boxes: Vec<BoxKey>,
    /// The shaped paragraphs the chunk's cold subtrees will break, moved into its measurer.
    paragraphs: Vec<ParagraphKey>,
}

impl<'a, C: MeasureContent> LayoutTree<'a, C> {
    /// The pool this pass may distribute batches on, if every condition for one holds.
    ///
    /// Exclusive passes only: a worker's own nested batches run serially on the worker, which
    /// keeps the store split single-level. A pass with a custom source installed stays serial
    /// because the source's contract carries no `Sync` bound.
    pub(crate) fn batch_pool(&self) -> Option<&'a LayoutPool> {
        match (&self.store, self.has_custom) {
            (StoreView::Exclusive(_), false) => self.parallel,
            _ => None,
        }
    }

    /// Runs one batch, serially where it stands or across the pool.
    pub(crate) fn run_batch(&mut self, requests: &[ChildRequest], results: &mut Vec<LayoutOutput>) {
        results.clear();
        let distributable = requests.len() >= MIN_COLD_NODES && self.batch_pool().is_some() && {
            let mut cold: Vec<u64> = Vec::new();
            for request in requests {
                let node = u64::from(request.node);
                if !cold.contains(&node)
                    && taffy::CacheTree::cache_get(self, request.node, &request.input).is_none()
                {
                    cold.push(node);
                }
            }
            cold.len() >= MIN_COLD_NODES
        };
        if !distributable {
            results.extend(
                requests
                    .iter()
                    .map(|request| self.compute_child_layout(request.node, request.input)),
            );
            return;
        }
        self.distribute(requests, results);
    }

    /// Runs one batch across the pool.
    fn distribute(&mut self, requests: &[ChildRequest], results: &mut Vec<LayoutOutput>) {
        let pool = self.batch_pool().expect("the caller tested for a pool");
        // The ownership unit is the node: one node can appear in several requests — the
        // flex-basis site asks two questions per item — and every question about a subtree has
        // to run on the worker that owns it. Nodes are grouped in first-appearance order and
        // chunked by how many requests they carry.
        let mut node_order: Vec<taffy::NodeId> = Vec::new();
        let mut node_requests: FxHashMap<u64, Vec<usize>> = FxHashMap::default();
        for (index, request) in requests.iter().enumerate() {
            let slot = node_requests.entry(u64::from(request.node)).or_default();
            if slot.is_empty() {
                node_order.push(request.node);
            }
            slot.push(index);
        }

        let wanted = pool.width().min(node_order.len() / 2).max(1);
        let per_chunk = requests.len().div_ceil(wanted);
        let mut chunks: Vec<(Vec<taffy::NodeId>, Vec<usize>)> = Vec::new();
        for node in node_order {
            let indices = node_requests
                .get(&u64::from(node))
                .expect("every ordered node was recorded");
            let fits = chunks
                .last()
                .is_some_and(|(_, held): &(_, Vec<usize>)| held.len() < per_chunk);
            if !fits {
                chunks.push((Vec::new(), Vec::new()));
            }
            let (nodes, held) = chunks.last_mut().expect("a chunk was just ensured");
            nodes.push(node);
            held.extend(indices.iter().copied());
        }
        let chunk_count = chunks.len();
        if chunk_count < 2 {
            results.extend(
                requests
                    .iter()
                    .map(|request| self.compute_child_layout(request.node, request.input)),
            );
            return;
        }

        // Plan every chunk before anything is borrowed: the boxes its subtrees hold, the
        // paragraphs its cold requests will break, and a snapshot of every standing resolution's
        // identifiers for the accounting afterwards.
        let mut plans: Vec<ChunkPlan> = Vec::with_capacity(chunk_count);
        let mut snapshots: Vec<(BoxKey, Vec<ParagraphId>)> = Vec::new();
        for (nodes, held) in &chunks {
            let mut plan = ChunkPlan::default();
            for &node in nodes {
                // A node is cold for ownership purposes when any of its questions is.
                let cold = held.iter().any(|&index| {
                    requests[index].node == node
                        && taffy::CacheTree::cache_get(self, node, &requests[index].input).is_none()
                });
                self.plan_subtree(from_node_id(node), cold, &mut plan, &mut snapshots);
            }
            plans.push(plan);
        }

        // A batch of many cold nodes can still be shallow. Below the floor, the pool's wake-up
        // costs more than the subtrees do, and the batch runs serially where it stands.
        if plans.iter().map(|plan| plan.boxes.len()).sum::<usize>() < MIN_BATCH_BOXES {
            results.extend(
                requests
                    .iter()
                    .map(|request| self.compute_child_layout(request.node, request.input)),
            );
            return;
        }

        // Fork one measurer per chunk, each taking ownership of its chunk's shaped paragraphs.
        let mut measurers: Vec<Box<dyn WorkerMeasure>> = Vec::with_capacity(chunk_count);
        for plan in &plans {
            match self.content.fork_measurer(&plan.paragraphs) {
                Some(measurer) => measurers.push(measurer),
                None => break,
            }
        }
        if measurers.len() < chunk_count {
            // A partly forked batch would hand chunks to measurers owning another chunk's
            // paragraphs; the forks give back whatever they were handed and the batch runs
            // serially.
            for measurer in measurers {
                self.content.absorb_measurer(measurer);
            }
            results.extend(
                requests
                    .iter()
                    .map(|request| self.compute_child_layout(request.node, request.input)),
            );
            return;
        }

        counter::bump(Counter::LayoutBatchesDistributed);
        tracing::debug!(
            target: "zgui::layout",
            requests = requests.len(),
            workers = measurers.len(),
            "batch distributed"
        );

        // Carve one exclusive borrow per planned box out of the layout column. The sort is what
        // lets the carving be ordinary splitting, and its strict-ascending requirement is also
        // the disjointness check: two chunks claiming one box cannot survive it.
        let mut tagged: Vec<(BoxKey, u32)> = plans
            .iter()
            .enumerate()
            .flat_map(|(chunk, plan)| plan.boxes.iter().map(move |&key| (key, chunk as u32)))
            .collect();
        tagged.sort_unstable_by_key(|(key, _)| key.index());
        let sorted: Vec<BoxKey> = tagged.iter().map(|&(key, _)| key).collect();

        let device = self.device;
        let (column, structure) = self.store.get_mut().split_for_batch();
        let mut tables: Vec<FxHashMap<u32, &mut BoxLayout>> = plans
            .iter()
            .map(|plan| FxHashMap::with_capacity_and_hasher(plan.boxes.len(), Default::default()))
            .collect();
        for (&(key, chunk), slot) in tagged.iter().zip(column.disjoint_mut(&sorted)) {
            let state = slot.as_mut().expect("a planned box holds layout state");
            tables[chunk as usize].insert(key.index(), state);
        }

        let mut outcomes: Vec<Option<Vec<(usize, LayoutOutput)>>> =
            (0..chunk_count).map(|_| None).collect();
        pool.scope(|scope| {
            for (((measurer, (_, held)), table), outcome) in measurers
                .iter_mut()
                .zip(chunks.iter())
                .zip(tables.into_iter())
                .zip(outcomes.iter_mut())
            {
                scope.spawn(move |_| {
                    let mut tree = LayoutTree {
                        store: StoreView::Worker(WorkerStore::new(structure, table)),
                        content: measurer,
                        device,
                        atomic: AtomicMemo::default(),
                        text: TextStyles::default(),
                        custom: &crate::custom::NoCustomLayout,
                        has_custom: false,
                        parallel: None,
                    };
                    *outcome = Some(
                        held.iter()
                            .map(|&index| {
                                let request = requests[index];
                                (
                                    index,
                                    tree.compute_child_layout(request.node, request.input),
                                )
                            })
                            .collect(),
                    );
                });
            }
        });

        // Every result back into request order, then the paragraph identifiers, then the
        // measurers — the same sequence a serial pass produces.
        let mut ordered: Vec<Option<LayoutOutput>> = vec![None; requests.len()];
        for outcome in outcomes {
            for (index, output) in outcome.expect("every spawned worker finished") {
                ordered[index] = Some(output);
            }
        }
        results.extend(
            ordered
                .into_iter()
                .map(|output| output.expect("every request was computed")),
        );
        self.finish_batch_paragraphs(&plans, &snapshots);
        for measurer in measurers {
            self.content.absorb_measurer(measurer);
        }
    }

    /// Plans one request's subtree: its boxes, its cold shaped paragraphs, and a snapshot of
    /// every standing resolution's identifiers.
    ///
    /// A resolution carries its key outright, marks included. A context flattened but not yet
    /// resolved — the first layout after the pre-shape prepass — is keyed the way the prepass
    /// keyed it, which is only possible for a context with no inline items; one with items shapes
    /// on the worker.
    fn plan_subtree(
        &self,
        root: BoxKey,
        cold: bool,
        plan: &mut ChunkPlan,
        snapshots: &mut Vec<(BoxKey, Vec<ParagraphId>)>,
    ) {
        let structure = self.store.structure();
        let scale = self.device.scale;
        let mut stack = vec![root];
        while let Some(key) = stack.pop() {
            plan.boxes.push(key);
            if let Some(resolution) = self.store.inline_resolution(key) {
                snapshots.push((key, resolution.paragraphs().collect()));
                if cold {
                    plan.paragraphs.push(resolution.key);
                    for mark in [resolution.ellipsis.start, resolution.ellipsis.end]
                        .into_iter()
                        .flatten()
                    {
                        plan.paragraphs.push(mark.key);
                    }
                }
            } else if cold {
                if let Some(flattened) = self.store.flattened(key) {
                    let generated = flattened.generated();
                    if generated.items.is_empty() {
                        let content = ParagraphContent {
                            text: &generated.text,
                            map: &generated.map,
                            runs: &generated.runs,
                            boxes: &[],
                            paragraph: &generated.paragraph,
                            scale,
                        };
                        plan.paragraphs.push(generated.key(&content));
                    }
                }
            }
            stack.extend(structure.node(key).children.iter().copied());
        }
    }

    /// Replaces worker-recorded paragraph markers with real identifiers, and reconstructs the
    /// retain-and-release accounting the workers' in-place writes bypassed.
    fn finish_batch_paragraphs(
        &mut self,
        plans: &[ChunkPlan],
        snapshots: &[(BoxKey, Vec<ParagraphId>)],
    ) {
        let store = self.store.get_mut();
        for plan in plans {
            for &key in &plan.boxes {
                let written = store.inline_resolution(key).is_some_and(|resolution| {
                    resolution
                        .paragraphs()
                        .any(|id| id == PROVISIONAL_PARAGRAPH)
                });
                if !written {
                    continue;
                }
                let mut resolution = store
                    .state_mut(key)
                    .inline
                    .take()
                    .expect("the resolution tested just above");
                resolution.paragraph = store.intern_paragraph(resolution.key);
                for mark in resolution.ellipsis.marks_mut() {
                    mark.paragraph = store.intern_paragraph(mark.key);
                }
                let old: &[ParagraphId] = snapshots
                    .iter()
                    .find(|(snapshot, _)| *snapshot == key)
                    .map_or(&[], |(_, ids)| ids);
                let new: Vec<ParagraphId> = resolution.paragraphs().collect();
                for id in &new {
                    if !old.contains(id) {
                        store.retain_paragraph(*id);
                    }
                }
                for id in old {
                    if !new.contains(id) {
                        store.release_paragraph(*id);
                    }
                }
                store.state_mut(key).inline = Some(resolution);
            }
        }
    }
}
