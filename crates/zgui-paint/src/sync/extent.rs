//! Where the recorded chunks of each coordinate system draw.

use std::cell::Cell;

use iceblink::ChunkKey;
use rustc_hash::FxHashMap;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_space::SpatialId;

/// The extent of the recorded chunks drawn in each coordinate system, kept per chunk, so a chunk
/// that moves or is removed changes it.
#[derive(Debug, Default)]
pub(crate) struct Extents {
    /// Each chunk's coordinate system, its extent where it was recorded and its offset since.
    chunks: FxHashMap<ChunkKey, Placed>,
    /// The extent of the chunks of each coordinate system, by slot.
    spaces: FxHashMap<u32, Space>,
}

/// Where one chunk draws.
#[derive(Clone, Copy, Debug)]
struct Placed {
    /// The coordinate system the chunk draws in.
    anchor: SpatialId,
    slot: u32,
    /// The extent where the chunk was recorded, as `[left, top, right, bottom]`.
    recorded: [i32; 4],
    /// The offset the chunk is drawn at, in whole device pixels.
    offset: [i32; 2],
    /// The largest emboldening of the chunk's glyph runs in device pixels, or `None` when it draws
    /// no glyphs.
    glyphs: Option<f32>,
}

/// The extent of the chunks of one coordinate system.
#[derive(Debug)]
struct Space {
    /// How many chunks draw in it.
    count: u32,
    /// The union of their extents as `[left, top, right, bottom]`, unless it is stale.
    union: Cell<[i32; 4]>,
    /// Whether a chunk on the union's edge moved or went since the union was taken.
    stale: Cell<bool>,
}

impl Placed {
    fn edges(&self) -> [i32; 4] {
        let [left, top, right, bottom] = self.recorded;
        let [x, y] = self.offset;
        [left + x, top + y, right + x, bottom + y]
    }
}

impl Extents {
    /// Notes that the chunk `key` was recorded to draw over `ink`, in the coordinate system
    /// `anchor`, with glyph runs emboldened by at most `glyphs` device pixels among what it draws,
    /// or none for `None`.
    pub(crate) fn recorded(
        &mut self,
        key: ChunkKey,
        anchor: SpatialId,
        ink: Rect<i32, Device>,
        glyphs: Option<f32>,
    ) {
        let placed = Placed {
            anchor,
            slot: anchor.index(),
            recorded: [
                ink.origin.x,
                ink.origin.y,
                ink.origin.x + ink.size.width,
                ink.origin.y + ink.size.height,
            ],
            offset: [0, 0],
            glyphs,
        };
        if let Some(held) = self.chunks.get_mut(&key)
            && held.anchor == anchor
            && held.edges() == placed.edges()
        {
            // The chunk draws where it drew, so the extents stay as they are.
            *held = placed;
            return;
        }
        self.removed(key);
        self.add(placed);
        self.chunks.insert(key, placed);
    }

    /// Notes that the chunk `key` is drawn at `offset` from where it was recorded.
    pub(crate) fn moved(&mut self, key: ChunkKey, offset: [f32; 2]) {
        let Some(mut placed) = self.chunks.get(&key).copied() else {
            return;
        };
        self.take(placed);
        placed.offset = offset.map(|axis| axis.round() as i32);
        self.add(placed);
        self.chunks.insert(key, placed);
    }

    /// Returns where the chunk `key` draws in its coordinate system, if it is recorded.
    pub(crate) fn ink(&self, key: ChunkKey) -> Option<Rect<i32, Device>> {
        self.placed_ink(key).map(|(_, ink)| ink)
    }

    /// Returns the coordinate system of the chunk `key` and where it draws in it, if it is
    /// recorded.
    pub(crate) fn placed_ink(&self, key: ChunkKey) -> Option<(SpatialId, Rect<i32, Device>)> {
        let placed = self.chunks.get(&key)?;
        let [left, top, right, bottom] = placed.edges();
        Some((
            placed.anchor,
            Rect::new(Point::new(left, top), Size::new(right - left, bottom - top)),
        ))
    }

    /// Returns the largest emboldening of the glyph runs of the chunk `key`, or `None` when it is
    /// not recorded or draws no glyphs.
    pub(crate) fn glyphs(&self, key: ChunkKey) -> Option<f32> {
        self.chunks.get(&key).and_then(|placed| placed.glyphs)
    }

    /// Notes that the chunk `key` is no longer recorded.
    pub(crate) fn removed(&mut self, key: ChunkKey) {
        if let Some(placed) = self.chunks.remove(&key) {
            self.take(placed);
        }
    }

    /// Returns the extent the recorded chunks of the coordinate system in `slot` draw over.
    pub(crate) fn of(&self, slot: u32) -> Option<Rect<i32, Device>> {
        let space = self.spaces.get(&slot)?;
        if space.stale.get() {
            let union = self
                .chunks
                .values()
                .filter(|placed| placed.slot == slot)
                .fold(EMPTY, |union, placed| widened(union, placed.edges()));
            space.union.set(union);
            space.stale.set(false);
        }
        let [left, top, right, bottom] = space.union.get();
        Some(Rect::new(
            Point::new(left, top),
            Size::new(right - left, bottom - top),
        ))
    }

    fn add(&mut self, placed: Placed) {
        let space = self.spaces.entry(placed.slot).or_insert_with(|| Space {
            count: 0,
            union: Cell::new(EMPTY),
            stale: Cell::new(false),
        });
        space.count += 1;
        if !space.stale.get() {
            space.union.set(widened(space.union.get(), placed.edges()));
        }
    }

    fn take(&mut self, placed: Placed) {
        let Some(space) = self.spaces.get_mut(&placed.slot) else {
            return;
        };
        space.count -= 1;
        if space.count == 0 {
            self.spaces.remove(&placed.slot);
            return;
        }
        // A chunk inside the union leaves it as it is.
        let union = space.union.get();
        if placed
            .edges()
            .iter()
            .zip(union)
            .any(|(edge, bound)| *edge == bound)
        {
            space.stale.set(true);
        }
    }
}

/// The union of no extent, which widens to the first extent added.
const EMPTY: [i32; 4] = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];

/// Returns the union of the extents `union` and `edges`, both as `[left, top, right, bottom]`.
fn widened(union: [i32; 4], edges: [i32; 4]) -> [i32; 4] {
    [
        union[0].min(edges[0]),
        union[1].min(edges[1]),
        union[2].max(edges[2]),
        union[3].max(edges[3]),
    ]
}

#[cfg(test)]
mod tests {
    use iceblink::ChunkKey;
    use zgui_geom::{Device, Point, Rect, Size};
    use zgui_space::SpatialId;

    use super::Extents;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    #[test]
    fn the_extent_follows_the_chunks_on_its_edge() {
        let space = SpatialId::VIEWPORT;
        let (a, b) = (ChunkKey::new(1, 0), ChunkKey::new(2, 0));
        let mut extents = Extents::default();
        extents.recorded(a, space, rect(0, 0, 10, 10), None);
        extents.recorded(b, space, rect(20, 0, 10, 40), None);
        assert_eq!(extents.of(space.index()), Some(rect(0, 0, 30, 40)));

        extents.moved(a, [5.0, 0.0]);
        assert_eq!(extents.of(space.index()), Some(rect(5, 0, 25, 40)));
        extents.removed(b);
        assert_eq!(extents.of(space.index()), Some(rect(5, 0, 10, 10)));
        extents.removed(a);
        assert_eq!(extents.of(space.index()), None);
    }
}
