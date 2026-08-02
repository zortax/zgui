//! One pool: the textures of a single kind, and the shelf allocator inside each.

use core::fmt;

use etagere::{AllocId, BucketedAtlasAllocator, size2};
use zgui_geom::{Device, Point, Rect, Size};

use crate::atlas::limits::AtlasLimits;
use crate::error::AtlasError;
use crate::sink::queue::TextureQueue;
use crate::texture::{TextureId, TextureKind};
use crate::tile::TileId;

/// One texture and the allocator that hands out rectangles of it.
struct PoolTexture {
    /// Shelf-and-bucket packing over the texture's extent.
    allocator: BucketedAtlasAllocator,
    /// The extent the texture was created with.
    size: Size<i32, Device>,
    /// How many tiles are currently allocated from it.
    ///
    /// The texture is destroyed when this reaches zero, which is what stops a long session from
    /// holding a texture alive for content that was evicted long ago.
    live: u32,
}

/// Every texture of one [`TextureKind`].
///
/// Slots are reused: destroying a texture leaves a hole that the next new texture fills, so
/// texture indices stay dense and a consumer can key a bind group by index without the index space
/// growing forever.
pub(crate) struct Pool {
    /// Which kind this pool serves.
    kind: TextureKind,
    /// The textures, `None` where a slot is free.
    textures: Vec<Option<PoolTexture>>,
}

impl fmt::Debug for Pool {
    /// Prints the pool's kind and occupancy; the shelf allocator's own state is not printable and
    /// is summarised by its allocated space instead.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Pool")
            .field("kind", &self.kind)
            .field("textures", &self.live_textures())
            .field("fully_reclaimed", &self.is_fully_reclaimed())
            .finish()
    }
}

impl Pool {
    /// An empty pool for `kind`.
    pub(crate) fn new(kind: TextureKind) -> Self {
        Self {
            kind,
            textures: Vec::new(),
        }
    }

    /// How many textures currently exist.
    pub(crate) fn live_textures(&self) -> usize {
        self.textures.iter().flatten().count()
    }

    /// How many texels every live texture of this pool holds between them.
    pub(crate) fn texels(&self) -> u64 {
        self.textures
            .iter()
            .flatten()
            .map(|texture| texture.size.width as u64 * texture.size.height as u64)
            .sum()
    }

    /// Whether every texture of this pool has had all its space returned.
    ///
    /// A pool that has served and released a million tiles reports `true` here, and a pool that
    /// leaks even one does not. That is the whole content of the leak property test.
    pub(crate) fn is_fully_reclaimed(&self) -> bool {
        self.textures
            .iter()
            .flatten()
            .all(|texture| texture.allocator.allocated_space() == 0)
    }

    /// Allocates room for `size`, recording a texture creation if none has space.
    ///
    /// Existing textures are tried newest first, because the newest is the one most likely to have
    /// contiguous room left.
    pub(crate) fn allocate(
        &mut self,
        size: Size<i32, Device>,
        limits: AtlasLimits,
        device: &mut TextureQueue,
    ) -> Result<(TextureId, TileId, Rect<i32, Device>), AtlasError> {
        if size.width <= 0 || size.height <= 0 {
            return Err(AtlasError::OutOfSpace { requested: size });
        }
        if size.width > limits.max_texture_size || size.height > limits.max_texture_size {
            return Err(AtlasError::TooLarge {
                requested: size,
                limit: limits.largest_tile(),
            });
        }

        for index in (0..self.textures.len()).rev() {
            let Some(texture) = self.textures[index].as_mut() else {
                continue;
            };
            if let Some(allocation) = texture.allocator.allocate(size2(size.width, size.height)) {
                texture.live += 1;
                return Ok((
                    TextureId::new(self.kind, index as u32),
                    TileId(allocation.id.serialize()),
                    rect_of(allocation.rectangle, size),
                ));
            }
        }

        let index = self.push_texture(size, limits, device)?;
        let texture = self.textures[index]
            .as_mut()
            .expect("the slot was just filled");
        let allocation = texture
            .allocator
            .allocate(size2(size.width, size.height))
            .ok_or(AtlasError::OutOfSpace { requested: size })?;
        texture.live += 1;
        Ok((
            TextureId::new(self.kind, index as u32),
            TileId(allocation.id.serialize()),
            rect_of(allocation.rectangle, size),
        ))
    }

    /// Returns a tile's space, recording the texture's destruction when it holds nothing else.
    pub(crate) fn deallocate(
        &mut self,
        texture: TextureId,
        tile: TileId,
        device: &mut TextureQueue,
    ) {
        let Some(slot) = self.textures.get_mut(texture.index as usize) else {
            return;
        };
        let Some(held) = slot.as_mut() else { return };
        held.allocator.deallocate(AllocId::deserialize(tile.0));
        held.live = held.live.saturating_sub(1);
        if held.live == 0 {
            *slot = None;
            device.destroy(texture);
        }
    }

    /// Destroys every texture of the pool.
    pub(crate) fn clear(&mut self, device: &mut TextureQueue) {
        for (index, slot) in self.textures.iter_mut().enumerate() {
            if slot.take().is_some() {
                device.destroy(TextureId::new(self.kind, index as u32));
            }
        }
        self.textures.clear();
    }

    /// Reserves a texture large enough for `size` and returns the slot it went into.
    fn push_texture(
        &mut self,
        size: Size<i32, Device>,
        limits: AtlasLimits,
        device: &mut TextureQueue,
    ) -> Result<usize, AtlasError> {
        if self.live_textures() as u32 >= limits.max_textures_per_pool {
            return Err(AtlasError::OutOfSpace { requested: size });
        }
        let extent = limits.texture_extent_for(size);
        let index = self
            .textures
            .iter()
            .position(Option::is_none)
            .unwrap_or(self.textures.len());
        if index == self.textures.len() {
            self.textures.push(None);
        }
        let id = TextureId::new(self.kind, index as u32);
        device.create(id, extent, self.kind.format());
        self.textures[index] = Some(PoolTexture {
            allocator: BucketedAtlasAllocator::new(size2(extent.width, extent.height)),
            size: extent,
            live: 0,
        });
        Ok(index)
    }
}

/// The allocator's rectangle, trimmed to the size that was asked for.
///
/// Shelf packing rounds an allocation up, so the rectangle handed back can be larger than the
/// request; the tile is the requested extent at the allocation's origin, and the slack stays
/// unused rather than being handed out as if it held content.
fn rect_of(allocation: etagere::Rectangle, size: Size<i32, Device>) -> Rect<i32, Device> {
    Rect::new(Point::new(allocation.min.x, allocation.min.y), size)
}
