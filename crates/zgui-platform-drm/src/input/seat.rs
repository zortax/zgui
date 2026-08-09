//! The open devices, the grab, and turning batches into surface events.
//!
//! A seat is the devices one person is working with. It opens every one somebody could type on or
//! point with, takes each away from everything else, pushes the keys that were already down into
//! the layout, and turns what the kernel reports into what a surface is told.
//!
//! # Arrivals and departures
//!
//! The set is not fixed. [`Seat::open`] watches the directory the nodes are in before it walks it,
//! so a device that appears between the two is in one or the other. A node reported afterwards is
//! opened and taken in the same read that reads the devices, and a device that answers a read with
//! a failure is dropped in it.
//!
//! Both edges repair state as well as record it. A device that goes holds nothing, so
//! [`Keys::resynchronise`] takes its keys off the layout and `cancelled` ends the interactions its
//! buttons were holding open. Without them a modifier held while its keyboard was unplugged stays
//! held for the rest of the program, and a control gripped by a vanished mouse never lets go. A
//! device that arrives is asked what is held on it for the same reason, before anything it reports
//! is believed.
//!
//! # One device with two jobs
//!
//! A device is opened once and read once, and what it is read *as* is two independent questions.
//! A wireless receiver presents one node carrying a full key map, two relative axes and a wheel, so
//! that node is a keyboard and a pointer at the same time. Two sets of state hang off one device,
//! because a second open would be a second grab of a descriptor only one client may hold.
//!
//! # The grab
//!
//! A grabbed device reports to this process and to nothing else. Without it every keystroke also
//! reaches the shell behind the console, so somebody typing `reboot` into a text field types it
//! into that shell as well, and the shell runs it when the program exits.
//!
//! **A grabbed keyboard also means `Ctrl+C` never reaches the terminal's line discipline**, so no
//! `SIGINT` is raised and an application with no way out has to be killed from another terminal.
//! This backend invents no quit key: which key leaves a program is the program's own decision, and
//! a backend that chose one would take that key away from every application that wanted it for
//! something else. `examples/tty.rs` binds Escape, which an example has to do to be usable.
//!
//! # Order against DRM master
//!
//! The frame loop takes master first and fails to start while a compositor holds it. That ordering
//! is the interlock: a run on a busy machine cannot take the keyboard from the desktop, because it
//! never reaches the point where it would ask for one. [`Seat::open`] being called from the loop
//! after `become_master` keeps it, and nothing else does.
//!
//! # Which devices are keyboards
//!
//! [`Role::Keyboard`](zgui_evdev::Role) is udev's `ID_INPUT_KEY`, and it is meant to be broad — a
//! remote control is a keyboard under it, so is a gaming mouse that advertises `KEY_MACRO27` and
//! its neighbours, and so is the power button. [`types_on`] asks the narrower question, and it asks
//! for a *letter*: taking a device somebody does not type on removes a function from the session
//! with no way to get it back while the program runs.
//!
//! [`points_with`](crate::input::pointer::points_with) is the same question for the pointer, and
//! its own module says which two directions the broad answer is wrong in.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rustix::fd::{AsFd, BorrowedFd};
use tracing::{info, warn};
use zgui_evdev::{Absolute, Batch, Capabilities, Device, EventType, Key, Synchronisation, Watch};
use zgui_platform::{Clock, SurfaceEvent, SurfaceId};
use zgui_vocab::{KeyState, Modifiers, PointerAction, Timestamp};

use crate::input::keyboard;
use crate::input::keyboard::layout::{Layout, Reading};
use crate::input::keyboard::{code, layout};
use crate::input::pointer::{self, Axes, Pointer, Screen, Span};
use crate::input::wheel::{self, HighResolution};

/// The twenty-six letter positions of a keyboard.
///
/// A code rather than a character: `KEY_A` is where a standard layout puts `A`, and a keyboard set
/// to any layout at all reports these same codes. So this asks whether the *hardware* is a
/// keyboard, and a Russian or a Dvorak one answers yes.
const LETTERS: &[Key] = &[
    Key::KEY_A,
    Key::KEY_B,
    Key::KEY_C,
    Key::KEY_D,
    Key::KEY_E,
    Key::KEY_F,
    Key::KEY_G,
    Key::KEY_H,
    Key::KEY_I,
    Key::KEY_J,
    Key::KEY_K,
    Key::KEY_L,
    Key::KEY_M,
    Key::KEY_N,
    Key::KEY_O,
    Key::KEY_P,
    Key::KEY_Q,
    Key::KEY_R,
    Key::KEY_S,
    Key::KEY_T,
    Key::KEY_U,
    Key::KEY_V,
    Key::KEY_W,
    Key::KEY_X,
    Key::KEY_Y,
    Key::KEY_Z,
];

/// Returns `true` if a person types on this device.
///
/// **The device has to have a letter.** Sitting below `BTN_MISC` is not enough, and the power
/// button proves it: `KEY_POWER` is 116, inside the block a keyboard sends, so a rule written that
/// way takes it. `EVIOCGRAB` then routes the power button to this process alone — `acpid` and
/// `logind` never see it, and this backend does nothing with it — so **pressing power stops working
/// for as long as the program runs**. Beside a grabbed keyboard raising no `SIGINT`, that leaves a
/// machine with no soft way to stop. The same rule refuses a laptop's own hotkey node and a
/// webcam's consumer-control node, which report brightness and camera keys and no letters.
///
/// The trade is deliberate and it runs one way. A device wrongly taken is a function somebody loses
/// with no way to get it back while the program runs. A device wrongly left alone still works for
/// the session, and it costs this program the keys on that device. A numeric keypad on its own is
/// the price: it carries no letter, so it stays with the session.
///
/// ```
/// use zgui_evdev::{Bitmap, Capabilities, EventType, Key};
/// use zgui_platform_drm::input::seat::types_on;
///
/// let keyboard = Capabilities::new(
///     Bitmap::from_codes([EventType::EV_KEY]),
///     Bitmap::from_codes([Key::KEY_ESC, Key::KEY_Q, Key::KEY_LEFTSHIFT]),
///     Bitmap::default(),
///     Bitmap::default(),
/// );
/// let power = Capabilities::new(
///     Bitmap::from_codes([EventType::EV_KEY]),
///     Bitmap::from_codes([Key::KEY_POWER, Key::KEY_SLEEP]),
///     Bitmap::default(),
///     Bitmap::default(),
/// );
///
/// assert!(types_on(&keyboard));
/// assert!(!types_on(&power));
/// ```
pub fn types_on(capabilities: &Capabilities) -> bool {
    capabilities.has(EventType::EV_KEY)
        && LETTERS
            .iter()
            .any(|letter| capabilities.keys().contains(*letter))
}

/// Returns `true` if this code is one a person typed.
///
/// A button belongs to the pointer, which reads the same batch and takes it there instead — see
/// [`pointer::button`]. Everything else in the kernel's three key ranges is delivered, including
/// the blocks it added behind the buttons: a keyboard with media keys sends those and a person
/// pressed them.
fn typed(key: Key) -> bool {
    key.is_key()
}

/// Returns the surface a key press reaches, where there is one.
///
/// A console has no window manager, so focus is a decision this backend makes: **the display the
/// pointer is over**, and the first surface the application claimed while the pointer is over none
/// of them. A pointer that has not been moved yet starts in the middle of the first claimed
/// display, so the two answers agree until somebody moves it.
///
/// `over` is believed only where the application claimed that display. A display it has not asked
/// for draws nothing and is told nothing, so a pointer standing on one leaves the keys where they
/// were.
///
/// A program that claimed no display is told about no key. There is nowhere for one to go.
///
/// ```
/// use zgui_platform::SurfaceId;
/// use zgui_platform_drm::input::seat::focused;
///
/// let claimed = [SurfaceId::new(1), SurfaceId::new(2)];
///
/// assert_eq!(focused(&claimed, Some(SurfaceId::new(2))), Some(SurfaceId::new(2)));
/// assert_eq!(focused(&claimed, None), Some(SurfaceId::new(1)));
/// assert_eq!(
///     focused(&claimed, Some(SurfaceId::new(7))),
///     Some(SurfaceId::new(1)),
///     "a display the application never claimed leaves the keys where they were"
/// );
/// assert_eq!(focused(&[], Some(SurfaceId::new(1))), None);
/// ```
pub fn focused(claimed: &[SurfaceId], over: Option<SurfaceId>) -> Option<SurfaceId> {
    over.filter(|id| claimed.contains(id))
        .or_else(|| claimed.first().copied())
}

/// One thing a person did, and the surface it belongs to.
///
/// A key belongs to whatever holds the keyboard, which is the loop's decision and changes between
/// turns. A pointer event belongs to the display the pointer was on when it happened, which only
/// the pointer knows and which can change inside one turn — a pointer that crosses between two
/// displays in one batch tells one surface it left and the other that it arrived.
#[derive(Debug)]
pub struct Report {
    /// The display it happened on, where it happened on one.
    pub surface: Option<SurfaceId>,
    /// What happened.
    pub event: SurfaceEvent,
}

impl Report {
    /// Creates an event for whichever surface holds the keyboard.
    const fn focused(event: SurfaceEvent) -> Self {
        Self {
            surface: None,
            event,
        }
    }

    /// Creates an event for the display it happened on.
    const fn on(surface: SurfaceId, event: SurfaceEvent) -> Self {
        Self {
            surface: Some(surface),
            event,
        }
    }
}

/// Which way a key moved, as the value the kernel wrote says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transition {
    /// The key went down.
    Pressed,
    /// The key came up.
    Released,
    /// The key is being held, and the kernel is saying so again.
    Repeated,
}

impl Transition {
    /// Returns the transition `value` is.
    ///
    /// The kernel writes `1` for a press, `0` for a release and **`2` for a repeat** on any device
    /// with `EV_REP`. Reading `value != 0` as a press is the defect this exists to refuse: it
    /// records one transition per repeat and one release, so the count never returns to zero and a
    /// held shift stays down for the rest of the run.
    ///
    /// A value the kernel does not write reads as a repeat, which is the answer that records
    /// nothing and therefore cannot unbalance anything.
    const fn of(value: i32) -> Self {
        match value {
            0 => Self::Released,
            1 => Self::Pressed,
            _ => Self::Repeated,
        }
    }
}

/// How one device's events are given a moment.
///
/// The kernel stamps an event when the key moved. A loop that stamped when it woke instead would
/// give every event in one wake the same moment, and a double click and a key repeat are measured
/// against the difference between two of them.
///
/// [`Device::has_monotonic_timestamps`](zgui_evdev::Device::has_monotonic_timestamps) chooses
/// between the two, and it has to be asked. `zgui-evdev` requests `CLOCK_MONOTONIC` for every
/// device it opens and a driver may refuse, which leaves that device's stream on `CLOCK_REALTIME` —
/// around 1.75e9 seconds. Anchoring one of those against a few hours of uptime stamps every key
/// some fifty-five years after the application started, beside frame stamps measured in seconds,
/// and nothing downstream could tell.
///
/// ```
/// use std::time::Duration;
/// use zgui_platform_drm::input::seat::Stamps;
///
/// // What the kernel's clock read when the frame loop started.
/// let stamps = Stamps::from_origin(Duration::from_secs(1_000));
///
/// assert_eq!(
///     stamps.at(Duration::from_secs(1_002)).since_origin(),
///     Duration::from_secs(2)
/// );
/// // A device can report a moment from a hair before the anchor was taken.
/// assert_eq!(
///     stamps.at(Duration::from_secs(999)).since_origin(),
///     Duration::ZERO
/// );
/// ```
#[derive(Clone, Copy, Debug)]
pub enum Stamps {
    /// The kernel's own moment, anchored to the frame loop's origin.
    ///
    /// One reading of the kernel's clock taken beside one reading of the loop's puts the two on the
    /// same zero. Both run at the same rate, so the anchor holds for as long as the program does.
    Monotonic {
        /// What the kernel's clock read at the loop's own origin.
        origin: Duration,
    },
    /// The moment the loop read them, for a device whose stream is on the wall clock.
    ///
    /// Coarser: every event read in one wake shares it. That is what the driver's refusal costs.
    /// The moment can still be compared with the frame it arrived in, and a wall-clock moment
    /// cannot.
    Read(Timestamp),
}

impl Stamps {
    /// Returns the anchor between the two clocks, read now.
    pub fn anchored(clock: &dyn Clock) -> Self {
        Self::Monotonic {
            origin: monotonic().saturating_sub(clock.timestamp().since_origin()),
        }
    }

    /// Returns the anchor a caller states, for a test with no clock in it.
    pub const fn from_origin(origin: Duration) -> Self {
        Self::Monotonic { origin }
    }

    /// Returns these stamps as they read on a turn that is happening at `now`.
    ///
    /// A stream on the wall clock is stamped when the loop read it, and this is where that moment
    /// is taken. One moment kept from when the device was taken would stamp every event that device
    /// ever reports, so a key struck an hour in would arrive dated to start-up and every interval
    /// measured across two of them would be zero.
    ///
    /// The kernel's own moments need no such thing: both clocks run at one rate, so the anchor
    /// holds for as long as the program does.
    pub const fn read_at(self, now: Timestamp) -> Self {
        match self {
            Self::Monotonic { origin } => Self::Monotonic { origin },
            Self::Read(_) => Self::Read(now),
        }
    }

    /// Returns the moment `at` is, in the contract's numbering.
    ///
    /// Saturating, because a device may report an event stamped a hair before the anchor was
    /// taken: the kernel timestamps when the key moved and the anchor is read afterwards.
    ///
    /// On a stream stamped when it was read, `at` is discarded: every event of that read carries
    /// the moment the loop read it.
    pub fn at(self, at: Duration) -> Timestamp {
        match self {
            Self::Monotonic { origin } => Timestamp::from_origin(at.saturating_sub(origin)),
            Self::Read(read) => read,
        }
    }
}

/// Returns what the kernel's monotonic clock reads now.
fn monotonic() -> Duration {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Duration::new(
        u64::try_from(now.tv_sec).unwrap_or(0),
        u32::try_from(now.tv_nsec).unwrap_or(0),
    )
}

/// Returns `true` if the kernel threw part of this update away.
///
/// A client that reads too slowly overruns the kernel's queue, and the kernel then discards that
/// client's whole queue and puts a `SYN_DROPPED` in its place. What arrives next is the tail of an
/// update whose beginning no longer exists, so a key that went down in the discarded part would
/// stay down for the rest of the run.
///
/// A batch that carries one is answered with nothing, and the device is asked what it holds now:
/// [`Keys::batch`] does the first and [`Seat::read`] the second.
fn dropped(batch: &Batch) -> bool {
    batch.events.iter().any(|event| {
        event.kind == EventType::EV_SYN && event.code == Synchronisation::SYN_DROPPED.raw()
    })
}

/// The layout, what it last reported held, and what a batch of events means.
///
/// The whole translation, and it holds no device, so every part of it can be exercised over bytes
/// written by hand.
///
/// # One set of held keys per device
///
/// Every method here takes the calling device's own `down` set. One layout serves every keyboard
/// this seat holds, and libxkbcommon counts a modifier's transitions — so shift held on two
/// keyboards is two transitions and needs two releases. A set shared between them would hold one
/// code, repair one, and leave the count stuck at one with nothing holding the key.
pub struct Keys {
    /// What a key means, or nothing on a machine with no layout source at all.
    ///
    /// With no layout a press still reaches a document: the position is the kernel's own and needs
    /// no layout, so a binding written against where a key sits keeps working and only what the
    /// key *types* is lost.
    layout: Option<Box<dyn Layout>>,
    /// The held set as it was last reported, so a change is announced once.
    modifiers: Modifiers,
}

impl Keys {
    /// Creates a translation over `layout`.
    pub fn new(layout: Option<Box<dyn Layout>>) -> Self {
        Self {
            layout,
            modifiers: Modifiers::NONE,
        }
    }

    /// Returns the modifiers held, as this translation last reported them.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Puts one device's keys back in step with what the kernel says it has down.
    ///
    /// Three moments need this and it is the same repair in all three.
    ///
    /// * **A device was just taken.** `down` is empty and `held` is what `EVIOCGKEY` reported: a
    ///   modifier held before this process was listening is in the kernel's own map and in no
    ///   event, so without this it stays invisible until it is released and pressed again.
    /// * **The kernel dropped part of an update.** Its rule is to discard everything up to the next
    ///   `SYN_REPORT` — which [`Keys::batch`] does by answering with nothing — and then to ask the
    ///   device what its state is now.
    /// * **A device stopped answering.** `held` is then empty, because a device that is gone holds
    ///   nothing. The releases the kernel queued for it are never read, so without this a modifier
    ///   held while it was unplugged stays held for the rest of the process, and every later key
    ///   struck on another keyboard comes out shifted with no way back.
    ///
    /// A key `down` holds and `held` does not is released; a key `held` reports and `down` has not
    /// seen is recorded down with no reading taken. Neither is a key press, because nobody pressed
    /// anything: what changed is what this process knows, so what comes back is at most a change in
    /// the held set.
    pub fn resynchronise(
        &mut self,
        down: &mut BTreeSet<u16>,
        held: &BTreeSet<u16>,
    ) -> Option<SurfaceEvent> {
        let held: BTreeSet<u16> = held
            .iter()
            .copied()
            .filter(|code| typed(Key::new(*code)))
            .collect();
        if let Some(layout) = self.layout.as_mut() {
            for code in down.difference(&held) {
                layout.release(Key::new(*code));
            }
            for code in held.difference(down) {
                layout.hold(Key::new(*code));
            }
        }
        *down = held;
        self.announce()
    }

    /// What one batch from one device amounts to.
    ///
    /// A batch is one coherent update, and a key event in it is a press, a release or a repeat.
    /// Everything else the batch carries — a relative axis, a scan code, a button — belongs to work
    /// this backend has not done and is left alone.
    ///
    /// `stamps` is how this device's moments are read. It belongs to the device rather than to the
    /// seat because the choice does: a driver may refuse the monotonic clock for one device and
    /// accept it for the next.
    pub fn batch(
        &mut self,
        down: &mut BTreeSet<u16>,
        batch: &Batch,
        stamps: Stamps,
    ) -> Vec<SurfaceEvent> {
        if dropped(batch) {
            return Vec::new();
        }
        let mut events = Vec::new();
        for event in &batch.events {
            let Some(key) = event.key().filter(|key| typed(*key)) else {
                continue;
            };
            let transition = Transition::of(event.value);
            let reading = self.read(down, key, transition);
            let (state, repeat) = match transition {
                Transition::Pressed => (KeyState::Pressed, false),
                Transition::Repeated => (KeyState::Pressed, true),
                Transition::Released => (KeyState::Released, false),
            };
            // The held set is read *after* the transition is recorded, so the press of shift
            // carries shift and its release carries nothing — which is what a browser reports and
            // what a handler reading the modifiers off a key event expects.
            let modifiers = self
                .layout
                .as_ref()
                .map_or(Modifiers::NONE, |layout| layout.modifiers());
            // Before the key, because the state a key was struck in is announced before the event
            // that happened in it.
            if modifiers != self.modifiers {
                self.modifiers = modifiers;
                events.push(SurfaceEvent::ModifiersChanged(modifiers));
            }
            events.push(SurfaceEvent::Key {
                state,
                event: keyboard::event(
                    code::physical(key),
                    reading.key,
                    reading.without_modifiers,
                    repeat,
                ),
                modifiers,
                timestamp: stamps.at(event.at),
            });
        }
        events
    }

    /// Reads the layout for one transition, and records the transition where there is one.
    ///
    /// The device's own `down` set decides whether the layout is told at all. A press of a key that
    /// device already has down records nothing, and a release of one it does not have down records
    /// nothing either — so the layout's count follows the keyboard rather than the stream.
    ///
    /// The press case is reachable on any machine: `zgui_evdev::Device::open` starts the kernel
    /// queuing events to this client, and the grab and the read of `EVIOCGKEY` happen after it, so
    /// a key struck in between arrives through the map *and* through the stream. Counted twice, it
    /// needs two releases and gets one.
    fn read(&mut self, down: &mut BTreeSet<u16>, key: Key, transition: Transition) -> Reading {
        let moved = match transition {
            Transition::Pressed => down.insert(key.raw()),
            Transition::Released => down.remove(&key.raw()),
            Transition::Repeated => false,
        };
        let Some(layout) = self.layout.as_mut() else {
            // No layout at all. The position still crosses, so a binding written against where the
            // key sits keeps working.
            return Reading {
                key: zgui_vocab::Key::Unidentified,
                without_modifiers: zgui_vocab::Key::Unidentified,
            };
        };
        match transition {
            // One call, which reads before it records so that a latched modifier is spent by the
            // key that consumed it rather than by the key before.
            Transition::Pressed if moved => layout.press(key),
            // A release reports what the key meant while it was down, so it is read first. The two
            // are one call rather than two lines that could be swapped: reading afterwards reports
            // what the key means with itself already up, which for a modifier is a different level
            // of every key it was holding.
            Transition::Released if moved => layout.reading_before_release(key),
            // A repeat is no transition at all, and so is a transition this device had already
            // made.
            _ => layout.reading(key),
        }
    }

    /// Returns a change in the held set, when it moved.
    fn announce(&mut self) -> Option<SurfaceEvent> {
        let held = self
            .layout
            .as_ref()
            .map_or(Modifiers::NONE, |layout| layout.modifiers());
        (held != self.modifiers).then(|| {
            self.modifiers = held;
            SurfaceEvent::ModifiersChanged(held)
        })
    }
}

/// What a device somebody points with needs remembered between updates.
struct Pointing {
    /// How it says where the pointer is.
    axes: Axes,
    /// Which of its wheels count in hundred-and-twentieths of a detent.
    ///
    /// A wheel that reports the high-resolution axis reports the coarse one for the same movement,
    /// so a reader that took both would scroll twice as far as the wheel was turned.
    /// [`wheel::delta`] reads one of the two, and this says which.
    wheel: HighResolution,
    /// Which of its buttons this seat believes are down.
    ///
    /// The device's own rather than the seat's, for the reason [`Keys`] gives about keys: a button
    /// held on one device is that device's, and a set shared between two would let letting go of
    /// one release the other's press.
    down: BTreeSet<u16>,
}

/// One device this seat took.
struct Taken {
    /// The device, grabbed for as long as this lives.
    device: Device,
    /// Which of its keys this seat believes are down.
    down: BTreeSet<u16>,
    /// How its moments are read.
    stamps: Stamps,
    /// Whether a person types on it.
    types: bool,
    /// How a person points with it, where they do.
    points: Option<Pointing>,
}

/// Every device on this machine a person works with, taken.
pub struct Seat {
    /// The devices, each grabbed for as long as this lives.
    devices: Vec<Taken>,
    /// The layout and the translation over it.
    keys: Keys,
    /// What this seat has to say before it has read anything.
    ///
    /// The modifiers that were already held when the devices were taken. Nothing has been told
    /// about them yet — the loop asks for events, and this is the first answer — and a caller left
    /// to work them out from key events alone would believe none were held while every key event
    /// said otherwise.
    pending: Vec<SurfaceEvent>,
    /// The anchor between the kernel's clock and the frame loop's, taken when this seat opened.
    ///
    /// Kept because a device taken later needs the same one, and because it is the only way back to
    /// the loop's own numbering from inside a read: both clocks run at one rate, so an anchor taken
    /// at start-up holds for as long as the program does.
    anchored: Stamps,
    /// The watch on the directory the devices come from, where one could be made.
    ///
    /// Nothing on a machine with no watch changes: the set of devices is then what it was at
    /// start-up, the way this backend behaved before the watch existed.
    watch: Option<Watch>,
}

impl Seat {
    /// Opens every device this process may read and work with, takes each one, and finds a layout.
    ///
    /// **Call this after DRM master has been taken.** See the module documentation for why that
    /// ordering is the safety interlock rather than a preference.
    ///
    /// Nothing here fails. A machine with no readable device and no layout is a console that draws
    /// and cannot be typed into, which is what this backend was before this existed, and every
    /// refusal is reported through the crate's log rather than turned into an error the frame loop
    /// would have to decide about.
    pub fn open(clock: &dyn Clock) -> Self {
        let found = layout::find();
        for refusal in &found.refused {
            info!(target: "zgui::platform", "no layout from {refusal}");
        }
        match &found.layout {
            Some(layout) => info!(
                target: "zgui::platform",
                "the keyboard is read through {}", layout.describe()
            ),
            None => warn!(
                target: "zgui::platform",
                "this machine has no keyboard layout, so a key reaches a document by its position \
                 alone and types nothing"
            ),
        }

        // Before the walk, and that ordering carries the whole guarantee. A device that arrives
        // between the two is in the directory the walk reads or in a report this watch holds; a
        // watch made afterwards has neither, and that device reaches nothing for the rest of the
        // program.
        let watch = match Watch::new() {
            Ok(watch) => Some(watch),
            Err(error) => {
                warn!(
                    target: "zgui::platform",
                    "the device directory cannot be watched, so a keyboard or a mouse plugged in \
                     while this runs reaches nothing: {error}"
                );
                None
            }
        };

        let mut seat = Self {
            devices: Vec::new(),
            keys: Keys::new(found.layout),
            pending: Vec::new(),
            anchored: Stamps::anchored(clock),
            watch,
        };
        match zgui_evdev::discover() {
            Ok(discovery) => {
                for skipped in &discovery.skipped {
                    info!(
                        target: "zgui::platform",
                        "{} cannot be read: {}", skipped.path.display(), skipped.reason
                    );
                }
                for device in discovery.opened {
                    let announced = seat.admit(device);
                    seat.pending.extend(announced);
                }
            }
            Err(error) => warn!(
                target: "zgui::platform",
                "no input device can be found on this machine: {error}"
            ),
        }
        if !seat.devices.iter().any(|taken| taken.types) {
            warn!(
                target: "zgui::platform",
                "no keyboard on this machine could be taken, so nothing can be typed into this \
                 program"
            );
        }
        if !seat.devices.iter().any(|taken| taken.points.is_some()) {
            warn!(
                target: "zgui::platform",
                "no pointing device on this machine could be taken, so the cursor cannot be moved"
            );
        }
        seat
    }

    /// Returns the descriptors the frame loop waits on beside the device and the wake channel.
    ///
    /// Every device this seat took, and the watch on the directory they came from. A node made in
    /// that directory wakes the loop the way a key pressed does, because what has to happen next is
    /// the same: read the descriptor and act on what it said.
    pub fn descriptors(&self) -> impl Iterator<Item = BorrowedFd<'_>> {
        self.devices
            .iter()
            .map(|taken| taken.device.as_fd())
            .chain(self.watch.iter().map(AsFd::as_fd))
    }

    /// Returns what the frame loop's clock reads now, through this seat's own anchor.
    ///
    /// The anchor came from the clock the loop runs on and the two run at one rate, so this is the
    /// loop's own reading taken without the loop being here to ask.
    fn now(&self) -> Timestamp {
        self.anchored.at(monotonic())
    }

    /// Takes one device into this seat, and reports what was already held on it.
    ///
    /// A device nobody types on and nobody points with is left alone, and so is one the kernel
    /// refuses to hand over. What can come back is a change in the held set and nothing else: a key
    /// that is down now was pressed by nobody this process was listening to, so what changed is
    /// what this process knows.
    fn admit(&mut self, device: Device) -> Option<SurfaceEvent> {
        let device = take(device)?;
        // Asked rather than assumed. A driver that refused `EVIOCSCLOCKID` leaves this device's
        // stream on the wall clock, which shares no zero with the frame loop's own reading, so its
        // events are stamped when they are read instead.
        let stamps = if device.has_monotonic_timestamps() {
            self.anchored
        } else {
            warn!(
                target: "zgui::platform",
                "{} refused the monotonic clock, so what it reports is stamped when the loop reads \
                 it rather than when it happened",
                device.path().display()
            );
            Stamps::Read(self.now())
        };
        let mut taken = Taken {
            types: types_on(device.capabilities()),
            points: pointing(&device),
            device,
            down: BTreeSet::new(),
            stamps,
        };
        // After the grab, so that nothing else can change what is held between the two.
        let announced = match taken.device.pressed_keys() {
            Ok(held) => {
                let held: BTreeSet<u16> = held.iter().map(Key::raw).collect();
                if let Some(points) = taken.points.as_mut() {
                    // Buttons alone, and no event: what changed is what this process knows. A
                    // button held now is recorded so that its release is the one thing that is
                    // delivered.
                    points.down = held.iter().copied().filter(pressed_on_a_pointer).collect();
                }
                self.keys.resynchronise(&mut taken.down, &held)
            }
            Err(error) => {
                warn!(
                    target: "zgui::platform",
                    "{} will not say what is held on it, so a modifier or a button held now stays \
                     invisible until it is pressed again: {error}",
                    taken.device.path().display()
                );
                None
            }
        };
        self.devices.push(taken);
        announced
    }

    /// Takes every device that has arrived since the last read.
    ///
    /// **Call this after the devices that stopped answering have gone.** A node removed and made
    /// again under the same name is a different device at the same path, and an arrival at a path
    /// this seat still holds is refused — so the stale one has to be dropped first. [`Seat::read`]
    /// is where that ordering is kept, by the author reading it rather than by any type here.
    fn arrivals(&mut self) -> Vec<Report> {
        let read = self.watch.as_ref().map(Watch::arrived);
        let arrived = match read {
            None => return Vec::new(),
            Some(Ok(arrived)) => arrived,
            // The same rule a device that stops answering follows, for the same reason: a
            // descriptor that answers a failure and stays readable turns every later wait into a
            // wait of no length, and a loop that kept one would spin for as long as it ran.
            Some(Err(error)) => {
                warn!(
                    target: "zgui::platform",
                    "the device directory can no longer be watched, so a keyboard or a mouse \
                     plugged in from now on reaches nothing: {error}"
                );
                self.watch = None;
                return Vec::new();
            }
        };

        let held: Vec<&Path> = self
            .devices
            .iter()
            .map(|taken| taken.device.path())
            .collect();
        let opening: Vec<PathBuf> = untaken(&held, &arrived);
        opening
            .iter()
            .filter_map(|path| self.take_node(path))
            .map(Report::focused)
            .collect()
    }

    /// Opens the node at `path` and takes it, reporting what was already held on it.
    fn take_node(&mut self, path: &Path) -> Option<SurfaceEvent> {
        match Device::open(path) {
            Ok(device) => self.admit(device),
            // The ordinary answer for a node udev has not finished with, and the change that says
            // it has finished is one more report the watch brings. So this is the first of two
            // tries rather than a device lost.
            Err(error) => {
                info!(
                    target: "zgui::platform",
                    "{} cannot be read yet: {error}", path.display()
                );
                None
            }
        }
    }

    /// Reads every device and reports what a person did, moving `pointer` as they moved it.
    ///
    /// `screens` is the ground the pointer moves over, so it decides which surface each pointer
    /// event belongs to and how far the pointer can go. It is passed in rather than kept because
    /// it changes with what the application has claimed, and a seat holding a stale copy would
    /// clamp a pointer to a display that is no longer being drawn.
    ///
    /// A device that answers a read with a failure is dropped, and its keys and its buttons are
    /// let go. Any errno is treated that way, because the one that matters cannot be told from the
    /// others by anything this loop could do differently: `ENODEV` is what an unplugged device and
    /// a descriptor `logind` revoked both answer, and both then answer every later read the same
    /// way while `poll` reports the descriptor permanently ready — so a loop that kept one would
    /// spin at the speed of the processor for as long as it ran. A device dropped over a passing
    /// failure costs a keyboard or a mouse that has to be plugged in again; a device kept costs the
    /// whole program.
    pub fn read(&mut self, pointer: &mut Pointer, screens: &[Screen]) -> Vec<Report> {
        let now = self.now();
        let Self {
            devices,
            keys,
            pending,
            ..
        } = self;
        let mut reports: Vec<Report> = std::mem::take(pending)
            .into_iter()
            .map(Report::focused)
            .collect();
        let mut lost = Vec::new();
        for (index, taken) in devices.iter_mut().enumerate() {
            // Here, because here is where the loop reads it. See `Stamps::read_at`.
            taken.stamps = taken.stamps.read_at(now);
            let batches = match taken.device.read() {
                Ok(batches) => batches,
                Err(error) => {
                    warn!(
                        target: "zgui::platform",
                        "{} stopped answering and is no longer watched: {error}",
                        taken.device.path().display()
                    );
                    lost.push(index);
                    continue;
                }
            };
            let mut resynchronise = false;
            for batch in &batches {
                resynchronise |= dropped(batch);
                if taken.types {
                    reports.extend(
                        keys.batch(&mut taken.down, batch, taken.stamps)
                            .into_iter()
                            .map(Report::focused),
                    );
                }
                // The same rule the keyboard follows: a batch the kernel dropped part of is the
                // tail of an update whose beginning no longer exists, so a button in it was
                // pressed by nobody and a motion in it went nowhere anybody can name.
                if let Some(points) = taken.points.as_mut()
                    && !dropped(batch)
                {
                    reports.extend(pointed(
                        points,
                        batch,
                        taken.stamps,
                        keys.modifiers(),
                        pointer,
                        screens,
                    ));
                }
            }
            if resynchronise {
                match taken.device.pressed_keys() {
                    Ok(held) => {
                        let held: BTreeSet<u16> = held.iter().map(Key::raw).collect();
                        if let Some(points) = taken.points.as_mut() {
                            reports.extend(cancelled(
                                points,
                                &held,
                                keys.modifiers(),
                                taken.stamps,
                                pointer,
                                screens,
                            ));
                        }
                        reports.extend(
                            keys.resynchronise(&mut taken.down, &held)
                                .map(Report::focused),
                        );
                    }
                    // What is believed is left alone. Repairing against nothing would release every
                    // key the person is holding, which is worse than carrying a stale belief until
                    // the next answer.
                    Err(error) => warn!(
                        target: "zgui::platform",
                        "{} will not say what is held on it, so what this loop believes is down \
                         stays as it was: {error}",
                        taken.device.path().display()
                    ),
                }
            }
        }
        for index in lost.into_iter().rev() {
            let mut gone = devices.remove(index);
            // A device that is gone holds nothing. The releases the kernel queued for it are never
            // read, so this is the only thing that takes its keys back off the layout and ends the
            // interactions its buttons were holding open.
            if let Some(points) = gone.points.as_mut() {
                reports.extend(cancelled(
                    points,
                    &BTreeSet::new(),
                    keys.modifiers(),
                    gone.stamps,
                    pointer,
                    screens,
                ));
            }
            reports.extend(
                keys.resynchronise(&mut gone.down, &BTreeSet::new())
                    .map(Report::focused),
            );
        }
        // After the devices that stopped answering have gone. A node removed and made again under
        // the same name is a different device at the same path, and an arrival at a path this seat
        // still holds is refused — so a stale one left here would keep its own replacement out.
        reports.extend(self.arrivals());
        reports
    }
}

/// Returns which of the nodes that arrived are ones to open.
///
/// A node the seat already holds is left alone, and it has to be: **one hotplug names the same node
/// twice.** The kernel makes it and udev sets its ownership afterwards, and both are reports this
/// backend acts on, because the first alone is too early to open the node. A second open is a
/// second client on a device one client already grabbed, so the kernel refuses the grab and the
/// device is reported as one that will not be handed over — a keyboard that works, logged as a
/// keyboard that does not, with a descriptor in the poll set that carries nothing.
///
/// The path identifies a device here without being an identity: two identical keyboards report the
/// same name and the same ids. It is enough because the kernel gives one node to one device at a
/// time, and it is enough only while the seat holds no node that has already gone — which
/// [`Seat::arrivals`] states and the caller keeps.
fn untaken(held: &[&Path], arrived: &[PathBuf]) -> Vec<PathBuf> {
    arrived
        .iter()
        .filter(|path| !held.contains(&path.as_path()))
        .cloned()
        .collect()
}

/// Returns `true` if this code is a button a pointer has.
fn pressed_on_a_pointer(code: &u16) -> bool {
    pointer::button(Key::new(*code)).is_some()
}

/// Translates one batch from one pointing device, and moves the pointer with it.
///
/// The order is the one a browser reports and the one a hover state depends on: the display that
/// was left hears first, the display that was reached hears next, and everything else happens
/// where the pointer now is. A press delivered at the place the pointer used to be is a click on
/// whatever was under the old position.
fn pointed(
    points: &mut Pointing,
    batch: &Batch,
    stamps: Stamps,
    modifiers: Modifiers,
    pointer: &mut Pointer,
    screens: &[Screen],
) -> Vec<Report> {
    let motion = pointer::batch(points.axes, &mut points.down, batch);
    let turned = wheel::delta(batch, points.wheel);
    if motion.is_empty() && turned.is_none() {
        return Vec::new();
    }

    let before = pointer.position(screens);
    if let Some((dx, dy)) = motion.by {
        pointer.moved_by(dx, dy, screens);
    }
    if let Some((x, y)) = motion.to {
        pointer.moved_to(x, y, screens);
    }
    let Some((surface, at)) = pointer.position(screens) else {
        // The application claimed no display, so there is nowhere for any of this to go.
        return Vec::new();
    };

    let timestamp = stamps.at(batch.at);
    let mut reports = Vec::new();
    let moved = |action, button, surface, at| {
        Report::on(
            surface,
            SurfaceEvent::Pointer {
                action,
                event: pointer::event(at, button),
                modifiers,
                timestamp,
            },
        )
    };
    match before {
        Some((left, was)) if left != surface => {
            reports.push(moved(PointerAction::Left, None, left, was));
            reports.push(moved(PointerAction::Entered, None, surface, at));
        }
        Some((_, was)) if was != at => reports.push(moved(PointerAction::Moved, None, surface, at)),
        // The pointer stayed where it was. A button or a wheel turn in this batch is still
        // reported below, at the place the pointer is.
        _ => {}
    }
    for (button, action) in motion.buttons {
        reports.push(moved(action, Some(button), surface, at));
    }
    if let Some(delta) = turned {
        reports.push(Report::on(
            surface,
            SurfaceEvent::Wheel {
                event: wheel::event(delta, at),
                modifiers,
                timestamp,
            },
        ));
    }
    reports
}

/// Ends every interaction this device was holding open that the kernel no longer reports.
///
/// A button held while its device is unplugged is never released: the kernel queues the release
/// and then answers `ENODEV` the moment the device is gone, so a control that listens for one
/// stays pressed for the rest of the program. The same is true of a button that went up inside a
/// queue overflow.
///
/// [`PointerAction::Cancelled`] rather than a release, because nobody let go. A control told about
/// a release fires; a control told about a cancel gives up, and giving up is what happened.
///
/// # A button this seat has not seen
///
/// Such a button is left alone, even where the kernel reports it held. [`Keys::resynchronise`] does
/// the opposite for a key: one the kernel says is held and the seat has not seen is recorded down.
/// The two are different on purpose. A key recorded down is fed to the layout and changes what the
/// next key means, so a modifier missed after an overflow comes out in every letter afterwards. A
/// button has no such state — nothing above was told it went down, and recording it here would
/// deliver a release that no control has a press for.
fn cancelled(
    points: &mut Pointing,
    held: &BTreeSet<u16>,
    modifiers: Modifiers,
    stamps: Stamps,
    pointer: &Pointer,
    screens: &[Screen],
) -> Vec<Report> {
    let ended: Vec<u16> = points.down.difference(held).copied().collect();
    points.down.retain(|code| held.contains(code));
    let Some((surface, at)) = pointer.position(screens) else {
        return Vec::new();
    };
    ended
        .into_iter()
        .filter_map(|code| pointer::button(Key::new(code)))
        .map(|button| {
            Report::on(
                surface,
                SurfaceEvent::Pointer {
                    action: PointerAction::Cancelled,
                    event: pointer::event(at, Some(button)),
                    modifiers,
                    // Now, read through this device's own anchor. What the kernel would have
                    // stamped the release with is in a queue nothing will ever read, and the
                    // application's origin is not a substitute: it is hours before the press that
                    // opened the interaction, so anything measuring an interval across the two —
                    // a double-click window, a drag velocity, a gesture timeout — reads a negative
                    // one.
                    timestamp: stamps.at(monotonic()),
                },
            )
        })
        .collect()
}

/// Returns how a person points with this device, where they do.
///
/// The absolute ranges are read here, once, because `EVIOCGABS` answers with the axis's own units
/// and nothing above knows what one of them is. A driver that refuses the query leaves this device
/// as a pointer with no way to read where it is, so it is left alone rather than read against a
/// range this backend invented.
fn pointing(device: &Device) -> Option<Pointing> {
    let capabilities = device.capabilities();
    if !pointer::points_with(capabilities) {
        return None;
    }
    // Relative first. A device that reports both is a tablet whose mouse mode also works, and the
    // relative axes are the ones a person expects to move a pointer with.
    let axes = if pointer::relative(capabilities) {
        Axes::Relative
    } else {
        match (device.axis(Absolute::ABS_X), device.axis(Absolute::ABS_Y)) {
            (Ok(x), Ok(y)) => Axes::Absolute {
                x: Span::of(x),
                y: Span::of(y),
            },
            _ => {
                warn!(
                    target: "zgui::platform",
                    "{} will not say what range its axes read in, so where it is pointing cannot \
                     be worked out and it is left alone",
                    device.path().display()
                );
                return None;
            }
        }
    };
    Some(Pointing {
        axes,
        wheel: HighResolution::of(capabilities),
        down: BTreeSet::new(),
    })
}

/// Opens the grab on a device somebody works with, or answers with nothing.
///
/// The grab keeps a keystroke away from the console behind the application, and it also stops
/// `Ctrl+C` reaching the terminal's line discipline. The module documentation states what that
/// costs.
///
/// A device nobody types on and nobody points with is left alone. So is one the kernel refuses to
/// hand over: a grab is exclusive, so another client already holding it is the ordinary reason, and
/// either way the device stays with whatever has it.
fn take(mut device: Device) -> Option<Device> {
    let types = types_on(device.capabilities());
    let points = pointer::points_with(device.capabilities());
    if !types && !points {
        return None;
    }
    match device.grab() {
        Ok(()) => {
            let doing = match (types, points) {
                (true, true) => "typing and pointing",
                (true, false) => "typing",
                _ => "pointing",
            };
            info!(
                target: "zgui::platform",
                "{doing} on {} ({})", device.name(), device.path().display()
            );
            Some(device)
        }
        Err(error) => {
            warn!(
                target: "zgui::platform",
                "{} will not be handed over and is left where it is: {error}",
                device.path().display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    //! The whole translation, over bytes written here.
    //!
    //! No device and no layout library. `zgui_evdev::Reader::feed` turns bytes into batches with no
    //! descriptor anywhere, so what a person did reaches a surface event through exactly the code
    //! the frame loop runs — and the cases that matter are the ones a working keyboard rarely
    //! produces: a repeat, an overflow, a button on a keyboard node.

    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use super::{
        Axes, HighResolution, Keys, Pointer, Pointing, Report, Screen, Seat, Span, Stamps,
        Transition, cancelled, focused, pointed, types_on, untaken,
    };
    use crate::input::keyboard::layout::{Layout, Reading, Source};
    use std::time::Duration;
    use zgui_evdev::{
        Absolute, Bitmap, Capabilities, EventType, Key, Reader, Relative, Synchronisation,
    };
    use zgui_geom::{CssPx, Point};
    use zgui_platform::{Clock, SurfaceEvent, SurfaceId};
    use zgui_vocab::{EventKind, KeyCode, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp};

    /// A layout that records what it was told, and holds shift the way a real one would.
    ///
    /// libxkbcommon counts a modifier's transitions, so what matters is the *count*: a caller that
    /// recorded a repeat as a press would leave it above zero and shift would never come up again.
    /// This counts the same way, so a test can assert the balance without the library.
    #[derive(Debug, Default)]
    struct Recording {
        /// How many times shift was recorded down, less the times it was recorded up.
        shift: i32,
        /// Every call, in the order they were made.
        ///
        /// One list rather than two, because the order *between* a read and a record is what half
        /// of these tests are about: a release that recorded before it read would report the key
        /// with itself already up.
        calls: Vec<(&'static str, u16)>,
    }

    impl Recording {
        /// What this stub says a key means, with shift applied where it matters.
        fn reading_of(&self, key: Key) -> Reading {
            let (upper, lower) = match key {
                Key::KEY_A => ("A", "a"),
                Key::KEY_SPACE => (" ", " "),
                Key::KEY_LEFTSHIFT => {
                    return Reading {
                        key: zgui_vocab::Key::Named(NamedKey::Shift),
                        without_modifiers: zgui_vocab::Key::Named(NamedKey::Shift),
                    };
                }
                _ => {
                    return Reading {
                        key: zgui_vocab::Key::Unidentified,
                        without_modifiers: zgui_vocab::Key::Unidentified,
                    };
                }
            };
            Reading {
                key: zgui_vocab::Key::character(if self.shift > 0 { upper } else { lower }),
                without_modifiers: zgui_vocab::Key::character(lower),
            }
        }
    }

    impl Layout for Recording {
        /// The trait asks which source a layout read and a stub reads neither. Nothing here looks
        /// at the answer.
        fn source(&self) -> Source {
            Source::Xkb
        }

        fn describe(&self) -> String {
            "a layout that records what it is told".to_owned()
        }

        fn press(&mut self, key: Key) -> Reading {
            // Read before the record, the order the real one is written in.
            let reading = self.reading_of(key);
            self.calls.push(("press", key.raw()));
            if key == Key::KEY_LEFTSHIFT {
                self.shift += 1;
            }
            reading
        }

        fn reading(&self, key: Key) -> Reading {
            // `&self`, so this cannot record. That is the guarantee, and `Shared::reading` below
            // is how a test sees that it was called.
            self.reading_of(key)
        }

        fn release(&mut self, key: Key) {
            self.calls.push(("release", key.raw()));
            if key == Key::KEY_LEFTSHIFT {
                self.shift -= 1;
            }
        }

        fn hold(&mut self, key: Key) {
            self.calls.push(("hold", key.raw()));
            if key == Key::KEY_LEFTSHIFT {
                self.shift += 1;
            }
        }

        fn modifiers(&self) -> Modifiers {
            Modifiers::NONE.with(Modifiers::SHIFT, self.shift > 0)
        }
    }

    /// A recording layout that a test can read back afterwards.
    ///
    /// The layout is owned by the translation, so what a test reads is a shared count rather than
    /// the layout itself.
    #[derive(Debug, Default, Clone)]
    struct Shared(std::rc::Rc<std::cell::RefCell<Recording>>);

    impl Layout for Shared {
        fn source(&self) -> Source {
            self.0.borrow().source()
        }

        fn describe(&self) -> String {
            self.0.borrow().describe()
        }

        fn press(&mut self, key: Key) -> Reading {
            self.0.borrow_mut().press(key)
        }

        fn reading(&self, key: Key) -> Reading {
            let reading = self.0.borrow().reading(key);
            self.0.borrow_mut().calls.push(("read", key.raw()));
            reading
        }

        fn release(&mut self, key: Key) {
            self.0.borrow_mut().release(key);
        }

        fn hold(&mut self, key: Key) {
            self.0.borrow_mut().hold(key);
        }

        fn modifiers(&self) -> Modifiers {
            self.0.borrow().modifiers()
        }
    }

    impl Shared {
        /// Every call the layout was given, in order.
        fn calls(&self) -> Vec<(&'static str, u16)> {
            self.0.borrow().calls.clone()
        }

        /// Every call that recorded a transition, in order.
        fn recorded(&self) -> Vec<(&'static str, u16)> {
            self.0
                .borrow()
                .calls
                .iter()
                .filter(|(call, _)| *call != "read")
                .copied()
                .collect()
        }

        /// How many times the layout was read without being told anything.
        fn reads(&self) -> usize {
            self.0
                .borrow()
                .calls
                .iter()
                .filter(|(call, _)| *call == "read")
                .count()
        }
    }

    /// One keyboard's worth of state: the translation, the record, and the keys it has down.
    ///
    /// The down set is the device's rather than the seat's, so a test that wants two keyboards
    /// keeps two of them over one `Keys`.
    fn keys() -> (Keys, Shared, BTreeSet<u16>) {
        let shared = Shared::default();
        (
            Keys::new(Some(Box::new(shared.clone()))),
            shared,
            BTreeSet::new(),
        )
    }

    /// What the kernel's clock read when this loop started, in these tests.
    const SINCE: Duration = Duration::from_secs(1_000);

    /// The bytes of one record, as the kernel lays out `input_event`.
    ///
    /// A `timeval` of two sixty-four-bit halves, then the type, the code and the value. A machine
    /// where those are not the widths fails these tests loudly rather than quietly: the reader
    /// would find no `SYN_REPORT` where one was written, and every assertion below would see no
    /// batch at all.
    fn record(at: Duration, kind: EventType, code: u16, value: i32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let seconds = i64::try_from(at.as_secs()).expect("the test uses a small moment");
        bytes.extend_from_slice(&seconds.to_ne_bytes());
        bytes.extend_from_slice(&i64::from(at.subsec_micros()).to_ne_bytes());
        bytes.extend_from_slice(&kind.raw().to_ne_bytes());
        bytes.extend_from_slice(&code.to_ne_bytes());
        bytes.extend_from_slice(&value.to_ne_bytes());
        bytes
    }

    /// The bytes of one key moving, and the report that ends the update.
    fn moved(at: Duration, key: Key, value: i32) -> Vec<u8> {
        let mut bytes = record(at, EventType::EV_KEY, key.raw(), value);
        bytes.extend(record(
            at,
            EventType::EV_SYN,
            Synchronisation::SYN_REPORT.raw(),
            0,
        ));
        bytes
    }

    /// What a stream of bytes from one keyboard turns into, through the whole translation.
    fn translate(keys: &mut Keys, down: &mut BTreeSet<u16>, bytes: &[u8]) -> Vec<SurfaceEvent> {
        let mut reader = Reader::new();
        let batches = reader.feed(bytes);
        let mut events = Vec::new();
        for batch in &batches {
            events.append(&mut keys.batch(down, batch, Stamps::from_origin(SINCE)));
        }
        events
    }

    /// The key events among what came out, as the fields a test asserts on.
    fn presses(events: &[SurfaceEvent]) -> Vec<(KeyState, PhysicalKey, bool, Modifiers)> {
        events
            .iter()
            .filter_map(|event| match event {
                SurfaceEvent::Key {
                    state,
                    event,
                    modifiers,
                    ..
                } => Some((*state, event.physical, event.repeat, *modifiers)),
                _ => None,
            })
            .collect()
    }

    /// The capabilities of a device with these types and these keys.
    fn capabilities(types: &[EventType], keys: &[Key]) -> Capabilities {
        Capabilities::new(
            Bitmap::from_codes(types.iter().copied()),
            Bitmap::from_codes(keys.iter().copied()),
            Bitmap::<Relative>::default(),
            Bitmap::<Absolute>::default(),
        )
    }

    #[test]
    fn the_kernel_writes_three_values_and_a_repeat_is_the_third() {
        // Reading `value != 0` as a press is the defect: it records one transition per repeat and
        // one release, so the count never returns to zero.
        assert_eq!(Transition::of(1), Transition::Pressed);
        assert_eq!(Transition::of(0), Transition::Released);
        assert_eq!(Transition::of(2), Transition::Repeated);
        assert_eq!(
            Transition::of(3),
            Transition::Repeated,
            "a value the kernel does not write records nothing, which cannot unbalance anything"
        );
    }

    #[test]
    fn a_press_and_its_release_arrive_as_two_events() {
        let (mut keys, _, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_A, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 0));

        let events = translate(&mut keys, &mut down, &bytes);

        assert_eq!(
            presses(&events),
            [
                (
                    KeyState::Pressed,
                    PhysicalKey::Code(KeyCode::KeyA),
                    false,
                    Modifiers::NONE
                ),
                (
                    KeyState::Released,
                    PhysicalKey::Code(KeyCode::KeyA),
                    false,
                    Modifiers::NONE
                ),
            ]
        );
    }

    #[test]
    fn what_comes_out_is_what_a_document_is_dispatched() {
        // The last hop this crate can assert on its own. A runtime queues an event because
        // `is_input` says so, and turns it into a document event through `to_dispatch`, which is
        // the contract's own bridge between the two vocabularies — so an event that answers both
        // is one that reaches a document.
        let (mut keys, _, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_A, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 0));

        let events = translate(&mut keys, &mut down, &bytes);

        let dispatched: Vec<_> = events
            .iter()
            .map(|event| {
                assert!(event.is_input(), "{event:?} is what a person did");
                let (kind, payload) = event
                    .to_dispatch()
                    .unwrap_or_else(|| panic!("{event:?} reaches a document"));
                assert!(
                    payload.matches(kind),
                    "{event:?} carries a mismatched payload"
                );
                assert!(event.modifiers().is_some(), "and says what was held");
                kind
            })
            .collect();
        assert_eq!(dispatched, [EventKind::KeyDown, EventKind::KeyUp]);
    }

    #[test]
    fn a_press_carries_all_three_readings_of_it() {
        let (mut keys, _, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 1));

        let events = translate(&mut keys, &mut down, &bytes);

        let SurfaceEvent::Key { event, .. } = events.last().expect("the letter arrived") else {
            panic!("the last event is the letter: {events:?}");
        };
        assert_eq!(
            event.key.inserted_text(),
            Some("A"),
            "what gets inserted has the modifiers applied"
        );
        assert_eq!(
            event.key_without_modifiers,
            zgui_vocab::Key::character("a"),
            "and the shortcut reading does not"
        );
        assert_eq!(event.physical, PhysicalKey::Code(KeyCode::KeyA));
        assert!(!event.repeat);
    }

    #[test]
    fn a_repeat_arrives_as_a_press_that_says_it_is_one() {
        let (mut keys, _, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_A, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 2));

        let events = translate(&mut keys, &mut down, &bytes);

        assert_eq!(
            presses(&events)
                .iter()
                .map(|(state, _, repeat, _)| (*state, *repeat))
                .collect::<Vec<_>>(),
            [(KeyState::Pressed, false), (KeyState::Pressed, true)],
            "a repeat is a press, and it says so, because inserting text takes it and running a \
             command does not"
        );
    }

    #[test]
    fn a_repeat_records_nothing_so_a_held_modifier_still_comes_up() {
        // The trap this translation exists to refuse. Every device with `EV_REP` reports a held
        // key over and over with value 2, and a caller that recorded each one would call the
        // layout's update eight times and its release once — leaving shift down for the rest of
        // the program, so every later letter comes out in the wrong case.
        let (mut keys, layout, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        for _ in 0..8 {
            bytes.extend(moved(SINCE, Key::KEY_LEFTSHIFT, 2));
        }
        bytes.extend(moved(SINCE, Key::KEY_LEFTSHIFT, 0));
        bytes.extend(moved(SINCE, Key::KEY_A, 1));

        let events = translate(&mut keys, &mut down, &bytes);

        assert_eq!(
            layout.recorded(),
            [
                ("press", Key::KEY_LEFTSHIFT.raw()),
                ("release", Key::KEY_LEFTSHIFT.raw()),
                ("press", Key::KEY_A.raw()),
            ],
            "one press and one release reached the layout, whatever came between them"
        );
        assert_eq!(
            layout.reads(),
            9,
            "every repeat was read, and so was the release, which reports what the key meant while \
             it was down"
        );
        assert_eq!(keys.modifiers(), Modifiers::NONE, "shift came back up");
        let SurfaceEvent::Key { event, .. } = events.last().expect("the letter arrived") else {
            panic!("the last event is the letter: {events:?}");
        };
        assert_eq!(
            event.key.inserted_text(),
            Some("a"),
            "so the letter after it is lower case"
        );
    }

    #[test]
    fn the_modifiers_are_read_after_the_transition_that_changed_them() {
        // What a browser reports and what a handler reading the modifiers off a key event expects:
        // the press of shift carries shift, and its release carries nothing.
        let (mut keys, _, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        bytes.extend(moved(SINCE, Key::KEY_LEFTSHIFT, 0));

        let events = translate(&mut keys, &mut down, &bytes);

        assert_eq!(
            presses(&events)
                .iter()
                .map(|(state, _, _, modifiers)| (*state, *modifiers))
                .collect::<Vec<_>>(),
            [
                (KeyState::Pressed, Modifiers::SHIFT),
                (KeyState::Released, Modifiers::NONE),
            ]
        );
    }

    #[test]
    fn a_change_in_what_is_held_is_announced_before_the_key_that_changed_it() {
        // The state a key was struck in, before the event that happened in it. A caller that keeps
        // the held set from this event alone stays right, and that is what the announcement is for.
        let (mut keys, _, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 1));

        let events = translate(&mut keys, &mut down, &bytes);

        assert!(
            matches!(events[0], SurfaceEvent::ModifiersChanged(held) if held == Modifiers::SHIFT),
            "the change comes first: {events:?}"
        );
        assert!(matches!(events[1], SurfaceEvent::Key { .. }));
        assert_eq!(
            events.len(),
            3,
            "and the letter that follows announces nothing, because nothing moved: {events:?}"
        );
    }

    #[test]
    fn a_button_on_a_keyboard_node_is_not_a_key() {
        // A keyboard with a trackpoint reports its buttons on the same node. A button belongs to
        // the pointer, which reads the same batch and takes it there instead.
        let (mut keys, layout, mut down) = keys();

        let events = translate(&mut keys, &mut down, &moved(SINCE, Key::BTN_LEFT, 1));

        assert!(events.is_empty(), "{events:?}");
        assert!(
            layout.recorded().is_empty(),
            "and the layout was never told about it"
        );
    }

    #[test]
    fn the_code_that_sends_nothing_is_not_a_key_either() {
        // `KEY_RESERVED` is code zero. A driver that reports it has said nothing.
        let (mut keys, _, mut down) = keys();

        assert!(translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_RESERVED, 1)).is_empty());
    }

    #[test]
    fn an_event_that_is_not_a_key_at_all_is_left_alone() {
        // A wheel and a scan code both arrive in a keyboard's own batches. Reading one as a key
        // would press whichever key the axis number happens to name.
        let (mut keys, _, mut down) = keys();
        let mut bytes = record(SINCE, EventType::EV_REL, Relative::REL_WHEEL.raw(), 1);
        bytes.extend(record(SINCE, EventType::EV_MSC, 4, 0x0007_0004));
        bytes.extend(record(
            SINCE,
            EventType::EV_SYN,
            Synchronisation::SYN_REPORT.raw(),
            0,
        ));

        assert!(translate(&mut keys, &mut down, &bytes).is_empty());
    }

    #[test]
    fn a_batch_the_kernel_dropped_part_of_reports_nothing() {
        // What arrives after a `SYN_DROPPED` is the tail of an update whose beginning no longer
        // exists. Delivering it would press a key nobody pressed, and release one nobody released.
        let (mut keys, layout, mut down) = keys();
        let mut bytes = record(
            SINCE,
            EventType::EV_SYN,
            Synchronisation::SYN_DROPPED.raw(),
            0,
        );
        bytes.extend(record(SINCE, EventType::EV_KEY, Key::KEY_A.raw(), 0));
        bytes.extend(record(
            SINCE,
            EventType::EV_SYN,
            Synchronisation::SYN_REPORT.raw(),
            0,
        ));

        let events = translate(&mut keys, &mut down, &bytes);

        assert!(events.is_empty(), "{events:?}");
        assert!(layout.recorded().is_empty());
    }

    #[test]
    fn a_resynchronisation_puts_the_layout_back_in_step_without_pressing_anything() {
        // The other half of what a `SYN_DROPPED` asks for. Shift went down before the overflow and
        // came up during it, so the layout believes it is held and the device says it is not.
        let (mut keys, layout, mut down) = keys();
        translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTSHIFT, 1));
        assert_eq!(keys.modifiers(), Modifiers::SHIFT);

        let announced = keys.resynchronise(&mut down, &BTreeSet::new());

        assert!(
            matches!(announced, Some(SurfaceEvent::ModifiersChanged(held)) if held.is_empty()),
            "the change is announced, and no key press is invented: {announced:?}"
        );
        assert_eq!(
            layout.recorded().last(),
            Some(&("release", Key::KEY_LEFTSHIFT.raw())),
            "the layout was told the key came up"
        );
        assert_eq!(keys.modifiers(), Modifiers::NONE);
    }

    #[test]
    fn a_resynchronisation_records_a_key_the_kernel_says_is_down() {
        let (mut keys, layout, mut down) = keys();

        let announced = keys.resynchronise(&mut down, &BTreeSet::from([Key::KEY_LEFTSHIFT.raw()]));

        assert!(matches!(announced, Some(SurfaceEvent::ModifiersChanged(_))));
        assert_eq!(
            layout.recorded(),
            [("hold", Key::KEY_LEFTSHIFT.raw())],
            "a key that was down through the overflow is held rather than pressed"
        );
        assert_eq!(keys.modifiers(), Modifiers::SHIFT);
    }

    #[test]
    fn a_key_already_down_when_the_seat_opened_reaches_the_layout() {
        // `EVIOCGKEY` says shift is under a finger. Nothing later in the stream says so, so
        // without this it stays invisible until it is released and pressed again.
        let (mut keys, layout, mut down) = keys();

        let announced = keys.resynchronise(
            &mut down,
            &BTreeSet::from([Key::KEY_LEFTSHIFT.raw(), Key::BTN_LEFT.raw()]),
        );

        assert!(
            matches!(announced, Some(SurfaceEvent::ModifiersChanged(held)) if held == Modifiers::SHIFT)
        );
        assert_eq!(
            layout.recorded(),
            [("hold", Key::KEY_LEFTSHIFT.raw())],
            "a button is no key, and a held key is recorded with no reading taken"
        );

        // And the release the kernel sends when the finger comes up balances it.
        let events = translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTSHIFT, 0));
        assert_eq!(keys.modifiers(), Modifiers::NONE);
        assert!(!events.is_empty());
    }

    #[test]
    fn the_moment_an_event_carries_is_the_moment_the_kernel_stamped_it() {
        // The kernel timestamps when the key moved. A loop that stamped when it woke would give
        // every event in one wake the same moment, and a double click and a key repeat are
        // measured against the difference between two of them.
        let (mut keys, mut down) = (Keys::new(None), BTreeSet::new());
        let mut bytes = moved(SINCE + Duration::from_millis(250), Key::KEY_A, 1);
        bytes.extend(moved(SINCE + Duration::from_millis(400), Key::KEY_A, 0));

        let events = translate(&mut keys, &mut down, &bytes);

        let moments: Vec<_> = events
            .iter()
            .filter_map(SurfaceEvent::timestamp)
            .map(|stamp| stamp.since_origin())
            .collect();
        assert_eq!(
            moments,
            [Duration::from_millis(250), Duration::from_millis(400)],
            "two events from one read carry two moments"
        );
    }

    #[test]
    fn an_event_stamped_before_the_anchor_was_taken_is_the_origin() {
        // The kernel stamps when the key moved and the anchor is read afterwards, so a device with
        // something already queued can report a moment a hair before it.
        let stamps = Stamps::from_origin(SINCE);

        assert_eq!(
            stamps.at(SINCE - Duration::from_millis(5)).since_origin(),
            Duration::ZERO
        );
        assert_eq!(stamps.at(SINCE).since_origin(), Duration::ZERO);
    }

    #[test]
    fn a_stream_on_the_wall_clock_is_stamped_afresh_on_every_read() {
        // One moment kept from when the device was taken would stamp every event that device ever
        // reports. A key struck an hour in would arrive dated to start-up, and a double-click
        // window measured across two of them would read no interval at all.
        let taken = Timestamp::from_origin(Duration::from_millis(10));
        let later = Timestamp::from_origin(Duration::from_secs(3_600));

        assert!(
            matches!(Stamps::Read(taken).read_at(later), Stamps::Read(at) if at == later),
            "the moment is the one this turn is happening at"
        );
        assert!(
            matches!(
                Stamps::from_origin(SINCE).read_at(later),
                Stamps::Monotonic { origin } if origin == SINCE
            ),
            "and the kernel's own moments keep the anchor, which holds for the whole program"
        );
    }

    #[test]
    fn a_seat_with_no_layout_still_reports_where_the_key_was() {
        // A machine with neither libxkbcommon nor a readable console. The position is the kernel's
        // own and needs no layout, so a binding written against where a key sits keeps working.
        let (mut keys, mut down) = (Keys::new(None), BTreeSet::new());

        let events = translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_A, 1));

        let SurfaceEvent::Key { event, .. } = &events[0] else {
            panic!("a press arrived: {events:?}");
        };
        assert_eq!(event.physical, PhysicalKey::Code(KeyCode::KeyA));
        assert_eq!(event.key, zgui_vocab::Key::Unidentified);
        assert_eq!(event.key.inserted_text(), None);
    }

    /// A clock that says the application started `ago` in the past.
    ///
    /// This exists so the anchor's subtraction is a term worth subtracting. `SystemClock::new()`
    /// takes its origin as it is built, so a test that builds one and anchors against it in the
    /// next line subtracts about nothing — and cannot tell [`Stamps::anchored`]'s arithmetic from
    /// `origin: monotonic()`, or from the same expression with its sign the other way round.
    ///
    /// In the frame loop that term is real and it is not small. The clock is built before the
    /// application is asked for its surfaces, and that is where a program builds its interface and
    /// opens its graphics device: on a machine that spends most of a second compiling shaders, a
    /// dropped term stamps every key and every click most of a second out of step with the frames
    /// they arrived in.
    struct Aged {
        /// When this clock says the application started.
        origin: std::time::Instant,
    }

    impl Aged {
        /// A clock whose origin is `ago` in the past.
        fn started(ago: Duration) -> Self {
            let now = std::time::Instant::now();
            Self {
                // A machine that has been up for less than `ago` has no such moment, and this is
                // about the arithmetic rather than about the machine.
                origin: now.checked_sub(ago).unwrap_or(now),
            }
        }
    }

    impl Clock for Aged {
        fn now(&self) -> std::time::Instant {
            std::time::Instant::now()
        }

        fn origin(&self) -> std::time::Instant {
            self.origin
        }
    }

    #[test]
    fn the_anchor_puts_the_kernels_clock_and_the_loops_on_one_zero() {
        // The one piece of arithmetic that joins two clocks, and the only thing here that reads a
        // real one. An anchor with its sign inverted, or one taken against the wrong origin, lands
        // decades away rather than microseconds — and every stamp downstream is beside frame stamps
        // measured in seconds, where nothing could tell.
        //
        // The clock is aged on purpose. See `Aged`: against a clock built in the line above, the
        // term being subtracted is about zero and this assertion holds whether it is subtracted,
        // added or dropped.
        let clock = Aged::started(Duration::from_secs(5));
        let stamps = Stamps::anchored(&clock);

        let now = super::monotonic();
        let mapped = stamps.at(now).since_origin();
        let reads = clock.timestamp().since_origin();

        assert!(
            reads > Duration::from_secs(4),
            "the clock says the application has been running for {reads:?}, which is the term the \
             anchor has to subtract"
        );
        assert!(
            mapped.abs_diff(reads) < Duration::from_millis(50),
            "the kernel's {now:?} became {mapped:?} where the loop reads {reads:?}"
        );
        // And the two run at one rate, so the anchor holds for as long as the program does.
        assert_eq!(
            stamps.at(now + Duration::from_secs(5)) - stamps.at(now),
            Duration::from_secs(5)
        );
    }

    #[test]
    fn a_device_that_refused_the_monotonic_clock_is_stamped_when_it_is_read() {
        // `EVIOCSCLOCKID` can be refused, which leaves that device's stream on the wall clock.
        // Anchored against a few hours of uptime, a wall-clock moment is some fifty-five years
        // after the application started.
        let read = Timestamp::from_origin(Duration::from_millis(40));
        let wall = Duration::from_secs(1_750_000_000);

        assert_eq!(Stamps::Read(read).at(wall), read);
        assert!(
            Stamps::from_origin(SINCE).at(wall).since_origin() > Duration::from_secs(1_000_000_000),
            "which is what asking the device saves"
        );
    }

    #[test]
    fn a_release_reads_the_key_before_it_records_that_it_came_up() {
        // Reading afterwards reports what the key means with itself already up, and for a modifier
        // that is a different level of every key it was holding. The order is one call on the
        // layout so that nothing can write it the other way round, and this asserts it.
        let (mut keys, layout, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        bytes.extend(moved(SINCE, Key::KEY_LEFTSHIFT, 0));

        translate(&mut keys, &mut down, &bytes);

        assert_eq!(
            layout.calls(),
            [
                ("press", Key::KEY_LEFTSHIFT.raw()),
                ("read", Key::KEY_LEFTSHIFT.raw()),
                ("release", Key::KEY_LEFTSHIFT.raw()),
            ]
        );
    }

    #[test]
    fn a_keyboard_that_stops_answering_leaves_no_modifier_behind() {
        // Two keyboards over one layout, which is how this seat works. Shift is held on the
        // external one and it is unplugged: the kernel queues its releases and then answers
        // `ENODEV` the moment the device is gone, so those releases are never read. Without the
        // repair the count never returns to zero, and every letter typed on the built-in keyboard
        // comes out shifted for the rest of the process with no way back.
        let (mut keys, _, mut external) = keys();
        let mut internal = BTreeSet::new();
        translate(
            &mut keys,
            &mut external,
            &moved(SINCE, Key::KEY_LEFTSHIFT, 1),
        );
        assert_eq!(keys.modifiers(), Modifiers::SHIFT);

        // What `Seat::read` does with a device it has just dropped: one that is gone holds nothing.
        keys.resynchronise(&mut external, &BTreeSet::new());

        let events = translate(&mut keys, &mut internal, &moved(SINCE, Key::KEY_A, 1));
        let SurfaceEvent::Key { event, .. } = events.last().expect("the letter arrived") else {
            panic!("the last event is the letter: {events:?}");
        };
        assert_eq!(keys.modifiers(), Modifiers::NONE);
        assert_eq!(
            event.key.inserted_text(),
            Some("a"),
            "the keyboard that is still here types in lower case"
        );
    }

    #[test]
    fn a_key_held_on_two_keyboards_needs_two_releases() {
        // One layout serves the whole seat and it counts a modifier's transitions, so shift under a
        // finger on each of two keyboards is two of them. A set of held keys shared between the two
        // would hold one code, and letting go of one keyboard would stop reporting shift while the
        // other was still held down.
        let (mut keys, _, mut first) = keys();
        let mut second = BTreeSet::new();

        translate(&mut keys, &mut first, &moved(SINCE, Key::KEY_LEFTSHIFT, 1));
        translate(&mut keys, &mut second, &moved(SINCE, Key::KEY_LEFTSHIFT, 1));
        translate(&mut keys, &mut first, &moved(SINCE, Key::KEY_LEFTSHIFT, 0));

        assert_eq!(
            keys.modifiers(),
            Modifiers::SHIFT,
            "a finger is still on the other one"
        );

        translate(&mut keys, &mut second, &moved(SINCE, Key::KEY_LEFTSHIFT, 0));
        assert_eq!(keys.modifiers(), Modifiers::NONE);
    }

    #[test]
    fn a_key_the_map_and_the_stream_both_report_is_recorded_once() {
        // `zgui_evdev::Device::open` starts the kernel queuing events to this client, and the grab
        // and the read of `EVIOCGKEY` both happen after it — so a key struck in between arrives
        // through the map *and* through the stream. Counted twice it needs two releases and gets
        // one, and the modifier sticks.
        let (mut keys, layout, mut down) = keys();

        keys.resynchronise(&mut down, &BTreeSet::from([Key::KEY_LEFTSHIFT.raw()]));
        translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTSHIFT, 1));
        translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTSHIFT, 0));

        assert_eq!(keys.modifiers(), Modifiers::NONE, "one release balanced it");
        assert_eq!(
            layout.recorded(),
            [
                ("hold", Key::KEY_LEFTSHIFT.raw()),
                ("release", Key::KEY_LEFTSHIFT.raw()),
            ],
            "the press the stream repeated recorded nothing"
        );
    }

    #[test]
    fn a_release_of_a_key_that_was_never_down_records_nothing() {
        // The other half of the same rule. A release with no press behind it would take the count
        // below zero, where nothing brings it back.
        let (mut keys, layout, mut down) = keys();

        translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTSHIFT, 0));
        translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTSHIFT, 1));

        assert_eq!(keys.modifiers(), Modifiers::SHIFT);
        assert_eq!(
            layout.recorded(),
            [("press", Key::KEY_LEFTSHIFT.raw())],
            "only the press was a transition this keyboard made"
        );
    }

    #[test]
    fn the_power_button_is_not_something_a_person_types_on() {
        // `/proc/bus/input/devices` on the development machine: `Name="Power Button"`, carrying
        // `KEY_POWER` and `KEY_SLEEP` and nothing else. Both are under `BTN_MISC`, so a rule
        // written that way takes it — and `EVIOCGRAB` then routes the power button to this process
        // alone, where `acpid` and `logind` never see it and this backend does nothing with it.
        // Beside a grabbed keyboard raising no `SIGINT`, that is a machine with no soft way to stop.
        let power = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_POWER, Key::KEY_SLEEP],
        );

        assert!(
            power.roles().contains(zgui_evdev::Role::Keyboard),
            "udev's rule calls it a keyboard, which is why the narrower question exists"
        );
        assert!(!types_on(&power), "and pressing power has to keep working");
    }

    #[test]
    fn a_hotkey_node_is_not_something_a_person_types_on() {
        // A laptop's own WMI node and a webcam's consumer-control node. Every key on them is under
        // `BTN_MISC` and none of them is a letter.
        let hotkeys = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[
                Key::KEY_BRIGHTNESSUP,
                Key::KEY_BRIGHTNESSDOWN,
                Key::KEY_WLAN,
                Key::KEY_CAMERA,
            ],
        );

        assert!(!types_on(&hotkeys));
    }

    #[test]
    fn a_keyboard_set_to_any_layout_at_all_is_one() {
        // The codes are positions rather than characters, so a Russian, a Dvorak and a French
        // keyboard all report the same ones. A rule written against what a key *types* would refuse
        // every keyboard outside the Latin alphabet.
        let keyboard = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_ESC, Key::KEY_Q, Key::KEY_LEFTSHIFT],
        );

        assert!(types_on(&keyboard));
    }

    #[test]
    fn a_device_with_a_key_a_person_types_on_is_a_keyboard() {
        let keyboard = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_ESC, Key::KEY_A, Key::KEY_LEFTSHIFT],
        );

        assert!(types_on(&keyboard));
    }

    #[test]
    fn a_mouse_that_udev_calls_a_keyboard_is_not_one() {
        // The Razer mouse node on the development machine, as `/proc/bus/input/devices` reports
        // it: buttons, and `KEY_MACRO27` to `KEY_MACRO30`. Those are in the kernel's third key
        // block, so udev's `ID_INPUT_KEY` counts them and `Role::Keyboard` says yes. Grabbing it
        // would take the mouse away from the session and deliver nothing anybody typed.
        let mouse = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL],
            &[
                Key::BTN_LEFT,
                Key::BTN_RIGHT,
                Key::KEY_MACRO27,
                Key::KEY_MACRO28,
                Key::KEY_MACRO29,
                Key::KEY_MACRO30,
            ],
        );

        assert!(
            mouse.roles().contains(zgui_evdev::Role::Keyboard),
            "udev's rule calls this a keyboard, which is why the narrower question exists"
        );
        assert!(!types_on(&mouse), "and a person types on none of it");
    }

    #[test]
    fn a_remote_control_is_not_something_a_person_types_on() {
        // Every key it has is in a block the kernel added behind the buttons. It is a keyboard by
        // udev's rule and there is nothing on it to type with.
        let remote = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_OK, Key::KEY_CHANNELUP, Key::KEY_SUBTITLE],
        );

        assert!(!types_on(&remote));
    }

    #[test]
    fn a_device_whose_only_key_is_the_reserved_one_is_not_a_keyboard() {
        let odd = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY],
            &[Key::KEY_RESERVED],
        );

        assert!(!types_on(&odd));
    }

    #[test]
    fn a_device_with_no_keys_at_all_is_not_a_keyboard() {
        let lid = capabilities(&[EventType::EV_SYN, EventType::EV_SW], &[]);

        assert!(!types_on(&lid));
    }

    #[test]
    fn the_display_the_pointer_is_over_is_the_focused_one() {
        // A console has no window manager, so this is a decision rather than an answer. The pointer
        // makes it, the way a desktop with focus-follows-mouse does, and it is the only rule a
        // machine with no window manager can apply.
        let claimed = [SurfaceId::new(1), SurfaceId::new(2)];

        assert_eq!(
            focused(&claimed, Some(SurfaceId::new(2))),
            Some(SurfaceId::new(2))
        );
    }

    #[test]
    fn the_first_claimed_display_holds_the_keys_until_the_pointer_says_otherwise() {
        // The answer the keyboard milestone gave, kept for the moment before there is a pointer
        // anywhere. It is also what the pointer itself says on the first turn, because a pointer
        // starts in the middle of the first claimed display.
        let claimed = [SurfaceId::new(1), SurfaceId::new(2)];

        assert_eq!(focused(&claimed, None), Some(SurfaceId::new(1)));
        assert_eq!(
            focused(&[], Some(SurfaceId::new(1))),
            None,
            "a program that claimed no display is told about no key"
        );
    }

    #[test]
    fn a_pointer_over_a_display_nothing_claimed_leaves_the_keys_where_they_were() {
        // A display the application never asked for draws nothing and is told nothing, so a
        // pointer standing on one would take the keyboard away from every surface at once.
        let claimed = [SurfaceId::new(1)];

        assert_eq!(
            focused(&claimed, Some(SurfaceId::new(2))),
            Some(SurfaceId::new(1))
        );
    }

    /// Two displays side by side, both claimed.
    fn screens() -> Vec<Screen> {
        vec![
            Screen {
                id: SurfaceId::new(1),
                left: 0.0,
                width: 800.0,
                height: 600.0,
                scale: 1.0,
            },
            Screen {
                id: SurfaceId::new(2),
                left: 800.0,
                width: 800.0,
                height: 600.0,
                scale: 1.0,
            },
        ]
    }

    /// A device that reports how far it moved, with a notched wheel and three buttons.
    fn mouse() -> Pointing {
        Pointing {
            axes: Axes::Relative,
            wheel: HighResolution::default(),
            down: BTreeSet::new(),
        }
    }

    /// What one update of these records amounts to, over `points` and `pointer`.
    fn point(
        points: &mut Pointing,
        pointer: &mut Pointer,
        screens: &[Screen],
        records: &[Vec<u8>],
    ) -> Vec<Report> {
        let mut bytes: Vec<u8> = records.concat();
        bytes.extend(record(
            SINCE,
            EventType::EV_SYN,
            Synchronisation::SYN_REPORT.raw(),
            0,
        ));
        let mut reader = Reader::new();
        let batches = reader.feed(&bytes);
        let [read] = &batches[..] else {
            panic!("one report is one batch: {batches:?}");
        };
        pointed(
            points,
            read,
            Stamps::from_origin(SINCE),
            Modifiers::NONE,
            pointer,
            screens,
        )
    }

    /// What each report was, as the fields a test asserts on.
    fn did(reports: &[Report]) -> Vec<(Option<SurfaceId>, String)> {
        reports
            .iter()
            .map(|report| {
                let what = match &report.event {
                    SurfaceEvent::Pointer { action, event, .. } => format!(
                        "{action:?} {:?} at ({}, {})",
                        event.button, event.position.x.0, event.position.y.0
                    ),
                    SurfaceEvent::Wheel { event, .. } => format!("Wheel {:?}", event.delta),
                    other => format!("{other:?}"),
                };
                (report.surface, what)
            })
            .collect()
    }

    #[test]
    fn a_pointer_event_goes_to_the_display_the_pointer_is_on() {
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = mouse();

        let reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[record(SINCE, EventType::EV_REL, Relative::REL_X.raw(), 10)],
        );

        assert_eq!(
            did(&reports),
            [(
                Some(SurfaceId::new(1)),
                "Moved None at (410, 300)".to_owned()
            )]
        );
    }

    #[test]
    fn a_pointer_that_crosses_tells_one_display_it_left_and_the_other_it_arrived() {
        // The two edges, in the order a hover state depends on. A display never told the pointer
        // left keeps whatever was under it highlighted for the rest of the program.
        let screens = screens();
        let mut pointer = Pointer::at(790.0, 100.0, &screens);
        let mut points = mouse();

        let reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[record(SINCE, EventType::EV_REL, Relative::REL_X.raw(), 20)],
        );

        assert_eq!(
            did(&reports),
            [
                (
                    Some(SurfaceId::new(1)),
                    "Left None at (790, 100)".to_owned()
                ),
                (
                    Some(SurfaceId::new(2)),
                    "Entered None at (10, 100)".to_owned()
                ),
            ],
            "and the place each carries is measured on its own display"
        );
    }

    #[test]
    fn a_press_is_delivered_where_the_pointer_ended_up() {
        // A batch can move and press at once, and a press delivered at the old place clicks
        // whatever used to be under the pointer.
        let screens = screens();
        let mut pointer = Pointer::at(100.0, 100.0, &screens);
        let mut points = mouse();

        let reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[
                record(SINCE, EventType::EV_REL, Relative::REL_X.raw(), 50),
                record(SINCE, EventType::EV_KEY, Key::BTN_LEFT.raw(), 1),
            ],
        );

        assert_eq!(
            did(&reports),
            [
                (
                    Some(SurfaceId::new(1)),
                    "Moved None at (150, 100)".to_owned()
                ),
                (
                    Some(SurfaceId::new(1)),
                    "Pressed Some(Primary) at (150, 100)".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn a_wheel_turn_is_reported_where_the_pointer_is() {
        // A wheel carries no position of its own on any device, so without the pointer's there is
        // nothing to route a turn to.
        let screens = screens();
        let mut pointer = Pointer::at(100.0, 100.0, &screens);
        let mut points = mouse();

        let reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[record(
                SINCE,
                EventType::EV_REL,
                Relative::REL_WHEEL.raw(),
                1,
            )],
        );

        assert_eq!(
            did(&reports),
            [(
                Some(SurfaceId::new(1)),
                "Wheel Lines { x: 0.0, y: -1.0 }".to_owned()
            )],
            "and the wheel moved the pointer nowhere"
        );
        let SurfaceEvent::Wheel { event, .. } = &reports[0].event else {
            panic!("a turn arrived: {reports:?}");
        };
        assert_eq!(event.position, Point::new(CssPx(100.0), CssPx(100.0)));
    }

    #[test]
    fn what_a_pointer_produces_is_what_a_document_is_dispatched() {
        // The last hop this crate can assert on its own, the way the keyboard asserts it: an event
        // that answers both halves of the contract's own bridge is one that reaches a document.
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = mouse();
        let mut reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[
                record(SINCE, EventType::EV_REL, Relative::REL_X.raw(), 5),
                record(SINCE, EventType::EV_KEY, Key::BTN_LEFT.raw(), 1),
            ],
        );
        reports.extend(point(
            &mut points,
            &mut pointer,
            &screens,
            &[
                record(SINCE, EventType::EV_KEY, Key::BTN_LEFT.raw(), 0),
                record(SINCE, EventType::EV_REL, Relative::REL_WHEEL.raw(), -1),
            ],
        ));

        let dispatched: Vec<_> = reports
            .iter()
            .map(|report| {
                assert!(report.event.is_input(), "{report:?} is what a person did");
                let (kind, payload) = report
                    .event
                    .to_dispatch()
                    .unwrap_or_else(|| panic!("{report:?} reaches a document"));
                assert!(
                    payload.matches(kind),
                    "{report:?} carries the wrong payload"
                );
                assert!(report.event.modifiers().is_some(), "and says what was held");
                assert!(report.event.timestamp().is_some(), "and when");
                kind
            })
            .collect();
        assert_eq!(
            dispatched,
            [
                EventKind::PointerMove,
                EventKind::PointerDown,
                EventKind::PointerUp,
                EventKind::Wheel,
            ]
        );
    }

    #[test]
    fn the_moment_a_pointer_event_carries_is_the_moment_the_kernel_stamped_it() {
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = mouse();

        let reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[record(
                SINCE + Duration::from_millis(250),
                EventType::EV_REL,
                Relative::REL_X.raw(),
                1,
            )],
        );

        // The batch's own moment is the one its terminating report carried, and `pointed` stamps
        // with that — so this asserts the report's moment rather than the record's.
        assert_eq!(
            reports[0].event.timestamp().map(Timestamp::since_origin),
            Some(Duration::ZERO),
            "the report that ended this update was stamped at the anchor"
        );
    }

    #[test]
    fn a_button_held_when_its_device_goes_ends_the_interaction_rather_than_firing_it() {
        // The kernel queues the release and then answers `ENODEV`, so that release is never read.
        // A control told about a release fires; a control told about a cancel gives up, and giving
        // up is what happened.
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = mouse();
        point(
            &mut points,
            &mut pointer,
            &screens,
            &[record(SINCE, EventType::EV_KEY, Key::BTN_LEFT.raw(), 1)],
        );

        let ended = cancelled(
            &mut points,
            &BTreeSet::new(),
            Modifiers::NONE,
            Stamps::from_origin(SINCE),
            &pointer,
            &screens,
        );

        assert_eq!(
            did(&ended),
            [(
                Some(SurfaceId::new(1)),
                "Cancelled Some(Primary) at (400, 300)".to_owned()
            )]
        );
        assert!(
            cancelled(
                &mut points,
                &BTreeSet::new(),
                Modifiers::NONE,
                Stamps::from_origin(SINCE),
                &pointer,
                &screens
            )
            .is_empty(),
            "and it is ended once"
        );
    }

    #[test]
    fn a_cancelled_interaction_is_stamped_when_it_was_noticed() {
        // The press that opened the interaction is stamped by the kernel, and the cancel has to be
        // comparable with it. The application's origin is hours earlier, so a control measuring an
        // interval across the two — a double-click window, a drag velocity, a gesture timeout —
        // reads a negative one and nothing downstream can tell.
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = mouse();
        let pressed = point(
            &mut points,
            &mut pointer,
            &screens,
            &[record(SINCE, EventType::EV_KEY, Key::BTN_LEFT.raw(), 1)],
        );
        let opened = pressed[0]
            .event
            .timestamp()
            .expect("a press carries a moment");

        // The anchor the loop would hold, taken against a clock that started five seconds ago.
        let clock = Aged::started(Duration::from_secs(5));
        let stamps = Stamps::anchored(&clock);
        let ended = cancelled(
            &mut points,
            &BTreeSet::new(),
            Modifiers::NONE,
            stamps,
            &pointer,
            &screens,
        );

        let closed = ended[0]
            .event
            .timestamp()
            .expect("a cancel carries one too");
        assert!(
            closed > opened,
            "the interaction ended at {closed:?}, after the press at {opened:?} that opened it"
        );
        assert!(
            (closed - opened) < Duration::from_secs(60),
            "and the interval between them is a moment rather than an age: {:?}",
            closed - opened
        );
    }

    #[test]
    fn a_button_the_kernel_still_reports_is_left_held() {
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = mouse();
        point(
            &mut points,
            &mut pointer,
            &screens,
            &[record(SINCE, EventType::EV_KEY, Key::BTN_LEFT.raw(), 1)],
        );

        let ended = cancelled(
            &mut points,
            &BTreeSet::from([Key::BTN_LEFT.raw()]),
            Modifiers::NONE,
            Stamps::from_origin(SINCE),
            &pointer,
            &screens,
        );

        assert!(ended.is_empty(), "{ended:?}");
    }

    #[test]
    fn an_absolute_device_puts_the_pointer_where_the_finger_is() {
        // The other half of the pointer, and the half no machine here has. A touchscreen says
        // where it is rather than how far it moved, so a backend that read only the relative axes
        // would leave the pointer wherever it already was and land the press there — in the middle
        // of the first display, whatever part of the glass was touched.
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = Pointing {
            axes: Axes::Absolute {
                x: Span {
                    minimum: 0,
                    maximum: 4095,
                },
                y: Span {
                    minimum: 0,
                    maximum: 4095,
                },
            },
            wheel: HighResolution::default(),
            down: BTreeSet::new(),
        };

        let reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[
                record(SINCE, EventType::EV_ABS, Absolute::ABS_X.raw(), 0),
                record(SINCE, EventType::EV_ABS, Absolute::ABS_Y.raw(), 4095),
                record(SINCE, EventType::EV_KEY, Key::BTN_TOUCH.raw(), 1),
            ],
        );

        assert_eq!(
            did(&reports),
            [
                (Some(SurfaceId::new(1)), "Moved None at (0, 599)".to_owned()),
                (
                    Some(SurfaceId::new(1)),
                    "Pressed Some(Primary) at (0, 599)".to_owned()
                ),
            ],
            "the bottom left corner of the first display, which is where the finger was"
        );
    }

    #[test]
    fn an_absolute_device_can_reach_the_display_beside_the_first() {
        // A tablet or a touchscreen states no display of its own — there is no session daemon here
        // to bind one to an output — so it drives the whole arrangement. The cost is stated where
        // `Pointer::moved_to` is written; what this pins is that the far end of the glass is the
        // far end of the row rather than the far end of the first display.
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = Pointing {
            axes: Axes::Absolute {
                x: Span {
                    minimum: 0,
                    maximum: 4095,
                },
                y: Span {
                    minimum: 0,
                    maximum: 4095,
                },
            },
            wheel: HighResolution::default(),
            down: BTreeSet::new(),
        };

        let reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[
                record(SINCE, EventType::EV_ABS, Absolute::ABS_X.raw(), 4095),
                record(SINCE, EventType::EV_ABS, Absolute::ABS_Y.raw(), 0),
            ],
        );

        assert_eq!(
            did(&reports),
            [
                (
                    Some(SurfaceId::new(1)),
                    "Left None at (400, 300)".to_owned()
                ),
                (
                    Some(SurfaceId::new(2)),
                    "Entered None at (799, 0)".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn an_update_that_moved_nothing_and_pressed_nothing_says_nothing() {
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);
        let mut points = mouse();

        let reports = point(
            &mut points,
            &mut pointer,
            &screens,
            &[record(SINCE, EventType::EV_MSC, 4, 0x0007_0004)],
        );

        assert!(reports.is_empty(), "{reports:?}");
    }

    #[test]
    fn a_node_this_seat_already_holds_is_taken_no_second_time() {
        // One hotplug names the same node twice: the kernel makes it, udev sets its ownership, and
        // this backend acts on both because the creation alone is too early to open it. A second
        // open is a second client on a device one client already grabbed, so the kernel refuses the
        // grab and a working keyboard is reported as one that will not be handed over.
        let held = [Path::new("/dev/input/event4")];
        let arrived = [
            PathBuf::from("/dev/input/event4"),
            PathBuf::from("/dev/input/event9"),
        ];

        assert_eq!(
            untaken(&held, &arrived),
            [PathBuf::from("/dev/input/event9")]
        );
    }

    #[test]
    fn a_node_made_again_under_a_name_this_seat_holds_waits_for_the_stale_one_to_go() {
        // The same path with a different device behind it. While the seat still holds the
        // descriptor that answers `ENODEV`, the arrival is refused — which is why `Seat::read`
        // drops the devices that stopped answering before it takes the ones that arrived.
        let arrived = [PathBuf::from("/dev/input/event4")];

        assert!(untaken(&[Path::new("/dev/input/event4")], &arrived).is_empty());
        assert_eq!(
            untaken(&[], &arrived),
            arrived,
            "and once it has gone, the node that took its place is opened"
        );
    }

    /// A directory a test makes nodes in, removed when it goes out of scope.
    ///
    /// Named after the test that asked for it, so two tests running at once do not share one.
    struct Scratch(PathBuf);

    impl Scratch {
        /// An empty directory of its own.
        fn new(test: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("zgui-platform-drm-{}-{test}", std::process::id()));
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

    /// A seat holding no device, watching `directory`.
    ///
    /// Built here rather than through [`Seat::open`], which grabs every device on the machine: a
    /// test that called it would take the keyboard the developer is typing on. Every field is this
    /// module's own, so what is exercised is the seat itself rather than a stand-in for it.
    fn watching(directory: &Path) -> Seat {
        Seat {
            devices: Vec::new(),
            keys: Keys::new(None),
            pending: Vec::new(),
            anchored: Stamps::from_origin(SINCE),
            watch: zgui_evdev::Watch::new_in(directory).ok(),
        }
    }

    #[test]
    fn the_watch_is_one_more_descriptor_the_loop_waits_on() {
        // The join that lets a device plugged in reach a program that is already running: the loop
        // parks on this beside the devices, so a node made in the directory ends a wait. Left out
        // of the set, the watch fills and nothing ever reads it.
        let root = Scratch::new("the_watch_is_one_more_descriptor_the_loop_waits_on");
        let seat = watching(&root.0);

        assert_eq!(seat.descriptors().count(), 1);

        let mut blind = watching(&root.0);
        blind.watch = None;
        assert_eq!(
            blind.descriptors().count(),
            0,
            "and a machine whose kernel refused the watch waits on its devices alone"
        );
    }

    #[test]
    fn a_read_takes_the_nodes_that_arrived() {
        // The other end of the same join. A report left in the queue is a device nothing takes, and
        // a descriptor left readable turns every later wait into a wait of no length.
        //
        // An ordinary file stands in for the node. It draws the report a device node draws and it
        // opens as no device, which is also what a node udev has not finished with does — so this
        // asserts that a node that will not open costs the loop nothing.
        let root = Scratch::new("a_read_takes_the_nodes_that_arrived");
        let mut seat = watching(&root.0);
        std::fs::write(root.0.join("event0"), []).expect("the node is made");

        let reports = seat.read(&mut Pointer::centred(&[]), &[]);

        assert!(
            reports.is_empty(),
            "an empty file is no device: {reports:?}"
        );
        assert!(
            seat.watch
                .as_ref()
                .expect("the directory can be watched")
                .arrived()
                .expect("the reports read")
                .is_empty(),
            "and the read is what drained the watch"
        );
    }

    #[test]
    fn a_pointer_event_with_no_claimed_display_goes_nowhere() {
        // A program that has claimed no display has nowhere for a pointer to be, so nothing is
        // reported rather than an event addressed to a surface that does not exist.
        let mut pointer = Pointer::centred(&[]);
        let mut points = mouse();

        let reports = point(
            &mut points,
            &mut pointer,
            &[],
            &[record(SINCE, EventType::EV_REL, Relative::REL_X.raw(), 10)],
        );

        assert!(reports.is_empty(), "{reports:?}");
    }
}
