//! The pool of targets isolated content is composed in.

use zgui_geom::{Device, Size};
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
}

impl GroupSlot {
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
}

/// Targets for isolated content, grown on demand and reused within a frame.
///
/// Every target covers the whole of the composed target's region, at one of two resolutions, so
/// that content drawn into one lands at exactly the device coordinates it would have landed at
/// without the isolation — no second coordinate system, and therefore no chance of a clip
/// evaluated in one and a shape in another.
///
/// **There is no depth limit.** A fixed one is a correctness cliff: CSS nests filters and
/// stacking contexts as deep as an author writes them, and a document that exceeds the limit does
/// not render slightly worse, it renders wrongly. What is limited is memory, and at the limit the
/// pool degrades to half resolution rather than to no isolation — a blurrier frosted panel is a
/// visible cost, a panel composited without its group is a wrong picture.
///
/// Isolated content is held in `Rgba16Float`, for headroom under stacked opacity and repeated
/// blends and for no other reason: every value in it is still premultiplied and gamma-encoded,
/// exactly as in every other target, and the composite reading it is a plain textured quad that
/// performs no colour conversion at all. A target lent to capture what is *beneath* something
/// takes the format of whatever it is capturing from instead, because a copy between two textures
/// requires them to agree — and copying is both cheaper and more faithful than a pass that would
/// convert on the way.
#[derive(Debug)]
pub struct GroupPool {
    /// The device-pixel region every target covers.
    region: Size<i32, Device>,
    /// The targets.
    entries: Vec<Entry>,
    /// How many bytes of targets may be resident.
    budget: u64,
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
    /// Sized so that a 4K window has room for several full-resolution targets — deep nesting on a
    /// large surface is where isolation is both most expensive and most likely — while a document
    /// that isolates dozens of things at once is pushed to half resolution rather than allowed to
    /// take the whole device's memory.
    pub const BUDGET: u64 = 256 * 1024 * 1024;

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
            entries: Vec::new(),
            budget,
            degraded: 0,
            peak: 0,
            leases: 0,
        }
    }

    /// Points the pool at a region of a different size, discarding every target it holds.
    ///
    /// Nothing survives, because a target that no longer covers the region cannot be composited
    /// into the right place. Nothing is lost either: targets hold one frame's isolated content and
    /// a resize redraws the whole frame anyway.
    pub fn resize(&mut self, region: Size<i32, Device>) {
        let region = region.non_negative();
        if self.allocated(TargetScale::Full) != allocated_for(region, TargetScale::Full) {
            self.entries.clear();
        }
        self.region = region;
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
            .or_else(|| self.reuse(TargetScale::Half, format))
            .or_else(|| {
                // The budget refused a full-resolution target, so isolation continues at half of
                // it: the composite magnifies through a filtering sampler, which is the whole
                // reason its bind-group layout carries one.
                let degraded = self.allocate(gpu, TargetScale::Half, format);
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
        self.peak = self.peak.max(lent + 1);
        self.leases += 1;
        counter::bump(Counter::GroupTargets);
        Some(slot)
    }

    /// Returns a target to the pool.
    pub fn release(&mut self, slot: GroupSlot) {
        if let Some(entry) = self.entries.get_mut(slot.index) {
            entry.lent = false;
        }
    }

    /// Returns every target to the pool, which is what the end of a frame does.
    pub fn release_all(&mut self) {
        for entry in &mut self.entries {
            entry.lent = false;
        }
    }

    /// A view of a lent target.
    pub fn view(&self, slot: GroupSlot) -> &wgpu::TextureView {
        &self.entries[slot.index].view
    }

    /// A lent target's texture, for a copy into or out of it.
    pub fn texture(&self, slot: GroupSlot) -> &wgpu::Texture {
        &self.entries[slot.index].texture
    }

    /// The device-pixel region every target covers.
    pub fn region(&self) -> Size<i32, Device> {
        self.region
    }

    /// The texel extent a target at `scale` is actually allocated at.
    ///
    /// Larger than the region it holds, because targets are rounded to a size class. A read of one
    /// has to divide by *this* rather than by the region, or it samples the wrong texels.
    pub fn allocated_extent(&self, scale: TargetScale) -> Size<i32, Device> {
        allocated_for(self.region, scale)
    }

    /// How many bytes of isolated targets may be resident.
    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// How many bytes of device memory the pool holds.
    pub fn bytes(&self) -> u64 {
        self.entries
            .iter()
            .map(|entry| texture_bytes(&entry.texture))
            .sum()
    }

    /// How many targets are lent out right now.
    pub fn lent(&self) -> u32 {
        self.entries.iter().filter(|entry| entry.lent).count() as u32
    }

    /// How many bytes of device memory the targets lent out right now hold.
    pub fn lent_bytes(&self) -> u64 {
        self.entries
            .iter()
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
    /// **All or nothing, and it has to be.** A [`GroupSlot`] is a position in the entry list, so
    /// dropping some entries and keeping others would silently repoint every lease taken after the
    /// hole. A pool with anything lent out therefore frees nothing and says so by returning zero,
    /// which is the state a call from inside a composite would find; the end of a frame returns
    /// every lease, so a caller enforcing a budget between frames frees the whole pool.
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
        let index = self
            .entries
            .iter()
            .position(|entry| !entry.lent && entry.scale == scale && entry.format == format)?;
        self.entries[index].lent = true;
        Some(GroupSlot {
            index,
            scale,
            format,
        })
    }

    /// Allocates a new target at `scale`, unless the budget will not take it.
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
        if self.bytes() + cost > self.budget {
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
        self.entries.push(Entry {
            texture,
            view,
            scale,
            format,
            lent: true,
        });
        Some(GroupSlot {
            index: self.entries.len() - 1,
            scale,
            format,
        })
    }

    /// The texel extent a target at `scale` is allocated at.
    fn allocated(&self, scale: TargetScale) -> Size<i32, Device> {
        allocated_for(self.region, scale)
    }
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
    fn a_half_resolution_target_costs_a_quarter_of_a_full_one() {
        let region = Size::new(1024, 1024);
        assert_eq!(
            cost(region, TargetScale::Half) * 4,
            cost(region, TargetScale::Full)
        );
    }
}
