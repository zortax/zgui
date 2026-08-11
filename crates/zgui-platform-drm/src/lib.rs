//! The platform contract on a raw Linux console: a display, a mode, and a page flip.
//!
//! This is the platform backend for a machine with no display server. It drives `zgui-drm`
//! directly, so the picture a program presents goes to the screen through the kernel and through
//! nothing else. A user interface written against the contract runs here unchanged.
//!
//! # Input
//!
//! **A person can type into an application here, and point at it.** The frame loop opens every
//! device somebody could type on or point with, takes each one away from everything else, and
//! turns what the kernel reports into the events a document is dispatched. `input` holds that
//! translation, all of it.
//!
//! What a key *means* comes from libxkbcommon where the machine has it and from the kernel's own
//! console keymap where it does not, and which of the two a program got is stated once at
//! start-up. Dead keys and compose sequences are applied where libxkbcommon has the data for them,
//! so `´` then `e` inserts `é`.
//!
//! **Which keyboard the machine has is read from the machine.** A console session has no session
//! manager to state it, so `XKB_DEFAULT_LAYOUT` and its siblings, `/etc/vconsole.conf` and
//! `/etc/X11/xorg.conf.d/00-keyboard.conf` are read in that order, and the first that states a
//! layout answers. A machine that states none of them is read from the kernel's console keymap,
//! which is the table its terminal is already typing with — libxkbcommon's own answer there is
//! `us`. The source that answered is part of the line stated at start-up.
//!
//! **The set of devices is not fixed.** A keyboard or a mouse plugged in while the program runs is
//! opened, taken and read from the next turn on, because the loop waits on the device directory
//! beside the devices themselves. One that goes is dropped, and it lets go of every key and every
//! button it was holding — so a modifier held while its keyboard was unplugged comes back up
//! rather than shifting every letter typed afterwards.
//!
//! The pointer's position is this backend's own, because a mouse says how far it moved and never
//! where it is. It can cross between displays, and it stays inside them. Which display holds the
//! keyboard is the display it is over. `cursor` is what a person sees of it: the shapes are drawn
//! in code, and the picture reaches the screen on a hardware cursor plane where the device has one
//! and inside the frame where it does not.
//!
//! A grabbed keyboard costs one thing worth knowing before running anything here: `Ctrl+C` never
//! reaches the terminal's line discipline, so no `SIGINT` is raised. This backend invents no quit
//! key — which key leaves a program is the program's own decision — so an application that binds
//! none has to be killed from another terminal. The pointer is grabbed with it, so a mouse also
//! stops reaching whatever else was reading it.
//!
//! # What this does not have yet
//!
//! * **One pointer, and no touch protocol.** Every device drives the same pointer and every event
//!   reports it as the mouse. A touchscreen and a graphics tablet move it through `ABS_X` and
//!   `ABS_Y`, and the multi-touch codes under `ABS_MT_SLOT` are read by nothing — so two fingers
//!   are one pointer that jumps between them, no event carries a pressure, and a tablet is not
//!   bound to the display it is stuck to.
//! * **The displays are arranged by this backend rather than by the machine.** The kernel says
//!   where none of them is, so the pointer crosses from one to the next left to right in the order
//!   the connectors enumerated — which is not how the monitors sit on the desk unless it happens
//!   to be. There is nothing here to ask: a console has no desktop coordinate space, and every
//!   display reports its position as the origin.
//! * **What a key types depends on which of three layout answers this machine gives.** With
//!   libxkbcommon and its keyboard data, every level of every key is read, and dead keys and
//!   compose sequences work. With the kernel's own console keymap instead, three things go: a
//!   character the console cannot report outside `K_UNICODE` — a German keymap keeps its umlauts,
//!   which are Latin-1, and loses its euro sign; a name for every key that types nothing, so
//!   escape, the arrows and the function keys are named from where they sit; and caps lock and the
//!   meta modifier, which sit outside the eight bits the kernel builds its map index from, so a
//!   shortcut naming meta never matches. With neither source, a key still arrives by its position,
//!   so a binding written against where a key sits keeps working and what the key types is lost.
//! * **No input method.** Dead keys and compose sequences work, because libxkbcommon carries them
//!   and this backend feeds them. What is absent is an input method with a candidate window, so a
//!   Japanese or a Chinese keyboard types no more here than its Latin keys and
//!   `DrmSurface::set_text_input` does nothing.
//! * **No pointer confinement and no pointer lock.** The position is this backend's own and is
//!   already kept inside the union of the displays, so both are close. The contract offers no
//!   method that asks for either, and a pointer event carries a position and no movement, so
//!   neither is declared. `cx` is where that decision is written down.
//! * **No switching on the degraded path.** A machine with no libseat gets the older behaviour
//!   whole: the loop opens the card and takes DRM master itself, which needs a free virtual
//!   terminal or root, and a terminal switch there leaves the program holding the display. A
//!   terminal a key asks for is refused, once and with the reason. **A person sees a dead key.**
//!   The chord reaches no application — a key that asks for a terminal reaches no surface on either
//!   path — and this path holds the console in `KD_GRAPHICS`, so the reason reaches a log file
//!   rather than the screen. A switch there comes from elsewhere: `chvt` over a network connection
//!   is the usual way. A machine that has libseat and a seat that says nothing at all reaches the
//!   same path two seconds later, and holds `TakeControl` on its own terminal — the console
//!   keyboard turned off with it — for that long.
//! * **A held terminal is held until the process exits.** A daemon puts the terminal into
//!   `KD_GRAPHICS` and turns the console keyboard off when it grants control, and gives both back
//!   when the controlling process goes. So a seated program that stops answering leaves a machine
//!   that draws nothing and answers no key, and `Ctrl+Alt+F2` does not recover it.
//!
//! # How a frame reaches the screen
//!
//! `Scanout` owns one display's buffers and rotates through them, and it has two shapes.
//!
//! **Copied.** Two buffers the driver allocated. A frame the renderer read back is copied into
//! whichever is off screen, the flip is asked for, and the completion event says the frame arrived
//! and the other buffer is free again. The copy chooses its fourcc from the channel order of the
//! readback, so a frame is copied rather than swizzled pixel by pixel.
//!
//! **Imported.** Three Vulkan images in a layout the display hardware can read, exported as
//! dma-bufs and registered as framebuffers, that the renderer composes straight into. Neither the
//! readback nor the copy happens at all. A frame there is bracketed by two barriers — one takes the
//! buffer back from the display engine before it is drawn into, one gives it over afterwards — and
//! then the flip follows. `import` makes the images and records the barriers.
//!
//! **What the third buffer is for.** One image is on the screen and one is in the flip, which
//! leaves one to draw into — so a frame asked for while a flip is on its way starts at once. The
//! kernel takes one page flip per CRTC, so a frame that finishes inside that window is held and
//! committed by the completion, with its own fence and in its own buffer. `scanout::rotation` holds
//! that decision, and it is written apart from the card so that it can be asserted with no
//! hardware.
//!
//! **Nothing on that path waits for the graphics device on this thread.** The barrier that gives a
//! frame over signals a semaphore exported as a sync file, and the flip carries that descriptor as
//! the plane's `IN_FENCE_FD`, so the kernel holds the frame back until the device has finished it.
//! A display that can carry no fence — one on the legacy interface, one whose plane has no such
//! property, or a graphics driver that exports no sync file — is waited for here instead, and that
//! is what every frame used to cost.
//!
//! Which shape a display takes is settled once, when it is set up, and written to the log with its
//! reason. `Copied` names the four reasons: a display whose engine composites no pointer keeps the
//! copied shape whatever else it could do, because a pointer is drawn into the frame by the
//! processor and a tiled image is not something the processor can address.
//!
//! On both shapes the mode is set by the first present rather than when the buffers are made. The
//! imported shape cannot set it any earlier — its images belong to a graphics device that exists
//! only once the renderer has been built — and deferring it leaves the console's own text on the
//! screen until there is a frame to replace it with.
//!
//! A window system presents for a caller; a console does not. So the last step belongs to whatever
//! draws, and this crate offers the things that step needs, all of them on `DrmDisplay` except the
//! first: `FORMAT`, the texture a frame is composed into; `DrmDisplay::textures`, which hands out
//! the buffers a renderer composes into and answers nothing on the copied shape, so it says which
//! of the two paths a display is on; `DrmDisplay::present`, which copies a composed frame into the
//! buffer a display is about to scan out of and asks for the flip; `DrmDisplay::acquire` and
//! `DrmDisplay::present_drawn`, which bracket a frame drawn straight into a display's own buffer;
//! and `Displays`, which says which display a surface is. The renderer that uses them lives in
//! `zgui`, because a renderer is built by the runtime and a backend at this layer cannot name the
//! runtime.
//!
//! # The loop
//!
//! `run` is the driver. It opens the session, takes the card from it, lights every display it
//! finds, and then turns: read what the session daemon said, read the completions, draw the frames
//! that were asked for, ask the application how to wait, and wait on the device, the wake channel,
//! the daemon and every input device together. `park` decides the waiting, and it is the same state
//! machine the windowing backend parks with.
//!
//! A seated run also survives a person switching terminal. `session::presence` turns a turn's worth
//! of the daemon's changes into one of three answers — give the devices back, take them again, or
//! carry on — and the loop carries that answer out: the surfaces are told they are occluded and
//! every input device goes back, and on the way in every device is opened again, every display is
//! put back into its mode with the frame it last showed, and every surface is damaged in full and
//! asked to draw. A run started on a terminal that is not the live one is the same machinery from
//! the other end: it waits, and the display lights when somebody switches to it.
//!
//! **A person can start a switch from the keyboard.** The daemon turns the console keyboard off
//! when it grants control and this backend grabs the physical keyboards, so `Ctrl+Alt+F2` reaches
//! this program as a key and reaches the console driver as nothing. Both layouts already say which
//! terminal that chord asks for — it is a keysym of its own under libxkbcommon and a `KT_CONS`
//! entry in a console keymap — so the key is read as terminal 2, the session is asked for it, and
//! the key reaches no surface. Nothing here binds a chord, so an application loses no key it could
//! have had.
//!
//! The console's own mode goes with the card on the direct path: the session puts the terminal
//! into graphics mode as it takes the card, so the kernel's text console stops drawing over the
//! picture, and back into text mode as it hands the card back, so the console redraws. That is two
//! ioctls and no more, and a seated run makes neither — `console` says what the pair does and what
//! it does not.
//!
//! It also writes the displays it lit into the `Displays` it was given, for as long as it turns.
//! That map and the renderer are one decision, so `App::run_drm` makes one map and hands it to
//! both.
//!
//! `run` takes the graphics device beside the map, for the same reason and one more: the images a
//! display scans out of belong to the device the renderer draws on, so that device is opened before
//! the displays exist. A caller that has none hands in nothing and every display copies.
//!
//! # The handles a surface reports
//!
//! A `DrmSurface` carries the native handles a KMS display has: the device descriptor as
//! `DrmDisplayHandle`, and the primary plane as `DrmWindowHandle`. They are the only route to this
//! backend's native state, so a DRM-aware renderer written outside this workspace reaches it
//! through the platform contract and needs no fork of the backend.
//!
//! No graphics API in this workspace's dependency set reads those two variants yet. wgpu answers a
//! DRM handle with "not a Vulkan-compatible handle", which is a true report of where the gap is.
//! `App::run_drm` replaces the renderer factory with one that draws through this backend.

// Every item this doc names is Linux-only and the doc itself is not, so the names above are in
// backticks rather than intra-doc links. A link to a `cfg`-gated item is broken on every other
// platform, and this workspace denies a broken one.

#![deny(missing_docs)]
// This crate is on the unsafe ledger's allowlist for two reasons outside its tests. A surface hands
// out its native handles, and `raw-window-handle`'s `borrow_raw` constructors are unsafe. And
// `import` creates the images a display scans out of directly and submits the barrier that gives a
// drawn one to the display engine, which is Vulkan reached through wgpu's hal: every call into a
// driver is unsafe, and so is handing the finished image back to wgpu. The tests hold a third: an
// `ioctl` declared and called in `input/seat.rs`. Every unsafe block states what makes it sound.
// This backend issues no ioctl of its own outside those tests.
#![allow(unsafe_code)]

// The kernel's display interface exists on Linux and nowhere else, so on any other platform this
// crate is empty rather than broken.
#[cfg(target_os = "linux")]
pub mod app;
#[cfg(target_os = "linux")]
pub mod clipboard;
#[cfg(target_os = "linux")]
pub mod clock;
#[cfg(target_os = "linux")]
pub mod console;
#[cfg(target_os = "linux")]
pub mod cursor;
#[cfg(target_os = "linux")]
pub mod cx;
#[cfg(target_os = "linux")]
pub mod display;
#[cfg(target_os = "linux")]
pub mod import;
#[cfg(target_os = "linux")]
pub mod input;
#[cfg(target_os = "linux")]
pub mod output;
// How the loop waits. Private because nothing outside this crate parks this loop, and the model it
// follows is stated in full in `zgui-platform-winit`'s own `park` module.
#[cfg(target_os = "linux")]
mod park;
#[cfg(target_os = "linux")]
pub mod scanout;
#[cfg(target_os = "linux")]
pub mod session;
#[cfg(target_os = "linux")]
pub mod surface;
#[cfg(target_os = "linux")]
pub mod waker;

#[cfg(target_os = "linux")]
pub use crate::app::run;
#[cfg(target_os = "linux")]
pub use crate::clipboard::ConsoleClipboard;
#[cfg(target_os = "linux")]
pub use crate::clock::SystemClock;
#[cfg(target_os = "linux")]
pub use crate::console::ConsoleScreen;
#[cfg(target_os = "linux")]
pub use crate::cursor::Cursor;
#[cfg(target_os = "linux")]
pub use crate::cx::DrmCx;
#[cfg(target_os = "linux")]
pub use crate::display::{Displays, DrmDisplay};
#[cfg(target_os = "linux")]
pub use crate::import::{EXTENSIONS, Handover, Imported, Offered, Plane, Unsupported};
#[cfg(target_os = "linux")]
pub use crate::output::Output;
#[cfg(target_os = "linux")]
pub use crate::scanout::{Copied, FORMAT, Scanout};
#[cfg(target_os = "linux")]
pub use crate::session::Session;
#[cfg(target_os = "linux")]
pub use crate::surface::DrmSurface;
#[cfg(target_os = "linux")]
pub use crate::waker::EventfdWaker;
