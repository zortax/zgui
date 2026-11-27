//! A parameter block an application keeps changing while items draw it.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use iceblink::ParamsBlock;

/// What wakes a window to draw a changed block.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// A parameter block shared between the code that changes it and every window that draws it.
///
/// Every window that draws the block keeps one parameter slot for it, and writes the slot whenever
/// the block changed since the window's last frame, so a change is drawn without the items that
/// name it being recorded again. Changing the block wakes every such window.
///
/// Clones share the block.
#[derive(Clone)]
pub struct SharedParams(Arc<Mutex<Held>>);

/// The block, how often it changed, and the windows to wake.
struct Held {
    block: ParamsBlock,
    revision: u64,
    wakes: Vec<Wake>,
}

impl SharedParams {
    /// Returns a block of zeroes.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Held {
            block: ParamsBlock {
                pointer: [0.0; 2],
                hovered: 0.0,
                _reserved: 0.0,
                user: [0; super::MAX_PARAMS_BYTES],
            },
            revision: 0,
            wakes: Vec::new(),
        })))
    }

    /// Returns the block.
    pub fn block(&self) -> ParamsBlock {
        self.lock().block
    }

    /// Returns how many times the block changed.
    pub fn revision(&self) -> u64 {
        self.lock().revision
    }

    /// Applies `change` to the block, counts it, and wakes every window that draws the block.
    pub fn update(&self, change: impl FnOnce(&mut ParamsBlock)) {
        let wakes = {
            let mut held = self.lock();
            change(&mut held.block);
            held.revision = held.revision.wrapping_add(1);
            held.wakes.clone()
        };
        for wake in wakes {
            wake();
        }
    }

    /// Returns the block and its revision.
    pub(crate) fn read(&self) -> (ParamsBlock, u64) {
        let held = self.lock();
        (held.block, held.revision)
    }

    /// Adds `wake` to what a change wakes, unless it is there already.
    pub(crate) fn wake(&self, wake: &Wake) {
        let mut held = self.lock();
        if !held.wakes.iter().any(|held| Arc::ptr_eq(held, wake)) {
            held.wakes.push(Arc::clone(wake));
        }
    }

    /// Removes `wake` from what a change wakes.
    pub(crate) fn forget(&self, wake: &Wake) {
        self.lock().wakes.retain(|held| !Arc::ptr_eq(held, wake));
    }

    /// Returns a number that is equal for clones and differs between blocks.
    pub(crate) fn identity(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }

    fn lock(&self) -> MutexGuard<'_, Held> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for SharedParams {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for SharedParams {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let held = self.lock();
        formatter
            .debug_struct("SharedParams")
            .field("block", &held.block)
            .field("revision", &held.revision)
            .finish_non_exhaustive()
    }
}
