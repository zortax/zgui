//! `calc()` expressions, and the handles the layout algorithms carry them by.
//!
//! The layout algorithms hold a `calc()` as an opaque machine word and hand it back when they know
//! the percentage basis. The word has to be non-null and eight-byte aligned and is never
//! dereferenced, so what travels in it here is an index shifted three places up — which satisfies
//! both requirements by construction and involves no pointer and no borrow.

use zgui_css::values::length::{Length, LengthPercentage};

/// Where a conversion may intern a `calc()` it has no other representation for.
///
/// A trait rather than the table itself so a test can supply its own store, and so the conversions
/// stay the single statement of how a CSS value becomes an engine one whoever calls them.
pub(crate) trait InternCalc {
    /// A handle for one expression.
    fn intern_calc(&mut self, value: &LengthPercentage) -> *const ();
}

/// The `calc()` expressions the interned style lowerings refer to.
///
/// Entries are owned: each lowering records the identifiers it interned and gives them back when it
/// is replaced or dropped, so a handle embedded in a lowering stays meaningful for exactly as long
/// as the lowering itself.
#[derive(Debug, Default)]
pub(crate) struct CalcTable {
    /// The expressions, by identifier.
    exprs: Vec<Option<LengthPercentage>>,
    /// Identifiers whose expression was released and may be reissued.
    free: Vec<u32>,
    /// Identifiers interned since the last drain, in interning order.
    issued: Vec<u32>,
    /// Device pixels per CSS pixel.
    scale: f32,
}

impl CalcTable {
    /// Prepares the table for lowerings at `scale` device pixels per CSS pixel.
    pub(crate) fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    /// A handle for one expression. The identifier is recorded for [`CalcTable::drain_issued`].
    pub(crate) fn intern(&mut self, value: &LengthPercentage) -> *const () {
        let id = match self.free.pop() {
            Some(id) => {
                self.exprs[id as usize] = Some(value.clone());
                id
            }
            None => {
                let id =
                    u32::try_from(self.exprs.len()).expect("far fewer than four billion calcs");
                self.exprs.push(Some(value.clone()));
                id
            }
        };
        self.issued.push(id);
        handle(id)
    }

    /// Moves the identifiers interned since the last drain into `into`.
    pub(crate) fn drain_issued(&mut self, into: &mut Vec<u32>) {
        into.append(&mut self.issued);
    }

    /// Releases one identifier, whose handle stops being meaningful.
    pub(crate) fn release(&mut self, id: u32) {
        debug_assert!(
            self.exprs.get(id as usize).is_some_and(Option::is_some),
            "released a calc identifier twice"
        );
        if let Some(slot) = self.exprs.get_mut(id as usize) {
            *slot = None;
            self.free.push(id);
        }
    }

    /// What one handle's expression evaluates to at `basis`, in device pixels.
    ///
    /// The basis arrives in device pixels because every length a layout pass handles is in device
    /// pixels, while the expression is written in CSS pixels — so the basis is converted down, the
    /// expression evaluated, and the result converted back up.
    ///
    /// # Panics
    ///
    /// If the handle's expression was never interned here, or was released.
    pub(crate) fn resolve(&self, value: *const (), basis: f32) -> f32 {
        let index = index(value);
        let expression = self
            .exprs
            .get(index as usize)
            .and_then(Option::as_ref)
            .expect("every calc handle names a live expression");
        expression.resolve(Length::new(basis / self.scale)).px() * self.scale
    }

    /// How many expressions are live.
    pub(crate) fn live(&self) -> usize {
        self.exprs.iter().filter(|it| it.is_some()).count()
    }
}

impl InternCalc for CalcTable {
    fn intern_calc(&mut self, value: &LengthPercentage) -> *const () {
        self.intern(value)
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

#[cfg(test)]
mod tests {
    use zgui_css::values::length::{Length, LengthPercentage, percent};

    use super::{CalcTable, handle, index};

    fn table(scale: f32) -> CalcTable {
        let mut table = CalcTable::default();
        table.set_scale(scale);
        table
    }

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
    fn a_percentage_resolves_against_a_basis_measured_in_device_pixels() {
        let value = percent(0.25);
        let mut table = table(2.0);
        let handle = table.intern(&value);
        // A quarter of a 200-device-pixel basis, whatever the scale, because a percentage has no
        // unit of its own.
        assert_eq!(table.resolve(handle, 200.0), 50.0);
    }

    #[test]
    fn an_absolute_length_is_scaled_and_a_basis_does_not_change_it() {
        let value = LengthPercentage::new_length(Length::new(10.0));
        let mut table = table(2.0);
        let handle = table.intern(&value);
        assert_eq!(table.resolve(handle, 0.0), 20.0);
        assert_eq!(table.resolve(handle, 999.0), 20.0);
    }

    #[test]
    fn released_identifiers_are_reissued_and_their_owners_are_tracked() {
        let value = percent(0.5);
        let mut table = table(1.0);
        let first = table.intern(&value);
        let mut owned = Vec::new();
        table.drain_issued(&mut owned);
        assert_eq!(owned.len(), 1);
        assert_eq!(table.live(), 1);

        table.release(owned[0]);
        assert_eq!(table.live(), 0);

        let second = table.intern(&percent(0.75));
        assert_eq!(first, second, "a dead identifier grows the table forever");
        assert_eq!(table.resolve(second, 100.0), 75.0);
    }
}
