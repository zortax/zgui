//! The pool of targets isolated content is composed in.

use zgui_geom::{Device, Point, Size};
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
}

impl GroupSlot {
    /// The device pixel this target's first texel holds.
    ///
    /// A texel of the target is `(device - origin) * scale`. Every lease starts at the surface
    /// origin, because every target covers the whole region, unless the test mode
    /// [`GroupPool::POISON_ORIGIN`] moves it. The origin is even on both axes, so the texel grid
    /// of a half-resolution target stays anchored to the device origin.
    pub fn origin(self) -> Point<i32, Device> {
        self.origin
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
    /// The resolution it holds.
    scale: TargetScale,
    /// The format it holds.
    format: wgpu::TextureFormat,
    /// Whether it is currently lent out.
    lent: bool,
    /// Whether this frame lent it, which keeps it resident until the frame is recorded: the plan
    /// names its targets by slot, and recording reads them after planning ends.
    touched: bool,
}

/// Targets for isolated content, grown on demand and reused within a frame.
///
/// Every target covers the whole of the composed target's region, at one of two resolutions, so
/// that content drawn into one lands at exactly the device coordinates it would have landed at
/// without the isolation. All targets share device-pixel addressing, so a clip and a shape are
/// always evaluated in the same coordinates. Each lease carries the device pixel its first texel
/// holds ([`GroupSlot::origin`]), and every pass that writes or reads a target maps device pixels
/// to texels through it.
///
/// **There is no depth limit.** A fixed one is a correctness cliff: CSS nests filters and
/// stacking contexts as deep as an author writes them, and a document that exceeds the limit does
/// not render slightly worse, it renders wrongly. What is limited is memory, and at the limit the
/// pool degrades to half resolution rather than to no isolation — a blurrier frosted panel is a
/// visible cost, a panel composited without its group is a wrong picture. Before it degrades, the
/// pool frees the targets no lease of this frame has used.
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
    /// The device-pixel region every target covers.
    region: Size<i32, Device>,
    /// How many device pixels every target starts above and to the left of the surface origin.
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
    /// Set `ZGUI_POISON_TARGET_ORIGIN=<int>` to start every target `P` device pixels above and to
    /// the left of the surface origin, with `P` rounded up to an even number. Each target grows by
    /// `P` on each axis, so it still covers the whole region, and the pixels stay the same. A
    /// reader that uses a device rectangle as a texel rectangle of a pool target is then wrong by
    /// `P` texels, and a pixel test finds it.
    pub const POISON_ORIGIN: &str = "ZGUI_POISON_TARGET_ORIGIN";

    /// The usage an isolated target needs: drawn into, sampled by the composite, copied into by a
    /// backdrop capture, and copied out by a test.
    const USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
        .union(wgpu::TextureUsages::TEXTURE_BINDING)
        .union(wgpu::TextureUsages::COPY_DST)
        .union(wgpu::TextureUsages::COPY_SRC);

    /// An empty pool for targets covering `region`, holding at most `budget` bytes.
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
        }
    }

    /// An empty pool for targets covering `region`, whose budget follows the region.
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

    /// Points the pool at a region of a different size, discarding every target it holds.
    ///
    /// Nothing survives, because a target that no longer covers the region cannot be composited
    /// into the right place. Nothing is lost either: targets hold one frame's isolated content and
    /// a resize redraws the whole frame anyway.
    pub fn resize(&mut self, region: Size<i32, Device>) {
        let region = region.non_negative();
        if self.allocated(TargetScale::Full)
            != allocated_for(grown(region, self.margin), TargetScale::Full)
        {
            self.entries.clear();
        }
        self.region = region;
        if self.scaled {
            self.budget = Self::budget_for(region);
        }
    }

    /// Lends a target at `scale`, reducing the resolution if the budget will not take another.
    ///
    /// Returns `None` only when the budget will not take even a half-resolution target, which
    /// means the budget is smaller than one of them.
    pub fn acquire(
        &mut self,
        gpu: &Gpu,
        scale: TargetScale,
        format: wgpu::TextureFormat,
    ) -> Option<GroupSlot> {
        let lent = self.lent();
        let slot = self
            .reuse(scale, format)
            .or_else(|| self.allocate(gpu, scale, format))
            .or_else(|| {
                // The budget refused a full-resolution target, so isolation continues at half of
                // it: the composite magnifies through a filtering sampler, which is the whole
                // reason its bind-group layout carries one. Counted whether the half-resolution
                // target is lent again or made, because what is counted is the frame drawn at
                // the lower resolution.
                let degraded = self
                    .reuse(TargetScale::Half, format)
                    .or_else(|| self.allocate(gpu, TargetScale::Half, format));
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

    /// Lends a target at exactly `scale`, or nothing when the budget will not take one.
    ///
    /// For a target whose texels have to line up with another one, such as the destination of a
    /// copy: a target at a different resolution holds the copied texels at the wrong place.
    pub fn acquire_exact(
        &mut self,
        gpu: &Gpu,
        scale: TargetScale,
        format: wgpu::TextureFormat,
    ) -> Option<GroupSlot> {
        let lent = self.lent();
        let slot = self
            .reuse(scale, format)
            .or_else(|| self.allocate(gpu, scale, format))?;
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
    pub fn release_all(&mut self) {
        for entry in self.entries.iter_mut().flatten() {
            entry.lent = false;
            entry.touched = false;
        }
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

    /// The device-pixel region every target covers.
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

    /// The device-pixel size every target covers from its origin.
    ///
    /// The region, grown by the margin of the test mode [`GroupPool::POISON_ORIGIN`].
    pub fn covered(&self) -> Size<i32, Device> {
        grown(self.region, self.margin)
    }

    /// The texel extent a target at `scale` is actually allocated at.
    ///
    /// Larger than the region it holds, because targets are rounded to a size class. A read of one
    /// has to divide by *this* rather than by the region, or it samples the wrong texels.
    pub fn allocated_extent(&self, scale: TargetScale) -> Size<i32, Device> {
        self.allocated(scale)
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

    /// Lends a free target already held at `scale`, if there is one.
    fn reuse(&mut self, scale: TargetScale, format: wgpu::TextureFormat) -> Option<GroupSlot> {
        let index = self.entries.iter().position(|entry| {
            entry
                .as_ref()
                .is_some_and(|entry| !entry.lent && entry.scale == scale && entry.format == format)
        })?;
        let origin = self.origin();
        let entry = self.entries[index].as_mut()?;
        entry.lent = true;
        entry.touched = true;
        Some(GroupSlot {
            index,
            scale,
            format,
            origin,
        })
    }

    /// Allocates a new target at `scale`, unless the budget will not take it.
    ///
    /// Frees targets this frame has not lent when that makes room. Those are of another scale or
    /// format, since [`GroupPool::reuse`] takes a matching one first, and without this they stay
    /// resident and keep the pool at its budget for every later frame.
    fn allocate(
        &mut self,
        gpu: &Gpu,
        scale: TargetScale,
        format: wgpu::TextureFormat,
    ) -> Option<GroupSlot> {
        let allocated = self.allocated(scale);
        let cost = u64::from(format.block_copy_size(None).unwrap_or(8))
            * allocated.width.max(1) as u64
            * allocated.height.max(1) as u64;
        if self.bytes() + cost > self.budget && !self.evict(cost) {
            return None;
        }
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("zgui.group"),
            size: wgpu::Extent3d {
                width: allocated.width.max(1) as u32,
                height: allocated.height.max(1) as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: Self::USAGE,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let entry = Entry {
            texture,
            view,
            scale,
            format,
            lent: true,
            touched: true,
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
            origin: self.origin(),
        })
    }

    /// Frees targets this frame has not lent until `cost` more bytes fit in the budget.
    ///
    /// Frees nothing and returns `false` when freeing all of them would still not make room.
    fn evict(&mut self, cost: u64) -> bool {
        let idle = |entry: &Entry| !entry.lent && !entry.touched;
        let freeable: u64 = self
            .entries
            .iter()
            .flatten()
            .filter(|entry| idle(entry))
            .map(|entry| texture_bytes(&entry.texture))
            .sum();
        let mut resident = self.bytes();
        if resident - freeable + cost > self.budget {
            return false;
        }
        for held in &mut self.entries {
            if resident + cost <= self.budget {
                break;
            }
            if let Some(entry) = held.take_if(|entry| idle(entry)) {
                resident -= texture_bytes(&entry.texture);
            }
        }
        while self.entries.last().is_some_and(Option::is_none) {
            self.entries.pop();
        }
        true
    }

    /// The texel extent a target at `scale` is allocated at.
    fn allocated(&self, scale: TargetScale) -> Size<i32, Device> {
        allocated_for(self.covered(), scale)
    }

    /// The device pixel every lease's first texel holds.
    fn origin(&self) -> Point<i32, Device> {
        Point::new(-self.margin, -self.margin)
    }
}

/// `region` grown by `margin` device pixels on each axis.
fn grown(region: Size<i32, Device>, margin: i32) -> Size<i32, Device> {
    Size::new(region.width + margin, region.height + margin)
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
/// Rounded to a size class for the same reason the composed target is: a resize delivers one new
/// extent per frame, and reallocating every target of the pool per event is a multi-megabyte
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
    use super::{GroupPool, allocated_for};
    use crate::target::scale::TargetScale;
    use zgui_geom::{Device, Size};

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
}
