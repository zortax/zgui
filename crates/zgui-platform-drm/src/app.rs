//! The frame loop: take the device, light the displays, and turn until the application stops.
//!
//! The driver, and the only one this backend has. It opens the device, takes DRM master, discovers
//! the displays, gives each one its buffers, and then turns: read the device, hand the frames that
//! were asked for to the application, ask it how to wait, and wait.
//!
//! Which shape those buffers take is settled here, once per display, and it needs the graphics
//! device the renderer will draw on — so the caller opens that device first and hands it in. See
//! [`run`].
//!
//! # The seven ways a turn happens
//!
//! The list is exhaustive on purpose. A missing entry is an application that quietly stops
//! answering one whole class of event.
//!
//! 1. **A display finished a flip.** The device becomes readable, the completion is read, and the
//!    buffer the display had before the flip is free for the next frame.
//! 2. **Work finished on another thread.** It reaches the parked loop through the wake channel,
//!    which is the second descriptor the wait watches, and arrives as a
//!    [`WakeReason`](zgui_platform::WakeReason).
//! 3. **Somebody pressed a key, or moved the pointer.** A device's descriptor becomes readable,
//!    and every device the seat took is one more descriptor the wait watches — so the watch set is
//!    built per turn and shrinks with a device that stopped answering. A key goes to the focused
//!    surface, and a pointer event goes to the display the pointer is on — which is the display
//!    that decides the focused surface.
//! 4. **Somebody plugged a device in.** The seat's watch on the device directory is one more
//!    descriptor in the same set, and a node made there ends the wait. The device is opened,
//!    grabbed and read from the next turn on. Nothing is dispatched for the arrival itself, beyond
//!    a change in the held set where the device already had a modifier under a finger.
//! 5. **A surface asked to be drawn.** A request on a console is a flag on the surface and moves no
//!    descriptor, so the loop reads the flags — before it parks as well as after it wakes.
//! 6. **A deadline arrived.** The wait ran to its end, and the moment is reported through
//!    [`AppHandler::deadline_reached`]. It draws nothing by itself. What draws is the request the
//!    application makes while it is being told, and entry 5 picks that up.
//! 7. **The session changed hands.** A person switched terminal, so the session daemon's own
//!    descriptor — the third descriptor the wait watches, where this run has one — has a change to
//!    report. It is read at the top of the turn, before anything else, because it moves the
//!    devices, DRM master and the terminal, and everything below it is then looking at a different
//!    machine.
//!
//! # What holds the device
//!
//! One process, for as long as the loop runs. [`crate::session`] is where the device comes from,
//! and it has two shapes: a session daemon opens the card and hands it over, which needs no
//! privilege at all, or this process opens it and takes DRM master, which needs root or a free
//! virtual terminal. A run started inside a desktop's own session gets the second shape, because a
//! session that already has a controlling client is refused the seat, and DRM master is what
//! refuses the run there.
//!
//! The session is also what gives everything back — the console, the master and every device the
//! seat opened — so this loop opens one, holds it for longer than the card, and pairs nothing up
//! itself.
//!
//! # Going away, and coming back
//!
//! A seated run gives its devices up when a person switches to another terminal. The
//! `session::presence` module is what a turn's worth of changes becomes, and this loop is what
//! carries the answer out.
//!
//! **Going away.** Every claimed surface is told it is occluded, every input device goes back
//! through the seat, and nothing is committed. There is no window to finish a frame in: the
//! terminal has already moved, DRM master has already gone and every input descriptor already
//! answers `ENODEV` by the time the change is read.
//!
//! **Coming back.** The input devices are opened again, every display is put back into its mode
//! with the buffer it last presented, every cursor goes back on its plane, and every surface is
//! asked for a frame and told it is visible again. The buffers are this process's own and still
//! hold the last frame, so the picture is back at the commit rather than at the frame after it.
//!
//! While the session is away the loop turns and commits nothing. Redraw requests are left where
//! they are, so the frames that were asked for are drawn on the way back.
//!
//! # What holds the screen
//!
//! The console is put into graphics mode after the device is taken and back into text mode before
//! it is given up, so the kernel's own text console stops drawing over the picture and redraws
//! when the program stops. [`crate::console`] is that pair of calls and says where their scope
//! ends: it is two ioctls, and it is not terminal switching.
//!
//! Both calls belong to [`crate::session`], because the session takes the card: a direct run takes
//! the screen along with the master and gives the two back together. A seated run makes neither
//! call, because the session daemon put the terminal into graphics mode when it granted control.
//!
//! # What holds the keyboard and the mouse
//!
//! The same process, and it takes both **after** the device. [`crate::session`] is where each one
//! comes from, the way the card does: a seated run is handed every input device by the daemon, and
//! a direct run opens the node itself. That ordering is the safety interlock:
//! a direct run on a busy machine fails at DRM master and never reaches the grab, so it cannot take
//! either away from the desktop that is using them. A machine where a compositor holds the devices
//! reaches that refusal, because a session that already has a controlling client is refused the
//! seat and falls back to the direct shape. See [`crate::input::seat`] for what the grab gives and
//! for the one thing it costs — a grabbed keyboard raises no `SIGINT`, so an application that binds
//! no way out has to be killed from another terminal.
//!
//! The interlock holds on the seated path too, and there it is the daemon's own. A run started on a
//! terminal that is not the live one gets a seat that is open and waiting: logind hands over every
//! evdev node it has already revoked, so the walk below takes none of them and this run grabs
//! nothing at all while another session is on the screen. What it may have arrives with the enable,
//! which is a person switching to this terminal.
//!
//! # What moves the cursor
//!
//! This loop, once a turn. The shape comes from the surface, where the runtime left it, and the
//! place comes from the pointer this loop owns — so the two meet here and nowhere else. A display
//! with a cursor plane is committed to; a display without one is asked for a frame, because there
//! the picture is what carries the pointer. [`crate::cursor`] is where that difference lives.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
use tracing::{info, warn};
use zgui_drm::Device;
use zgui_drm::commit::{self, Commit};
use zgui_platform::{
    AppHandler, Clock, PlatformCx, PlatformError, Surface, SurfaceEvent, SurfaceId, Waker,
};
use zgui_render_wgpu::Gpu;

use crate::clipboard::ConsoleClipboard;
use crate::clock::SystemClock;
use crate::cursor::Cursor;
use crate::cx::DrmCx;
use crate::display::{Displays, DrmDisplay};
use crate::input::pointer::{Pointer, Screen};
use crate::input::seat::{self, Report, Seat};
use crate::output::{self, Output, backend};
use crate::park::{Park, Parked, timeout};
use crate::scanout::{BGRA, Scanout};
use crate::session::Session;
use crate::session::presence::{Presence, Transition};
use crate::surface::{self, DrmSurface};
use crate::waker::EventfdWaker;

/// Runs `handler` on the displays this machine's first usable device drives.
///
/// Blocks until the application asks to finish. This is the driver `App::run_drm` hands to the
/// framework, and the two belong together: a frame reaches a display through a renderer that draws
/// into this loop's scanouts, and through nothing else.
///
/// `displays` is how that renderer finds them. The loop writes each display in under the surface it
/// is seen as, for as long as it turns, so the caller makes the map, gives it to the renderer it
/// installs, and gives the same one to this. A map nothing reads costs one allocation.
///
/// `gpu` is the graphics device that renderer will draw on, and it decides how a frame reaches a
/// screen. With one, every display that can take the imported shape gets images the renderer
/// composes straight into. With `None`, every display keeps the copied shape — a machine with no
/// usable graphics device and a machine whose driver refused the Vulkan device extensions both
/// arrive here that way. The device has to be **the same one** the renderer draws on: the images
/// belong to it, and a set made on another device is refused much later by the renderer.
///
/// ```no_run
/// use zgui_platform::{
///     AppHandler, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
/// };
/// use zgui_platform_drm::{Displays, run};
///
/// struct Console;
///
/// impl AppHandler for Console {
///     fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
///         cx.create_surface(&SurfaceAttributes::new("console"))
///             .expect("a console hands out a display it already has");
///
///         assert_eq!(
///             cx.surfaces().len(),
///             1,
///             "the loop draws the displays that were claimed, and no others"
///         );
///     }
///
///     fn surface_event(&mut self, _cx: &dyn PlatformCx, _on: SurfaceId, _event: SurfaceEvent) {}
///
///     fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {}
/// }
///
/// // One map. The renderer reads it, and this loop writes into it for as long as it turns.
/// let displays = Displays::new();
/// run(Box::new(Console), &displays, None)?;
/// # Ok::<(), zgui_platform::PlatformError>(())
/// ```
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when there is no device to open, when a direct run cannot
/// become DRM master — a compositor holding the device looks like that — when a display refuses a
/// buffer or a mode, and when the device stops answering while the loop runs.
pub fn run(
    handler: Box<dyn AppHandler>,
    displays: &Displays,
    gpu: Option<&Gpu>,
) -> Result<(), PlatformError> {
    // The session is what took the card, so it is what gives back everything the card cost: the
    // console's screen, the master a direct run took, and every device the seat opened. That
    // happens whatever the loop below did, including an error return and a panic.
    let mut session = Session::open();
    let device = session.card()?;

    // The two names on the card go in this order: `device` is the loop's, and it is dropped at the
    // end of this function, and `session` — which holds the other one, and the master taken over
    // it — after it.
    drive(&device, &mut session, handler, displays, gpu)
}

/// Runs everything between taking the device and giving it back.
fn drive(
    device: &Arc<Device>,
    session: &mut Session,
    mut handler: Box<dyn AppHandler>,
    displays: &Displays,
    gpu: Option<&Gpu>,
) -> Result<(), PlatformError> {
    let outputs = Output::discover(device)?;
    let monitors = output::describe(&outputs);

    // One commit for the device, and one only. An atomic commit caches every object's properties
    // and the mode blob of every CRTC it has set: a second one would read the properties again, and
    // would leak the blob of every mode it set when it went. The renderers share it, because a flip
    // is what they ask for.
    let commit = Rc::new(RefCell::new(commit::for_device(device)));

    // One cursor per display, and a plane for each that the device can give one to. The list of
    // planes already taken travels with it: a plane drives one CRTC at a time, so a second display
    // given the first one's plane takes the first one's cursor away with nothing reported.
    //
    // Before the buffers, because whether the display engine composites the pointer decides which
    // shape those buffers take: a display whose frames carry the pointer keeps the copied shape,
    // since nothing can draw a pointer into a tiled image from the processor.
    let mut taken = Vec::new();
    let cursors: Vec<Rc<RefCell<Cursor>>> = outputs
        .iter()
        .map(|output| Rc::new(RefCell::new(Cursor::new(device, output, &mut taken))))
        .collect();

    let mut scanouts: Vec<Rc<RefCell<Scanout>>> = Vec::with_capacity(outputs.len());
    for (output, cursor) in outputs.iter().zip(&cursors) {
        // Without a graphics device there is nothing to make images on, so every display copies.
        let made = match gpu {
            Some(gpu) => {
                Scanout::for_display(device, output, gpu, cursor.borrow().on_a_plane(), BGRA)
            }
            None => Scanout::copied(device, output, BGRA),
        };
        match made {
            Ok(scanout) => scanouts.push(Rc::new(RefCell::new(scanout))),
            Err(error) => {
                for scanout in scanouts {
                    release(scanout, device);
                }
                for cursor in cursors {
                    release_cursor(cursor, device);
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
    // desktop that is using it. The devices coming from the session leaves that route as it was:
    // `run` asks for the card first, and a seated run whose seat was refused or said nothing has
    // already fallen back to the direct shape and failed there.
    //
    // What the session adds is a second gate on the seated path. Every device below is opened
    // through it, so which of them this run may have is the daemon's answer rather than this
    // process's permission. A run whose terminal is not the live one is handed nodes the daemon has
    // already revoked, so it takes none of them and grabs nothing at all.
    let mut seat = Seat::open(session, &*clock);
    // Which surface the keys reach, and whether it has been told it has them.
    let mut focused: Option<SurfaceId> = None;
    // Where the pointer is. It starts in the middle of the first display the application claimed,
    // which is nowhere at all until it has claimed one — so the arrangement is rebuilt each turn
    // and the pointer is placed the first time there is somewhere to place it.
    let mut screens: Vec<Screen> = Vec::new();
    let mut pointer = Pointer::centred(&screens);
    // Whether this session holds the screen. A run starts holding it on both shapes: a direct run
    // is never told otherwise, and a seated one whose terminal is not the live one is told on its
    // first turn, by the change its seat left in the queue.
    let mut presence = Presence::holding();

    let outcome: Result<(), PlatformError> = 'running: {
        let mut park = Park::new();
        while !cx.is_exiting() {
            // The queue first, because a transition moves the devices, DRM master and the terminal
            // — so everything below it in this turn is looking at a different machine.
            let changes = match session.dispatch() {
                Ok(changes) => changes,
                Err(error) => break 'running Err(error),
            };
            match presence.turn(&changes) {
                Some(Transition::Suspend) => suspend(
                    &mut *handler,
                    &cx,
                    &surfaces[..cx.claimed()],
                    &mut seat,
                    session,
                    &pointer,
                    &screens,
                    focused,
                ),
                Some(Transition::Resume) => resume(
                    &mut *handler,
                    &cx,
                    &surfaces[..cx.claimed()],
                    &mut seat,
                    session,
                    device,
                    &commit,
                    &scanouts,
                    &cursors,
                    focused,
                ),
                None => {}
            }

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
            // Every one of the three below belongs to a session that holds its devices. While
            // another session has the screen there is nothing to read — the descriptors went back
            // with the devices — nothing to commit, and no frame that could reach a display. The
            // requests stay where they are, so the resume draws them.
            if presence.is_active() {
                // Read before the focus is worked out and before anything is dispatched. Reading
                // moves the pointer, and the pointer decides the focus. Worked out first, a key
                // struck in the same turn as a crossing goes to the display the pointer has just
                // left.
                let reports = seat.read(session, &mut pointer, &screens);
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
                deliver(&mut *handler, &cx, reports, focused);
                if cx.is_exiting() {
                    break;
                }

                // After the input and before the frames. The shape comes from what the runtime
                // asked for while it was being told about the pointer, and the place comes from
                // where that pointer ended up — so both halves are settled by the time a frame is
                // drawn, and a display on the fallback asks for that frame here rather than one
                // turn late.
                //
                // Once a turn rather than once per motion. A move is one cheap ioctl that can still
                // wait for an outstanding flip, and a change of shape is a property commit that
                // waits for up to two refreshes — and a loop that reads flips, deadlines and input
                // on one thread does none of the three while it waits. One turn is one wake, so a
                // person moving the mouse gets one move per report all the same.
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
                        // The whole frame carries the pointer here, so the picture under it has to
                        // be drawn again. `asked_for` stops this asking every turn for the rest of
                        // the program.
                        drawn.request_redraw();
                        cursor.asked_for();
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
            }

            let policy = handler.idle(&cx);
            // The clock is read after the application has decided, and therefore later than the
            // reading it decided against: a moment it picked a few microseconds ahead can already
            // be behind this one. Such a moment is paid at once rather than waited for.
            let install = park.install(policy, clock.now());
            let answered = install.overdue().is_some();
            let parked = install.park(|_| handler.deadline_reached(&cx));

            let owed = owes_a_turn(
                presence.is_active(),
                answered,
                surfaces[..claimed].iter().any(|drawn| drawn.wants_redraw()),
            );
            let parked = if owed { park.handed_back() } else { parked };

            match wait(
                device,
                &waker,
                session,
                presence.is_active().then_some(&seat),
                parked,
                clock.now(),
            ) {
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

/// Hands `reports` to the surfaces they belong to.
///
/// A pointer event names the display it happened on; everything else goes to whatever holds the
/// keyboard. A report with neither is one about a display the application has not claimed, and it
/// reaches nothing.
fn deliver(
    handler: &mut dyn AppHandler,
    cx: &DrmCx,
    reports: Vec<Report>,
    focused: Option<SurfaceId>,
) {
    for report in reports {
        let Some(id) = report.surface.or(focused) else {
            continue;
        };
        handler.surface_event(cx, id, report.event);
    }
}

/// Gives the screen up to another session.
///
/// Everything is already gone by the time this runs: the terminal has moved, DRM master has been
/// dropped, and every input descriptor answers `ENODEV`. So there is no frame to finish and no mode
/// to put back — what is left is to say so, and to give the devices back so that the daemon holds no
/// record of them while another session has the screen.
///
/// Nothing is committed. The application's own redraw requests are left where they are by the loop,
/// and the frames they ask for are drawn on the way back.
#[expect(
    clippy::too_many_arguments,
    reason = "this is one step of the loop's turn, written out so that the loop reads as the list \
              of its own steps. Every argument is a distinct thing the loop owns, and grouping them \
              into a struct would name a thing that exists for the length of one call"
)]
fn suspend(
    handler: &mut dyn AppHandler,
    cx: &DrmCx,
    claimed: &[Arc<DrmSurface>],
    seat: &mut Seat,
    session: &mut Session,
    pointer: &Pointer,
    screens: &[Screen],
    focused: Option<SurfaceId>,
) {
    info!(
        target: "zgui::platform",
        "another session has the screen, so this one draws nothing until it comes back"
    );
    for drawn in claimed {
        handler.surface_event(cx, drawn.id(), SurfaceEvent::Occluded(true));
    }
    // After the surfaces are told. What comes back from this is a modifier or a button being let
    // go, and that belongs to a surface which already knows it is not being looked at.
    let reports = seat.let_go(session, pointer, screens);
    deliver(handler, cx, reports, focused);
}

/// Takes the screen back.
///
/// The input devices are opened again, because an evdev descriptor another session took is revoked
/// for good. Every display is put back into its mode with the buffer it last presented, and every
/// cursor goes back on its plane, because the session that had the screen set its own mode on every
/// CRTC and put its own image on every plane.
///
/// **Every surface is damaged in full rather than by what survived.** A frame skipped for occlusion
/// retires its damage, so what the runtime believes is undrawn says nothing about what is on the
/// screen. Telling a surface it is visible again is what marks the whole of it, and the redraw
/// request beside it is what asks for the frame that draws it.
#[expect(
    clippy::too_many_arguments,
    reason = "this is one step of the loop's turn, written out so that the loop reads as the list \
              of its own steps. Every argument is a distinct thing the loop owns, and grouping them \
              into a struct would name a thing that exists for the length of one call"
)]
fn resume(
    handler: &mut dyn AppHandler,
    cx: &DrmCx,
    claimed: &[Arc<DrmSurface>],
    seat: &mut Seat,
    session: &mut Session,
    device: &Device,
    commit: &Rc<RefCell<Box<dyn Commit>>>,
    scanouts: &[Rc<RefCell<Scanout>>],
    cursors: &[Rc<RefCell<Cursor>>],
    focused: Option<SurfaceId>,
) {
    info!(
        target: "zgui::platform",
        "the screen is this session's again, so every display is put back into its mode and every \
         device is opened again"
    );
    let reports = seat.take_again(session);
    deliver(handler, cx, reports, focused);

    for scanout in scanouts {
        let mut committing = commit.borrow_mut();
        if let Err(error) = scanout.borrow_mut().restore(device, &mut **committing) {
            warn!(
                target: "zgui::platform",
                "a display could not be put back after another session had it, so it stays dark \
                 until the frame after this one: {error}"
            );
        }
    }
    for cursor in cursors {
        let mut cursor = cursor.borrow_mut();
        // The plane holds whatever the session that had the screen put on it, so what this cursor
        // believes is up there is worth nothing. Forgotten first, the commit below writes the image
        // instead of moving one that is not there.
        cursor.forget_the_plane();
        let mut committing = commit.borrow_mut();
        if let Err(error) = cursor.commit(device, &mut **committing) {
            warn!(
                target: "zgui::platform",
                "the pointer could not be put back on its plane, so it is drawn into this \
                 display's frames from now on: {error}"
            );
        }
    }
    for drawn in claimed {
        handler.surface_event(cx, drawn.id(), SurfaceEvent::Occluded(false));
        drawn.request_redraw();
    }
}

/// Returns `true` if the loop reads its descriptors and goes round again instead of waiting.
///
/// Nothing on a console wakes a parked loop for a redraw request: a request is a flag on a surface
/// and no descriptor moves. So a frame asked for while the application was being asked how to wait
/// — a deadline that turned into a redraw is the ordinary case — hands the turn back instead of
/// being slept through.
///
/// **A request made while the session is away is held where it is.** No frame reaches a display
/// another session owns, so a turn handed back for one would be answered by a turn that draws
/// nothing and asks again — the loop at the speed of the processor until a person switches back.
/// The request survives, and the resume draws it.
///
/// The moment goes with the turn either way. The wait then has no length and always runs to its end,
/// and a moment left installed over one would be reported reached as soon as the poll came back,
/// while the moment itself was still ahead.
const fn owes_a_turn(active: bool, answered: bool, wants_a_frame: bool) -> bool {
    answered || (active && wants_a_frame)
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

/// Waits on the device, the wake channel, the session daemon and every device a person works with,
/// reporting whether the wait ran to its end.
///
/// `false` covers a descriptor with something to report and a signal that cut the wait short. Both
/// mean the same thing to the loop: the wait ended before its moment.
///
/// The watch set is built once per wait, because it is not fixed. Every device the seat took is one
/// more descriptor, a device plugged in adds one, and a device that stopped answering is dropped
/// from the seat and leaves the set with it. A set that kept a closed descriptor would fail every
/// later wait.
///
/// `seat` is nothing while the session is away. It holds no device then, so what is left of its set
/// is the watch on the device directory — and a node made there while another session has the screen
/// leaves that watch readable with nothing to read it, which would turn every wait into a wait of no
/// length. The watch is read on the way back instead, where the resume has already walked the
/// directory.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when the kernel refuses the wait, which is a descriptor the
/// loop can no longer watch.
fn wait(
    device: &Device,
    waker: &EventfdWaker,
    session: &Session,
    seat: Option<&Seat>,
    parked: Parked,
    now: Instant,
) -> Result<bool, PlatformError> {
    let mut watched = vec![
        PollFd::new(device, PollFlags::IN),
        PollFd::new(waker, PollFlags::IN),
    ];
    watched.extend(
        session
            .descriptor()
            .map(|daemon| PollFd::from_borrowed_fd(daemon, PollFlags::IN)),
    );
    watched.extend(
        seat.into_iter()
            .flat_map(Seat::descriptors)
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

#[cfg(test)]
mod tests {
    //! The one decision in a turn that a device cannot help with, and the one that costs a whole
    //! processor when it is wrong.
    //!
    //! **What is not here.** Everything else this loop does across a terminal switch needs a card:
    //! putting a mode back, putting a cursor on its plane, and the devices a session daemon hands
    //! over and takes away. The transitions themselves are
    //! [`Presence`](crate::session::presence::Presence), which is pure and carries its own tests;
    //! the input half is [`Seat::let_go`] and [`Seat::take_again`], which the seat's own tests hold
    //! to the order a daemon needs. What is left over — a real switch, a real modeset, the picture
    //! coming back — is the hardware run.

    use super::owes_a_turn;

    #[test]
    fn a_frame_that_was_asked_for_hands_the_turn_back() {
        // Nothing on a console wakes a parked loop for a redraw request, so a request made while
        // the application was being asked how to wait would otherwise be slept through.
        assert!(owes_a_turn(true, false, true));
    }

    #[test]
    fn a_turn_with_nothing_owed_waits() {
        assert!(!owes_a_turn(true, false, false));
        assert!(!owes_a_turn(false, false, false));
    }

    #[test]
    fn a_frame_asked_for_while_the_session_is_away_is_left_where_it_is() {
        // The spin this covers: no frame reaches a display another session owns, so a turn handed
        // back for one is answered by a turn that draws nothing and leaves the request set — the
        // loop at the speed of the processor until a person switches back.
        assert!(!owes_a_turn(false, false, true));
    }

    #[test]
    fn a_deadline_that_was_already_owed_hands_the_turn_back_on_either_side_of_a_switch() {
        // A moment the application named has been answered from inside the park, so the frame it
        // asked for is owed whatever the session is. Nothing here stops a timer.
        assert!(owes_a_turn(true, true, false));
        assert!(owes_a_turn(false, true, false));
    }
}
