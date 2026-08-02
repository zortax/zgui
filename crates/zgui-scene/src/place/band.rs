//! The region a moving box is ordered against, instead of the rectangle it starts at.

use rustc_hash::FxHashMap;
use zgui_geom::{Device, DevicePx, Rect};

use crate::spatial::SpatialId;

/// The whole region one coordinate system's movement will visit, in device pixels.
///
/// Conservative on purpose: the endpoints of the movement and everything between them, computed
/// once from the keyframes rather than sampled per frame. A rotation sweeps its corners through an
/// arc, a scale grows about its origin, and both are contained by a rectangle that can be stated
/// before the first frame is drawn.
///
/// What it buys is the one thing a property write cannot buy for itself. Draw order is assigned
/// from rectangles as primitives are pushed, and a matrix written afterwards would move a box
/// underneath or on top of neighbours it was ordered against. Ordering the box against this instead
/// makes that impossible for the whole of the movement — at the cost of a box that overlaps rather
/// more than it needs to, which costs draw calls and never correctness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Travel {
    /// Everything the movement covers.
    region: Rect<DevicePx, Device>,
}

impl Travel {
    /// The region containing every rectangle in `visited`.
    ///
    /// Empty input declares a region that admits nothing, which refuses every write — the honest
    /// answer for a movement nobody could state the extent of.
    ///
    /// ```
    /// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    /// use zgui_scene::Travel;
    ///
    /// let at = |x: f32| -> Rect<DevicePx, Device> {
    ///     Rect::new(Point::new(DevicePx(x), DevicePx(0.0)), Size::new(DevicePx(10.0), DevicePx(10.0)))
    /// };
    /// let travel = Travel::over([at(0.0), at(40.0)]);
    /// assert!(travel.admits(at(20.0)), "half way along is inside the region");
    /// assert!(!travel.admits(at(80.0)), "twice as far as it said it would go is not");
    /// ```
    pub fn over(visited: impl IntoIterator<Item = Rect<DevicePx, Device>>) -> Self {
        let mut region: Option<Rect<DevicePx, Device>> = None;
        for rect in visited {
            region = Some(match region {
                Some(held) => held.union(rect),
                None => rect,
            });
        }
        Self {
            region: region.unwrap_or(Rect::ZERO),
        }
    }

    /// The region itself, which is the rectangle the moving box is ordered against.
    pub fn region(self) -> Rect<DevicePx, Device> {
        self.region
    }

    /// Whether `ink` is inside the declared region.
    ///
    /// An empty rectangle is admitted whatever the region is: a box that covers nothing overlaps
    /// nothing, so no order it holds can be wrong.
    pub fn admits(self, ink: Rect<DevicePx, Device>) -> bool {
        ink.is_empty() || self.region.contains_rect(ink)
    }
}

/// What each moving coordinate system declared.
///
/// A map rather than a column on the node, because it is empty for every coordinate system in a
/// document that is not animating — which is nearly all of them, nearly all of the time — and a
/// field would be a rectangle per coordinate system per frame paid by documents with no animation
/// in them at all.
#[derive(Clone, Debug, Default)]
pub struct Travels {
    /// The region declared for each coordinate system that declared one.
    declared: FxHashMap<SpatialId, Travel>,
}

impl Travels {
    /// Nothing declared.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares `travel` for `node`, replacing whatever it had.
    pub fn declare(&mut self, node: SpatialId, travel: Travel) {
        self.declared.insert(node, travel);
    }

    /// Withdraws `node`'s declaration.
    pub fn withdraw(&mut self, node: SpatialId) {
        self.declared.remove(&node);
    }

    /// What `node` declared, if anything.
    pub fn of(&self, node: SpatialId) -> Option<Travel> {
        self.declared.get(&node).copied()
    }

    /// How many coordinate systems have declared a region.
    pub fn len(&self) -> usize {
        self.declared.len()
    }

    /// Whether nothing has.
    pub fn is_empty(&self) -> bool {
        self.declared.is_empty()
    }
}
