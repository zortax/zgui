//! Where the coverage of overlapping marks is summed before it is painted.
//!
//! A mark whose prims overlap paints the union of their coverage once: each prim adds its
//! coverage into a single-channel scratch, and one composite paints the item through the sum. The
//! scratch is cut into bins, one per item, packed into pages the size of the surface. Which items
//! get a bin and where it lies is decided here, from the display list and the damage set, so the
//! renderer executes the plan and decides nothing.

use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_profile::{Counter, counter};

use crate::ops::PaintOp;
use crate::pass::region::covering;
use crate::prim::{MarkItem, PrimitiveKind};
use crate::scene::Primitives;
use crate::spatial::{Placements, SpatialTree};

/// The most scratch pages one frame may use.
pub const MAX_PAGES: u32 = 8;

/// The scratch region one union item adds its coverage into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkBin {
    /// The item, by its index in [`Primitives::marks`](crate::scene::Primitives::marks).
    pub item: u32,
    /// The device pixels the bin holds.
    pub region: Rect<i32, Device>,
    /// Where the region's origin lies in its page, in texels.
    pub at: Point<i32, Device>,
    /// The page the bin is on.
    pub page: u32,
}

/// The bins of one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkPlan {
    /// One bin per union item that reaches the damage, in emission order.
    pub bins: Vec<MarkBin>,
    /// How many pages the bins use.
    pub pages: u32,
    /// The size of one page, in texels: the surface's extent.
    pub extent: Size<i32, Device>,
    /// How many union items found no page, and draw each prim on its own instead.
    pub overflow: usize,
    /// Whether the plan was made for a frame that redraws every pixel.
    ///
    /// A bin outside every group holds only the damaged part of its item. A renderer that redraws
    /// more than the damage this plan was made for draws a union item without its bin.
    pub full_damage: bool,
}

impl MarkPlan {
    /// The bin of the item at `item` in the marks array, if it has one.
    pub fn bin(&self, item: u32) -> Option<&MarkBin> {
        self.bins
            .binary_search_by_key(&item, |bin| bin.item)
            .ok()
            .map(|at| &self.bins[at])
    }

    /// Whether no item has a bin.
    pub fn is_empty(&self) -> bool {
        self.bins.is_empty()
    }

    /// Empties the plan, keeping its allocation.
    pub fn clear(&mut self) {
        self.bins.clear();
        self.pages = 0;
        self.extent = Size::new(0, 0);
        self.overflow = 0;
        self.full_damage = false;
    }
}

/// Places bins in rows across pages of one extent.
struct Shelves {
    /// The page extent.
    extent: Size<i32, Device>,
    /// The page being filled.
    page: u32,
    /// The next free texel of the current row.
    cursor: Point<i32, Device>,
    /// The height of the current row.
    row: i32,
}

impl Shelves {
    /// Where a region of `size` goes, or `None` when every page is full.
    fn place(&mut self, size: Size<i32, Device>) -> Option<(u32, Point<i32, Device>)> {
        if self.cursor.x + size.width > self.extent.width {
            self.cursor = Point::new(0, self.cursor.y + self.row);
            self.row = 0;
        }
        if self.cursor.y + size.height > self.extent.height {
            self.page += 1;
            self.cursor = Point::new(0, 0);
            self.row = 0;
        }
        if self.page >= MAX_PAGES {
            return None;
        }
        let at = self.cursor;
        self.cursor.x += size.width;
        self.row = self.row.max(size.height);
        Some((self.page, at))
    }
}

/// What the bin plan reads.
pub(crate) struct Input<'a> {
    /// The frame's log, in emission order.
    pub(crate) ops: &'a [PaintOp],
    /// The frame's primitives.
    pub(crate) primitives: &'a Primitives,
    /// The coordinate systems the marks are drawn under.
    pub(crate) spatial: &'a SpatialTree,
    /// The surface's extent.
    pub(crate) viewport: Size<i32, Device>,
    /// What the frame redraws.
    pub(crate) damage: &'a DamageSet,
}

/// Plans one bin per union mark that reaches the damage, into `plan`.
pub(crate) fn plan(input: Input<'_>, plan: &mut MarkPlan) {
    plan.clear();
    let Input {
        ops,
        primitives,
        spatial,
        viewport: extent,
        damage,
    } = input;
    if !primitives.marks.iter().any(MarkItem::is_union) {
        return;
    }
    let viewport = Rect::new(Point::new(0, 0), extent);
    plan.extent = extent;
    plan.full_damage = damage.is_full();
    // A backdrop that reads past what it writes makes the renderer redraw more than the damage, so
    // the bins keep their whole ink there.
    let cut = !damage.is_full()
        && primitives
            .backdrops
            .iter()
            .all(|backdrop| backdrop.reads_only_what_it_writes());
    let placements = Placements::of(spatial);
    let mut shelves = Shelves {
        extent,
        page: 0,
        cursor: Point::new(0, 0),
        row: 0,
    };
    let mut depth = 0usize;
    for op in ops {
        let index = op.index as usize;
        match op.kind {
            PrimitiveKind::GroupStart => depth += 1,
            PrimitiveKind::GroupEnd => depth = depth.saturating_sub(1),
            PrimitiveKind::Marks if primitives.marks[index].is_union() => {
                let mark = &primitives.marks[index];
                let local = mark.ink();
                let ink = spatial
                    .at(mark.transform)
                    .and_then(|space| placements.get(space))
                    .and_then(|matrix| matrix.to_affine2())
                    .map_or(local, |affine| affine.transform_rect(local));
                let covered = covering(ink);
                let mut region = match covered.intersection(viewport) {
                    Some(region) if !region.is_empty() => region,
                    _ => continue,
                };
                // A group redraws its whole region whatever the damage, so an item inside one keeps
                // its whole ink.
                if depth == 0 {
                    if !damage.intersects(covered) {
                        continue;
                    }
                    if cut {
                        let touched = damage
                            .rects()
                            .iter()
                            .filter(|rect| rect.intersects(region))
                            .fold(None, |held: Option<Rect<i32, Device>>, rect| {
                                Some(held.map_or(*rect, |held| held.union(*rect)))
                            });
                        match touched.and_then(|touched| touched.intersection(region)) {
                            Some(within) if !within.is_empty() => region = within,
                            _ => continue,
                        }
                    }
                }
                match shelves.place(region.size) {
                    Some((page, at)) => {
                        plan.pages = plan.pages.max(page + 1);
                        plan.bins.push(MarkBin {
                            item: op.index,
                            region,
                            at,
                            page,
                        });
                    }
                    None => plan.overflow += 1,
                }
            }
            _ => {}
        }
    }
    counter::add(Counter::MarksUnionBins, plan.bins.len() as u64);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use smallvec::smallvec;
    use zgui_bits::DamageSet;
    use zgui_color::Color;
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use super::MAX_PAGES;
    use crate::group::GroupBoundary;
    use crate::paint::PaintRef;
    use crate::prim::{MarkFlags, MarkItem, MarkPayload};
    use crate::scene::Scene;

    /// A device rectangle.
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    /// A scene over a surface of `width` by `height`, with one paint interned.
    fn scene(width: i32, height: i32) -> (Scene, PaintRef) {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(width, height));
        let fill = PaintRef::solid(scene.paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0)));
        (scene, fill)
    }

    /// Pushes one mark over `ink`, with or without the union flag.
    fn push(scene: &mut Scene, fill: PaintRef, ink: Rect<DevicePx, Device>, union: bool) {
        let payload = MarkPayload {
            discs: vec![[ink.origin.x.0 + 1.0, ink.origin.y.0 + 1.0, 1.0, 0.0]],
            ..MarkPayload::default()
        };
        let mut item = MarkItem::new(ink, fill, payload.counts());
        if union {
            item.flags |= MarkFlags::UNION;
        }
        scene.push_marks(item, Arc::new(payload));
    }

    #[test]
    fn direct_items_take_no_bin() {
        let (mut scene, fill) = scene(200, 200);
        push(&mut scene, fill, rect(10.0, 10.0, 40.0, 40.0), false);
        scene.finish(&DamageSet::full());
        assert!(scene.mark_plan().is_empty());
        assert_eq!(scene.mark_plan().pages, 0);
    }

    #[test]
    fn union_bins_are_culled_by_damage() {
        let (mut scene, fill) = scene(200, 200);
        push(&mut scene, fill, rect(10.0, 10.0, 40.0, 40.0), true);
        push(&mut scene, fill, rect(120.0, 120.0, 40.0, 40.0), true);
        let mut damage = DamageSet::new();
        damage.absorb(Rect::new(Point::new(0, 0), Size::new(30, 100)));
        scene.finish(&damage);
        let plan = scene.mark_plan();
        let [bin] = plan.bins.as_slice() else {
            panic!("{:?}", plan.bins);
        };
        assert_eq!(bin.item, 0);
        assert_eq!(
            bin.region,
            Rect::new(Point::new(10, 10), Size::new(20, 40)),
            "the bin holds only the damaged part of the ink"
        );
        assert!(plan.bin(1).is_none());
        assert!(!plan.full_damage);
    }

    #[test]
    fn bins_pack_into_shelves_and_overflow_to_a_new_page() {
        let (mut scene, fill) = scene(100, 100);
        // Two 60 x 60 bins cannot share a row, and two rows of 60 do not fit one page.
        for at in 0..3 {
            push(
                &mut scene,
                fill,
                rect(at as f32 * 10.0, 0.0, 60.0, 60.0),
                true,
            );
        }
        push(&mut scene, fill, rect(0.0, 70.0, 30.0, 30.0), true);
        scene.finish(&DamageSet::full());
        let plan = scene.mark_plan();
        let placed: Vec<(u32, Point<i32, Device>)> =
            plan.bins.iter().map(|bin| (bin.page, bin.at)).collect();
        assert_eq!(
            placed,
            vec![
                (0, Point::new(0, 0)),
                (1, Point::new(0, 0)),
                (2, Point::new(0, 0)),
                (2, Point::new(60, 0)),
            ]
        );
        assert_eq!(plan.pages, 3);
        assert_eq!(plan.extent, Size::new(100, 100));
    }

    #[test]
    fn items_past_the_last_page_overflow() {
        let (mut scene, fill) = scene(100, 100);
        for at in 0..MAX_PAGES + 2 {
            push(&mut scene, fill, rect(at as f32, 0.0, 90.0, 90.0), true);
        }
        scene.finish(&DamageSet::full());
        assert_eq!(scene.mark_plan().bins.len(), MAX_PAGES as usize);
        assert_eq!(scene.mark_plan().overflow, 2);
    }

    #[test]
    fn a_bin_never_exceeds_the_viewport() {
        let (mut scene, fill) = scene(100, 80);
        push(&mut scene, fill, rect(-50.0, -50.0, 400.0, 400.0), true);
        scene.finish(&DamageSet::full());
        let [bin] = scene.mark_plan().bins.as_slice() else {
            panic!("{:?}", scene.mark_plan().bins);
        };
        assert_eq!(bin.region, Rect::new(Point::new(0, 0), Size::new(100, 80)));
        assert_eq!(bin.at, Point::new(0, 0));
    }

    #[test]
    fn a_bin_inside_a_group_keeps_its_whole_ink() {
        let (mut scene, fill) = scene(200, 200);
        let group = GroupBoundary::start(
            rect(0.0, 0.0, 200.0, 200.0),
            0.5,
            crate::peniko::BlendMode::default(),
            smallvec![],
        );
        scene.push_group(group.clone());
        push(&mut scene, fill, rect(10.0, 10.0, 40.0, 40.0), true);
        scene.push_group(group.end());
        let mut damage = DamageSet::new();
        damage.absorb(Rect::new(Point::new(0, 0), Size::new(20, 20)));
        scene.finish(&damage);
        let [bin] = scene.mark_plan().bins.as_slice() else {
            panic!("{:?}", scene.mark_plan().bins);
        };
        assert_eq!(bin.region, Rect::new(Point::new(10, 10), Size::new(40, 40)));
    }
}
