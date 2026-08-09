//! The frame loop: take the device, light the displays, and turn until the application stops.
//!
//! This is the backend's only driver. It opens the device, takes DRM master, discovers the
//! displays, gives each one two buffers and a mode, and then turns: read the device, hand the
//! frames that were asked for to the application, ask it how to wait, and wait.
//!
//! # The five ways a turn happens
//!
//! The list is exhaustive on purpose. A missing entry is an application that quietly stops
//! answering one whole class of event.
//!
//! 1. **A display finished a flip.** The device becomes readable, the completion is read, and the
//!    buffer the display had before it is free for the next frame.
//! 2. **Work finished on another thread.** It reaches the parked loop through the wake channel,
//!    which is the second descriptor the wait watches, and arrives as a
//!    [`WakeReason`](zgui_platform::WakeReason).
//! 3. **Somebody pressed a key.** A keyboard's descriptor becomes readable, and every device the
//!    seat took is one more descriptor the wait watches — so the watch set is built per turn and
//!    shrinks with a device that stopped answering. What is read goes to the focused surface.
//! 4. **A surface asked to be drawn.** A request on a console is a flag on the surface and moves no
//!    descriptor, so the loop reads the flags — before it parks as well as after it wakes.
//! 5. **A deadline arrived.** The wait ran to its end, and the moment is reported through
//!    [`AppHandler::deadline_reached`]. It draws nothing by itself. What draws is the request the
//!    application makes while it is being told, and entry 4 picks that up.
//!
//! # What holds the device
//!
//! One process, for as long as the loop runs. Nothing hands the device back on a terminal switch
//! and nothing asks a session daemon for it, so this needs a free virtual terminal or root and
//! fails to start while a compositor holds the device.
//!
//! # What holds the keyboard
//!
//! The same process, and it takes it **after** DRM master. That ordering is the safety interlock:
//! a run on a busy machine fails at the master and never reaches the grab, so it cannot take the
//! keyboard away from the desktop that is using it. See [`crate::input::seat`] for what the grab
//! gives and for the one thing it costs — a grabbed keyboard raises no `SIGINT`, so an application
//! that binds no way out has to be killed from another terminal.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use tracing::warn;
use zgui_drm::Device;
use zgui_drm::commit;
use zgui_platform::{
    AppHandler, Clock, PlatformCx, PlatformError, Surface, SurfaceEvent, SurfaceId, Waker,
};

use crate::clipboard::ConsoleClipboard;
use crate::clock::SystemClock;
use crate::cursor::Cursor;
use crate::cx::DrmCx;
use crate::display::{Displays, DrmDisplay};
use crate::input::pointer::{Pointer, Screen};
use crate::input::seat::{self, Seat};
use crate::output::{self, Output, backend};
use crate::park::{Park, Parked, timeout};
use crate::scanout::{BGRA, Scanout};
use crate::surface;
use crate::waker::EventfdWaker;

/// Runs `handler` on the displays this machine's first usable device drives.
///
/// Blocks until the application asks to finish. This is the driver `App::run_drm` hands to the
/// framework, and the two belong together: a frame reaches a display only through a renderer that
/// draws into this loop's scanouts.
///
/// `displays` is how that renderer finds them. The loop writes each display in under the surface it
/// is seen as, for as long as it turns, so the caller makes the map, gives it to the renderer it
/// installs, and gives the same one to this. A map nothing reads costs one allocation.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when there is no device to open, when this process cannot
/// become DRM master — which is what a compositor holding the device looks like — when a display
/// refuses a buffer or a mode, and when the device stops answering while the loop runs.
pub fn run(handler: Box<dyn AppHandler>, displays: &Displays) -> Result<(), PlatformError> {
    let device = Arc::new(Device::open_first().map_err(backend)?);
    device.become_master().map_err(backend)?;

    let outcome = drive(&device, handler, displays);

    // The device goes back whatever happened above. A process that kept master would leave the
    // console with no way to draw for anybody else until it exits.
    if let Err(error) = device.drop_master() {
        warn!("the device could not be handed back before this process exits: {error}");
    }
    outcome
}

/// Runs everything between taking the device and giving it back.
fn drive(
    device: &Arc<Device>,
    mut handler: Box<dyn AppHandler>,
    displays: &Displays,
) -> Result<(), PlatformError> {
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

    // One cursor per display, and a plane for each that the device can give one to. The list of
    // planes already taken travels with it: a plane drives one CRTC at a time, so a second display
    // given the first one's plane takes the first one's cursor away with nothing reported.
    let mut taken = Vec::new();
    let cursors: Vec<Rc<RefCell<Cursor>>> = outputs
        .iter()
        .map(|output| Rc::new(RefCell::new(Cursor::new(device, output, &mut taken))))
        .collect();

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

    // Written in before the first surface is asked for, because that is when the renderer is built.
    // The renderer is handed a surface and nothing else, so this map says which display that
    // surface is.
    let driving = displays.drive(
        surfaces
            .iter()
            .zip(&scanouts)
            .zip(&cursors)
            .map(|((drawn, scanout), cursor)| {
                (
                    drawn.id(),
                    DrmDisplay::new(
                        Arc::clone(device),
                        Rc::clone(&commit),
                        Rc::clone(scanout),
                        Rc::clone(cursor),
                    ),
                )
            })
            .collect(),
    );

    handler.surfaces_available(&cx);

    // After the master, and only here. A run on a machine where a compositor holds the device has
    // already returned above, so it never reaches the grab and cannot take the keyboard from the
    // desktop that is using it.
    let mut seat = Seat::open(&*clock);
    // Which surface the keys reach, and whether it has been told it has them.
    let mut focused: Option<SurfaceId> = None;
    // Where the pointer is. It starts in the middle of the first display the application claimed,
    // which is nowhere at all until it has claimed one — so the arrangement is rebuilt each turn
    // and the pointer is placed the first time there is somewhere to place it.
    let mut screens: Vec<Screen> = Vec::new();
    let mut pointer = Pointer::centred(&screens);

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

            // Before the frames, so that a key pressed since the last turn is dispatched into the
            // document the frame below then draws. The keyboards are read whether or not anything
            // can be told about them: a descriptor left unread stays ready, and every later wait
            // would return at once.
            //
            // Which surface holds the keys is worked out every turn rather than once, because the
            // answer moves as soon as there is a pointer to move it. Both edges are reported.
            // Losing the keyboard settles a field being typed into and ends a composition, and a
            // surface never told it lost them holds both open for ever.
            let claimed_ids: Vec<SurfaceId> =
                surfaces[..claimed].iter().map(|drawn| drawn.id()).collect();
            // Rebuilt per turn for the same reason the watch set is: a display the application has
            // only just claimed is one more place the pointer may go.
            let arranged = Screen::row(
                surfaces[..claimed]
                    .iter()
                    .map(|drawn| &**drawn as &dyn Surface),
            );
            if arranged != screens {
                let first = screens.is_empty();
                screens = arranged;
                pointer = if first {
                    // The first display the application claimed is where the pointer starts. Until
                    // there was one there was nowhere to start.
                    Pointer::centred(&screens)
                } else {
                    // The ground moved under it, so it is put back inside whatever is left.
                    let (x, y) = pointer.union();
                    Pointer::at(x, y, &screens)
                };
            }
            let holds_keys =
                seat::focused(&claimed_ids, pointer.on(&screens).map(|screen| screen.id));
            if holds_keys != focused {
                if let Some(left) = focused {
                    handler.surface_event(&cx, left, SurfaceEvent::Focused(false));
                }
                if let Some(gained) = holds_keys {
                    handler.surface_event(&cx, gained, SurfaceEvent::Focused(true));
                }
                focused = holds_keys;
            }
            for report in seat.read(&mut pointer, &screens) {
                // A pointer event names the display it happened on; everything else goes to
                // whatever holds the keyboard. The pointer can cross displays inside one turn, so
                // its own answer is the one that is used rather than the focus computed above.
                let Some(id) = report.surface.or(focused) else {
                    continue;
                };
                handler.surface_event(&cx, id, report.event);
            }
            if cx.is_exiting() {
                break;
            }

            // After the input and before the frames. The shape comes from what the runtime asked
            // for while it was being told about the pointer, and the place comes from where that
            // pointer ended up — so both halves are settled by the time a frame is drawn, and a
            // display on the fallback asks for that frame here rather than one turn late.
            //
            // Once a turn rather than once per motion. A cursor commit blocks for up to two
            // refreshes, and a loop that reads flips, deadlines and input on one thread does none
            // of the three while it waits.
            for (drawn, cursor) in surfaces[..claimed].iter().zip(&cursors) {
                let mut cursor = cursor.borrow_mut();
                if let Some(style) = drawn.take_cursor() {
                    cursor.set_style(style);
                }
                cursor.place(
                    pointer
                        .on(&screens)
                        .filter(|screen| screen.id == drawn.id())
                        .map(|screen| {
                            let (x, y) = pointer.union();
                            ((x - screen.left) as i32, y as i32)
                        }),
                );
                if !cursor.changed() {
                    continue;
                }
                if cursor.on_a_plane() {
                    if let Err(error) = cursor.commit(device, &mut **commit.borrow_mut()) {
                        warn!("the pointer could not be put on its plane: {error}");
                    }
                } else {
                    // The whole frame is what carries the pointer here, so the picture under it
                    // has to be drawn again. `settled` is what stops this asking every turn for
                    // the rest of the program.
                    drawn.request_redraw();
                    cursor.settled();
                }
            }

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
            //
            // The moment goes with the turn. The wait below then has no length and always runs to
            // its end, and a moment left installed over one would be reported reached as soon as
            // the poll came back, while the moment itself was still ahead.
            let owed = answered || surfaces[..claimed].iter().any(|drawn| drawn.wants_redraw());
            let parked = if owed { park.handed_back() } else { parked };

            match wait(device, &waker, &seat, parked, clock.now()) {
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
    // is released by value, so what draws goes first and the map it was written into goes next.
    drop(handler);
    drop(driving);
    // Before the device is handed back: removing a framebuffer an enabled plane is scanning out
    // disables that plane, which is a display going dark on shutdown.
    for scanout in scanouts {
        release(scanout, device);
    }
    for cursor in cursors {
        release_cursor(cursor, device);
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

/// Gives one display's cursor buffer back, if nothing that draws still holds it.
///
/// The same rule the scanout follows, for the same reason: the buffer is released by value, so
/// whatever still holds it is reported. The kernel releases it when the device closes, which is
/// moments later.
fn release_cursor(cursor: Rc<RefCell<Cursor>>, device: &Device) {
    match Rc::try_unwrap(cursor) {
        Ok(held) => held.into_inner().release(device),
        Err(_) => warn!(
            "a display's cursor is still held by something that draws, so its buffer stays \
             allocated until the device closes"
        ),
    }
}

/// Waits on the device, the wake channel and every device a person works with, reporting whether
/// the wait ran to its end.
///
/// `false` covers a descriptor with something to report and a signal that cut the wait short. Both
/// mean the same thing to the loop: the wait ended before its moment.
///
/// The watch set is built once per wait, because it is not fixed. Every device the seat took is one
/// more descriptor, and a device that stopped answering is dropped from the seat and leaves the set
/// with it. A set that kept a closed descriptor would fail every later wait.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when the kernel refuses the wait, which is a descriptor the
/// loop can no longer watch.
fn wait(
    device: &Device,
    waker: &EventfdWaker,
    seat: &Seat,
    parked: Parked,
    now: Instant,
) -> Result<bool, PlatformError> {
    let mut watched = vec![
        PollFd::new(device, PollFlags::IN),
        PollFd::new(waker, PollFlags::IN),
    ];
    watched.extend(
        seat.descriptors()
            .map(|keyboard| PollFd::from_borrowed_fd(keyboard, PollFlags::IN)),
    );
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
