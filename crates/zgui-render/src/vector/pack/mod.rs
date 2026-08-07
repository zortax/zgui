//! Sharing scratch layers between the passes of one frame.
//!
//! A rasteriser cannot give every pass a layer of its own. Every pass of a frame is rasterised
//! before any of them is composited, so a layer holds its pass's coverage until the frame's own
//! draws have read it, and one layer per pass makes the scratch a function of the worst frame of
//! the session rather than of the surface.
//!
//! Two passes can share a layer when their regions do not meet. A layer is in device coordinates —
//! one texel per device pixel of the surface, at the surface's own coordinates — so a pass writes
//! where it belongs and two passes that do not overlap on the screen do not overlap in the layer
//! either. Nothing about the order changes: each pass still has its own composite, at its own place
//! in the draw order, reading its own region.
//!
//! # What sharing rests on
//!
//! **A pass paints inside its region and nowhere else.** That is already true twice over — a
//! region is the union of its items' inks, each of which is a control-box measurement widened by
//! the whole reach of the stroke, and the union is then rounded outwards onto a sixteen-pixel grid.
//! It is also already load-bearing: a pass composited one item at a time reads each item's ink and
//! nothing outside it. What sharing changes is the cost of breaking it. A drawing that reached
//! outside its region used to lose the part that did; on a shared layer it would land in a
//! neighbour's texels and be composited as the neighbour.
//!
//! Neither implementation is obliged to use this, which is why it is a helper and not part of the
//! [`VectorRaster`](crate::VectorRaster) contract.

pub mod grid;

use zgui_geom::{Device, Point, Rect, Size};

use crate::vector::pack::grid::Grid;
use crate::vector::target::VectorTarget;

/// A frame's passes with a scratch layer assigned to each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layering {
    /// One target per pass, in the plan's order.
    targets: Vec<VectorTarget>,
    /// How many layers the assignment uses.
    layers: u32,
    /// How many of the passes, counting from the first, were given a layer.
    placed: usize,
}

impl Layering {
    /// Assigns each of `regions` a layer of a scratch no deeper than `ceiling`.
    ///
    /// First fit, in the plan's order: a pass takes the lowest-numbered layer whose claimed cells it
    /// does not meet, and opens a new one when every open layer is in its way. Order matters only in
    /// that a pass is never placed on top of one it overlaps; which layer either of them ends up in
    /// is not observable, because a composite names its pass and reads that pass's region.
    ///
    /// **The passes that fit are a prefix.** A frame that reaches the ceiling stops there rather
    /// than skipping the pass that did not fit and placing a later one, because the passes that were
    /// rasterised are then exactly the ones a shortened plan composites, and a composite is named by
    /// its index.
    pub fn of(regions: &[Rect<i32, Device>], ceiling: u32) -> Self {
        let width = regions
            .iter()
            .map(|region| region.right())
            .max()
            .unwrap_or(0);
        let height = regions
            .iter()
            .map(|region| region.bottom())
            .max()
            .unwrap_or(0);
        let mut grid = Grid::new(width, height);
        let mut targets = Vec::with_capacity(regions.len());
        for region in regions {
            let span = grid.span(
                region.origin.x,
                region.origin.y,
                region.right(),
                region.bottom(),
            );
            // A pass covering nothing rasterises nothing, so it claims nothing and cannot be the
            // reason a frame runs out of layers.
            if span.is_empty() {
                targets.push(VectorTarget(0));
                continue;
            }
            let Some(layer) = place(&mut grid, span, ceiling) else {
                break;
            };
            targets.push(VectorTarget(layer as u64));
        }
        let placed = targets.len();
        targets.resize(regions.len(), VectorTarget::NONE);
        Self {
            targets,
            layers: grid.layers() as u32,
            placed,
        }
    }

    /// Where pass `index` goes, or [`VectorTarget::NONE`] for one that had nowhere to go.
    pub fn target(&self, index: usize) -> VectorTarget {
        self.targets
            .get(index)
            .copied()
            .unwrap_or(VectorTarget::NONE)
    }

    /// How many layers the assignment uses.
    pub fn layers(&self) -> u32 {
        self.layers
    }

    /// How many passes, counting from the first, were given a layer.
    pub fn placed(&self) -> usize {
        self.placed
    }

    /// Packs each assigned layer's disjoint regions into compact shelf rows.
    pub fn compact(&self, regions: &[Rect<i32, Device>]) -> (Vec<Rect<i32, Device>>, u32, u32) {
        let mut packed = vec![Rect::new(Point::new(0, 0), Size::new(0, 0)); regions.len()];
        let mut width = 1_i32;
        let mut height = 1_i32;
        for layer in 0..self.layers {
            let indices: Vec<_> = (0..self.placed)
                .filter(|&index| self.target(index).0 == u64::from(layer))
                .collect();
            let area: i64 = indices
                .iter()
                .map(|&index| {
                    let size = regions[index].size;
                    i64::from(size.width.max(0)) * i64::from(size.height.max(0))
                })
                .sum();
            let widest = indices
                .iter()
                .map(|&index| regions[index].size.width.max(0))
                .max()
                .unwrap_or(1);
            let shelf = ((area as f64).sqrt().ceil() as i32).max(widest).max(1);
            let (mut x, mut y, mut row): (i32, i32, i32) = (0, 0, 0);
            for index in indices {
                let size = regions[index].size.non_negative();
                if x > 0 && x.saturating_add(size.width) > shelf {
                    y += row;
                    x = 0;
                    row = 0;
                }
                packed[index] = Rect::new(Point::new(x, y), size);
                x += size.width;
                row = row.max(size.height);
                width = width.max(x);
                height = height.max(y + row);
            }
        }
        (packed, width as u32, height as u32)
    }
}

/// The lowest open layer `span` fits in, opening one more when it fits in none and there is room.
fn place(grid: &mut Grid, span: grid::Span, ceiling: u32) -> Option<usize> {
    for layer in 0..grid.layers() {
        if grid.claim(layer, span) {
            return Some(layer);
        }
    }
    if grid.layers() as u32 >= ceiling {
        return None;
    }
    let layer = grid.open();
    grid.claim(layer, span).then_some(layer)
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, Point, Rect, Size};

    use super::Layering;
    use crate::vector::target::VectorTarget;

    /// A tile-aligned region.
    fn region(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    #[test]
    fn two_disjoint_passes_share_a_layer() {
        let layering = Layering::of(&[region(0, 0, 32, 32), region(64, 0, 32, 32)], 64);
        assert_eq!(layering.layers(), 1);
        assert_eq!(layering.target(0), layering.target(1));
        assert_eq!(layering.placed(), 2);
    }

    #[test]
    fn two_overlapping_passes_do_not_share_a_layer() {
        let layering = Layering::of(&[region(0, 0, 64, 64), region(32, 32, 64, 64)], 64);
        assert_eq!(layering.layers(), 2);
        assert_ne!(layering.target(0), layering.target(1));
    }

    #[test]
    fn passes_that_only_touch_at_an_edge_still_share_a_layer() {
        // Half-open rectangles: one ends where the next begins, so they share no pixel and no cell.
        let layering = Layering::of(&[region(0, 0, 32, 32), region(32, 0, 32, 32)], 64);
        assert_eq!(layering.layers(), 1);
    }

    #[test]
    fn a_third_pass_reuses_the_first_layer_the_second_could_not() {
        let layering = Layering::of(
            &[
                region(0, 0, 64, 64),
                region(32, 32, 64, 64),
                region(128, 0, 32, 32),
            ],
            64,
        );
        assert_eq!(layering.layers(), 2);
        assert_eq!(layering.target(2), layering.target(0));
    }

    #[test]
    fn five_hundred_disjoint_passes_fit_in_one_layer() {
        let regions: Vec<_> = (0..500)
            .map(|index| region((index % 50) * 32, (index / 50) * 32, 16, 16))
            .collect();
        let layering = Layering::of(&regions, 64);
        assert_eq!(layering.layers(), 1);
        assert_eq!(layering.placed(), 500);
        assert!(
            layering
                .targets
                .iter()
                .all(|target| *target == VectorTarget(0))
        );
    }

    #[test]
    fn far_away_passes_are_packed_without_the_empty_space_between_them() {
        let regions = [
            region(10_000, 20_000, 16, 16),
            region(30_000, 40_000, 16, 16),
        ];
        let layering = Layering::of(&regions, 4);
        let (packed, width, height) = layering.compact(&regions);
        assert_eq!(packed[0].size, regions[0].size);
        assert_eq!(packed[1].size, regions[1].size);
        assert!(
            width <= 32 && height <= 32,
            "compact scratch was {width} by {height}"
        );
    }

    #[test]
    fn a_frame_past_the_ceiling_places_a_prefix_and_says_where_it_stopped() {
        // Every region covers the same cell, so each needs a layer of its own.
        let regions: Vec<_> = (0..6).map(|_| region(0, 0, 16, 16)).collect();
        let layering = Layering::of(&regions, 4);
        assert_eq!(layering.layers(), 4);
        assert_eq!(layering.placed(), 4);
        assert_eq!(layering.target(3), VectorTarget(3));
        assert_eq!(layering.target(4), VectorTarget::NONE);
        assert_eq!(layering.target(5), VectorTarget::NONE);
    }

    #[test]
    fn an_empty_region_costs_no_layer() {
        let layering = Layering::of(&[region(0, 0, 0, 0), region(0, 0, 16, 16)], 64);
        assert_eq!(layering.layers(), 1);
        assert_eq!(layering.placed(), 2);
    }
}
