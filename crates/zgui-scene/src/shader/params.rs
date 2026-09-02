//! One effect's parameters: a fixed-width block, interned so that equal parameters are one draw.

use crate::content::{Content, ContentHash};
use crate::table::{Table, TableId};

/// How many bytes of parameters an effect may declare.
///
/// Four vectors of four floats, which is the width the shading prelude declares. The limit buys a
/// fixed uniform stride and costs an effect nothing it would otherwise have: anything larger than
/// this is a texture rather than a parameter.
pub const MAX_PARAMS_BYTES: usize = 64;

/// A parameter block, resolved through a [`ShaderParamsTable`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShaderParamsSlot(pub u32);

impl ShaderParamsSlot {
    /// The handle's numeric value, for indexing and for transcripts.
    pub const fn index(self) -> u32 {
        self.0
    }
}

impl TableId for ShaderParamsSlot {
    fn from_index(index: u32) -> Self {
        Self(index)
    }

    fn index(self) -> u32 {
        self.0
    }
}

/// What one effect is drawn with: the framework's half, then the application's own bytes.
///
/// The framework's half is here rather than on the instance because it changes with the pointer
/// and not with the rectangle, and because putting it beside the application's bytes means one
/// uniform block and one dynamic offset instead of two of each.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShaderParams {
    /// The pointer, in the element's own device pixels.
    pub pointer: [f32; 2],
    /// One while the pointer is over the element, zero while it is elsewhere.
    pub hovered: f32,
    /// The application's own bytes, zero-padded to the declared width.
    pub user: [u8; MAX_PARAMS_BYTES],
}

impl Default for ShaderParams {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl ShaderParams {
    /// A block an effect that reads neither the pointer nor any parameter draws with.
    pub const EMPTY: Self = Self {
        pointer: [0.0, 0.0],
        hovered: 0.0,
        user: [0; MAX_PARAMS_BYTES],
    };

    /// A block carrying `bytes`, zero-padded.
    ///
    /// Bytes past [`MAX_PARAMS_BYTES`] are dropped. The macro that builds an effect refuses a
    /// parameter structure wider than that, so reaching this is a caller that built one by hand.
    pub fn of(bytes: &[u8]) -> Self {
        let mut params = Self::EMPTY;
        let width = bytes.len().min(MAX_PARAMS_BYTES);
        params.user[..width].copy_from_slice(&bytes[..width]);
        params
    }

    /// The same block with the pointer at `pointer`, over the element or not.
    pub fn with_pointer(mut self, pointer: [f32; 2], hovered: bool) -> Self {
        self.pointer = pointer;
        self.hovered = f32::from(hovered);
        self
    }

    /// The block as the bytes a uniform buffer holds: the framework's half, then the padding that
    /// puts the application's half on a sixteen-byte boundary, then the application's own.
    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut out = [0u8; Self::BYTES];
        out[0..4].copy_from_slice(&self.pointer[0].to_ne_bytes());
        out[4..8].copy_from_slice(&self.pointer[1].to_ne_bytes());
        out[8..12].copy_from_slice(&self.hovered.to_ne_bytes());
        // Bytes 12..16 are the reserved lane the prelude declares.
        out[16..].copy_from_slice(&self.user);
        out
    }

    /// How wide the block is once written out.
    pub const BYTES: usize = 16 + MAX_PARAMS_BYTES;
}

impl Content for ShaderParams {
    fn content_hash(&self) -> u64 {
        ContentHash::new()
            .bytes(&self.pointer[0].to_ne_bytes())
            .bytes(&self.pointer[1].to_ne_bytes())
            .bytes(&self.hovered.to_ne_bytes())
            .bytes(&self.user)
            .finish()
    }
}

/// Every parameter block in the document, interned by content and kept across frames.
pub type ShaderParamsTable = Table<ShaderParamsSlot, ShaderParams>;

#[cfg(test)]
mod tests {
    use super::{MAX_PARAMS_BYTES, ShaderParams, ShaderParamsTable};

    #[test]
    fn equal_parameters_intern_to_one_slot() {
        let mut table = ShaderParamsTable::new();
        let first = table.intern(ShaderParams::of(&[1, 2, 3, 4]));
        let again = table.intern(ShaderParams::of(&[1, 2, 3, 4]));
        let other = table.intern(ShaderParams::of(&[1, 2, 3, 5]));
        assert_eq!(first, again, "the same parameters are one draw");
        assert_ne!(first, other);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn a_block_wider_than_the_limit_is_cut_rather_than_overrunning() {
        let params = ShaderParams::of(&[7u8; MAX_PARAMS_BYTES * 2]);
        assert_eq!(params.user, [7u8; MAX_PARAMS_BYTES]);
    }

    #[test]
    fn the_written_block_puts_the_application_half_on_a_sixteen_byte_boundary() {
        let params = ShaderParams::of(&[9, 8, 7]).with_pointer([2.0, 3.0], true);
        let bytes = params.to_bytes();
        assert_eq!(&bytes[0..4], &2.0f32.to_ne_bytes());
        assert_eq!(&bytes[4..8], &3.0f32.to_ne_bytes());
        assert_eq!(&bytes[8..12], &1.0f32.to_ne_bytes());
        assert_eq!(&bytes[12..16], &[0, 0, 0, 0]);
        assert_eq!(&bytes[16..19], &[9, 8, 7]);
        assert_eq!(bytes.len(), ShaderParams::BYTES);
    }

    #[test]
    fn the_pointer_is_part_of_what_a_slot_is_interned_by() {
        let mut table = ShaderParamsTable::new();
        let still = table.intern(ShaderParams::EMPTY.with_pointer([0.0, 0.0], false));
        let moved = table.intern(ShaderParams::EMPTY.with_pointer([1.0, 0.0], false));
        assert_ne!(still, moved);
    }
}
