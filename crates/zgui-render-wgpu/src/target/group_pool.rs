//! The pool of targets isolated content is composed in.

use zgui_geom::{Device, Edges, Point, Rect, Size};
use zgui_profile::{Counter, counter};

use crate::gpu::device::Gpu;
use crate::target::scale::TargetScale;
use crate::target::scene_texture::size_class;

/// A target lent out of the pool.
///
/// It names a lease rather than a texture, so that the pool stays the one owner and a target
/// cannot be handed to two composites at once. Returning it is [`GroupPool::release`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupSlot {
    /// Which entry of the pool.
    index: usize,
    /// What resolution it was lent at, which the composite reading it has to know.
    scale: TargetScale,
    /// What it holds, which decides what may be copied into it.
    format: wgpu::TextureFormat,
    /// The device pixel its first texel holds.
    origin: Point<i32, Device>,
    /// How many device pixels from the origin the lease covers.
    covered: Size<i32, Device>,
    /// The texel extent of the texture behind the lease.
    allocated: Size<i32, Device>,
}

impl GroupSlot {
    /// The device pixel this target's first texel holds.
    ///
    /// A texel of the target is `(device - origin) * scale`. A lease starts near the corner of the
    /// region it was taken for ([`GroupPool::acquire_covering`]). The test mode
    /// [`GroupPool::POISON_ORIGIN`] moves it further. The origin is even on both axes, so the texel
    /// grid of a half-resolution target stays anchored to the device origin.
    pub fn origin(self) -> Point<i32, Device> {
        self.origin
    }

    /// How many device pixels from [`GroupSlot::origin`] the lease covers.
    ///
    /// Every pass into the target maps this size onto [`GroupSlot::extent`] texels.
    pub fn covered(self) -> Size<i32, Device> {
        self.covered
    }

    /// The texels the lease covers, measured from the first texel.
    pub fn extent(self) -> Size<i32, Device> {
        self.scale.extent(self.covered)
    }

    /// The texel extent of the texture behind the lease.
    ///
    /// The texture is rounded to a size class and can be larger than the lease, so a read of it
    /// divides by this extent.
    pub fn allocated(self) -> Size<i32, Device> {
        self.allocated
    }

    /// Whether every device pixel of `rect` falls on a texel of the texture behind the lease.
    ///
    /// A copy into the target writes only these texels. The first pass of a lease clears the whole
    /// texture, so the texels past the lease read transparent. This and [`GroupPool::GUARD`]
    /// depend on that clear: a clear cut to the lease leaves what an earlier lease drew there.
    pub fn holds(self, rect: Rect<i32, Device>) -> bool {
        let pixels = match self.scale {
            TargetScale::Full => self.allocated,
            TargetScale::Half => Size::new(self.allocated.width * 2, self.allocated.height * 2),
        };
        Rect::new(self.origin, pixels).contains_rect(rect)
    }

    /// The resolution this target is held at.
    pub fn scale(self) -> TargetScale {
        self.scale
    }

    /// The format this target holds.
    pub fn format(self) -> wgpu::TextureFormat {
        self.format
    }
}

/// One target held by the pool.
#[derive(Debug)]
struct Entry {
    /// The texture.
    texture: wgpu::Texture,
    /// A view of the whole of it.
    view: wgpu::TextureView,
    /// Its extent in texels.
    extent: Size<i32, Device>,
    /// The resolution it holds.
    scale: TargetScale,
    /// The format it holds.
    format: wgpu::TextureFormat,
    /// Whether it is currently lent out.
    lent: bool,
    /// Whether this frame lent it, which keeps it resident until the frame is recorded: the plan
    /// names its targets by slot, and recording reads them after planning ends.
    touched: bool,
    /// How many frames in a row have ended without a lease of it.
    idle: u32,
}

/// Targets for isolated content, grown on demand and reused within a frame.
///
/// A lease covers the device region it was taken for, grown by [`GroupPool::GUARD`], at one of two
/// resolutions. Content drawn into it lands at exactly the device coordinates it would have landed
/// at without the isolation. All targets share device-pixel addressing, so a clip and a shape are
/// always evaluated in the same coordinates. Each lease carries the device pixel its first texel
/// holds ([`GroupSlot::origin`]) and the size it covers ([`GroupSlot::covered`]), and every pass
/// that writes or reads a target maps device pixels to texels through them.
///
/// Textures are rounded to a size class and a lease takes the smallest free one that holds it, so
/// a region that changes size a little from frame to frame allocates nothing. A texture stays
/// resident for at most [`GroupPool::IDLE_FRAMES`] frames in a row without a lease.
///
/// **There is no depth limit.** A fixed one is a correctness cliff: CSS nests filters and
/// stacking contexts as deep as an author writes them, and a document that exceeds the limit does
/// not render slightly worse, it renders wrongly. What is limited is memory: the bytes of the
/// targets lent at once, and the bytes resident between frames. At the limit the pool degrades to
/// half resolution rather than to no isolation — a blurrier frosted panel is a visible cost, a
/// panel composited without its group is a wrong picture. Before it degrades, the pool frees the
/// targets no lease of this frame has used.
///
/// Isolated content is held in `Rgba16Float`, for headroom under stacked opacity and repeated
/// blends and for no other reason: every value in it is still premultiplied and gamma-encoded,
/// exactly as in every other target, and the composite reading it is a plain textured quad that
/// performs no colour conversion at all. A target lent to capture what is *beneath* something
/// takes the format and resolution of whatever it is capturing from instead, because a copy
/// between two textures requires them to agree — and copying is both cheaper and more faithful
/// than a pass that would convert on the way. That lease never degrades
/// ([`GroupPool::acquire_exact`]).
#[derive(Debug)]
pub struct GroupPool {
    /// The device-pixel region of the surface, which every lease is cut to.
    region: Size<i32, Device>,
    /// How many device pixels further above and to the left every lease starts.
    ///
    /// Zero, unless the test mode [`GroupPool::POISON_ORIGIN`] sets it.
    margin: i32,
    /// The targets. A freed target leaves a hole, so that every other slot keeps its index.
    entries: Vec<Option<Entry>>,
    /// How many bytes of targets may be resident.
    budget: u64,
    /// Whether the budget follows the region, see [`GroupPool::scaled`].
    scaled: bool,
    /// How many leases were degraded to half resolution because the budget was reached.
    degraded: u32,
    /// The most targets lent at once since the pool was built.
    peak: u32,
    /// How many leases have been taken since the pool was built.
    ///
    /// Monotonic, unlike [`GroupPool::lent`], which is zero between frames however hard the document
    /// is isolating. Two readings subtracted are what say whether the pool was drawn from.
    leases: u64,
    /// How many textures the pool has created since it was built.
    allocations: u64,
}

impl GroupPool {
    /// The format every isolated target is held in.
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

    /// How many bytes of isolated targets are resident before the pool starts reducing resolution.
    ///
    /// The smallest budget of a pool whose budget follows the surface ([`GroupPool::scaled`]). A
    /// document that isolates dozens of things at once is pushed to half resolution rather than
    /// allowed to take the whole device's memory.
    pub const BUDGET: u64 = 256 * 1024 * 1024;

    /// How many full-resolution targets a budget that follows the region holds at least.
    ///
    /// A frosted card in a frosted panel in a group holds about this many at once: the group, the
    /// card, a copy of what is beneath the card, and the copies and blur targets of a backdrop in
    /// it. On a large surface [`GroupPool::BUDGET`] holds fewer, and the pool degrades.
    pub const SURFACE_TARGETS: u64 = 6;

    /// The variable that sets a test mode, which moves every target's origin away from the
    /// surface origin.
    ///
    /// Set `ZGUI_POISON_TARGET_ORIGIN=<int>` to start every lease `P` more device pixels above and
    /// to the left, with `P` rounded up to an even number. Each lease grows by `P` on each axis, so
    /// it still covers its region, and the pixels stay the same. A reader that uses a device
    /// rectangle as a texel rectangle of a pool target is then wrong by at least `P` texels, and a
    /// pixel test finds it.
    pub const POISON_ORIGIN: &str = "ZGUI_POISON_TARGET_ORIGIN";

    /// How many device pixels a lease covers past each edge of its region.
    ///
    /// One half-resolution texel. A bilinear read at the edge of the region takes one texel
    /// outside it, and the guard makes sure that texel exists and holds transparent, as it did
    /// when every target covered the whole surface.
    pub const GUARD: i32 = 2;

    /// How many frames in a row a texture stays resident with no lease before the end of a frame
    /// frees it.
    pub const IDLE_FRAMES: u32 = 60;

    /// The usage an isolated target needs: drawn into, sampled by the composite, copied into by a
    /// backdrop capture, and copied out by a test.
    const USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
        .union(wgpu::TextureUsages::TEXTURE_BINDING)
        .union(wgpu::TextureUsages::COPY_DST)
        .union(wgpu::TextureUsages::COPY_SRC);

    /// An empty pool for leases on a surface of `region`, holding at most `budget` bytes.
    pub fn new(region: Size<i32, Device>, budget: u64) -> Self {
        Self {
            region: region.non_negative(),
            margin: poison_margin(),
            entries: Vec::new(),
            budget,
            scaled: false,
            degraded: 0,
            peak: 0,
            leases: 0,
            allocations: 0,
        }
    }

    /// An empty pool for leases on a surface of `region`, whose budget follows the region.
    ///
    /// The budget is [`GroupPool::BUDGET`], or room for [`GroupPool::SURFACE_TARGETS`]
    /// full-resolution targets where that is more. A resize computes it again.
    pub fn scaled(region: Size<i32, Device>) -> Self {
        let mut pool = Self::new(region, Self::budget_for(region));
        pool.scaled = true;
        pool
    }

    /// The budget of a pool whose budget follows `region`.
    pub fn budget_for(region: Size<i32, Device>) -> u64 {
        let full = allocated_for(region.non_negative(), TargetScale::Full);
        let one = u64::from(Self::FORMAT.block_copy_size(None).unwrap_or(8))
            * full.width.max(1) as u64
            * full.height.max(1) as u64;
        Self::BUDGET.max(Self::SURFACE_TARGETS * one)
    }

    /// Points the pool at a region of a different size.
    ///
    /// Targets stay, because a lease takes its origin and size when it is lent. A target larger than
    /// a lease over the whole new region can be is freed.
    pub fn resize(&mut self, region: Size<i32, Device>) {
        let region = region.non_negative();
        self.region = region;
        let margin = self.margin;
        for held in &mut self.entries {
            if held.as_ref().is_some_and(|entry| {
                let largest = whole_allocated(region, margin, entry.scale);
                entry.extent.width > largest.width || entry.extent.height > largest.height
            }) {
                *held = None;
            }
        }
        trim(&mut self.entries);
        if self.scaled {
            self.budget = Self::budget_for(region);
        }
    }

    /// Lends a target covering the whole region at `scale`, reducing the resolution if the budget
    /// will not take another.
    ///
    /// For tests and whole-surface callers. The renderer leases the region it needs through
    /// [`GroupPool::acquire_covering`].
    ///
    /// Returns `None` only when the budget will not take even a half-resolution target, which
    /// means the budget is smaller than one of them.
    pub fn acquire(
        &mut self,
        gpu: &Gpu,
        scale: TargetScale,
        format: wgpu::TextureFormat,
    ) -> Option<GroupSlot> {
        self.acquire_covering(gpu, scale, format, self.surface())
    }

    /// Lends a target covering `region` at `scale`, reducing the resolution if the budget will not
    /// take another.
    ///
    /// Returns `None` only when the budget will not take even a half-resolution target, which
    /// means the budget is smaller than one of them.
    pub fn acquire_covering(
        &mut self,
        gpu: &Gpu,
        scale: TargetScale,
        format: wgpu::TextureFormat,
        region: Rect<i32, Device>,
    ) -> Option<GroupSlot> {
        let lent = self.lent();
        let cover = self.cover(region);
        let slot = self
            .reuse(scale, format, cover)
            .or_else(|| self.allocate(gpu, scale, format, cover))
            .or_else(|| {
                // The budget refused a full-resolution target, so isolation continues at half of
                // it: the composite magnifies through a filtering sampler, which is the whole
                // reason its bind-group layout carries one. Counted whether the half-resolution
                // target is lent again or made, because what is counted is the frame drawn at
                // the lower resolution.
                let degraded = self
                    .reuse(TargetScale::Half, format, cover)
                    .or_else(|| self.allocate(gpu, TargetScale::Half, format, cover));
                if degraded.is_some() && scale != TargetScale::Half {
                    self.degraded += 1;
                    tracing::debug!(
                        budget = self.budget,
                        resident = self.bytes(),
                        "isolating at half resolution because the target budget is full"
                    );
                }
                degraded
            })?;
        self.count(lent);
        Some(slot)
    }

    /// Lends a target covering the whole region at exactly `scale`, or nothing when the budget
    /// will not take one.
    ///
    /// For tests and whole-surface callers. The renderer leases the region it needs through
    /// [`GroupPool::acquire_exact_covering`].
    pub fn acquire_exact(
        &mut self,
        gpu: &Gpu,
        scale: TargetScale,
        format: wgpu::TextureFormat,
    ) -> Option<GroupSlot> {
        self.acquire_exact_covering(gpu, scale, format, self.surface())
    }

    /// Lends a target covering `region` at exactly `scale`, or nothing when the budget will not
    /// take one.
    ///
    /// For a target whose texels have to line up with another one, such as the destination of a
    /// copy: a target at a different resolution holds the copied texels at the wrong place.
    pub fn acquire_exact_covering(
        &mut self,
        gpu: &Gpu,
        scale: TargetScale,
        format: wgpu::TextureFormat,
        region: Rect<i32, Device>,
    ) -> Option<GroupSlot> {
        let lent = self.lent();
        let cover = self.cover(region);
        let slot = self
            .reuse(scale, format, cover)
            .or_else(|| self.allocate(gpu, scale, format, cover))?;
        self.count(lent);
        Some(slot)
    }

    /// Counts a lease taken while `lent` others were out.
    fn count(&mut self, lent: u32) {
        self.peak = self.peak.max(lent + 1);
        self.leases += 1;
        counter::bump(Counter::GroupTargets);
    }

    /// Returns a target to the pool.
    pub fn release(&mut self, slot: GroupSlot) {
        if let Some(Some(entry)) = self.entries.get_mut(slot.index) {
            entry.lent = false;
        }
    }

    /// Returns every target to the pool, which is what the end of a frame does.
    ///
    /// Frees each target that has gone without a lease for more than [`GroupPool::IDLE_FRAMES`]
    /// frames in a row. Leases of another size class leave such targets behind, and the budget
    /// alone would hold them until it is full. Then frees targets, the longest idle and largest
    /// first, until the pool is within its budget again ([`GroupPool::allocate`]).
    pub fn release_all(&mut self) {
        for held in &mut self.entries {
            let Some(entry) = held else {
                continue;
            };
            entry.idle = if entry.touched { 0 } else { entry.idle + 1 };
            entry.lent = false;
            entry.touched = false;
            if entry.idle > Self::IDLE_FRAMES {
                *held = None;
            }
        }
        let mut resident = self.bytes();
        while resident > self.budget {
            let Some((index, bytes)) = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(index, held)| {
                    let entry = held.as_ref()?;
                    Some((index, entry.idle, texture_bytes(&entry.texture)))
                })
                .max_by_key(|&(_, idle, bytes)| (idle, bytes))
                .map(|(index, _, bytes)| (index, bytes))
            else {
                break;
            };
            self.entries[index] = None;
            resident -= bytes;
        }
        trim(&mut self.entries);
    }

    /// A view of a lent target.
    pub fn view(&self, slot: GroupSlot) -> &wgpu::TextureView {
        &self.entry(slot).view
    }

    /// A lent target's texture, for a copy into or out of it.
    pub fn texture(&self, slot: GroupSlot) -> &wgpu::Texture {
        &self.entry(slot).texture
    }

    /// The entry a slot names.
    ///
    /// Panics on a hole: the pool frees only targets this frame has not lent, so a slot of this
    /// frame never names one.
    fn entry(&self, slot: GroupSlot) -> &Entry {
        self.entries[slot.index]
            .as_ref()
            .expect("a slot of this frame names a resident target")
    }

    /// The device-pixel region of the surface, which every lease is cut to.
    pub fn region(&self) -> Size<i32, Device> {
        self.region
    }

    /// Sets the margin of the test mode [`GroupPool::POISON_ORIGIN`] for this pool, discarding
    /// every target it holds.
    ///
    /// For tests that compare a pool with a margin against one without in one process.
    #[doc(hidden)]
    pub fn set_poison_margin(&mut self, margin: i32) {
        self.margin = even_margin(margin);
        self.entries.clear();
    }

    /// The device-pixel size a lease over the whole region covers from its origin.
    ///
    /// The region, grown by [`GroupPool::GUARD`] and by the margin of the test mode
    /// [`GroupPool::POISON_ORIGIN`]. For tests and whole-surface callers: the renderer reads the
    /// size of each lease from [`GroupSlot::covered`].
    pub fn covered(&self) -> Size<i32, Device> {
        self.whole().size
    }

    /// The texel extent a lease over the whole region at `scale` is allocated at.
    ///
    /// Larger than the region it holds, because targets are rounded to a size class. A read of one
    /// has to divide by *this* rather than by the region, or it samples the wrong texels. For
    /// tests and whole-surface callers: the renderer reads [`GroupSlot::allocated`].
    pub fn allocated_extent(&self, scale: TargetScale) -> Size<i32, Device> {
        self.allocated(scale)
    }

    /// How many textures the pool has created since it was built.
    ///
    /// Two readings subtracted say whether a frame allocated. Once the regions a document isolates
    /// stop changing, no frame does.
    pub fn allocations(&self) -> u64 {
        self.allocations
    }

    /// How many bytes of isolated targets may be resident.
    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// How many bytes of device memory the pool holds.
    pub fn bytes(&self) -> u64 {
        self.entries
            .iter()
            .flatten()
            .map(|entry| texture_bytes(&entry.texture))
            .sum()
    }

    /// How many targets are lent out right now.
    pub fn lent(&self) -> u32 {
        self.entries
            .iter()
            .flatten()
            .filter(|entry| entry.lent)
            .count() as u32
    }

    /// How many bytes of device memory the targets lent out right now hold.
    pub fn lent_bytes(&self) -> u64 {
        self.entries
            .iter()
            .flatten()
            .filter(|entry| entry.lent)
            .map(|entry| texture_bytes(&entry.texture))
            .sum()
    }

    /// How many leases have been taken since the pool was built.
    pub fn leases(&self) -> u64 {
        self.leases
    }

    /// Drops every target, and reports how many bytes that returned.
    ///
    /// Nothing is lost: a target holds one frame's isolated content and is cleared before it is
    /// drawn into, so what this costs is an allocation on the next frame that isolates anything.
    ///
    /// **All or nothing.** A plan names its targets by slot and recording reads them after
    /// planning ends, so a pool with anything lent out frees nothing and returns zero, which is
    /// the state a call from inside a composite would find; the end of a frame returns every
    /// lease, so a caller enforcing a budget between frames frees the whole pool.
    pub fn release_unused(&mut self) -> u64 {
        if self.lent() > 0 {
            return 0;
        }
        let before = self.bytes();
        self.entries.clear();
        before
    }

    /// The most targets ever lent at once.
    pub fn peak(&self) -> u32 {
        self.peak
    }

    /// How many leases the budget forced down to half resolution.
    pub fn degraded(&self) -> u32 {
        self.degraded
    }

    /// Lends the smallest free target already held at `scale` that holds `cover`, if there is one.
    fn reuse(
        &mut self,
        scale: TargetScale,
        format: wgpu::TextureFormat,
        cover: Rect<i32, Device>,
    ) -> Option<GroupSlot> {
        let needed = scale.extent(cover.size);
        let mut best: Option<(usize, u64)> = None;
        for (index, held) in self.entries.iter().enumerate() {
            let Some(entry) = held else {
                continue;
            };
            if entry.lent
                || entry.scale != scale
                || entry.format != format
                || entry.extent.width < needed.width
                || entry.extent.height < needed.height
            {
                continue;
            }
            let area = entry.extent.width as u64 * entry.extent.height as u64;
            if best.is_none_or(|(_, smallest)| area < smallest) {
                best = Some((index, area));
            }
        }
        let (index, _) = best?;
        let entry = self.entries[index].as_mut()?;
        entry.lent = true;
        entry.touched = true;
        Some(GroupSlot {
            index,
            scale,
            format,
            origin: cover.origin,
            covered: cover.size,
            allocated: entry.extent,
        })
    }

    /// Allocates a new target at `scale` that holds `cover`, unless the budget will not take it.
    ///
    /// Frees targets this frame has not lent when that makes room. Those are of another scale,
    /// format or size, since [`GroupPool::reuse`] takes a matching one first, and without this they
    /// stay resident and keep the pool at its budget for every later frame.
    ///
    /// The budget limits the targets lent at once. A target this frame lent and returned stays
    /// resident until the frame is recorded, and it can be too small for a later lease, so leases
    /// of growing regions can take more than the budget for one frame. [`GroupPool::release_all`]
    /// frees targets until the pool is within the budget again.
    fn allocate(
        &mut self,
        gpu: &Gpu,
        scale: TargetScale,
        format: wgpu::TextureFormat,
        cover: Rect<i32, Device>,
    ) -> Option<GroupSlot> {
        let allocated = allocated_for(cover.size, scale);
        let cost = u64::from(format.block_copy_size(None).unwrap_or(8))
            * allocated.width.max(1) as u64
            * allocated.height.max(1) as u64;
        if self.bytes() + cost > self.budget {
            if self.lent_bytes() + cost > self.budget {
                return None;
            }
            self.evict(cost);
        }
        let extent = Size::new(allocated.width.max(1), allocated.height.max(1));
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("zgui.group"),
            size: wgpu::Extent3d {
                width: extent.width as u32,
                height: extent.height as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: Self::USAGE,
            view_formats: &[],
        });
        self.allocations += 1;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let entry = Entry {
            texture,
            view,
            extent,
            scale,
            format,
            lent: true,
            touched: true,
            idle: 0,
        };
        let index = match self.entries.iter().position(Option::is_none) {
            Some(hole) => {
                self.entries[hole] = Some(entry);
                hole
            }
            None => {
                self.entries.push(Some(entry));
                self.entries.len() - 1
            }
        };
        Some(GroupSlot {
            index,
            scale,
            format,
            origin: cover.origin,
            covered: cover.size,
            allocated: extent,
        })
    }

    /// Frees targets this frame has not lent until `cost` more bytes fit in the budget, or until
    /// none is left.
    fn evict(&mut self, cost: u64) {
        let idle = |entry: &Entry| !entry.lent && !entry.touched;
        let mut resident = self.bytes();
        for held in &mut self.entries {
            if resident + cost <= self.budget {
                break;
            }
            if let Some(entry) = held.take_if(|entry| idle(entry)) {
                resident -= texture_bytes(&entry.texture);
            }
        }
        trim(&mut self.entries);
    }

    /// The texel extent a target over the whole region at `scale` is allocated at.
    fn allocated(&self, scale: TargetScale) -> Size<i32, Device> {
        whole_allocated(self.region, self.margin, scale)
    }

    /// The device pixels a lease over the whole region covers.
    fn whole(&self) -> Rect<i32, Device> {
        whole_cover(self.region, self.margin)
    }

    /// The device pixels of the whole surface.
    fn surface(&self) -> Rect<i32, Device> {
        Rect::new(Point::new(0, 0), self.region)
    }

    /// The device pixels a lease for `region` covers.
    fn cover(&self, region: Rect<i32, Device>) -> Rect<i32, Device> {
        cover_for(region, self.region, self.margin)
    }
}

/// The device pixels a lease for `region` covers, on a surface of `surface` with the test-mode
/// `margin`.
///
/// `region` grown by [`GroupPool::GUARD`], with the near corner moved to an even pixel. The near
/// side is cut to the surface origin, so a read past the near edge of the surface clamps to the
/// first texel. The far side is cut to the surface grown by the guard, so a read past the far edge
/// of the surface always reads a cleared guard texel, whatever texture holds the lease. The test
/// mode moves the near corner `margin` pixels further up and left. A region outside the surface
/// gets the whole surface.
fn cover_for(
    region: Rect<i32, Device>,
    surface: Size<i32, Device>,
    margin: i32,
) -> Rect<i32, Device> {
    let limit = Rect::from_corners(
        Point::new(-margin, -margin),
        Point::new(
            surface.width + GroupPool::GUARD,
            surface.height + GroupPool::GUARD,
        ),
    );
    let guarded = region.outset(Edges::uniform(GroupPool::GUARD));
    let near = Point::new(
        guarded.left().div_euclid(2) * 2 - margin,
        guarded.top().div_euclid(2) * 2 - margin,
    );
    Rect::from_corners(near, guarded.far_corner())
        .intersection(limit)
        .unwrap_or(limit)
}

/// The device pixels a lease over the whole of a surface of `surface` covers, with the test-mode
/// `margin`.
fn whole_cover(surface: Size<i32, Device>, margin: i32) -> Rect<i32, Device> {
    cover_for(Rect::new(Point::new(0, 0), surface), surface, margin)
}

/// The texel extent a target over the whole of `region` at `scale` is allocated at, with the
/// test-mode `margin`.
fn whole_allocated(
    region: Size<i32, Device>,
    margin: i32,
    scale: TargetScale,
) -> Size<i32, Device> {
    allocated_for(whole_cover(region, margin).size, scale)
}

/// Drops the holes at the end of `entries`.
fn trim(entries: &mut Vec<Option<Entry>>) {
    while entries.last().is_some_and(Option::is_none) {
        entries.pop();
    }
}

/// The margin the test mode [`GroupPool::POISON_ORIGIN`] gives, or zero when it is unset or is
/// not an integer.
///
/// Read when a pool is built, so a test can compare a pool with the margin against one without.
fn poison_margin() -> i32 {
    std::env::var(GroupPool::POISON_ORIGIN)
        .ok()
        .and_then(|raw| raw.trim().parse::<i32>().ok())
        .map_or(0, even_margin)
}

/// `margin` as the test mode uses it: positive, at most 4096, and rounded up to even.
fn even_margin(margin: i32) -> i32 {
    margin.unsigned_abs().min(1 << 12).next_multiple_of(2) as i32
}

/// The texel extent a target covering `region` at `scale` is allocated at.
///
/// Rounded to a size class for the same reason the composed target is: a resize or an animation
/// delivers one new extent per frame, and reallocating a target per frame is a multi-megabyte
/// allocate-and-free on the frame path.
fn allocated_for(region: Size<i32, Device>, scale: TargetScale) -> Size<i32, Device> {
    let classed: Size<i32, Device> = Size::new(size_class(region.width), size_class(region.height));
    scale.extent(classed)
}

/// How many bytes a texture occupies.
fn texture_bytes(texture: &wgpu::Texture) -> u64 {
    u64::from(texture.format().block_copy_size(None).unwrap_or(8))
        * u64::from(texture.width())
        * u64::from(texture.height())
}

#[cfg(test)]
mod tests {
    use super::{GroupPool, allocated_for, cover_for};
    use crate::target::scale::TargetScale;
    use zgui_geom::{Device, Point, Rect, Size};

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    /// How many bytes one target of `scale` costs for a region of this size.
    fn cost(region: Size<i32, Device>, scale: TargetScale) -> u64 {
        let allocated = allocated_for(region, scale);
        u64::from(GroupPool::FORMAT.block_copy_size(None).unwrap_or(8))
            * allocated.width as u64
            * allocated.height as u64
    }

    #[test]
    fn a_budget_of_nothing_lends_nothing_rather_than_lending_a_target_it_cannot_pay_for() {
        let pool = GroupPool::new(Size::new(256, 256), 0);
        assert_eq!(pool.bytes(), 0);
        assert_eq!(pool.lent(), 0);
    }

    #[test]
    fn a_region_of_the_same_size_class_keeps_every_target() {
        let mut pool = GroupPool::new(Size::new(200, 200), GroupPool::BUDGET);
        pool.resize(Size::new(250, 250));
        assert_eq!(pool.region(), Size::new(250, 250));
        assert_eq!(
            allocated_for(pool.region(), TargetScale::Full),
            allocated_for(Size::new(200, 200), TargetScale::Full),
            "both round to the same class, so nothing had to be thrown away"
        );
    }

    #[test]
    fn a_scaled_budget_holds_six_full_targets_of_a_large_surface() {
        let small = Size::new(1776, 1128);
        assert_eq!(GroupPool::budget_for(small), GroupPool::BUDGET);
        let large = Size::new(4608, 2534);
        assert_eq!(
            GroupPool::budget_for(large),
            GroupPool::SURFACE_TARGETS * cost(large, TargetScale::Full)
        );
        assert!(GroupPool::budget_for(large) > GroupPool::BUDGET);

        let mut pool = GroupPool::scaled(small);
        assert_eq!(pool.budget(), GroupPool::BUDGET);
        pool.resize(large);
        assert_eq!(pool.budget(), GroupPool::budget_for(large));
        let mut fixed = GroupPool::new(small, 1024);
        fixed.resize(large);
        assert_eq!(fixed.budget(), 1024, "a budget that was set stays set");
    }

    #[test]
    fn a_half_resolution_target_costs_a_quarter_of_a_full_one() {
        let region = Size::new(1024, 1024);
        assert_eq!(
            cost(region, TargetScale::Half) * 4,
            cost(region, TargetScale::Full)
        );
    }

    #[test]
    fn a_lease_covers_its_region_and_a_guard_from_an_even_corner() {
        let surface = Size::new(1000, 600);
        let cover = cover_for(rect(101, 51, 40, 30), surface, 0);
        assert_eq!(
            cover.origin,
            Point::new(98, 48),
            "an even corner a guard away"
        );
        assert_eq!(
            cover.far_corner(),
            Point::new(143, 83),
            "a guard past the far edge"
        );
        assert!(cover.contains_rect(rect(101, 51, 40, 30)));
    }

    #[test]
    fn a_lease_at_the_surface_edge_starts_at_the_surface_origin_and_ends_a_guard_past_it() {
        // A read past the near edge of the surface clamps to the first texel.
        let cover = cover_for(rect(0, 0, 40, 30), Size::new(1000, 600), 0);
        assert_eq!(cover.origin, Point::new(0, 0));
        // A read past the far edge reads a guard texel, also on a surface of a whole size class.
        let cover = cover_for(rect(1014, 500, 10, 12), Size::new(1024, 512), 0);
        assert_eq!(cover.far_corner(), Point::new(1026, 514));
        let cover = cover_for(rect(-50, -50, 2000, 2000), Size::new(1024, 512), 0);
        assert_eq!(cover, rect(0, 0, 1026, 514));
    }

    #[test]
    fn the_test_margin_moves_every_lease_further_up_and_left() {
        let plain = cover_for(rect(101, 51, 40, 30), Size::new(1000, 600), 0);
        let moved = cover_for(rect(101, 51, 40, 30), Size::new(1000, 600), 6);
        assert_eq!(moved.origin, plain.origin - Size::new(6, 6));
        assert_eq!(moved.far_corner(), plain.far_corner());
        let edge = cover_for(rect(0, 0, 40, 30), Size::new(1000, 600), 6);
        assert_eq!(edge.origin, Point::new(-6, -6));
    }

    #[test]
    fn a_region_off_the_surface_gets_the_whole_surface() {
        let cover = cover_for(rect(5000, 5000, 10, 10), Size::new(1000, 600), 0);
        assert_eq!(cover, rect(0, 0, 1002, 602));
    }
}
