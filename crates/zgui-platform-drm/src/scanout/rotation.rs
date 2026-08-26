//! Which buffer a frame is drawn into, and when the frame in it reaches the screen.
//!
//! A display scans out of one buffer while the next frame is written into another. Working out
//! which buffer that is needs no device: it is arithmetic over what each buffer is doing. So it is
//! written here, apart from the card, the graphics device and the commit that carries it out.
//!
//! # A state per buffer
//!
//! [`Slot`] says what one buffer is doing, and a rotation is the list of them. Three of the states
//! are the display's: the buffer it reads, the buffer the flip it has not reported names, and the
//! buffer a finished frame waits in. A buffer in none of them is free, and a frame is drawn into a
//! free one.
//!
//! **Three buffers let a frame be drawn while another is on the screen and a third is waiting.**
//! One buffer is on the screen, one is in a flip, and that leaves one free. So
//! [`Rotation::drawing`] answers a buffer while a flip is still on its way, and acquiring one never
//! waits for the buffer the display is scanning out.
//!
//! # The kernel takes one flip at a time
//!
//! A CRTC holds one page flip, so a frame that finishes while one is outstanding cannot be
//! committed: `drm_atomic_helper_setup_commit` refuses a non-blocking commit with `EBUSY` while a
//! previous one on the same CRTC has not completed. [`Rotation::finished`] holds the frame instead,
//! with the fence the display waits for, and [`Rotation::completed`] hands it back when the flip
//! reports. The frame reaches the screen one vertical blank later, with its own fence and in its
//! own buffer.
//!
//! # A second frame while one is held
//!
//! While a frame is held, [`Rotation::drawing`] answers nothing, so no second frame is ever drawn.
//! The alternative is to hand the held buffer back, draw over it and give it to the display engine
//! again, which puts a newer picture on the screen at the same vertical blank.
//!
//! This refuses, for three reasons. A refused frame stops **in front of** the composition, so it
//! costs nothing at all, and the caller keeps the damage it was going to draw — so the picture that
//! reaches the screen is the same either way. Replacing spends a whole composed frame to move a
//! picture that is under one refresh interval old, as often as the application asks inside that
//! interval. And it would take a buffer back from the display engine that the display never read,
//! which is a pair of ownership transfers over an image nothing looked at.
//!
//! # Two buffers never hold a frame
//!
//! The copied shape has two, and a held frame is a third thing to hold beside the buffer on the
//! screen and the buffer in the flip. So a frame there is refused a buffer while a flip is
//! outstanding. The rule is the same one; there is one fewer buffer for it to reach.

use std::mem;

/// What one buffer is doing.
///
/// A buffer in any of the three the display owns — [`Slot::Held`], [`Slot::Flipping`] and
/// [`Slot::OnScreen`] — is unavailable. At most one buffer is in each of them at a time.
#[derive(Debug, PartialEq, Eq)]
enum Slot<F> {
    /// Nothing needs it, so the next frame is drawn into it.
    Free,
    /// A frame has it and has not reached the driver.
    ///
    /// Covers a buffer a renderer is composing into and one holding a finished frame the caller is
    /// committing. Both mean the same thing to the rotation: the buffer is spoken for, and the
    /// frame that has it is the one that finishes next.
    Drawing,
    /// A finished frame is in it, waiting for the flip on its way to report.
    ///
    /// It carries the fence that frame is committed with, so the two travel together and the frame
    /// reaches the display with the fence its own drawing signals.
    Held(Option<F>),
    /// The flip the device has not reported names it.
    Flipping,
    /// The display is scanning out of it.
    OnScreen,
}

/// A frame the driver has to be given, and what the display waits for before it reads it.
///
/// Answered by the three places a frame becomes committable: it finished with nothing outstanding,
/// the flip it was waiting for reported, or the display is being put back after another session had
/// it. The fence is carried by value, so it goes to the driver with its own frame and closes with
/// it.
#[derive(Debug)]
#[must_use = "a frame that is not committed reaches no screen, and its fence stays open"]
pub(crate) struct Ready<F> {
    /// The buffer it is in.
    pub(crate) slot: usize,
    /// What the display waits for before it reads that buffer, where there is anything to wait for.
    pub(crate) fence: Option<F>,
}

/// The buffers of one display, and which of them a frame may be drawn into.
///
/// `F` is what a finished frame waits on: a sync file on a real display, and anything at all in a
/// test.
#[derive(Debug)]
pub(crate) struct Rotation<F> {
    /// What each buffer is doing, at the index the caller knows it by.
    slots: Vec<Slot<F>>,
}

impl<F> Rotation<F> {
    /// Creates a rotation over `buffers` buffers, with nothing on the screen.
    ///
    /// Every buffer is free, so the first frame goes into the first of them and the modeset puts
    /// that one up.
    pub(crate) fn new(buffers: usize) -> Self {
        Self {
            slots: (0..buffers).map(|_| Slot::Free).collect(),
        }
    }

    /// Returns the buffer the next frame is drawn into, taking a free one where no frame has one
    /// already.
    ///
    /// Answers the same buffer for as long as the frame in it has not been finished, so a frame
    /// that drew nothing leaves the next one where it was.
    ///
    /// Answers nothing when every buffer is the display's: on the screen, named by the flip on its
    /// way, or holding the frame that waits for it. The caller keeps the damage it was going to
    /// draw and asks again.
    pub(crate) fn drawing(&mut self) -> Option<usize> {
        if let Some(slot) = self.drawn() {
            return Some(slot);
        }
        // A frame that is already waiting for the flip is left where it is. See the head of this
        // module for why it is refused here rather than drawn over.
        if self.held().is_some() {
            return None;
        }
        let free = self
            .slots
            .iter()
            .position(|slot| matches!(slot, Slot::Free))?;
        self.slots[free] = Slot::Drawing;
        Some(free)
    }

    /// Returns the buffer a frame has, where one has it.
    ///
    /// Takes nothing, so a caller with a frame to finish is told about the buffer it drew into and
    /// never about a fresh one.
    pub(crate) fn drawn(&self) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| matches!(slot, Slot::Drawing))
    }

    /// Finishes the frame in `slot`, which waits on `fence`.
    ///
    /// Answers it where it can go to the driver now, which is a display with no flip on its way.
    /// Answers nothing where one is outstanding: the frame is held, and
    /// [`Rotation::completed`] hands it back when the completion arrives.
    ///
    /// `slot` is what [`Rotation::drawing`] or [`Rotation::drawn`] answered.
    pub(crate) fn finished(&mut self, slot: usize, fence: Option<F>) -> Option<Ready<F>> {
        if self.flipping().is_none() {
            return Some(Ready { slot, fence });
        }
        self.slots[slot] = Slot::Held(fence);
        None
    }

    /// Reads the completion of this display's flip, and answers the frame that was waiting for it.
    ///
    /// The buffer the flip named is the one the display reads from now, and the buffer it was
    /// reading is free. Answers the held frame where there is one, which the caller commits.
    ///
    /// Answers nothing when nothing was outstanding, which is a completion that names no flip of
    /// this rotation's — a display that has been put back after another session had it is the one
    /// that produces those.
    pub(crate) fn completed(&mut self) -> Option<Ready<F>> {
        let arrived = self.flipping()?;
        self.free_what_the_display_had();
        self.slots[arrived] = Slot::OnScreen;
        self.take_held()
    }

    /// Returns the frame a display coming back puts on the screen, and what it waits on.
    ///
    /// The held frame where there is one, because it is the newest this program drew and the buffer
    /// still holds it. The last frame the driver took otherwise.
    ///
    /// **Any flip from before is forgotten.** Its completion is not coming — the frame it named was
    /// on a CRTC another session then took — and a rotation that kept waiting for one would refuse
    /// a buffer to every frame for the rest of the program.
    ///
    /// Answers nothing for a display the driver has never taken a frame of, which is a run started
    /// on a terminal nobody was looking at.
    pub(crate) fn restores(&mut self) -> Option<Ready<F>> {
        let last = self.last_taken();
        self.free_what_the_display_had();
        if let Some(held) = self.take_held() {
            return Some(held);
        }
        let slot = last?;
        // Left free. Another session has the CRTC, so no buffer of this display's is the display's
        // until the modeset this answers returns — and a modeset the driver refuses leaves this
        // buffer to the next frame, which is where it belongs.
        Some(Ready { slot, fence: None })
    }

    /// Records that the driver took a flip naming `slot`.
    ///
    /// [`Rotation::completed`] reads the completion this leaves outstanding. The buffer the
    /// display is reading stays where it is: it is on the screen until the vertical blank, so
    /// writing into it would tear.
    pub(crate) fn flipped(&mut self, slot: usize) {
        self.slots[slot] = Slot::Flipping;
    }

    /// Records that a modeset put `slot` on the screen.
    ///
    /// A modeset carries no completion event — the call returning says the frame is up — so this
    /// leaves nothing outstanding and the frame after it is committed at once.
    ///
    /// Nothing is freed here, because a modeset happens from one of two states and neither holds a
    /// buffer of the display's: a display that has never been lit, and one
    /// [`Rotation::restores`] has just given every buffer back on.
    pub(crate) fn shown(&mut self, slot: usize) {
        self.slots[slot] = Slot::OnScreen;
    }

    /// Returns the buffer the flip the device has not reported names.
    fn flipping(&self) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| matches!(slot, Slot::Flipping))
    }

    /// Returns the buffer the frame that is waiting for a completion is in.
    fn held(&self) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| matches!(slot, Slot::Held(_)))
    }

    /// Returns the buffer of the last frame the driver took, which is the one a display is put
    /// back with.
    ///
    /// The buffer the flip on its way names, where there is one: the driver has taken that frame
    /// and it is the newest one committed. The buffer on the screen otherwise.
    fn last_taken(&self) -> Option<usize> {
        self.flipping().or_else(|| {
            self.slots
                .iter()
                .position(|slot| matches!(slot, Slot::OnScreen))
        })
    }

    /// Takes the held frame out, leaving its buffer spoken for until the caller commits it.
    ///
    /// [`Slot::Drawing`] rather than [`Slot::Free`], because the frame is in the caller's hands: a
    /// commit the driver refuses leaves that buffer to the next frame, and nothing else may take it
    /// in between. Nothing else is being drawn at the same time — a held frame means the buffer on
    /// the screen, the buffer in the flip and the held one are every buffer there is.
    fn take_held(&mut self) -> Option<Ready<F>> {
        let slot = self.held()?;
        match mem::replace(&mut self.slots[slot], Slot::Drawing) {
            Slot::Held(fence) => Some(Ready { slot, fence }),
            // The line above found a held frame at this place and nothing between the two moves
            // one. Written out rather than unwrapped, because a frame loop is the wrong place to
            // find out that it could have been something else.
            put_back => {
                self.slots[slot] = put_back;
                None
            }
        }
    }

    /// Frees every buffer the display was reading or about to read.
    ///
    /// What a completion and a restore both do to the buffers of the frame before. A frame a caller
    /// holds and a frame waiting for a completion are left alone: neither is the display's.
    fn free_what_the_display_had(&mut self) {
        for slot in &mut self.slots {
            if matches!(slot, Slot::Flipping | Slot::OnScreen) {
                *slot = Slot::Free;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Every decision, over rotations written here.
    //!
    //! No card, no graphics device and no commit. What a rotation answers is arithmetic over what
    //! each buffer is doing, and the orderings that matter — a frame finishing inside a flip
    //! window, two frames inside one, a terminal switch over a frame that never went out — are ones
    //! a machine cannot be asked to produce on demand.
    //!
    //! The fence is a number. What the real one is costs a Vulkan device and says nothing more than
    //! this does about where it goes: [`carried`] is the assertion that a held frame reaches the
    //! driver with its own.

    use super::{Rotation, Slot};

    /// How many buffers the imported shape drives a display from.
    const THREE: usize = 3;

    /// How many the copied shape does.
    const TWO: usize = 2;

    /// A rotation whose frames wait on a number.
    fn rotation(buffers: usize) -> Rotation<u32> {
        Rotation::new(buffers)
    }

    /// The fence the frame drawn into `slot` signals, as a number no other one is.
    fn carried(slot: usize) -> Option<u32> {
        Some(100 + slot as u32)
    }

    /// Asserts the two things that would put a torn or a twice-committed frame on a screen.
    ///
    /// One flip outstanding at a time, because the kernel takes one per CRTC and a second is
    /// refused. And no more than one buffer in each of the other two states the display owns, so a
    /// frame is never drawn into the buffer being scanned out.
    fn holds<F: std::fmt::Debug>(rotation: &Rotation<F>) {
        let count = |kind: fn(&Slot<F>) -> bool| rotation.slots.iter().filter(|s| kind(*s)).count();
        assert!(
            count(|slot| matches!(slot, Slot::Flipping)) <= 1,
            "the kernel takes one flip per CRTC, and this rotation asked for two: {:?}",
            rotation.slots
        );
        assert!(
            count(|slot| matches!(slot, Slot::OnScreen)) <= 1,
            "a display reads one buffer, and this rotation named two: {:?}",
            rotation.slots
        );
        assert!(
            count(|slot| matches!(slot, Slot::Held(_))) <= 1,
            "one frame waits for one flip, and this rotation held two: {:?}",
            rotation.slots
        );
        assert!(
            count(|slot| matches!(slot, Slot::Drawing)) <= 1,
            "one frame is drawn at a time, and this rotation handed out two: {:?}",
            rotation.slots
        );
    }

    /// Draws one frame and hands it to the driver, running `commit` on whatever it answers.
    ///
    /// The whole of what a caller does per frame. Answers what the buffer was and whether the frame
    /// went to the driver rather than being held.
    fn frame(rotation: &mut Rotation<u32>, lit: bool) -> Option<(usize, bool)> {
        let slot = rotation.drawing()?;
        let ready = rotation.finished(slot, carried(slot));
        holds(rotation);
        let Some(ready) = ready else {
            return Some((slot, false));
        };
        assert_eq!(ready.slot, slot, "another buffer's frame was answered");
        assert_eq!(
            ready.fence,
            carried(slot),
            "the frame reached the driver with another frame's fence"
        );
        if lit {
            rotation.flipped(ready.slot);
        } else {
            rotation.shown(ready.slot);
        }
        holds(rotation);
        Some((slot, true))
    }

    /// A three-buffer display showing its first frame, with nothing outstanding.
    ///
    /// The modeset, which every run starts with.
    fn lit() -> Rotation<u32> {
        let mut rotation = rotation(THREE);
        assert_eq!(
            frame(&mut rotation, false),
            Some((0, true)),
            "the first frame goes into the first buffer and the modeset puts it up"
        );
        rotation
    }

    #[test]
    fn the_first_frame_of_a_run_goes_into_the_first_buffer_and_is_committed_at_once() {
        // Nothing is on the screen and nothing is outstanding, so there is nothing to wait for. The
        // commit is a modeset, which carries no completion event, so the frame after it is
        // committed at once as well.
        let mut rotation = lit();

        assert_eq!(
            rotation.drawing(),
            Some(1),
            "the buffer on the screen is never handed to a frame"
        );
        assert_eq!(
            frame(&mut rotation, true),
            Some((1, true)),
            "a modeset leaves no flip on its way, so the second frame flips rather than waiting"
        );
    }

    #[test]
    fn a_frame_arriving_inside_a_flip_window_is_drawn_at_once_and_held() {
        // With one buffer on the screen and one in a flip, the third is free — so this frame is
        // drawn while the flip is still on its way rather than waiting for the vertical blank to
        // begin.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));

        assert_eq!(
            frame(&mut rotation, true),
            Some((2, false)),
            "a buffer was free while a flip was outstanding, and the frame in it waits"
        );
        assert_eq!(
            rotation.slots[0],
            Slot::OnScreen,
            "the display is still reading the frame the modeset put up"
        );
        assert_eq!(rotation.slots[1], Slot::Flipping);
        assert_eq!(rotation.slots[2], Slot::Held(carried(2)));
    }

    #[test]
    fn a_completion_with_a_frame_held_puts_that_frame_on_the_screen_with_its_own_fence() {
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));
        assert_eq!(frame(&mut rotation, true), Some((2, false)));

        let ready = rotation.completed().expect("the held frame goes out now");

        assert_eq!(ready.slot, 2, "the buffer the held frame was drawn into");
        assert_eq!(
            ready.fence,
            carried(2),
            "a held frame carries the fence its own drawing signals, and this one carried \
             whatever flipped last"
        );
        assert_eq!(
            rotation.slots[1],
            Slot::OnScreen,
            "the buffer the flip named is the one the display reads now"
        );
        assert_eq!(
            rotation.slots[0],
            Slot::Free,
            "and the buffer it was reading is free"
        );
        rotation.flipped(ready.slot);
        holds(&rotation);
    }

    #[test]
    fn a_completion_with_nothing_held_frees_the_buffer_that_was_on_the_screen() {
        // The ordinary completion. Nothing is committed, and the two buffers the display had swap.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));

        assert!(
            rotation.completed().is_none(),
            "no frame was waiting, so nothing goes to the driver"
        );

        assert_eq!(rotation.slots[1], Slot::OnScreen);
        assert_eq!(rotation.slots[0], Slot::Free);
        assert_eq!(rotation.slots[2], Slot::Free);
        holds(&rotation);
    }

    #[test]
    fn a_completion_naming_no_flip_of_this_rotations_does_nothing() {
        // What a display that has been put back after another session had it reads. The flip from
        // before the switch was forgotten, so a completion arriving late names nothing here — and
        // one that moved the buffers anyway would take the buffer on the screen away from the
        // display and hand it to the next frame, which is a frame drawn over the picture a person
        // is looking at.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));
        let ready = rotation
            .restores()
            .expect("this display has a frame to put back");
        rotation.shown(ready.slot);

        assert!(
            rotation.completed().is_none(),
            "nothing of this display's is outstanding, so nothing goes to the driver"
        );

        assert_eq!(
            rotation.slots[1],
            Slot::OnScreen,
            "the buffer on the screen stayed on the screen"
        );
        assert_eq!(rotation.drawing(), Some(0));
        holds(&rotation);
    }

    #[test]
    fn a_second_frame_finding_one_held_is_refused_a_buffer() {
        // The decision this module states: the held frame is left where it is. The caller keeps the
        // damage it was going to draw, so the picture that reaches the screen is the same, and
        // nothing is composed for a frame that would be thrown away.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));
        assert_eq!(frame(&mut rotation, true), Some((2, false)));

        assert_eq!(rotation.drawing(), None, "every buffer is the display's");
        assert_eq!(frame(&mut rotation, true), None);

        assert_eq!(
            rotation.slots[2],
            Slot::Held(carried(2)),
            "the frame that was waiting is the one that goes out"
        );
        holds(&rotation);
    }

    #[test]
    fn two_frames_inside_one_flip_window_put_the_first_of_them_on_the_screen() {
        // The pair from end to end. One frame is drawn and held, the second is refused a buffer,
        // and the completion commits the one that was drawn.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));
        assert_eq!(frame(&mut rotation, true), Some((2, false)));
        assert_eq!(frame(&mut rotation, true), None);

        let ready = rotation.completed().expect("the held frame goes out");
        assert_eq!((ready.slot, ready.fence), (2, carried(2)));
        rotation.flipped(ready.slot);

        assert_eq!(
            rotation.drawing(),
            Some(0),
            "the completion freed the buffer the display had finished with"
        );
        holds(&rotation);
    }

    #[test]
    fn a_frame_that_drew_nothing_leaves_the_next_one_the_buffer_it_took() {
        // A frame that composed nothing gives nothing over, so the buffer stays where the next
        // frame needs it. A rotation that moved on would leave that buffer holding a picture from
        // three frames ago and hand the next frame a different one.
        let mut rotation = lit();

        assert_eq!(rotation.drawing(), Some(1));
        assert_eq!(
            rotation.drawing(),
            Some(1),
            "the buffer a frame has is the buffer the next frame is offered"
        );
        assert_eq!(rotation.drawn(), Some(1));
        holds(&rotation);
    }

    #[test]
    fn a_frame_the_driver_refused_keeps_its_buffer_for_the_next_one() {
        // What a commit that was refused leaves: the frame is not on the screen and its buffer is
        // still the caller's, so the next frame draws over it. A buffer freed here would be handed
        // out twice.
        let mut rotation = lit();
        let slot = rotation.drawing().expect("a buffer is free");
        let ready = rotation
            .finished(slot, carried(slot))
            .expect("nothing is outstanding");

        // The driver refused it, so nothing is recorded about it.
        drop(ready);

        assert_eq!(rotation.drawn(), Some(slot));
        assert_eq!(rotation.drawing(), Some(slot));
        holds(&rotation);
    }

    #[test]
    fn a_restore_with_a_frame_held_puts_that_frame_up_rather_than_the_last_one_committed() {
        // The held frame never went out and it is the newest this program drew, so the display
        // comes back with it. Putting the last committed one up instead would show a picture this
        // program has already replaced, and would strand the held frame's fence.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));
        assert_eq!(frame(&mut rotation, true), Some((2, false)));

        let ready = rotation
            .restores()
            .expect("this display has a frame to put back");

        assert_eq!(ready.slot, 2, "the buffer the held frame is in");
        assert_eq!(
            ready.fence,
            carried(2),
            "and it goes to the driver with its own fence"
        );
        assert_eq!(
            rotation.slots[1],
            Slot::Free,
            "the flip from before was forgotten, because its completion is not coming"
        );
        assert_eq!(rotation.slots[0], Slot::Free);
        rotation.shown(ready.slot);

        assert_eq!(
            rotation.drawing(),
            Some(0),
            "and the display takes frames again"
        );
        holds(&rotation);
    }

    #[test]
    fn a_restore_with_nothing_held_puts_back_the_last_frame_the_driver_took() {
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));

        let ready = rotation
            .restores()
            .expect("this display has a frame to put back");

        assert_eq!(
            (ready.slot, ready.fence),
            (1, None),
            "the buffer the outstanding flip named is the newest frame the driver took"
        );
        assert_eq!(
            rotation.slots[0],
            Slot::Free,
            "the buffer the display was reading is another session's now"
        );
        rotation.shown(ready.slot);
        assert_eq!(rotation.drawing(), Some(0));
        holds(&rotation);
    }

    #[test]
    fn a_restore_the_driver_refused_still_forgot_the_flip_that_is_not_coming() {
        // A rotation that kept waiting for that flip declines every frame for the rest of the
        // program. The flip from before the switch named a CRTC another session then took, so
        // nothing reports it — and the forgetting has to happen where the frames are asked for
        // rather than where the mode is set, because a driver that refuses the mode sets none.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));

        let ready = rotation
            .restores()
            .expect("this display has a frame to put back");
        // The driver refused the mode, so the display is still unlit and nothing was put back.
        drop(ready);

        assert_eq!(
            frame(&mut rotation, false),
            Some((0, true)),
            "the next frame sets the mode rather than waiting for a completion nothing will send"
        );
        assert_eq!(frame(&mut rotation, true), Some((1, true)));
        assert_eq!(frame(&mut rotation, true), Some((2, false)));
        holds(&rotation);
    }

    #[test]
    fn a_restore_over_a_frame_a_caller_is_still_drawing_leaves_that_frame_its_buffer() {
        // A person switched terminal between the acquire and the frame. The buffer the renderer
        // holds is the renderer's, and the display comes back with the last frame the driver took —
        // which is another buffer. Handing the same buffer to both would give one of the two a
        // picture the other is writing.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));
        assert_eq!(rotation.drawing(), Some(2), "a frame is under way");

        let ready = rotation
            .restores()
            .expect("this display has a frame to put back");
        rotation.shown(ready.slot);

        assert_eq!(ready.slot, 1, "the last frame the driver took");
        assert_eq!(
            rotation.drawn(),
            Some(2),
            "the frame under way still has the buffer it was given"
        );
        holds(&rotation);
    }

    #[test]
    fn a_display_the_driver_never_took_a_frame_of_has_nothing_to_put_back() {
        // A run started on a terminal nobody was looking at. Nothing has been committed, so the
        // resume commits nothing and the first present sets the mode with the buffer it would have
        // used at start-up.
        let mut rotation = rotation(THREE);

        assert!(rotation.restores().is_none());

        assert_eq!(rotation.drawing(), Some(0));
    }

    #[test]
    fn three_buffers_come_round_in_order_and_never_hand_out_the_one_on_the_screen() {
        // The steady state of a display that keeps up: one frame drawn per refresh, held while the
        // flip in front of it is on its way, and committed by that flip's completion. What this
        // rules out is a rotation that comes back to a buffer the display is still reading, which
        // is a torn frame that nothing below here reports.
        let mut rotation = lit();
        assert_eq!(frame(&mut rotation, true), Some((1, true)));
        let mut drawn = vec![0, 1];

        for _ in 0..6 {
            let (slot, committed) = frame(&mut rotation, true).expect("a buffer is free");
            assert!(
                !committed,
                "a flip was outstanding, so the frame in buffer {slot} waits for it"
            );
            drawn.push(slot);
            assert_eq!(
                rotation.drawing(),
                None,
                "every buffer is the display's while a frame waits"
            );
            let ready = rotation.completed().expect("the held frame goes out");
            assert_eq!(ready.slot, slot, "another buffer's frame was committed");
            rotation.flipped(ready.slot);
            holds(&rotation);
        }

        assert_eq!(
            drawn,
            [0, 1, 2, 0, 1, 2, 0, 1],
            "the buffers come round rather than sticking on one"
        );
    }

    #[test]
    fn two_buffers_refuse_a_frame_while_a_flip_is_outstanding_rather_than_holding_one() {
        // The copied shape. A held frame is a third thing to hold beside the buffer on the screen
        // and the buffer in the flip, and there are two — so the rule that holds a frame on three
        // buffers refuses one here.
        let mut rotation = rotation(TWO);
        assert_eq!(frame(&mut rotation, false), Some((0, true)));
        assert_eq!(frame(&mut rotation, true), Some((1, true)));

        assert_eq!(
            rotation.drawing(),
            None,
            "one buffer is on the screen and the other is in the flip"
        );
        assert_eq!(
            rotation
                .slots
                .iter()
                .filter(|slot| matches!(slot, Slot::Held(_)))
                .count(),
            0,
            "two buffers held a frame, which leaves nothing to draw the next one into"
        );
        holds(&rotation);

        let ready = rotation.completed();
        assert!(ready.is_none(), "nothing was waiting for that completion");
        assert_eq!(
            rotation.drawing(),
            Some(0),
            "the completion freed the buffer the display had finished with"
        );
    }

    #[test]
    fn two_buffers_come_round_between_the_screen_and_the_flip() {
        let mut rotation = rotation(TWO);
        assert_eq!(frame(&mut rotation, false), Some((0, true)));
        let mut drawn = vec![0];

        for _ in 0..4 {
            let (slot, committed) = frame(&mut rotation, true).expect("a buffer is free");
            assert!(committed, "two buffers never hold a frame");
            drawn.push(slot);
            assert!(
                rotation.completed().is_none(),
                "nothing waits for a completion on two buffers"
            );
            holds(&rotation);
        }

        assert_eq!(drawn, [0, 1, 0, 1, 0]);
    }

    #[test]
    fn a_rotation_over_no_buffer_hands_out_nothing() {
        // Nothing builds one, and a rotation that answered a buffer anyway would name a framebuffer
        // its display does not have.
        let mut rotation = rotation(0);

        assert_eq!(rotation.drawing(), None);
        assert_eq!(rotation.drawn(), None);
        assert!(rotation.completed().is_none());
        assert!(rotation.restores().is_none());
    }
}
