//! `calc()` expressions, and the handles the layout algorithms carry them by.
//!
//! The layout algorithms hold a `calc()` as an opaque machine word and hand it back when they know
//! the percentage basis. The word has to be non-null and eight-byte aligned and is never
//! dereferenced, so what travels in it here is an index shifted three places up — which satisfies
//! both requirements by construction and involves no pointer and no borrow.

use core::cell::RefCell;

use rustc_hash::FxHashMap;
use zgui_css::values::length::{Length, LengthPercentage};

/// The `calc()` expressions one layout pass refers to.
///
/// Expressions are interned by the address of the computed value they came from, which is stable
/// for as long as the style holding it is alive — and every style a pass reads is held by a box for
/// the whole of that pass. Two occurrences of the same expression in two styles therefore get two
/// entries, which costs a machine word and keeps the interning free of any comparison.
#[derive(Debug, Default)]
pub struct CalcArena {
    /// The expressions, in the order they were first seen.
    exprs: Vec<LengthPercentage>,
    /// Where each address's expression landed.
    by_address: FxHashMap<usize, u32>,
    /// Device pixels per CSS pixel.
    scale: f32,
}

impl CalcArena {
    /// An empty arena for a pass running at `scale` device pixels per CSS pixel.
    pub fn new(scale: f32) -> Self {
        Self {
            exprs: Vec::new(),
            by_address: FxHashMap::default(),
            scale,
        }
    }

    /// How many expressions are held.
    pub fn len(&self) -> usize {
        self.exprs.len()
    }

    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.exprs.is_empty()
    }

    /// Forgets every expression. The handles handed out before this stop being meaningful.
    pub fn clear(&mut self) {
        self.exprs.clear();
        self.by_address.clear();
    }

    /// A handle for one expression, interning it if it has not been seen.
    pub fn intern(&mut self, value: &LengthPercentage) -> *const () {
        let address = core::ptr::from_ref(value) as usize;
        let index = *self.by_address.entry(address).or_insert_with(|| {
            let index = u32::try_from(self.exprs.len()).expect("far fewer than four billion calcs");
            self.exprs.push(value.clone());
            index
        });
        handle(index)
    }

    /// What one handle's expression evaluates to at `basis`, in device pixels.
    ///
    /// The basis arrives in device pixels because every length a layout pass handles is in device
    /// pixels, while the expression is written in CSS pixels — so the basis is converted down,
    /// the expression evaluated, and the result converted back up.
    ///
    /// # Panics
    ///
    /// If the handle did not come from [`CalcArena::intern`] on this arena.
    pub fn resolve(&self, value: *const (), basis: f32) -> f32 {
        let index = index(value);
        let expression = self
            .exprs
            .get(index as usize)
            .expect("every calc handle came from this arena");
        expression.resolve(Length::new(basis / self.scale)).px() * self.scale
    }
}

/// The handle for an index: non-null and eight-byte aligned for every index.
fn handle(index: u32) -> *const () {
    ((index as usize + 1) << 3) as *const ()
}

/// The index a handle carries.
fn index(handle: *const ()) -> u32 {
    ((handle as usize >> 3) - 1) as u32
}

/// Resolves a handle against an arena that may be borrowed elsewhere on the stack.
///
/// The borrow is taken and released inside this call, so no guard is ever alive while the layout
/// algorithms recurse.
pub(crate) fn resolve_in(arena: &RefCell<CalcArena>, value: *const (), basis: f32) -> f32 {
    arena.borrow().resolve(value, basis)
}

#[cfg(test)]
mod tests {
    use zgui_css::values::length::{Length, LengthPercentage, percent};

    use super::{CalcArena, handle, index};

    #[test]
    fn every_handle_is_non_null_and_eight_byte_aligned() {
        for raw in [0, 1, 2, 1000, u32::MAX / 8] {
            let handle = handle(raw);
            assert!(!handle.is_null(), "index {raw}");
            assert_eq!(handle as usize % 8, 0, "index {raw}");
            assert_eq!(index(handle), raw);
        }
    }

    #[test]
    fn one_expression_interns_once_however_often_it_is_asked_for() {
        let value = percent(0.5);
        let mut arena = CalcArena::new(1.0);
        let first = arena.intern(&value);
        let second = arena.intern(&value);
        assert_eq!(first, second);
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn a_percentage_resolves_against_a_basis_measured_in_device_pixels() {
        let value = percent(0.25);
        let mut arena = CalcArena::new(2.0);
        let handle = arena.intern(&value);
        // A quarter of a 200-device-pixel basis, whatever the scale, because a percentage has no
        // unit of its own.
        assert_eq!(arena.resolve(handle, 200.0), 50.0);
    }

    #[test]
    fn an_absolute_length_is_scaled_and_a_basis_does_not_change_it() {
        let value = LengthPercentage::new_length(Length::new(10.0));
        let mut arena = CalcArena::new(2.0);
        let handle = arena.intern(&value);
        assert_eq!(arena.resolve(handle, 0.0), 20.0);
        assert_eq!(arena.resolve(handle, 999.0), 20.0);
    }

    #[test]
    fn clearing_frees_the_expressions() {
        let value = percent(0.5);
        let mut arena = CalcArena::new(1.0);
        arena.intern(&value);
        assert!(!arena.is_empty());
        arena.clear();
        assert!(arena.is_empty());
    }
}
