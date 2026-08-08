//! The frame loop: take the device, light the displays, and turn until the application stops.
//!
//! The driver, and the only one this backend has. It opens the device, takes DRM master, discovers
//! the displays, gives each one two buffers and a mode, and then turns: read the device, hand the
//! frames that were asked for to the application, ask it how to wait, and wait.
//!
//! # The four ways a turn happens
//!
//! The list is exhaustive on purpose. A missing entry is an application that quietly stops
//! answering one whole class of event.
//!
//! 1. **A display finished a flip.** The device becomes readable, the completion is read, and the
//!    buffer the display had before the flip is free for the next frame.
//! 2. **Work finished on another thread.** It reaches the parked loop through the wake channel,
//!    which is the second descriptor the wait watches, and arrives as a
//!    [`WakeReason`](zgui_platform::WakeReason).
//! 3. **A surface asked to be drawn.** A request on a console is a flag on the surface and moves no
//!    descriptor, so the loop reads the flags — before it parks as well as after it wakes.
//! 4. **A deadline arrived.** The wait ran to its end, and the moment is reported through
//!    [`AppHandler::deadline_reached`]. It draws nothing by itself. What draws is the request the
//!    application makes while it is being told, and entry 3 picks that up.
//!
//! # What holds the device
//!
//! One process, for as long as the loop runs. Nothing hands the device back on a terminal switch
//! and nothing asks a session daemon for it, so this run needs root or a free virtual terminal and
//! fails to start while a compositor holds the device.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use tracing::warn;
use zgui_drm::Device;
use zgui_drm::commit;
use zgui_platform::{AppHandler, Clock, PlatformCx, PlatformError, Surface, SurfaceEvent, Waker};

use crate::clipboard::ConsoleClipboard;
use crate::clock::SystemClock;
use crate::cx::DrmCx;
use crate::display::{Driving, DrmDisplay};
use crate::graphics::BGRA;
use crate::output::{self, Output, backend};
use crate::park::{Park, Parked, timeout};
use crate::scanout::Scanout;
use crate::surface;
use crate::waker::EventfdWaker;

/// Runs `handler` on the displays this machine's first usable device drives.
///
/// Blocks until the application asks to finish. This is the driver `App::run_drm` hands to the
/// framework, and the two belong together: a frame reaches a display only through the renderer
/// factory that draws into this loop's scanouts.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when there is no device to open, when this process cannot
/// become DRM master — which is what a compositor holding the device looks like — when a display
/// refuses a buffer or a mode, and when the device stops answering while the loop runs.
pub fn run(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let device = Arc::new(Device::open_first().map_err(backend)?);
    device.become_master().map_err(backend)?;

    let outcome = drive(&device, handler);

    // The device goes back whatever happened above. A process that kept master would leave the
    // console with no way to draw for anybody else until it exits.
    if let Err(error) = device.drop_master() {
        warn!("the device could not be handed back before this process exits: {error}");
    }
    outcome
}

/// Runs everything between taking the device and giving it back.
fn drive(device: &Arc<Device>, mut handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let outputs = Output::discover(device)?;
    let monitors = output::describe(&outputs);

    // One commit for the device, and one only. An atomic commit caches every object's properties
    // and the mode blob of every CRTC it has set: a second one would read the properties again, and
    // would leak the blob of every mode it set when it went. The renderers share it, because a flip
    // is what they ask for.
    let commit = Rc::new(RefCell::new(commit::for_device(device)));
    let mut scanouts: Vec<Rc<RefCell<Scanout>>> = Vec::with_capacity(outputs.len());
    for output in &outputs {
        let made = Scanout::new(device, output, &mut **commit.borrow_mut(), BGRA);
        match made {
            Ok(scanout) => scanouts.push(Rc::new(RefCell::new(scanout))),
            Err(error) => {
                for scanout in scanouts {
                    release(scanout, device);
                }
                return Err(error);
            }
        }
    }

    let surfaces = surface::one_per_output(outputs, Arc::clone(device));
    let waker = Arc::new(EventfdWaker::new()?);
    let clock = Arc::new(SystemClock::new());
    let cx = DrmCx::new(
        surfaces
            .iter()
            .map(|surface| Arc::clone(surface) as Arc<dyn Surface>)
            .collect(),
        monitors,
        Arc::clone(&clock) as Arc<dyn Clock>,
        Arc::clone(&waker) as Arc<dyn Waker>,
        ConsoleClipboard::new(Arc::clone(&waker) as Arc<dyn Waker>),
    );

    // Published before the first surface is asked for, because that is when the renderer factory is
    // called: a factory is handed a surface and nothing else, so this is what says which display
    // that surface is.
    let driving = Driving::over(
        surfaces
            .iter()
            .zip(&scanouts)
            .map(|(drawn, scanout)| {
                (
                    drawn.id(),
                    DrmDisplay::new(Arc::clone(device), Rc::clone(&commit), Rc::clone(scanout)),
                )
            })
            .collect(),
    );

    handler.surfaces_available(&cx);

    let outcome: Result<(), PlatformError> = 'running: {
        let mut park = Park::new();
        while !cx.is_exiting() {
            // One read carries the completions of every display that finished, so every scanout is
            // shown the same slice and each keeps the one that names its own CRTC.
            let events = match device.poll_events() {
                Ok(events) => events,
                Err(error) => break 'running Err(backend(error)),
            };
            for scanout in &scanouts {
                scanout.borrow_mut().drain(&events);
            }

            // Only the displays the application claimed. A display nothing asked for is left out of
            // a frame, because a flip of a framebuffer nothing drew into blanks a screen the
            // application never took.
            let claimed = cx.claimed();
            for drawn in &surfaces[..claimed] {
                if drawn.take_redraw() {
                    handler.surface_event(&cx, drawn.id(), SurfaceEvent::RedrawRequested);
                }
            }
            if cx.is_exiting() {
                break;
            }

            let policy = handler.idle(&cx);
            // The clock is read after the application has decided, and therefore later than the
            // reading it decided against: a moment it picked a few microseconds ahead can already
            // be behind this one. Such a moment is paid at once rather than waited for.
            let install = park.install(policy, clock.now());
            let answered = install.overdue().is_some();
            let parked = install.park(|_| handler.deadline_reached(&cx));

            // Nothing on a console wakes a parked loop for a redraw request: a request is a flag on
            // a surface and no descriptor moves. So a frame asked for while the application was
            // being asked how to wait — a deadline that turned into a redraw is the ordinary
            // case — hands the turn back rather than being slept through.
            let owed = answered || surfaces[..claimed].iter().any(|drawn| drawn.wants_redraw());
            let parked = if owed { Parked::Never } else { parked };

            match wait(device, &waker, parked, clock.now()) {
                // The wait ran to its end. Where it carried a deadline, that is the deadline
                // arriving; where it carried none, nothing happened and nothing is reported.
                Ok(true) => {
                    if park.resumed() {
                        handler.deadline_reached(&cx);
                    }
                }
                // Something arrived first, so whatever was installed is no longer what the loop is
                // waiting on. The next turn computes the park again from scratch.
                Ok(false) => park.cancel(),
                Err(error) => break 'running Err(error),
            }

            // Once per turn, whichever descriptor woke the loop. The counter stays above zero until
            // it is read, and a descriptor that stays readable turns every following wait into a
            // wait of no length.
            for reason in waker.drain() {
                handler.wake(&cx, reason);
            }
        }
        Ok(())
    };

    handler.shutting_down(&cx);
    // The application holds the renderers, and a renderer holds the display it draws to. A scanout
    // is released by value, so what draws goes first and what it was published in goes next.
    drop(handler);
    drop(driving);
    // Before the device is handed back: removing a framebuffer an enabled plane is scanning out
    // disables that plane, which is a display going dark on shutdown.
    for scanout in scanouts {
        release(scanout, device);
    }
    outcome
}

/// Gives one display's buffers back, if nothing that draws still holds them.
///
/// A scanout is released by value, because everything it names is dead afterwards. Whatever still
/// holds it was built to draw and was not dropped, and that is reported: the kernel releases the
/// buffers when the device closes, which is moments later.
fn release(scanout: Rc<RefCell<Scanout>>, device: &Device) {
    match Rc::try_unwrap(scanout) {
        Ok(held) => held.into_inner().release(device),
        Err(_) => warn!(
            "a display is still held by something that draws, so its buffers stay allocated until \
             the device closes"
        ),
    }
}

/// Waits on the device and the wake channel, reporting whether the wait ran to its end.
///
/// `false` covers a descriptor with something to report and a signal that cut the wait short. Both
/// mean the same thing to the loop: the wait ended before its moment.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when the kernel refuses the wait, which is a descriptor the
/// loop can no longer watch.
fn wait(
    device: &Device,
    waker: &EventfdWaker,
    parked: Parked,
    now: Instant,
) -> Result<bool, PlatformError> {
    let mut watched = [
        PollFd::new(device, PollFlags::IN),
        PollFd::new(waker, PollFlags::IN),
    ];
    match poll(&mut watched, timeout(parked, now).as_ref()) {
        Ok(ready) => Ok(ready == 0),
        // A signal arrived first. Waiting again here would wait the whole length a second time on
        // top of what has already passed, so the turn is handed back and the park is computed
        // against a clock that has moved.
        Err(Errno::INTR) => Ok(false),
        Err(errno) => Err(PlatformError::Backend(format!(
            "the loop can no longer wait on {}: {errno}",
            device.path().display()
        ))),
    }
}
