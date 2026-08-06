//! What one cache is holding, in the terms eviction order is decided in.

use crate::budget::epoch::SceneEpoch;

/// Which cache a report describes.
///
/// A closed set rather than a string, because the registry is walked and indexed by it: a cache
/// added here without being registered fails to compile, and one registered without being named
/// here cannot be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CacheId {
    /// The rasterised glyphs and pictures in the window's texture atlas, and the placements
    /// remembered beside them.
    GlyphAtlas,
    /// The decoded texels attached to replaced nodes.
    DecodedImages,
    /// The shaped paragraphs the text engine holds between frames.
    ParagraphShaping,
    /// The placed outlines the window's drawings have been fitted into their boxes as.
    VectorResources,
    /// The reusable targets the renderer composes isolated content in.
    RenderTargets,
    /// Everything else the renderer holds on the device: its pipelines, its swapchain, the target
    /// a frame is composed into, the vector scratch and the buffers a frame uploads through.
    DeviceMemory,
}

impl CacheId {
    /// Every cache a window registers, in registration order.
    pub const ALL: [Self; 6] = [
        Self::GlyphAtlas,
        Self::DecodedImages,
        Self::ParagraphShaping,
        Self::VectorResources,
        Self::RenderTargets,
        Self::DeviceMemory,
    ];

    /// How many caches there are.
    pub const COUNT: usize = Self::ALL.len();

    /// Its position in [`CacheId::ALL`], which is also its slot in the manager's own bookkeeping.
    pub const fn index(self) -> usize {
        match self {
            Self::GlyphAtlas => 0,
            Self::DecodedImages => 1,
            Self::ParagraphShaping => 2,
            Self::VectorResources => 3,
            Self::RenderTargets => 4,
            Self::DeviceMemory => 5,
        }
    }

    /// A short name, for a report a person reads.
    pub const fn name(self) -> &'static str {
        match self {
            Self::GlyphAtlas => "glyph atlas",
            Self::DecodedImages => "decoded images",
            Self::ParagraphShaping => "paragraph shaping",
            Self::VectorResources => "vector resources",
            Self::RenderTargets => "render targets",
            Self::DeviceMemory => "device memory",
        }
    }
}

/// What a cache's figures are counted in.
///
/// Three caches here are texture or texel memory and are budgeted in bytes; the other two hold
/// objects whose bulk is inside a shaper's or an allocator's own structures, and a byte figure for
/// those would be a guess presented as a measurement. Carrying the unit is what stops a reader
/// adding them together, and what makes a stated level mean something without a comment beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheUnit {
    /// Bytes of memory, held by this process or by the device.
    Bytes,
    /// Cached entries, whatever one entry costs.
    Entries,
}

impl CacheUnit {
    /// The unit's short name, for a report a person reads.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bytes => "bytes",
            Self::Entries => "entries",
        }
    }
}

/// What rebuilding a cache's contents costs, on the one scale eviction order compares.
///
/// The rungs are an **ordinal and not a duration**. They are only ever compared with one another,
/// never added, scaled or converted into time, and nothing in this framework has measured a
/// nanosecond figure for any of them — a number that claimed to be one would be invented. What they
/// have to get right is the order, and the order is a property of what the work is rather than of
/// how fast a machine does it.
pub mod rebuild {
    /// Reproduced from data already in this process's memory, by arithmetic alone.
    ///
    /// Placing a drawing is this: the notation is on the element and fitting it to the box is a
    /// matrix and a walk over the curves.
    pub const ARITHMETIC: u64 = 1;

    /// Reproduced by running a rasteriser or a parser over data already in memory.
    pub const RECOMPUTED: u64 = 10;

    /// Reproduced only by re-running the most expensive stage in the pipeline.
    ///
    /// Shaping is this, and it is worse than its own cost suggests: everything measured from a
    /// shaped paragraph is invalidated with it, so the frame that reshapes also re-measures and
    /// re-lays-out every box the paragraph is in.
    pub const RESHAPED: u64 = 100;

    /// Reproduced by decoding a source that has to be read again first.
    ///
    /// Image texels are this: the loader keeps the path or the bytes URL, and getting the pixels
    /// back is a file read and a codec run on the blocking pool — dearer than anything reproduced
    /// from memory, but honestly reproducible, which for years this rung's neighbour below said
    /// they were not.
    pub const DECODED: u64 = 1000;

    /// Not reproducible from anything this process holds.
    ///
    /// Texels an embedder attached directly, with no source the runtime can go back to, are this.
    /// A cache at this rung reports everything it holds as pinned; the rung exists so that an
    /// ordering which somehow reached it still puts it last.
    pub const UNREPRODUCIBLE: u64 = u64::MAX;
}

/// A snapshot of one cache's occupancy, for budgeting and for the inspector.
///
/// Every field is in the cache's own [`CacheReport::unit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheReport {
    /// What the cache is holding.
    pub resident: u64,
    /// How much of that is held by something still using it, and so is never evictable.
    ///
    /// For the glyph atlas this is the tiles a live record retains — the rasters a replayed range
    /// draws from without ever looking them up, which is exactly the set that eviction used to be
    /// free to take.
    pub pinned: u64,
    /// The last frame in which anything read the cache.
    ///
    /// Recorded by the manager rather than by the cache, because a cache does not count frames. See
    /// [`Budgeted::observe`](crate::budget::Budgeted::observe).
    pub last_used: SceneEpoch,
    /// What rebuilding what it holds would cost, on the [`rebuild`] scale.
    pub rebuild_cost: u64,
    /// How much of what it holds was produced on speculation and has never been used.
    ///
    /// Evicted before anything in the used class whatever its `last_used`, because a guess that has
    /// not paid off is the one thing in a cache that is certainly not needed. Nothing in this
    /// window prewarms anything yet, so every registered cache reports zero here and will go on
    /// doing so until something speculates — which is why the ordering is asserted directly rather
    /// than inferred from a run.
    pub speculative: u64,
    /// What the four figures above are counted in.
    pub unit: CacheUnit,
}

impl CacheReport {
    /// A cache holding nothing, counted in `unit`.
    pub const fn empty(unit: CacheUnit) -> Self {
        Self {
            resident: 0,
            pinned: 0,
            last_used: SceneEpoch::FIRST,
            rebuild_cost: rebuild::ARITHMETIC,
            speculative: 0,
            unit,
        }
    }

    /// How much of what it holds could be freed.
    ///
    /// Saturating, because `pinned` is a count taken from the cache's own bookkeeping and a cache
    /// that reports more held than resident is describing a state a budget has no policy for; the
    /// answer that gets that case wrong in the safe direction is "nothing may go".
    pub const fn evictable(&self) -> u64 {
        self.resident.saturating_sub(self.pinned)
    }

    /// Whether nothing at all is held.
    pub const fn is_empty(&self) -> bool {
        self.resident == 0
    }
}

/// One cache's line in a window's budget report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheLine {
    /// Which cache.
    pub id: CacheId,
    /// What it is holding.
    pub report: CacheReport,
    /// The level it is expected to come back below, or `None` for a cache that states none.
    pub limit: Option<u64>,
}

impl CacheLine {
    /// How far over its limit the cache is, or zero when it is under one or has none.
    pub const fn over(&self) -> u64 {
        match self.limit {
            None => 0,
            Some(limit) => self.report.resident.saturating_sub(limit),
        }
    }
}

/// Every registered cache's line, in registration order.
///
/// A fixed array rather than a list, because the registry is closed: a report with a cache missing
/// from it would be a report a reader could not tell from one where that cache was empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetReport {
    /// One line per [`CacheId`], indexed by [`CacheId::index`].
    lines: [CacheLine; CacheId::COUNT],
}

impl Default for BudgetReport {
    /// Every cache empty and stating no level, which is what a window that has not run its budget
    /// step yet knows about itself.
    fn default() -> Self {
        Self::new(CacheId::ALL.map(|id| CacheLine {
            id,
            report: CacheReport::empty(CacheUnit::Entries),
            limit: None,
        }))
    }
}

impl BudgetReport {
    /// A report built from one line per cache, in registration order.
    pub const fn new(lines: [CacheLine; CacheId::COUNT]) -> Self {
        Self { lines }
    }

    /// One cache's line.
    pub const fn line(&self, id: CacheId) -> &CacheLine {
        &self.lines[id.index()]
    }

    /// Every line, in registration order.
    pub fn lines(&self) -> impl Iterator<Item = &CacheLine> {
        self.lines.iter()
    }

    /// Every cache that is over the level it stated, and by how much.
    pub fn over_limit(&self) -> impl Iterator<Item = (CacheId, u64)> + '_ {
        self.lines
            .iter()
            .filter(|line| line.over() > 0)
            .map(|line| (line.id, line.over()))
    }

    /// How many bytes the caches counted in bytes are holding between them.
    ///
    /// The caches counted in entries are deliberately absent rather than converted: an entry count
    /// added to a byte count is a number that means nothing.
    pub fn resident_bytes(&self) -> u64 {
        self.lines
            .iter()
            .filter(|line| line.report.unit == CacheUnit::Bytes)
            .map(|line| line.report.resident)
            .sum()
    }
}
