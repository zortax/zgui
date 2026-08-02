//! Where each toast sits on the stack, as a distance from the corner.
//!
//! The stack is placed by transform rather than by flow, and this is the arithmetic behind that
//! decision. A column of boxes in normal flow closes the gap left by one that goes away *in the
//! layout*, which no style sheet can interpolate: the remaining toasts are simply somewhere else on
//! the next frame. A distance from the corner is a `transform`, and a transform is a property a
//! transition can move — so the toast that leaves takes its box out of the stack at once and the
//! ones above it slide down into the space it left.
//!
//! Nothing here is a number written down. The step from one toast to the next is the height the last
//! layout measured for the slot below it, gap included, because the slot carries the gap as padding
//! on the side the next toast is on. A toast with a description is taller than one without, and this
//! arithmetic never has to know that.

use crate::toast::queue::entry::{Queued, ToastId};

/// How far the toast at `index` sits from the corner, in CSS pixels.
///
/// Zero for the newest, which is the one against the corner. Everything else is the total height of
/// the toasts between it and the corner — counting only the ones that are staying, so a toast on its
/// way out stops taking up room the moment it is asked to go.
#[must_use]
pub(super) fn offset(entries: &[Queued], index: usize) -> f32 {
    entries
        .iter()
        .take(index)
        .filter(|entry| !entry.is_leaving())
        .map(Queued::height)
        // Folded from a positive zero rather than summed, because the identity `Sum` uses for a
        // float is *negative* zero — and the toast against the corner would then publish a distance
        // of `-0px`, which is a length that reads as a mistake wherever anybody looks at it.
        .fold(0.0, |total, height| total + height)
}

/// How much of the window the stack's outline covers, measured away from its corner, in CSS
/// pixels.
///
/// This is the height the region styles itself to, and the region is what holds the pointer: the
/// slots are stacked pictures that must not take clicks of their own — a slot's gap padding lies
/// over the toast behind it, and a padding that took the pointer would sit between that toast's
/// close control and the press aimed at it. So the region needs a box of its own that covers the
/// whole outline, gaps included, and this is that box's size: the measured heights while the stack
/// is open, and the front card plus one gap-step per card behind it while it is a collapsed deck.
#[must_use]
pub(super) fn extent(entries: &[Queued], held: bool) -> f32 {
    let staying = entries.iter().filter(|entry| !entry.is_leaving());
    if held {
        staying.map(Queued::height).fold(0.0, |total, height| total + height)
    } else {
        let mut cards = staying;
        let front = cards.next().map_or(0.0, Queued::height);
        front + GAP * cards.count() as f32
    }
}

/// How far a collapsed deck steps per card behind the front one, in CSS pixels.
///
/// The same number the sheet writes as `--zui-toast-gap`; stated here as well because the region's
/// own size is computed rather than laid out, and the two must agree about what a step is. It is
/// also what the item adds to its measured content, because the height a row reports is the
/// toast *and* the gap the slot carries as padding.
pub(in crate::toast) const GAP: f32 = 14.0;

/// How far the toast called `id` sits from the corner, in CSS pixels, or zero when it is not here.
#[must_use]
pub(super) fn offset_of(entries: &[Queued], id: ToastId) -> f32 {
    entries
        .iter()
        .position(|entry| entry.id == id)
        .map_or(0.0, |index| offset(entries, index))
}

/// How many staying toasts are between the one called `id` and the corner.
///
/// A count rather than a distance, and the two are not the same question: a collapsed stack steps by
/// a fixed amount and shrinks by a fixed proportion per toast behind the front one, and neither of
/// those has anything to do with how tall any of them turned out to be.
#[must_use]
pub(super) fn depth_of(entries: &[Queued], id: ToastId) -> usize {
    entries
        .iter()
        .position(|entry| entry.id == id)
        .map_or(0, |index| {
            entries
                .iter()
                .take(index)
                .filter(|entry| !entry.is_leaving())
                .count()
        })
}

/// Where the stacking order runs down from, so the front toast can be the highest number.
///
/// Any fixed number bigger than a stack will ever be: the queue holds its limit plus however many
/// are still animating out, and a hundred toasts leaving at once is not an interface anybody has.
const TOP_LAYER: usize = 100;

/// Where the toast called `id` paints among its siblings: the newest highest.
///
/// The queue keeps its rows newest first, and so does the region — which means document order alone
/// paints the *oldest* on top, exactly upside down. This hands the sheet a number to stack by
/// instead: [`TOP_LAYER`] minus the row's place in the queue, so the front toast is the highest and
/// every toast covers the ones behind it.
///
/// The place in the queue rather than [`depth_of`], because a leaving toast shares its depth with
/// the one sliding into its spot — and a tie in a stacking number is settled by document order,
/// which is the very order this exists to overrule.
#[must_use]
pub(super) fn layer_of(entries: &[Queued], id: ToastId) -> usize {
    entries
        .iter()
        .position(|entry| entry.id == id)
        .map_or(TOP_LAYER, |index| TOP_LAYER.saturating_sub(index))
}

#[cfg(test)]
mod tests {
    use super::{layer_of, offset, offset_of};
    use crate::toast::message::Toast;
    use crate::toast::queue::entry::{Queued, ToastId};

    /// Three measured rows, newest first, of the heights given.
    fn stack(heights: [f32; 3]) -> Vec<Queued> {
        heights
            .iter()
            .enumerate()
            .map(|(index, height)| {
                let mut row = Queued::new(ToastId::new(index as u64 + 1), Toast::new("x"));
                row.measured(*height);
                row
            })
            .collect()
    }

    #[test]
    fn the_newest_is_against_the_corner() {
        let offset = offset(&stack([40.0, 50.0, 60.0]), 0);
        assert_eq!(offset, 0.0);
        assert_eq!(
            format!("{offset}px"),
            "0px",
            "and says so as a length, which a negative zero does not"
        );
    }

    #[test]
    fn each_toast_clears_the_ones_between_it_and_the_corner() {
        // The heights differ, because a toast with a description is taller than one without and a
        // step of some fixed size would put the third one through the second.
        let entries = stack([40.0, 50.0, 60.0]);
        assert_eq!(offset(&entries, 1), 40.0);
        assert_eq!(offset(&entries, 2), 90.0);
    }

    #[test]
    fn a_toast_on_its_way_out_stops_taking_up_room() {
        // What makes the gap close: the ones above a departing toast are placed as if it had already
        // gone, and the transform they are placed with is a property a transition can move.
        let mut entries = stack([40.0, 50.0, 60.0]);
        entries[0].leave();
        assert_eq!(offset(&entries, 1), 0.0, "the second takes the corner");
        assert_eq!(offset(&entries, 2), 50.0);
    }

    #[test]
    fn a_departing_toast_keeps_the_place_it_had() {
        // Its own offset counts what is between it and the corner, which its own departure does not
        // change — so it fades out where it is instead of sliding to the corner first.
        let mut entries = stack([40.0, 50.0, 60.0]);
        entries[1].leave();
        assert_eq!(offset(&entries, 1), 40.0);
    }

    #[test]
    fn one_that_has_never_been_laid_out_pushes_nothing() {
        // A toast is in the queue for one frame before anything has measured it. Treating that as a
        // height of zero puts the ones above it where they already were, and they move up when the
        // measurement arrives — rather than jumping to a guess and then correcting it.
        let mut entries = stack([40.0, 50.0, 60.0]);
        entries[0].measured(0.0);
        assert_eq!(offset(&entries, 1), 0.0);
    }

    #[test]
    fn the_newest_toast_paints_over_the_ones_behind_it() {
        // Document order alone would paint the oldest on top, because the stack is kept newest
        // first. The layer is what puts that right, and a leaving toast keeps a layer of its own so
        // no two rows ever tie.
        let mut entries = stack([40.0, 50.0, 60.0]);
        entries[0].leave();
        let layers: Vec<usize> = entries
            .iter()
            .map(|entry| layer_of(&entries, entry.id))
            .collect();
        assert!(layers[0] > layers[1] && layers[1] > layers[2]);
    }

    #[test]
    fn a_name_the_stack_does_not_hold_is_at_the_corner() {
        let entries = stack([40.0, 50.0, 60.0]);
        assert_eq!(offset_of(&entries, ToastId::new(2)), 40.0);
        assert_eq!(offset_of(&entries, ToastId::new(99)), 0.0);
    }
}
