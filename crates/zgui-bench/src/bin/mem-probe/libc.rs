//! The three C library calls the probe makes: the allocator's own statistics, giving free memory
//! back to the system, and the page size.

#![allow(unsafe_code)]

use core::ffi::c_int;

/// glibc's allocator statistics, as `mallinfo2` reports them.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Mallinfo2 {
    /// Bytes the main arena got from the system with `brk`.
    pub(crate) arena: usize,
    /// Free chunks.
    pub(crate) ordblks: usize,
    /// Free fastbin chunks.
    pub(crate) smblks: usize,
    /// Mapped chunks.
    pub(crate) hblks: usize,
    /// Bytes in mapped chunks.
    pub(crate) hblkhd: usize,
    /// Unused; always zero.
    pub(crate) usmblks: usize,
    /// Bytes in free fastbin chunks.
    pub(crate) fsmblks: usize,
    /// Bytes in chunks that are in use.
    pub(crate) uordblks: usize,
    /// Bytes in free chunks.
    pub(crate) fordblks: usize,
    /// Bytes at the top of the heap that `malloc_trim` can give back.
    pub(crate) keepcost: usize,
}

// SAFETY: the declarations match glibc's: `struct mallinfo2 mallinfo2(void)`,
// `int malloc_trim(size_t)` and `int getpagesize(void)`.
unsafe extern "C" {
    fn mallinfo2() -> Mallinfo2;
    fn malloc_trim(pad: usize) -> c_int;
    fn getpagesize() -> c_int;
}

/// The allocator's statistics now.
pub(crate) fn mallinfo() -> Mallinfo2 {
    // SAFETY: takes no arguments and returns a plain struct by value.
    unsafe { mallinfo2() }
}

/// Gives free memory at the top of the heap and inside it back to the system.
///
/// Returns whether anything was given back.
pub(crate) fn trim() -> bool {
    // SAFETY: takes a plain size; zero keeps no padding.
    unsafe { malloc_trim(0) != 0 }
}

/// The size of a memory page in bytes.
pub(crate) fn page_size() -> u64 {
    // SAFETY: takes no arguments and returns a plain integer.
    let size = unsafe { getpagesize() };
    u64::try_from(size).unwrap_or(4096)
}
