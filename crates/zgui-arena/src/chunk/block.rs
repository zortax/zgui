//! Fixed-size storage blocks, and the whole of this crate's raw-memory handling.
//!
//! A block is allocated once, never resized and never moved, so the address of a slot inside it
//! is fixed for the block's whole life. Everything that reads or writes a slot goes through this
//! module, so the contract each operation demands is stated in exactly one place.

// Every raw-memory operation in the crate lives here, each stating what its caller must uphold.
// Growing storage without moving what is already stored cannot be expressed any other way: a
// growable container of values reallocates, and a reallocation invalidates every reference into
// it.
#![allow(unsafe_code)]

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;

/// Number of slots per block. Sized so one block is about 64 KiB for a 128-byte value.
pub const BLOCK_LEN: usize = 512;

/// One block of slots, each either holding a value or uninitialised.
///
/// The slots sit behind [`UnsafeCell`] because writing one must not require a unique reference to
/// the block: a caller may be holding a shared reference to a *different* slot of the same block,
/// and a `&mut` to the block would invalidate it.
#[repr(transparent)]
pub(crate) struct Block<T>([UnsafeCell<MaybeUninit<T>>; BLOCK_LEN]);

impl<T> Block<T> {
    /// Allocates a block whose slots are all uninitialised.
    ///
    /// Every slot method indexes the block, so a slot number past [`BLOCK_LEN`] panics rather
    /// than reaching past the end.
    pub(crate) fn new() -> Box<Self> {
        let layout = core::alloc::Layout::new::<Self>();
        if layout.size() == 0 {
            // A block of zero-sized values needs no allocation, and asking the allocator for zero
            // bytes is not allowed, so it takes the ordinary path.
            return Box::new(Self(core::array::from_fn(|_| {
                UnsafeCell::new(MaybeUninit::uninit())
            })));
        }
        // SAFETY: the layout has a non-zero size, checked directly above. The allocation is
        // handed straight to `Box::from_raw`, which takes ownership of exactly the layout
        // `Layout::new::<Self>()` describes, so the eventual deallocation matches this
        // allocation. Every slot of `Self` is a `MaybeUninit`, for which any bit pattern
        // — including the uninitialised one the allocator returns — is a valid value.
        let pointer = unsafe { std::alloc::alloc(layout) }.cast::<Self>();
        if pointer.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        // SAFETY: `pointer` is non-null, checked directly above; it was produced by the global
        // allocator with the layout of `Self` and points at a valid `Self` by the argument above;
        // and it is not aliased, having just been allocated.
        unsafe { Box::from_raw(pointer) }
    }

    /// Places a value in a slot.
    ///
    /// # Safety
    ///
    /// The slot must be uninitialised — a slot holding a value must be emptied with
    /// [`Block::take`] or [`Block::drop_value`] first, or the value it holds is leaked. No
    /// reference into this slot, shared or unique, may be live.
    pub(crate) unsafe fn write(&self, slot: usize, value: T) {
        // SAFETY: the pointer comes from a cell of this block, so it is valid and aligned for a
        // write of `MaybeUninit<T>`, and by the caller's promise nothing else refers to this slot
        // for the duration of the write.
        unsafe { self.0[slot].get().write(MaybeUninit::new(value)) };
    }

    /// Borrows the value in a slot.
    ///
    /// # Safety
    ///
    /// The slot must hold a value. The returned reference is valid
    /// for as long as the block lives and the slot is not emptied; the caller is responsible for
    /// not emptying the slot while it is borrowed.
    pub(crate) unsafe fn get(&self, slot: usize) -> &T {
        // SAFETY: the slot is initialised by the caller's promise. The reference is derived from
        // `&self`, so it borrows the block rather than any unique access to it, and the caller's
        // promise rules out a concurrent write to this slot.
        unsafe { (*self.0[slot].get()).assume_init_ref() }
    }

    /// Borrows the value in a slot for modification.
    ///
    /// # Safety
    ///
    /// The slot must hold a value, and the caller must hold unique access to the block, so that no
    /// other reference into the slot exists for as long as the returned one does.
    pub(crate) unsafe fn get_mut(&mut self, slot: usize) -> &mut T {
        // SAFETY: the slot is initialised by the caller's promise, and the reference is derived
        // from `&mut self`, which is itself proof that no other reference into the block is live.
        unsafe { (*self.0[slot].get()).assume_init_mut() }
    }

    /// Moves the value out of a slot, leaving it uninitialised.
    ///
    /// # Safety
    ///
    /// The slot must hold a value, and no reference into it may be live. The slot is
    /// uninitialised afterwards.
    pub(crate) unsafe fn take(&self, slot: usize) -> T {
        // SAFETY: the slot is initialised by the caller's promise, so the pointer is valid and
        // aligned for a read of `T`. The caller promises no reference into the slot is live, so
        // moving the value out leaves nothing dangling, and promises to treat the slot as
        // uninitialised afterwards, so the value is not read twice.
        unsafe { self.0[slot].get().read().assume_init() }
    }

    /// Drops the value in a slot in place, leaving it uninitialised.
    ///
    /// # Safety
    ///
    /// The slot must hold a value, and no reference into it may be live. The slot is
    /// uninitialised afterwards.
    pub(crate) unsafe fn drop_value(&self, slot: usize) {
        // SAFETY: as `take`, except that the value is destroyed where it lies rather than moved
        // out; the caller's promises are the same and are what rule out a double drop.
        unsafe { (*self.0[slot].get()).assume_init_drop() };
    }
}

// SAFETY: every method that reaches a slot is `unsafe` and demands the caller rule out
// concurrent access to that slot, so safe code holding `&Block<T>` can observe nothing at all.
// The only value that can be obtained from a shared block is a `&T`, which is why sharing a block
// across threads requires exactly what sharing a `&T` requires.
unsafe impl<T: Sync> Sync for Block<T> {}

// SAFETY: a block owns its slots and nothing else, so moving one to another thread moves the
// values it holds and nothing more.
unsafe impl<T: Send> Send for Block<T> {}

#[cfg(test)]
mod tests {
    use super::{BLOCK_LEN, Block};

    #[test]
    fn a_slot_written_then_read_holds_its_value() {
        let block = Block::<u32>::new();
        // SAFETY: slot 0 is in bounds and uninitialised; nothing refers to it.
        unsafe { block.write(0, 5) };
        // SAFETY: slot 0 holds a value, written directly above.
        assert_eq!(unsafe { block.get(0) }, &5);
        // SAFETY: slot 0 holds a value and the reference above has been dropped.
        assert_eq!(unsafe { block.take(0) }, 5);
    }

    #[test]
    fn a_reference_survives_writes_to_other_slots() {
        let block = Block::<u32>::new();
        // SAFETY: every slot is in bounds and uninitialised, and written exactly once.
        unsafe { block.write(0, 1) };
        // SAFETY: slot 0 holds a value.
        let first = unsafe { block.get(0) };
        for slot in 1..BLOCK_LEN {
            // SAFETY: `slot` is in bounds, uninitialised, and unreferenced.
            unsafe { block.write(slot, slot as u32) };
        }
        assert_eq!(first, &1);
        for slot in 0..BLOCK_LEN {
            // SAFETY: every slot holds a value and none of them are referenced elsewhere.
            unsafe { block.drop_value(slot) };
        }
    }

    #[test]
    fn a_block_of_zero_sized_values_needs_no_allocation() {
        let block = Block::<()>::new();
        // SAFETY: slot 0 is in bounds and uninitialised.
        unsafe { block.write(0, ()) };
        // SAFETY: slot 0 holds a value.
        unsafe { block.drop_value(0) };
    }
}
