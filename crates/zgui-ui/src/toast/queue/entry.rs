//! One row of the queue: what it says, whether it is on its way out, and how tall it turned out.

use crate::toast::message::Toast;

/// What a toast is called in its queue.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct ToastId(u64);

impl ToastId {
    /// The name a queue hands out for its `number`th toast.
    pub(crate) const fn new(number: u64) -> Self {
        Self(number)
    }

    /// The number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One toast, with the name the queue gave it and what the stack has learnt about it.
///
/// A row outlives the decision to dismiss it. Taking a toast out of the queue the moment something
/// asks for it to go would delete its exit animation along with it, and would move every toast above
/// it in the same frame — so the row is marked instead, and the toast that owns it takes it out once
/// it has finished leaving.
#[derive(Clone, PartialEq, Debug)]
pub struct Queued {
    /// What it is called.
    pub id: ToastId,
    /// What it says.
    pub toast: Toast,
    /// Whether it has been asked to go and is on its way out.
    leaving: bool,
    /// How tall its slot is, in CSS pixels, as the last layout measured it.
    ///
    /// Zero until it has been laid out once, which is the honest answer for a toast that has not
    /// been anywhere yet: the ones above it are placed as if it were not there, and move up when it
    /// reports a height a frame later.
    height: f32,
}

impl Queued {
    /// A row for `toast`, called `id`, that is staying and has never been measured.
    pub(crate) const fn new(id: ToastId, toast: Toast) -> Self {
        Self {
            id,
            toast,
            leaving: false,
            height: 0.0,
        }
    }

    /// Whether it has been asked to go and is on its way out.
    #[must_use]
    pub const fn is_leaving(&self) -> bool {
        self.leaving
    }

    /// How tall its slot is, in CSS pixels, or zero before it has ever been laid out.
    #[must_use]
    pub const fn height(&self) -> f32 {
        self.height
    }

    /// Marks it as on its way out.
    pub(crate) const fn leave(&mut self) {
        self.leaving = true;
    }

    /// Records how tall its slot turned out to be, in CSS pixels.
    pub(crate) const fn measured(&mut self, height: f32) {
        self.height = height;
    }
}

#[cfg(test)]
mod tests {
    use super::{Queued, ToastId};
    use crate::toast::message::Toast;

    #[test]
    fn a_new_row_is_staying_and_has_no_height_yet() {
        let row = Queued::new(ToastId::new(1), Toast::new("Saved"));
        assert!(!row.is_leaving());
        assert_eq!(row.height(), 0.0);
        assert_eq!(row.id.get(), 1);
    }

    #[test]
    fn a_row_that_has_been_asked_to_go_says_so_and_keeps_its_message() {
        let mut row = Queued::new(ToastId::new(2), Toast::new("Saved"));
        row.measured(48.0);
        row.leave();
        assert!(row.is_leaving());
        assert_eq!(row.height(), 48.0);
        assert_eq!(row.toast.title(), "Saved");
    }
}
