//! A text engine, the paragraphs it has shaped, and the brushes they are drawn with.

use zgui_profile::{Counter, counter};
use zgui_scene::{TextPaint as SceneTextPaint, TextPaintTable};
use zgui_text::{
    BreakRequest, BrokenParagraph, Brush, ParagraphCache, ParagraphContent, ParagraphKey,
    ParagraphShaper, ShapedClusters, ShapedGlyphs, ShapedRun, StrutMetrics,
};
use zgui_text_style::{TextPaint, TextStyle};

use crate::measure::{MeasureContent, MeasureRequest, Measured, NaturalSize, ShapedSummary};

/// A shaper, its shaped paragraphs and its brush table, as one measurer.
///
/// The cache is here rather than inside a shaper because it is what outlives the pass: a paragraph
/// shaped for one frame is broken again in the next twenty without being touched, and a shaper that
/// held its own cache could not be replaced without replacing that behaviour too.
///
/// The brush table is here for the opposite reason — it must outlive *everything*. A shaped
/// paragraph stores brush slots, so a table rebuilt per frame would leave every cached paragraph
/// pointing at whatever landed in its slot next.
///
/// Content this engine does not lay out — an image, a video, an embedded surface — is answered by
/// `replaced`, which is a second measurer because knowing how big a picture is has nothing to do
/// with knowing how wide a word is.
#[derive(Debug)]
pub struct Paragraphs<S: ParagraphShaper, R = NaturalSize> {
    /// The engine.
    shaper: S,
    /// What it has shaped.
    cache: ParagraphCache<S::Engine>,
    /// The brushes runs are drawn with.
    paints: TextPaintTable,
    /// The cascade results those brushes were claimed against, held so that their addresses
    /// cannot be handed out to a later style while a shaped paragraph still names the slot.
    pinned: Vec<zgui_text_style::TextPaintKey>,
    /// Forked shapers for the pre-shape workers, kept warm across frames.
    forks: Vec<S>,
    /// Batch-worker measurers, kept warm across batches so a fork costs a font context once.
    workers: Vec<Paragraphs<S, NaturalSize>>,
    /// Whether this measurer belongs to a batch worker and so may claim no brush slot.
    ///
    /// A slot is an identity later frames compare against, and a worker's table is thrown away —
    /// a claim here would shape a run into a slot nothing owns. The pre-shape prepass flattens
    /// every dirty context serially before a batch runs, so a worker only ever reuses claims.
    sealed_paints: bool,
    /// Whoever can size content this engine does not lay out.
    replaced: R,
}

impl<S: ParagraphShaper> Paragraphs<S> {
    /// A measurer over `shaper`, sizing replaced content by its reported natural size.
    ///
    /// That default is what a window wants: an image measures as the picture it holds. A test
    /// that needs to control replaced answers — or refuse them — says so through
    /// [`Paragraphs::with_replaced`].
    pub fn new(shaper: S) -> Self {
        Self::with_replaced(shaper, NaturalSize)
    }
}

impl<S: ParagraphShaper, R: MeasureContent> Paragraphs<S, R> {
    /// A measurer over `shaper`, with `replaced` answering for content it does not lay out.
    pub fn with_replaced(shaper: S, replaced: R) -> Self {
        Self {
            shaper,
            cache: ParagraphCache::new(),
            paints: TextPaintTable::new(),
            pinned: Vec::new(),
            forks: Vec::new(),
            workers: Vec::new(),
            sealed_paints: false,
            replaced,
        }
    }

    /// A measurer one batch worker owns, drawing on a fork of this one's shaper.
    ///
    /// `None` when the shaper cannot fork, which keeps the batch serial. The shaped results held
    /// under `owned` move out of this cache and into the worker's: breaking mutates an entry, so
    /// the worker that will break a paragraph owns it for the batch, and everything comes back
    /// through [`Paragraphs::absorb_worker`]. A key nothing holds is a worker-side shape — a
    /// cost, never a wrong answer.
    ///
    /// Workers are pooled across calls, so a steady frame forks no font context at all.
    pub fn fork_worker(&mut self, owned: &[ParagraphKey]) -> Option<Paragraphs<S, NaturalSize>> {
        let mut worker = match self.workers.pop() {
            Some(worker) => worker,
            None => {
                let mut worker = Paragraphs::new(self.shaper.fork()?);
                worker.sealed_paints = true;
                worker
            }
        };
        for &key in owned {
            if let Some(shaped) = self.cache.take(key) {
                worker.cache.insert(shaped);
            }
        }
        Some(worker)
    }

    /// Takes a worker's shaped results into the cache the frame reads, and keeps the worker warm.
    ///
    /// Entries move whole — break state included — and replace what stands under their keys, so
    /// the lines a worker kept are the lines painting finds. Called in request order, which makes
    /// the winner of a shared key the one a serial pass would have kept.
    pub fn absorb_worker(&mut self, mut worker: Paragraphs<S, NaturalSize>) {
        for shaped in worker.cache.drain_shaped() {
            self.cache.insert(shaped);
        }
        self.workers.push(worker);
    }

    /// Shapes the given paragraphs before layout asks for them, across `pool`'s workers.
    ///
    /// Keys the cache already holds are skipped. The rest are shaped on forked shapers and
    /// inserted in job order, so layout finds every one warm and the cache is filled exactly as
    /// serial shaping would have filled it. An engine that cannot fork shapes serially here,
    /// which still warms the cache.
    pub fn pre_shape(
        &mut self,
        jobs: &[(ParagraphKey, ParagraphContent<'_>)],
        pool: &crate::tree::parallel::LayoutPool,
    ) where
        S: Send,
        S::Engine: Send,
    {
        // Identical contexts share one key — a list of identical rows is the common case — so a
        // key is taken once however many jobs carry it.
        //
        // A set rather than a list. The case the list was written for is the one where it stays
        // short, and that is also the case where it costs nothing either way; the case that decides
        // the shape is the other one — a cold first paint, or a virtualised list remounting a
        // screenful of distinct rows — where hundreds of distinct keys are scanned against each
        // other on the frame path.
        let mut taken: rustc_hash::FxHashSet<ParagraphKey> =
            rustc_hash::FxHashSet::with_capacity_and_hasher(jobs.len(), rustc_hash::FxBuildHasher);
        let misses: Vec<usize> = jobs
            .iter()
            .enumerate()
            .filter(|(_, (key, _))| !self.cache.holds(*key) && taken.insert(*key))
            .map(|(index, _)| index)
            .collect();
        if misses.is_empty() {
            return;
        }
        let wanted = pool.width().min(misses.len());
        while self.forks.len() < wanted {
            match self.shaper.fork() {
                Some(fork) => self.forks.push(fork),
                None => break,
            }
        }
        // One miss gains nothing from a worker, and an unforkable engine has none to give.
        if self.forks.is_empty() || misses.len() < 2 {
            for &index in &misses {
                let (key, content) = &jobs[index];
                let shaped = self.shaper.shape_keyed(*key, content);
                self.cache.insert(shaped);
            }
            return;
        }
        let per_chunk = misses.len().div_ceil(self.forks.len());
        let mut outs: Vec<Vec<zgui_text::ShapedParagraph<S::Engine>>> =
            (0..self.forks.len()).map(|_| Vec::new()).collect();
        pool.scope(|scope| {
            for ((fork, chunk), out) in self
                .forks
                .iter_mut()
                .zip(misses.chunks(per_chunk))
                .zip(outs.iter_mut())
            {
                scope.spawn(move |_| {
                    for &index in chunk {
                        let (key, content) = &jobs[index];
                        out.push(fork.shape_keyed(*key, content));
                    }
                });
            }
        });
        // Chunking preserved miss order, so this insertion order is the serial one.
        for shaped in outs.into_iter().flatten() {
            self.cache.insert(shaped);
        }
    }

    /// The engine.
    pub fn shaper(&self) -> &S {
        &self.shaper
    }

    /// The paragraphs shaped so far.
    pub fn cache(&self) -> &ParagraphCache<S::Engine> {
        &self.cache
    }

    /// The brushes, for a theme change that rewrites them.
    pub fn paints_mut(&mut self) -> &mut TextPaintTable {
        &mut self.paints
    }

    /// Whoever sizes replaced content.
    pub fn replaced_mut(&mut self) -> &mut R {
        &mut self.replaced
    }

    /// Drops every shaped paragraph, and reports how many that threw away.
    ///
    /// A font finishing loading makes this necessary, and so does a run whose brush can no longer
    /// be rewritten where it stands. The count is what tells a caller whether anything measured
    /// from those paragraphs is now stale: every one of them has to be measured again, and a cache
    /// that held nothing leaves nothing to do.
    pub fn forget_shaped(&mut self) -> usize {
        let dropped = self.cache.clear();
        counter::add(Counter::ParagraphsForgotten, dropped as u64);
        dropped
    }

    /// Drops the shaped paragraphs held under `keys`, and reports how many that threw away.
    ///
    /// The narrow form of [`Paragraphs::forget_shaped`], for the case where what stopped being
    /// true is one element's: the brush its glyphs carry was baked in when they were shaped, so a
    /// brush it can no longer be re-coloured through costs the paragraphs its text is in and
    /// nothing else. The count means the same thing either way — every measurement taken from a
    /// dropped paragraph has to be taken again — and a key that was not held contributes nothing,
    /// because a paragraph that was never shaped leaves no measurement behind.
    pub fn forget_paragraphs(&mut self, keys: &[ParagraphKey]) -> usize {
        let mut dropped = 0;
        for &key in keys {
            if self.cache.holds(key) {
                self.cache.forget(key);
                dropped += 1;
            }
        }
        counter::add(Counter::ParagraphsEvicted, dropped as u64);
        dropped
    }

    /// Drops the coldest shaped results no current layout resolution names.
    pub fn evict_inactive(&mut self, active: &[ParagraphKey], count: usize) -> usize {
        let dropped = self.cache.evict_inactive(active, count);
        counter::add(Counter::ParagraphsEvicted, dropped as u64);
        dropped
    }
}

impl<S: ParagraphShaper, R> ShapedGlyphs for Paragraphs<S, R> {
    fn visit_line(&self, paragraph: ParagraphKey, line: u16, visit: &mut dyn FnMut(ShapedRun<'_>)) {
        let Some(shaped) = self.cache.get(paragraph) else {
            return;
        };
        self.shaper.visit_line(shaped, line, visit);
    }
}

impl<S: ParagraphShaper, R> ShapedClusters for Paragraphs<S, R> {
    fn visit_clusters(
        &self,
        paragraph: ParagraphKey,
        line: u16,
        visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
        let Some(shaped) = self.cache.get(paragraph) else {
            return;
        };
        self.shaper.visit_clusters(shaped, line, visit);
    }
}

impl<S: ParagraphShaper, R: MeasureContent> MeasureContent for Paragraphs<S, R> {
    fn measure(&mut self, request: MeasureRequest<'_>) -> Measured {
        self.replaced.measure(request)
    }

    fn shape(&mut self, content: &ParagraphContent<'_>) -> ShapedSummary {
        let key = ParagraphKey::of(content);
        self.shape_keyed(key, content)
    }

    fn shape_keyed(&mut self, key: ParagraphKey, content: &ParagraphContent<'_>) -> ShapedSummary {
        if !self.cache.holds(key) {
            let shaped = self.shaper.shape_keyed(key, content);
            self.cache.insert(shaped);
        }
        let widths = self
            .cache
            .get(key)
            .map(|shaped| shaped.content_widths())
            .unwrap_or_default();
        ShapedSummary { key, widths }
    }

    fn break_lines(&mut self, key: ParagraphKey, request: &BreakRequest<'_>) -> BrokenParagraph {
        let Some(shaped) = self.cache.get_mut(key) else {
            return BrokenParagraph::default();
        };
        self.shaper.break_lines(shaped, request)
    }

    fn strut(&mut self, style: &TextStyle) -> StrutMetrics {
        self.shaper.strut(style)
    }

    fn paint_slot(&mut self, paint: &TextPaint) -> Brush {
        // See the field: a worker claiming would shape a run into a slot nothing owns, and the
        // wrong colour it produces later has no error attached anywhere. The prepass flattening
        // every dirty context first is what makes this unreachable.
        assert!(
            !self.sealed_paints,
            "a batch worker was asked to claim a brush slot; a context was not pre-flattened"
        );
        let address = paint.key.addr() as u64;
        let colour = paint.color;
        // The one thing that can be wrong here and produce no error anywhere. A slot answers to a
        // cascade result, and every run whose colour came from that result is drawn through it — so
        // a slot that holds some *other* colour when this run asks for it is a run about to be
        // shaped into the wrong brush, permanently, with correct damage and a display list that
        // says exactly what it means to say. There is no later stage that can tell.
        //
        // It is asked only in a debug build because the shaping this guards is the rare event: a
        // string being shaped for the first time, or every string in the window on the frame a
        // change of device scale re-shapes them all.
        debug_assert!(
            self.paints
                .slot_of(address)
                .is_none_or(|slot| { self.paints.get(slot) == Some(&SceneTextPaint::new(colour)) }),
            "a run is being shaped into a brush slot that holds another colour: the way back from \
             its cascade result to a slot has been pointed at something else's"
        );
        let pinned = &mut self.pinned;
        let held = paint.key.clone();
        self.paints.slot_for(address, || {
            pinned.push(held);
            SceneTextPaint::new(colour)
        })
    }
}
