//! How another thread reaches a loop parked on the device.

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::sync::Mutex;

use rustix::event::{EventfdFlags, eventfd};
use rustix::io::Errno;
use zgui_platform::{PlatformError, WakeReason, Waker};

/// What one wake adds to the counter.
const ONE: u64 = 1;

/// How many bytes an eventfd is read and written in.
const WIDTH: usize = 8;

/// A handle that wakes a loop parked on the device.
///
/// A frame loop with no compositor under it sleeps in `poll` on two descriptors: the device, which
/// reports a finished page flip, and this one. Everything that is not a page flip arrives here —
/// work finishing on a worker thread, an assistive technology asking for something on its own
/// connection — and the counter this writes ends the sleep.
///
/// One descriptor carries every wake. A wake is an eight-byte write and the kernel adds the counts,
/// so a wake sent to a loop that is already awake costs a write and changes nothing else.
///
/// The reason travels beside the counter. An eventfd carries a number and the loop needs a
/// [`WakeReason`], so the reason is queued and the counter is written afterwards.
///
/// # Draining
///
/// The counter stays above zero until it is read, and a descriptor that stays readable turns every
/// following park into a poll of no length. So the loop calls [`EventfdWaker::drain`] once per
/// wake, which reads the counter back to zero and answers with the reasons.
///
/// ```
/// use zgui_platform::{WakeReason, Waker};
/// use zgui_platform_drm::EventfdWaker;
///
/// let waker = EventfdWaker::new()?;
/// waker.wake(WakeReason::DeviceLost);
/// waker.wake(WakeReason::AppWork);
///
/// assert_eq!(
///     waker.drain().len(),
///     2,
///     "the counter ended the park, and the reasons say what it was for"
/// );
/// assert!(
///     waker.drain().is_empty(),
///     "a drained channel leaves the next park waiting"
/// );
/// # Ok::<(), zgui_platform::PlatformError>(())
/// ```
#[derive(Debug)]
pub struct EventfdWaker {
    /// The descriptor the loop parks on beside the device.
    fd: OwnedFd,
    /// The reasons delivered and not yet drained.
    pending: Mutex<Vec<WakeReason>>,
}

impl EventfdWaker {
    /// Opens a wake channel with nothing pending.
    ///
    /// The descriptor is close-on-exec, so a program this one starts inherits no way into its loop,
    /// and non-blocking, so neither a wake nor a drain can stop the thread that makes it.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformError::Backend`] when the kernel refuses the descriptor, which is a
    /// process that has run out of them.
    pub fn new() -> Result<Self, PlatformError> {
        let fd = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK).map_err(|errno| {
            PlatformError::Backend(format!("cannot open a wake channel: {errno}"))
        })?;
        Ok(Self {
            fd,
            pending: Mutex::new(Vec::new()),
        })
    }

    /// Takes everything delivered since the last call, and clears the wake.
    pub fn drain(&self) -> Vec<WakeReason> {
        // The counter is read before the queue is taken. In that order a wake that arrives between
        // the two is answered this turn and wakes the loop once more, which costs one turn. The
        // other order would clear the counter of a reason still in the queue, and the loop would
        // sleep with work waiting.
        let mut counter = [0_u8; WIDTH];
        match rustix::io::read(&self.fd, &mut counter[..]) {
            // `EAGAIN` is the counter already at zero, which is an ordinary answer: the loop woke
            // for the device and drained the channel as well.
            Ok(_) | Err(Errno::AGAIN | Errno::INTR) => {}
            Err(errno) => {
                tracing::warn!(
                    target: "zgui::platform",
                    "the wake channel could not be cleared: {errno}"
                );
            }
        }
        std::mem::take(&mut *self.pending.lock().expect("the queue is not poisoned"))
    }
}

impl AsFd for EventfdWaker {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

impl Waker for EventfdWaker {
    fn wake(&self, reason: WakeReason) {
        // Queued before the counter is written, so a loop that reads the counter finds the reason
        // behind it.
        self.pending
            .lock()
            .expect("the queue is not poisoned")
            .push(reason);
        match rustix::io::write(&self.fd, &ONE.to_ne_bytes()) {
            Ok(_) => {}
            // `EAGAIN` is the counter one short of its largest value, so a loop that has not
            // drained in about eighteen quintillion wakes is already awake and the reason is
            // already queued.
            Err(Errno::AGAIN | Errno::INTR) => {}
            Err(errno) => {
                tracing::debug!(
                    target: "zgui::platform",
                    "a wake reached a channel that is closing: {errno}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EventfdWaker;
    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    use rustix::io::Errno;
    use std::sync::Arc;
    use zgui_platform::{SurfaceId, WakeReason, Waker};

    /// How long a park in these tests waits before it reports that nothing happened.
    const TIMEOUT: Timespec = Timespec {
        tv_sec: 0,
        tv_nsec: 50_000_000,
    };

    /// Returns `true` if a real park on the waker ends before the timeout.
    ///
    /// The park is the behaviour under test. A check on the queue alone would pass for a waker that
    /// never writes the counter and for one that never clears it, which are the two ways this can
    /// be wrong.
    fn wakes(waker: &EventfdWaker) -> bool {
        let mut watched = [PollFd::new(waker, PollFlags::IN)];
        loop {
            match poll(&mut watched, Some(&TIMEOUT)) {
                Ok(ready) => return ready > 0,
                Err(Errno::INTR) => continue,
                Err(errno) => panic!("the park failed: {errno}"),
            }
        }
    }

    #[test]
    fn a_wake_ends_a_park_that_would_otherwise_wait() {
        let waker = EventfdWaker::new().expect("a wake channel is openable");
        assert!(
            !wakes(&waker),
            "a channel nothing has written leaves the loop parked"
        );

        waker.wake(WakeReason::DeviceLost);
        assert!(wakes(&waker), "a wake ends the park");
    }

    #[test]
    fn a_drained_channel_parks_again_rather_than_spinning() {
        let waker = EventfdWaker::new().expect("a wake channel is openable");
        waker.wake(WakeReason::ColorSchemeChanged);
        assert!(wakes(&waker));

        let delivered = waker.drain();
        assert_eq!(delivered.len(), 1);
        assert!(
            !wakes(&waker),
            "a drained channel is no longer readable, so the next park waits"
        );
    }

    #[test]
    fn every_wake_between_two_drains_arrives_with_the_surfaces_it_belongs_to() {
        let waker = EventfdWaker::new().expect("a wake channel is openable");
        waker.wake(WakeReason::DeviceLost);
        waker.wake(WakeReason::ReactiveWork {
            surfaces: Box::from([SurfaceId::new(2)]),
        });

        let delivered = waker.drain();
        assert_eq!(delivered.len(), 2);
        assert_eq!(delivered[1].surfaces(), [SurfaceId::new(2)]);
        assert!(waker.drain().is_empty(), "a second drain finds nothing");
    }

    #[test]
    fn a_wake_from_another_thread_reaches_the_channel() {
        let waker = Arc::new(EventfdWaker::new().expect("a wake channel is openable"));
        let sent: Arc<dyn Waker> = Arc::clone(&waker) as Arc<dyn Waker>;
        std::thread::spawn(move || sent.wake(WakeReason::AppWork))
            .join()
            .expect("the thread finished");

        assert!(wakes(&waker));
        assert_eq!(waker.drain().len(), 1);
    }
}
