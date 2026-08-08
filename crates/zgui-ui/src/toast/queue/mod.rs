//! The toasts that are on the screen, and everything the stack knows about them.

mod entry;
pub(in crate::toast) mod place;

pub use crate::toast::queue::entry::{Queued, ToastId};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};

use crate::toast::message::Toast;

/// The toasts that are on the screen, newest first.
///
/// Reached from anywhere under a [`Toaster`](crate::Toaster) with
/// [`use_toaster`](crate::use_toaster), so a handler four components deep announces something
/// without any of the components between it and the toaster knowing that it might.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::toast::{Toast, ToastQueue};
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let queue = ToastQueue::provide();
///     let first = queue.push(Toast::new("Saved"));
///     assert_eq!(queue.live().len(), 1);
///
///     // Asking for it to go starts it leaving. It is still on the screen, because its exit
///     // animation is still running; the toast itself takes the row out when that has finished.
///     queue.dismiss(first);
///     assert!(queue.live().is_empty());
///     assert_eq!(queue.showing().len(), 1);
///
///     queue.remove(first);
///     assert!(queue.showing().is_empty());
/// });
/// scope.unmount();
/// ```
///
/// # Why leaving is a state rather than a removal
///
/// Three of the things a stack of toasts has to do are impossible if dismissing one deletes it. Its
/// exit animation needs it to still be there to run on; the toasts above it need one frame in which
/// it is known to be going, so that they slide down into the gap instead of appearing in it; and the
/// oldest toast, when a fourth arrives and only three may show, has to leave the same way a dismissed
/// one does rather than blinking out. So dismissing marks the row and [`ToastQueue::remove`] is what
/// finally takes it out — called by the toast itself, once it has finished leaving.
#[derive(Copy, Clone)]
pub struct ToastQueue {
    /// What is on the screen, newest first, including the ones on their way out.
    items: RwSignal<Vec<Queued>, LocalStorage>,
    /// The next name to hand out.
    next: RwSignal<u64, LocalStorage>,
    /// How many toasts the pointer is inside.
    ///
    /// A count rather than a flag, because the pointer leaves one toast and enters the next in the
    /// same breath and a flag would be false in between.
    held: RwSignal<usize, LocalStorage>,
    /// How many may be staying at once.
    limit: usize,
}

impl ToastQueue {
    /// An empty queue keeping at most `limit` toasts at a time.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            items: RwSignal::new_local(Vec::new()),
            next: RwSignal::new_local(1),
            held: RwSignal::new_local(0),
            limit: limit.max(1),
        }
    }

    /// An empty queue with the usual limit, published to every scope below this one.
    #[must_use]
    pub fn provide() -> Self {
        let queue = Self::new(3);
        provide_local_context(queue);
        queue
    }

    /// The queue the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Every toast still on the screen, newest first — the ones on their way out included.
    ///
    /// What the stack is built from, because a toast that is leaving is still being drawn.
    #[must_use]
    pub fn showing(self) -> Vec<Queued> {
        self.items.get()
    }

    /// The toasts that are staying, newest first.
    #[must_use]
    pub fn live(self) -> Vec<Queued> {
        self.items
            .get()
            .into_iter()
            .filter(|entry| !entry.is_leaving())
            .collect()
    }

    /// How many may be staying at once.
    #[must_use]
    pub fn limit(self) -> usize {
        self.limit
    }

    /// Adds a toast, and reports what it is called.
    ///
    /// Newest first. Once more than [`ToastQueue::limit`] are staying, the oldest ones are asked to
    /// go — because a stack that grew without bound would cover the interface the toasts are about,
    /// and because leaving is something a toast does visibly rather than something done to it.
    pub fn push(self, toast: Toast) -> ToastId {
        let id = ToastId::new(self.next.get_untracked());
        self.next.set(id.get() + 1);
        let limit = self.limit;
        self.items.update(|items| {
            items.insert(0, Queued::new(id, toast));
            let mut staying = 0;
            for entry in items.iter_mut() {
                if entry.is_leaving() {
                    continue;
                }
                staying += 1;
                if staying > limit {
                    entry.leave();
                }
            }
        });
        id
    }

    /// Asks the toast called `id` to go.
    ///
    /// It stays on the screen until its exit animation has finished, and stops taking up room on the
    /// stack at once, so the toasts above it slide down while it fades.
    pub fn dismiss(self, id: ToastId) {
        self.items.try_update(|items| {
            if let Some(entry) = items
                .iter_mut()
                .find(|entry| entry.id == id && !entry.is_leaving())
            {
                entry.leave();
            }
        });
    }

    /// Takes the toast called `id` off the screen, whatever it was doing.
    ///
    /// Called by a toast that has finished leaving. Calling it directly takes a toast away with no
    /// exit animation at all, which is what a caller wants only when nobody is looking.
    pub fn remove(self, id: ToastId) {
        self.items
            .try_update(|items| items.retain(|entry| entry.id != id));
    }

    /// Asks every toast to go.
    pub fn clear(self) {
        self.items.try_update(|items| {
            for entry in items.iter_mut() {
                entry.leave();
            }
        });
    }

    /// Whether the toast called `id` is on its way out.
    #[must_use]
    pub fn is_leaving(self, id: ToastId) -> bool {
        self.items
            .get()
            .iter()
            .any(|entry| entry.id == id && entry.is_leaving())
    }

    /// The same, asked from somewhere that is not building anything.
    ///
    /// A timer's callback is not a reactive read: nothing it produces is redrawn because this
    /// answer changed, and subscribing to the queue from one would be a subscription nobody ever
    /// releases.
    #[must_use]
    pub(crate) fn is_leaving_untracked(self, id: ToastId) -> bool {
        self.items
            .get_untracked()
            .iter()
            .any(|entry| entry.id == id && entry.is_leaving())
    }

    /// How far the toast called `id` sits from the corner, in CSS pixels.
    #[must_use]
    pub fn offset_of(self, id: ToastId) -> f32 {
        place::offset_of(&self.items.get(), id)
    }

    /// How many staying toasts are between the one called `id` and the corner.
    ///
    /// What a collapsed stack is placed by: a step and a shrink per toast in front, rather than the
    /// measured heights an expanded stack clears.
    #[must_use]
    pub fn depth_of(self, id: ToastId) -> usize {
        place::depth_of(&self.items.get(), id)
    }

    /// Where the toast called `id` paints among its siblings: the newest highest.
    ///
    /// What the sheet stacks the slots by. The region's document order is newest first, which left
    /// alone would paint the oldest toast over everything in front of it.
    #[must_use]
    pub fn layer_of(self, id: ToastId) -> usize {
        place::layer_of(&self.items.get(), id)
    }

    /// How much of the window the stack's outline covers, measured away from its corner, in CSS
    /// pixels.
    ///
    /// What the region sizes its own box to, so that the pointer is held by one element covering
    /// toasts and gaps alike. A collapsed stack measures the front toast and one step for each
    /// toast behind it; an expanded one measures every toast's reported height.
    pub fn extent(self) -> f32 {
        place::extent(&self.items.get(), self.held.get() > 0)
    }

    /// Records how tall the toast called `id` turned out to be, in CSS pixels.
    ///
    /// Written only when the number has actually changed. Every toast reports its height on every
    /// frame that measured it, and a queue that took each report as news would ask the stack to be
    /// placed again sixty times a second while nothing moved.
    pub fn measure(self, id: ToastId, height: f32) {
        let known = self
            .items
            .get_untracked()
            .iter()
            .find(|entry| entry.id == id)
            .map(Queued::height);
        if known == Some(height) {
            return;
        }
        self.items.try_update(|items| {
            if let Some(entry) = items.iter_mut().find(|entry| entry.id == id) {
                entry.measured(height);
            }
        });
    }

    /// Says the pointer is inside one more toast.
    pub fn hold(self) {
        self.held.update(|held| *held += 1);
    }

    /// Says the pointer has left one.
    pub fn let_go(self) {
        self.held.update(|held| *held = held.saturating_sub(1));
    }

    /// Whether the pointer is on the stack, which is what stops every toast expiring.
    ///
    /// The whole stack rather than one toast: reading the second of three messages must not let the
    /// first and third disappear from under it.
    #[must_use]
    pub fn is_held(self) -> bool {
        self.held.get() > 0
    }
}

#[cfg(test)]
mod tests {
    use zgui::reactive::{Mounted, install};

    use super::ToastQueue;
    use crate::toast::message::Toast;

    /// Runs `body` inside a mounted reactive scope, which is what signals need.
    fn mounted(body: impl FnOnce()) {
        install().ok();
        let scope = Mounted::new();
        scope.with(body);
        scope.unmount();
    }

    #[test]
    fn the_newest_toast_is_the_first_one() {
        mounted(|| {
            let queue = ToastQueue::new(5);
            queue.push(Toast::new("first"));
            queue.push(Toast::new("second"));
            let showing = queue.showing();
            assert_eq!(showing[0].toast.title(), "second");
            assert_eq!(showing[1].toast.title(), "first");
        });
    }

    #[test]
    fn the_oldest_is_asked_to_leave_once_the_limit_is_passed() {
        // Not truncated away. A toast that vanished the instant a fourth arrived would have no exit
        // animation, and everything above it would move in the same frame.
        mounted(|| {
            let queue = ToastQueue::new(2);
            for index in 0..4 {
                queue.push(Toast::new(index.to_string()));
            }
            let staying: Vec<String> = queue
                .live()
                .iter()
                .map(|entry| entry.toast.title().to_owned())
                .collect();
            assert_eq!(staying, ["3", "2"], "the newest two are staying");
            assert_eq!(queue.showing().len(), 4, "the other two are still leaving");
            for entry in queue.showing().iter().skip(2) {
                assert!(entry.is_leaving());
            }
        });
    }

    #[test]
    fn dismissing_one_leaves_the_others_alone() {
        mounted(|| {
            let queue = ToastQueue::new(5);
            let first = queue.push(Toast::new("first"));
            queue.push(Toast::new("second"));
            queue.dismiss(first);
            assert_eq!(queue.live().len(), 1);
            assert_eq!(queue.live()[0].toast.title(), "second");
            assert!(queue.is_leaving(first));
        });
    }

    #[test]
    fn a_toast_that_has_finished_leaving_is_gone() {
        mounted(|| {
            let queue = ToastQueue::new(5);
            let only = queue.push(Toast::new("first"));
            queue.dismiss(only);
            queue.remove(only);
            assert!(queue.showing().is_empty());
        });
    }

    #[test]
    fn the_stack_is_held_while_the_pointer_is_on_any_of_it() {
        // A count, not a flag: the pointer leaves one toast and enters the next in the same breath,
        // and a flag would let every timer restart in between.
        mounted(|| {
            let queue = ToastQueue::new(5);
            assert!(!queue.is_held());
            queue.hold();
            queue.hold();
            queue.let_go();
            assert!(queue.is_held(), "the pointer is still on the second one");
            queue.let_go();
            assert!(!queue.is_held());
            queue.let_go();
            assert!(!queue.is_held(), "and it does not go negative");
        });
    }

    #[test]
    fn the_stack_places_each_toast_clear_of_the_ones_below_it() {
        mounted(|| {
            let queue = ToastQueue::new(5);
            let first = queue.push(Toast::new("first"));
            let second = queue.push(Toast::new("second"));
            queue.measure(first, 40.0);
            queue.measure(second, 56.0);
            assert_eq!(queue.offset_of(second), 0.0, "the newest is at the corner");
            assert_eq!(queue.offset_of(first), 56.0);

            queue.dismiss(second);
            assert_eq!(
                queue.offset_of(first),
                0.0,
                "and it moves down as soon as the one below it is going"
            );
        });
    }
}
