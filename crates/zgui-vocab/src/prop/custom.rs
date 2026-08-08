//! The property a custom element's implementation is carried in.

/// The property naming which registered implementation owns an element's box.
///
/// The value is an integer packing a registry token and two revisions, written by [`reference()`]
/// and read back by [`parts`]. It is a *name*, exactly as a canvas's property is: the trait
/// object lives in a registry beside the frame loop, because an implementation can no more cross
/// the document than a shape list can.
pub const ELEMENT: &str = "custom-element";

/// Packs a registry token and the two revisions into the integer [`ELEMENT`] carries.
///
/// The revisions are sixteen bits each and wrap; the cost of a wrap landing exactly on a held
/// value is one missed relayout or repaint of one element — the same declared trade the canvas
/// packing makes.
pub fn reference(token: u32, layout_revision: u16, paint_revision: u16) -> i64 {
    ((token as i64) << 32) | ((layout_revision as i64) << 16) | paint_revision as i64
}

/// Reads back what [`reference()`] packed: the token, the layout revision, the paint revision.
pub fn parts(value: i64) -> (u32, u16, u16) {
    ((value >> 32) as u32, (value >> 16) as u16, value as u16)
}

/// Which obligations a change of the packed value owes.
///
/// Presence flipping is the caller's to notice; this answers for a value that moved: a layout
/// revision that moved owes a relayout, and any movement owes a repaint.
pub fn relayouts(before: i64, after: i64) -> bool {
    parts(before).1 != parts(after).1
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_reference_survives_the_packing() {
        let packed = super::reference(7, 3, 9);
        assert_eq!(super::parts(packed), (7, 3, 9));
        assert!(super::relayouts(packed, super::reference(7, 4, 9)));
        assert!(!super::relayouts(packed, super::reference(7, 3, 10)));
    }
}
