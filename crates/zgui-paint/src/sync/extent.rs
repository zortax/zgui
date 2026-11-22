//! Where the recorded chunks of each coordinate system draw.

use std::collections::BTreeMap;

use compositor_types::ChunkKey;
use rustc_hash::FxHashMap;
use zgui_geom::{Device, Point, Rect, Size};

/// The extent of the recorded chunks drawn in each coordinate system, kept per chunk, so a chunk
/// that moves or is removed changes it.
#[derive(Debug, Default)]
pub(crate) struct Extents {
    /// Each chunk's coordinate system, its extent where it was recorded and its offset since.
    chunks: FxHashMap<ChunkKey, Placed>,
    /// The edges of every chunk of each coordinate system, by slot.
    spaces: FxHashMap<u32, Edges>,
}

/// Where one chunk draws.
#[derive(Clone, Copy, Debug)]
struct Placed {
    slot: u32,
    /// The extent where the chunk was recorded, as `[left, top, right, bottom]`.
    recorded: [i32; 4],
    /// The offset the chunk is drawn at, in whole device pixels.
    offset: [i32; 2],
}

/// How many chunks of one coordinate system have each of their edges at each place.
#[derive(Debug, Default)]
struct Edges {
    edges: [BTreeMap<i32, u32>; 4],
}

impl Placed {
    fn edges(&self) -> [i32; 4] {
        let [left, top, right, bottom] = self.recorded;
        let [x, y] = self.offset;
        [left + x, top + y, right + x, bottom + y]
    }
}

impl Extents {
    /// Notes that the chunk `key` was recorded to draw over `ink`, in the coordinate system in
    /// `slot`.
    pub(crate) fn recorded(&mut self, key: ChunkKey, slot: u32, ink: Rect<i32, Device>) {
        self.removed(key);
        let placed = Placed {
            slot,
            recorded: [
                ink.origin.x,
                ink.origin.y,
                ink.origin.x + ink.size.width,
                ink.origin.y + ink.size.height,
            ],
            offset: [0, 0],
        };
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

    /// Notes that the chunk `key` is no longer recorded.
    pub(crate) fn removed(&mut self, key: ChunkKey) {
        if let Some(placed) = self.chunks.remove(&key) {
            self.take(placed);
        }
    }

    /// Returns the extent the recorded chunks of the coordinate system in `slot` draw over.
    pub(crate) fn of(&self, slot: u32) -> Option<Rect<i32, Device>> {
        let edges = &self.spaces.get(&slot)?.edges;
        let left = *edges[0].keys().next()?;
        let top = *edges[1].keys().next()?;
        let right = *edges[2].keys().next_back()?;
        let bottom = *edges[3].keys().next_back()?;
        Some(Rect::new(
            Point::new(left, top),
            Size::new(right - left, bottom - top),
        ))
    }

    fn add(&mut self, placed: Placed) {
        let edges = self.spaces.entry(placed.slot).or_default();
        for (map, at) in edges.edges.iter_mut().zip(placed.edges()) {
            *map.entry(at).or_insert(0) += 1;
        }
    }

    fn take(&mut self, placed: Placed) {
        let Some(edges) = self.spaces.get_mut(&placed.slot) else {
            return;
        };
        for (map, at) in edges.edges.iter_mut().zip(placed.edges()) {
            if let Some(count) = map.get_mut(&at) {
                *count -= 1;
                if *count == 0 {
                    map.remove(&at);
                }
            }
        }
        if edges.edges[0].is_empty() {
            self.spaces.remove(&placed.slot);
        }
    }
}
