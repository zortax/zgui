//! The focus a self-focusing trap is owed, and why it cannot be paid where it is asked for.
//!
//! A trap that focuses itself is installed from a render effect, which runs while the surface it
//! confines is being built. At that moment the surface has no boxes: it was mounted in this frame's
//! reactive flush and layout has not run yet, so *every focusable element inside it is invisible* —
//! an element that generates no box cannot be focused, and asking for the first one there answers
//! with nothing. The menu opens, the caret stays on the button that opened it, and every key the
//! surface claims goes somewhere else.
//!
//! Nor is one layout always enough. A surface that floats beside its trigger is laid out once with
//! `visibility: hidden` so that it can be measured before it is placed, and an element that is
//! hidden cannot be focused either. So the ask is recorded here, offered to each frame after
//! layout, and kept until it lands.

use zgui_view::NodeId;
use zgui_view::host::FocusTrapId;

/// How many frames an ask is carried before it is dropped.
///
/// A cap rather than a promise: a trap over a subtree that never becomes focusable — one whose
/// content is entirely disabled, or which is kept hidden by a style sheet — would otherwise ask for
/// a frame, fail, and ask again forever. Eight is well past the two a measured surface takes.
pub const ATTEMPTS: u8 = 8;

/// A trap that has asked for the focus and not been given it yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owed {
    /// Which trap asked.
    pub trap: FocusTrapId,
    /// The subtree the focus is owed to.
    pub root: NodeId,
    /// How many frames have tried to pay it.
    pub attempts: u8,
}

/// The traps waiting to be entered, in the order they were installed.
///
/// The order is the answer to a dialog opened from a dialog in one frame: focus ends up in the
/// innermost one, because entering them oldest-first leaves the last one holding it.
///
/// ```
/// use zgui_runtime::host::Entering;
/// use zgui_view::host::FocusTrapId;
/// use zgui_view::{DocumentId, NodeId};
///
/// let node = |raw| NodeId::new(DocumentId::FIRST, raw).expect("in range");
/// let mut entering = Entering::default();
/// assert!(entering.is_empty());
///
/// entering.owe(FocusTrapId::new(1), node(10));
/// entering.owe(FocusTrapId::new(2), node(20));
/// // A surface that opened and closed inside one frame is owed nothing: there is no longer
/// // anywhere to put the caret, and moving it would take focus off whatever the user was on.
/// entering.forget(FocusTrapId::new(1));
///
/// let owed = entering.take();
/// assert_eq!(owed.len(), 1);
/// assert_eq!(owed[0].root, node(20));
/// assert!(entering.is_empty(), "taking it clears it, so it is paid once");
///
/// // A surface that is still being measured is not focusable yet, and the ask is carried on.
/// assert!(entering.carry(owed[0]));
/// assert!(!entering.is_empty());
/// ```
#[derive(Clone, Debug, Default)]
pub struct Entering {
    /// The asks, oldest first.
    owed: Vec<Owed>,
}

impl Entering {
    /// Records that `trap` over `root` is owed the focus.
    pub fn owe(&mut self, trap: FocusTrapId, root: NodeId) {
        self.owed.push(Owed {
            trap,
            root,
            attempts: 0,
        });
    }

    /// Drops whatever `trap` was owed, because it is no longer installed.
    pub fn forget(&mut self, trap: FocusTrapId) {
        self.owed.retain(|owed| owed.trap != trap);
    }

    /// Takes everything owed, leaving nothing behind.
    pub fn take(&mut self) -> Vec<Owed> {
        core::mem::take(&mut self.owed)
    }

    /// Puts an ask that could not be paid back on the list, and says whether it was kept.
    ///
    /// `false` once it has been offered [`ATTEMPTS`] frames, which is where an ask that will never
    /// be payable stops costing frames.
    pub fn carry(&mut self, owed: Owed) -> bool {
        if owed.attempts + 1 >= ATTEMPTS {
            return false;
        }
        self.owed.push(Owed {
            attempts: owed.attempts + 1,
            ..owed
        });
        true
    }

    /// Whether nothing is owed.
    pub fn is_empty(&self) -> bool {
        self.owed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{ATTEMPTS, Entering};
    use zgui_view::host::FocusTrapId;
    use zgui_view::{DocumentId, NodeId};

    /// A handle to hang an ask off.
    fn node(raw: u64) -> NodeId {
        NodeId::new(DocumentId::FIRST, raw).expect("in range")
    }

    #[test]
    fn traps_are_entered_in_the_order_they_went_up() {
        let mut entering = Entering::default();
        entering.owe(FocusTrapId::new(1), node(1));
        entering.owe(FocusTrapId::new(2), node(2));

        let owed = entering.take();
        assert_eq!(
            owed.iter().map(|owed| owed.root).collect::<Vec<_>>(),
            [node(1), node(2)],
            "the innermost trap has to be entered last, or the outer one takes the caret back",
        );
    }

    #[test]
    fn a_surface_that_closed_before_the_frame_is_owed_nothing() {
        let mut entering = Entering::default();
        entering.owe(FocusTrapId::new(7), node(7));
        entering.forget(FocusTrapId::new(7));
        assert!(entering.take().is_empty());
    }

    #[test]
    fn forgetting_a_trap_leaves_the_others_waiting() {
        let mut entering = Entering::default();
        entering.owe(FocusTrapId::new(1), node(1));
        entering.owe(FocusTrapId::new(2), node(2));
        entering.forget(FocusTrapId::new(1));

        let owed = entering.take();
        assert_eq!(owed.len(), 1);
        assert_eq!(owed[0].trap, FocusTrapId::new(2));
    }

    #[test]
    fn an_ask_that_can_never_be_paid_stops_asking() {
        let mut entering = Entering::default();
        entering.owe(FocusTrapId::new(1), node(1));
        let mut frames = 0;
        while let Some(owed) = entering.take().first().copied() {
            frames += 1;
            if !entering.carry(owed) {
                break;
            }
            assert!(frames < 100, "the ask never gave up");
        }
        assert_eq!(
            u8::try_from(frames).expect("well under a byte"),
            ATTEMPTS,
            "an ask is offered every frame up to the cap and then dropped",
        );
        assert!(entering.is_empty());
    }
}
