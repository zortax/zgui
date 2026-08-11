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
//! 1. **A display finished a flip.** The device becomes readable, the completion is read, the
//!    buffer the display had before it is free for the next frame, and a frame that finished while
//!    the flip was on its way is committed. The kernel takes one page flip per CRTC, so the
//!    completion is the moment the frame waiting behind it becomes legal.
//! 2. **Work finished on another thread.** It reaches the parked loop through the wake channel,
//!    which is the second descriptor the wait watches, and arrives as a
//!    [`WakeReason`](zgui_platform::WakeReason).
//! 3. **Somebody pressed a key, or moved the pointer.** A device's descriptor becomes readable,
//!    and every device the seat took is one more descriptor the wait watches — so the watch set is
//!    built per turn and shrinks with a device that stopped answering. A key goes to the focused
//!    surface, and a pointer event goes to the display the pointer is on — which is the display
//!    that decides the focused surface. **A key that asks for a terminal goes to the session
//!    instead**, and to nothing else: `Ctrl+Alt+F2` is a keysym of its own under libxkbcommon and a
//!    `KT_CONS` entry in a console keymap, so the layout reads it as terminal 2 and `switch` asks
//!    for it.
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
//! through the seat, the surface that held the keyboard is told it lost it, and nothing is
//! committed. There is no window to finish a frame in: the terminal has already moved, DRM master
//! has already gone and every input descriptor already answers `ENODEV` by the time the change is
//! read.
//!
//! **Coming back.** Whatever the seat still holds goes back and every input device is opened again,
//! every display is put back into its mode with the newest frame it drew, every cursor goes back on
//! its plane, and every surface is asked for a frame and told it is visible again. The buffers are
//! this process's own and still hold that frame, so the picture is back at the commit rather than
//! at the frame after it.
//!
//! The give-back on the way back is there for the turn that read a disable and an enable together.
//! That turn is one resume over a seat that never gave anything up, and `resume` below sets it out.
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
//!
//! **A plane goes back before this program asks for another terminal.** Whatever is on a cursor
//! plane stays on it until somebody names that plane in a commit, and a session that draws its own
//! pointer into the frame names it never — so this program's pointer would sit over the next
//! session's picture for the rest of the run. The ask is the last moment this session is active and
//! holds DRM master, so the planes travel with it through
//! [`Heard::answered`](crate::input::seat::Heard::answered). The involuntary path has no such
//! moment: by the time a suspend is read, the terminal has moved and the master has gone.

use std::cell::RefCell;
use std::os::fd::{AsFd, BorrowedFd};
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
use crate::cursor::{self, Cursor};
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
            // What a resume has to say about the devices, held until the focus below has been
            // worked out. The surface that holds the keyboard is the one every key report goes to,
            // and after a suspend there is none: a report delivered here would reach nothing.
            let resumed = match presence.turn(&changes) {
                Some(Transition::Suspend) => {
                    suspend(
                        &mut *handler,
                        &cx,
                        &surfaces[..cx.claimed()],
                        &mut seat,
                        session,
                        &pointer,
                        &screens,
                        &mut focused,
                    );
                    Vec::new()
                }
                Some(Transition::Resume) => {
                    let reports = resume(
                        &mut *handler,
                        &cx,
                        &surfaces[..cx.claimed()],
                        &mut seat,
                        session,
                        &pointer,
                        &screens,
                    );
                    relight(device, &commit, &scanouts, &cursors);
                    reports
                }
                None => Vec::new(),
            };

            // One read carries the completions of every display that finished, so every scanout is
            // shown the same slice and each keeps the one that names its own CRTC.
            //
            // A display commits here as well as reads: a frame that finished while a flip was on
            // its way is put up by the completion of that flip, because the kernel takes one page
            // flip per CRTC. So it runs while this session holds its devices and never while
            // another one has the screen — where a commit is refused and the resume puts every
            // display back anyway.
            let events = match device.poll_events() {
                Ok(events) => events,
                Err(error) => break 'running Err(backend(error)),
            };
            if presence.is_active() {
                for scanout in &scanouts {
                    let mut committing = commit.borrow_mut();
                    if let Err(error) =
                        scanout
                            .borrow_mut()
                            .drain(device, &mut **committing, &events)
                    {
                        warn!(
                            target: "zgui::platform",
                            "the frame a display was holding could not be put on the screen, so it \
                             stays dark until the frame after this one: {error}"
                        );
                    }
                }
            }

            // Only the displays the application claimed. A display nothing asked for is left out of
            // a frame, because a flip of a framebuffer nothing drew into blanks a screen the
            // application never took.
            let claimed = cx.claimed();

            // Which surfaces a key can reach. Worked out every turn rather than once: which
            // surface holds the keys has an answer that moves as soon as there is a pointer to move
            // it. Both edges are reported. Losing the keyboard settles a field being typed into and
            // ends a composition, and a surface never told it lost them holds both open for ever.
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
                let heard = seat.read(session, &mut pointer, &screens);
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
                // A resume's own reports first: they say what is held on the devices it opened, and
                // a key struck since then is read against that.
                deliver(&mut *handler, &cx, resumed, focused);
                // Here, and inside this block. The session holds its devices for the whole of it,
                // so a terminal is asked for while this run owns one. `Heard::answered` is what asks
                // and what hands the reports over, so the two cannot come apart — and a key that
                // asked reaches no surface, so what is delivered below carries nothing for it.
                //
                // The cursor planes go with it. This turn is the last moment a session that is
                // leaving is active and holds DRM master, so it is the only place a plane can be
                // cleared before the next session inherits it.
                let mut planes = OnPlanes {
                    device,
                    commit: &commit,
                    cursors: &cursors,
                };
                deliver(
                    &mut *handler,
                    &cx,
                    heard.answered(session, &mut planes),
                    focused,
                );
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

/// Every display's cursor, for the turn that asks for another terminal.
///
/// The three things one hide takes, borrowed from the loop that owns them. Built per ask rather
/// than kept, because it is a view of what the loop already holds and it lives for the length of
/// one call.
struct OnPlanes<'a> {
    /// The card every plane is on.
    device: &'a Device,
    /// The one commit for that card.
    commit: &'a Rc<RefCell<Box<dyn Commit>>>,
    /// One cursor per display.
    cursors: &'a [Rc<RefCell<Cursor>>],
}

/// Clearing every plane before the ask, and putting them back where the ask went nowhere.
///
/// A refusal is reported and the rest of the displays carry on. The screen is going either way, and
/// a plane the kernel would not clear is one nothing here can do more about. It costs this
/// program's pointer left over the next session's picture, which the give-back exists to prevent.
impl cursor::Planes for OnPlanes<'_> {
    fn give_them_back(&mut self) {
        for cursor in self.cursors {
            let mut cursor = cursor.borrow_mut();
            let mut committing = self.commit.borrow_mut();
            if let Err(error) = cursor.give_the_plane_back(self.device, &mut **committing) {
                warn!(
                    target: "zgui::platform",
                    "a cursor plane could not be cleared before this session asked for another \
                     terminal, so this program's pointer may stay on the screen over the next \
                     session's picture: {error}"
                );
            }
        }
    }

    fn take_them_again(&mut self) {
        for cursor in self.cursors {
            cursor.borrow_mut().take_the_plane_again();
        }
    }
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
///
/// **The keyboard goes with the devices.** `focused` is emptied, and whatever held it is told so,
/// because another session is typing into its own program from here on. A surface left focused
/// keeps a caret blinking and a field unsettled for as long as the switch lasts, and the loop asks
/// the question again on the way back.
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
    focused: &mut Option<SurfaceId>,
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
    deliver(handler, cx, reports, *focused);
    // After those reports, and the ordering carries the whole of the step: a key report names no
    // surface and reaches whatever holds the keyboard, so the releases above would be delivered to
    // nothing if the keyboard were given up first.
    if let Some(left) = focused.take() {
        handler.surface_event(cx, left, SurfaceEvent::Focused(false));
    }
}

/// Takes the devices and the surfaces back, and answers what is held on the devices.
///
/// The input devices are opened again, because `EVIOCREVOKE` cannot be undone and an evdev
/// descriptor another session took stays dead. [`relight`] is the other half — the displays and the
/// cursor planes — and it is apart because it is the half that needs the card.
///
/// # Giving back before opening
///
/// Both arms of [`Transition::Resume`] reach this and the seat holds different things in them. A
/// turn that reads an enable after a suspend holds nothing: the suspend gave every device back. A
/// turn that reads a disable and an enable together — a person holding `Ctrl+Alt+Fn` down — holds
/// **every** device, each revoked by logind before it reported either change.
///
/// So the give-back runs whatever is there. Over an empty list it asks the daemon for nothing. Over
/// a held list every device in it is revoked, so giving it back is right. Left out, the walk that
/// follows asks the session for paths it already holds, is refused every one of them, and the
/// program has no keyboard and no pointer for the rest of the run.
///
/// # What comes back
///
/// The caller delivers it. The reports name no surface, so they reach whichever surface holds the
/// keyboard, and after a suspend that is none. The loop works the focus out further down the same
/// turn and delivers them there.
///
/// **Every surface is damaged in full.** A frame skipped for occlusion retires its damage, so what
/// the runtime believes is undrawn says nothing about what is on the screen. Telling a surface it
/// is visible again marks the whole of it, and the redraw request beside it asks for the frame that
/// draws it.
///
/// **A surface that was never told it was occluded is told it is visible all the same.** On the arm
/// where both changes arrived in one turn there was no suspend, so this is an edge with nothing in
/// front of it — the runtime reads it as the level it already holds and marks no damage. That is
/// the right answer there: no frame was ever skipped for occlusion, so nothing retired its damage,
/// and [`Scanout::restore`] puts the last frame this program presented back on the screen.
fn resume(
    handler: &mut dyn AppHandler,
    cx: &DrmCx,
    claimed: &[Arc<DrmSurface>],
    seat: &mut Seat,
    session: &mut Session,
    pointer: &Pointer,
    screens: &[Screen],
) -> Vec<Report> {
    info!(
        target: "zgui::platform",
        "the screen is this session's again, so every display is put back into its mode and every \
         device is opened again"
    );
    let mut reports = seat.let_go(session, pointer, screens);
    reports.extend(seat.take_again(session));

    for drawn in claimed {
        handler.surface_event(cx, drawn.id(), SurfaceEvent::Occluded(false));
        drawn.request_redraw();
    }
    reports
}

/// Puts every display back into its mode and every cursor back on its plane.
///
/// The half of a resume that needs the card. The session that had the screen set its own mode on
/// every CRTC and put its own image on every plane, so neither is carried across: the mode is set
/// again with the newest frame this display drew, and the cursor is written rather than moved.
///
/// A refusal is reported and the display carries on. Every one of them is repaired by the frame the
/// resume asked for, one refresh later.
fn relight(
    device: &Device,
    commit: &Rc<RefCell<Box<dyn Commit>>>,
    scanouts: &[Rc<RefCell<Scanout>>],
    cursors: &[Rc<RefCell<Cursor>>],
) {
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
        // instead of moving one that is not there, and clears the plane of a display the pointer is
        // on no part of — which would otherwise keep the other session's pointer for the rest of
        // the run.
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
    let mut watched = watched(device.as_fd(), waker.as_fd(), session.descriptor(), seat);
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

/// Returns the descriptors one wait watches, in the order they go in.
///
/// The device and the wake channel are always there. `session` is the session daemon's, and a
/// direct run has none: nothing owns its terminal and a switch reaches it through nothing at all.
/// `seat` adds every input device and the watch on the directory they come from, and it is nothing
/// while another session has the screen.
///
/// Apart from [`wait`] so that what the set holds can be read without a card, a daemon or a
/// terminal. A descriptor missing from here is a class of event that reaches the program late or
/// never, and on the session's own descriptor it is every terminal switch for the rest of the run.
fn watched<'a>(
    device: BorrowedFd<'a>,
    waker: BorrowedFd<'a>,
    session: Option<BorrowedFd<'a>>,
    seat: Option<&'a Seat>,
) -> Vec<PollFd<'a>> {
    let mut watched = vec![
        PollFd::from_borrowed_fd(device, PollFlags::IN),
        PollFd::from_borrowed_fd(waker, PollFlags::IN),
    ];
    watched.extend(session.map(|daemon| PollFd::from_borrowed_fd(daemon, PollFlags::IN)));
    watched.extend(
        seat.into_iter()
            .flat_map(Seat::descriptors)
            .map(|worked_with| PollFd::from_borrowed_fd(worked_with, PollFlags::IN)),
    );
    watched
}

#[cfg(test)]
mod tests {
    //! What a turn decides, over a session that holds no terminal and a seat that holds no device.
    //!
    //! Three of the loop's steps run with no card at all: the decision that costs a whole processor
    //! when it is wrong, the set of descriptors a wait watches, and both halves of what a terminal
    //! switch does to the input devices and to the keyboard. Each of those is here, and each is
    //! asserted through what the session was *asked* for. [`Asked`] records every input call before
    //! it reads the shape, so what a seated run would be asked for, and in which order, is visible
    //! with no daemon and no terminal.
    //!
    //! **What is not here.** [`relight`] and everything under it: putting a mode back, putting a
    //! cursor on its plane, and telling a claimed surface it is visible again — a surface needs the
    //! card that discovered its display. [`OnPlanes`] is the same shape from the other end: what
    //! each cursor records is [`Cursor`]'s own and pure, and hiding one is a request to a driver.
    //! [`Scanout::restore`] clearing an outstanding flip is covered by `tests/scanout.rs`, which
    //! needs DRM master and a monitor. The transitions themselves are
    //! [`Presence`](crate::session::presence::Presence), which is pure and carries its own tests.
    //! The terminal a key asked for is [`Heard::answered`](crate::input::seat::Heard::answered),
    //! where the ask, the planes and the reports are one call — the turn above cannot deliver a
    //! report without making the ask, so there is nothing here for a test to hold up. What is left
    //! over — a real switch, a real modeset, the picture coming back — is the hardware run.

    use std::os::fd::{AsFd, AsRawFd};
    use std::path::{Path, PathBuf};

    use zgui_platform::{PlatformCx, WakeReason};

    use super::{
        Arc, Clock, ConsoleClipboard, DrmCx, EventfdWaker, Pointer, Seat, Session, SurfaceEvent,
        SurfaceId, SystemClock, Waker, owes_a_turn, resume, suspend, watched,
    };
    use crate::session::Asked;

    /// A handler that records what it was told, and answers with nothing.
    #[derive(Default)]
    struct Recording {
        /// Every surface event, in the order it arrived.
        told: Vec<(SurfaceId, SurfaceEvent)>,
    }

    impl super::AppHandler for Recording {
        fn surfaces_available(&mut self, _cx: &dyn PlatformCx) {}

        fn surface_event(&mut self, _cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            self.told.push((surface, event));
        }

        fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {}
    }

    /// Returns a context over no display at all.
    ///
    /// Everything the loop hands a callback is a value something else built, and the device stays
    /// with the loop, so a context with no surfaces in it needs no `/dev/dri`. The two steps below
    /// are written against a test that claims no display: what they do to a surface needs the card,
    /// and what they do to the devices and to the keyboard does not.
    fn over_no_display() -> DrmCx {
        let waker = Arc::new(EventfdWaker::new().expect("this machine makes an eventfd"));
        DrmCx::new(
            Vec::new(),
            Vec::new(),
            Arc::new(SystemClock::new()) as Arc<dyn Clock>,
            Arc::clone(&waker) as Arc<dyn Waker>,
            ConsoleClipboard::new(waker as Arc<dyn Waker>),
        )
    }

    /// A directory a test makes nodes in, removed when it goes out of scope.
    ///
    /// Named after the test that asked for it, so two tests running at once do not share one. Its
    /// own rather than the seat module's, because a helper shared between two `cfg(test)` modules
    /// would have to be reachable from the crate root.
    struct Scratch(PathBuf);

    impl Scratch {
        /// Returns an empty directory of its own.
        fn new(test: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("zgui-drm-app-{}-{test}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("the directory is made");
            Self(root)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Returns a seat over `directory`, holding nothing.
    ///
    /// The directory is the test's own and starts empty, so this grabs no device. A seat opened
    /// over the real one takes every keyboard on the machine and holds it for as long as it lives.
    fn seat_over(directory: &Path, session: &mut Session) -> Seat {
        Seat::open_in(session, &SystemClock::new(), directory)
    }

    #[test]
    fn a_resume_gives_back_what_it_still_holds_before_it_opens_anything() {
        // The defect this covers takes every input device away for the rest of the run. A person
        // holding `Ctrl+Alt+Fn` down leaves a disable and an enable in one turn, which is one
        // resume — and the seat still holds every device, each of them revoked by logind before it
        // reported either change. A resume that only walked would ask the session for paths it
        // already holds, be refused every one of them, take nothing, and leave the program with no
        // keyboard and no pointer.
        let test = "a_resume_gives_back_what_it_still_holds_before_it_opens_anything";
        let root = Scratch::new(test);
        let mut session = Session::direct();
        let mut seat = seat_over(&root.0, &mut session);
        // Made after the seat, so that the only calls recorded below are the resume's own.
        let node = root.0.join("event0");
        std::fs::write(&node, []).expect("the file is made");
        let cx = over_no_display();
        let mut handler = Recording::default();

        let reports = resume(
            &mut handler,
            &cx,
            &[],
            &mut seat,
            &mut session,
            &Pointer::centred(&[]),
            &[],
        );

        assert_eq!(
            session.asked(),
            [Asked::CloseEvery, Asked::Open(node)],
            "every device went back before the directory was walked, and it is the give-back that \
             lets the walk open anything at all"
        );
        assert!(
            reports.is_empty(),
            "a seat with no layout and nothing held has nothing to say: {reports:?}"
        );
    }

    #[test]
    fn a_suspend_gives_the_devices_back_and_gives_the_keyboard_up_with_them() {
        // Two things, and the order between them carries the second. Every device goes back through
        // the session, because dropping one leaves the daemon holding its record of it. And the
        // surface that held the keyboard is told it lost it: another session is being typed into
        // from here on, and a surface left focused keeps a caret blinking and a field unsettled for
        // as long as the switch lasts.
        let test = "a_suspend_gives_the_devices_back_and_gives_the_keyboard_up_with_them";
        let root = Scratch::new(test);
        let mut session = Session::direct();
        let mut seat = seat_over(&root.0, &mut session);
        let cx = over_no_display();
        let mut handler = Recording::default();
        let typed_into = SurfaceId::new(7);
        let mut focused = Some(typed_into);

        suspend(
            &mut handler,
            &cx,
            &[],
            &mut seat,
            &mut session,
            &Pointer::centred(&[]),
            &[],
            &mut focused,
        );

        assert_eq!(
            session.asked(),
            [Asked::CloseEvery],
            "every device this session opened went back to it"
        );
        assert_eq!(focused, None, "and the keyboard is another session's");
        assert!(
            matches!(
                handler.told.as_slice(),
                [(surface, SurfaceEvent::Focused(false))] if *surface == typed_into
            ),
            "so the surface that held it was told, once: {:?}",
            handler.told
        );
    }

    #[test]
    fn the_session_is_one_of_the_descriptors_a_wait_watches() {
        // A terminal switch reaches this program through this descriptor and through nothing else,
        // so a wait that left it out would read a switch only when something else happened to wake
        // the loop — and on an idle console that is never. Nothing else reports it: the input
        // descriptors are revoked without a word and the card stops answering, so the symptom is a
        // program that draws nothing and says nothing.
        let device = EventfdWaker::new().expect("this machine makes an eventfd");
        let waker = EventfdWaker::new().expect("this machine makes an eventfd");
        let daemon = EventfdWaker::new().expect("this machine makes an eventfd");

        let watching = watched(device.as_fd(), waker.as_fd(), Some(daemon.as_fd()), None);

        assert_eq!(
            watching.len(),
            3,
            "the device, the wake channel and the daemon"
        );
        assert_eq!(
            watching[2].as_fd().as_raw_fd(),
            daemon.as_fd().as_raw_fd(),
            "and the daemon's is the third of them, which is what the list of the ways a turn \
             happens says it is"
        );
        assert_eq!(
            watched(device.as_fd(), waker.as_fd(), None, None).len(),
            2,
            "a direct run has no daemon, and nothing owns its terminal"
        );
    }

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
