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
//! # The session
//!
//! [`crate::session`] decides whether this run opens a node itself or is handed the descriptor. A
//! seated run asks the session daemon for each one, so an ordinary login shell reaches a keyboard
//! it is in no group for; a direct run opens the node itself, as this backend always did.
//!
//! A device that came from the daemon goes back to it through the session. Dropping the descriptor
//! leaves the daemon holding its record of the device, so the paths are opened and closed here
//! rather than anywhere else.
//!
//! **A session that is waiting for its terminal is handed every node revoked**, so a walk there
//! takes nothing at all. That is a state rather than a fault: each node is a device this run has
//! the moment somebody switches to this terminal. One line names the count, in place of one line
//! per node and two more saying the machine has no keyboard and no mouse. `Found` is the decision,
//! and [`Seat::take_again`] takes the devices when the terminal moves.
//!
//! A terminal switch is those two halves a moment apart. [`Seat::let_go`] gives every device back
//! and [`Seat::take_again`] opens them all again, and the loop puts the wait between them. The
//! give-back runs before every resume, including the one where the loop read both changes in one
//! turn and gave nothing back: [`Seat::take_again`] states why.
//!
//! # Terminal switches
//!
//! A grabbed keyboard is the only place `Ctrl+Alt+F2` reaches, because the daemon turns the console
//! keyboard off when it grants control. Both layouts read that chord as the terminal it asks for,
//! so [`Seat::read`] answers the terminal beside the reports, and [`Heard::answered`] hands it to
//! the session. The reports leave through that same call, so a turn that delivered them asked for
//! the terminal. The key that asked reaches no surface — [`Keys::key`] states why.
//!
//! Every cursor plane goes back through that call as well, just before the ask. It is the one
//! moment a session that is leaving is still active and still holds DRM master, and a plane nobody
//! claims keeps whatever was on it: [`Heard::answered`] states the rest.
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
//! The frame loop takes the card first and fails to start while a compositor holds it: a direct
//! run stops at `become_master`, and a seated one stops there too, because a seat that a daemon
//! refuses or never enables is a run that falls back to the direct shape. That ordering is the
//! interlock: a run on a busy machine cannot take the keyboard from the desktop, because it never
//! reaches the point where it would ask for one. [`Seat::open`] being called from the loop after
//! the card keeps it, and nothing else does.
//!
//! Opening the devices through the session leaves that route as it was, and adds a second gate on
//! the seated path: the daemon decides which devices this session may have, so a run it refuses
//! gets no descriptor to grab.
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
use zgui_vocab::{KeyState, Modifiers, PointerAction, ScrollDelta, Timestamp};

use crate::cursor::Planes;
use crate::input::keyboard;
use crate::input::keyboard::layout::{Layout, Reading};
use crate::input::keyboard::{code, layout};
use crate::input::pointer::{self, Axes, Pointer, Screen, Span};
use crate::input::wheel::{self, HighResolution};
use crate::session::{Session, Switched, Unopened};

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
    pub(crate) const fn focused(event: SurfaceEvent) -> Self {
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

/// What one read of the devices produced.
///
/// The two halves travel apart because they go to different places. The reports reach the
/// surfaces. The terminal reaches the session, because a key that asks for one is the session's the
/// way a key that toggles caps lock is the console driver's — and that key produces no report of
/// its own.
///
/// **The reports leave through [`Heard::answered`] and through no other route.** Both halves belong
/// to the same turn: the session has to be asked while this run still holds its devices, and a
/// caller that could take the reports on their own could drop the ask and leave a chord that does
/// nothing anybody can see. The cursor planes travel with the ask for the same reason —
/// [`Heard::answered`] is the one place they can be given back in time.
#[derive(Debug)]
pub struct Heard {
    /// What people did, for the surfaces each belongs to.
    reports: Vec<Report>,
    /// The terminal a key asked for, where a key asked for one.
    ///
    /// The last one asked for, when a read carried more than one. That is what a person asked for
    /// last, and a switch overrides whatever was asked for before it anyway.
    terminal: Option<u32>,
}

impl Heard {
    /// Asks `session` for the terminal a key asked for, and answers with the reports.
    ///
    /// A read where nobody pressed the chord asks for nothing. A session asked every turn would
    /// switch terminal on its own, and `planes` would be taken from a screen nobody is leaving.
    ///
    /// # Order
    ///
    /// Every cursor comes off its plane **before** the ask. This is the last moment the run is
    /// active and still holds DRM master, and by the time the terminal moving is reported both are
    /// gone. [`Planes`] and
    /// [`Cursor::give_the_plane_back`](crate::cursor::Cursor::give_the_plane_back) say the rest.
    ///
    /// A switch the session refused is a session that keeps the screen, so the planes are taken
    /// again. An ask that went out leaves them back and waits, and that wait has an end, because a
    /// daemon can take a request and move no terminal — [`Planes::wait_for_the_switch`] is where it
    /// starts.
    pub fn answered(self, session: &mut Session, planes: &mut dyn Planes) -> Vec<Report> {
        if let Some(terminal) = self.terminal {
            // Before the ask. A cursor plane this program enabled keeps this program's pointer
            // until somebody names that plane in a commit, and a session that draws its own pointer
            // into the frame names it never.
            planes.give_them_back();
            match session.switch(terminal) {
                Switched::Asked => planes.wait_for_the_switch(),
                // Every switch on the direct shape is refused, and a person there would otherwise
                // lose the pointer at the first press of the chord and never get it back.
                Switched::Refused => planes.take_them_again(),
            }
        }
        self.reports
    }
}

/// What one batch from one keyboard amounted to.
///
/// [`Heard`] over one device's key events, and it is apart from it for the same reason: the events
/// are the surfaces' and the terminal is the session's.
#[derive(Debug)]
pub struct Translated {
    /// What the surfaces are told.
    pub events: Vec<SurfaceEvent>,
    /// The terminal a key in this batch asked for.
    pub terminal: Option<u32>,
}

impl Translated {
    /// Creates the answer a batch that produced nothing at all gives.
    const fn nothing() -> Self {
        Self {
            events: Vec::new(),
            terminal: None,
        }
    }

    /// Takes what a later key amounted to as well.
    ///
    /// The events follow the ones already here, and the terminal follows the rule [`ask`] states:
    /// the last key that asked for one is the one a person asked for.
    fn and(&mut self, later: Self) {
        self.events.extend(later.events);
        ask(&mut self.terminal, later.terminal);
    }
}

/// Records the terminal a key asked for, where it asked for one.
///
/// **The last ask wins.** A person can press two chords inside one turn, and the second is the one
/// they asked for — a fold that kept the first would send them somewhere they had already left.
///
/// An ask of nothing leaves the answer where it is, so a chord followed by a hundred ordinary keys
/// still answers the chord. The two arguments have different types, so a caller cannot write them
/// the wrong way round.
pub(crate) fn ask(answered: &mut Option<u32>, asked: Option<u32>) {
    if asked.is_some() {
        *answered = asked;
    }
}

/// Which way a key moved.
///
/// The kernel's own stream reports a value that is read into one of these, and libinput reports two
/// of the three: how long a key is held before it repeats is a decision about a person rather than
/// about a device, so libinput makes none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
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

/// What one key event amounted to.
///
/// The reading is the layout's answer. Whether it moved is the device's own: a transition it had
/// already made is one the layout was told nothing about, and the two travel together so that a
/// caller cannot read one without the other.
#[derive(Debug)]
struct Struck {
    /// What the layout answered for the key.
    reading: Reading,
    /// Whether this event moved the key, as this device's own belief says.
    moved: bool,
}

/// What one device believes about its own keys.
///
/// One of these per keyboard, for the reason [`Keys`] states: a set shared between two of them
/// repairs one and leaves the other holding a key nothing is on.
#[derive(Debug, Default)]
pub struct Down {
    /// The codes this device has down.
    keys: BTreeSet<u16>,
    /// The codes whose press asked for a terminal, so reached no surface.
    ///
    /// A key stays here until it comes up, and every transition of it in between reaches no surface
    /// either. The press decides, because the terminal sits on the level the modifiers select: with
    /// control let go of first, the release of the same key reads as an ordinary `F1`, and would
    /// tell an application about half a chord. See [`Keys::key`].
    swallowed: BTreeSet<u16>,
}

impl Down {
    /// Returns `true` if this key's press reached no surface.
    fn swallows(&self, key: Key) -> bool {
        self.swallowed.contains(&key.raw())
    }

    /// Remembers that this key reached no surface, until it comes up.
    fn swallow(&mut self, key: Key) {
        self.swallowed.insert(key.raw());
    }

    /// Records the transition, and returns `true` if this device's belief moved.
    ///
    /// A press of a key it already holds moves nothing, and so does a release of one it does not
    /// hold. A repeat is no transition at all.
    fn record(&mut self, key: Key, transition: Transition) -> bool {
        match transition {
            Transition::Pressed => self.keys.insert(key.raw()),
            Transition::Released => {
                self.swallowed.remove(&key.raw());
                self.keys.remove(&key.raw())
            }
            Transition::Repeated => false,
        }
    }
}

/// The layout, what it last reported held, and what a batch of events means.
///
/// The whole translation, and it holds no device, so every part of it can be exercised over bytes
/// written by hand.
///
/// ```
/// use std::time::Duration;
/// use zgui_evdev::Key;
/// use zgui_platform_drm::input::seat::{Down, Keys, Transition};
/// use zgui_vocab::Timestamp;
///
/// // No layout, so a key arrives by its position and types nothing.
/// let mut keys = Keys::new(None);
/// let mut down = Down::default();
/// let at = Timestamp::from_origin(Duration::from_millis(250));
///
/// let struck = keys.key(&mut down, Key::KEY_A, Transition::Pressed, at);
///
/// assert_eq!(struck.events.len(), 1);
/// assert_eq!(struck.terminal, None, "an ordinary key asks for no terminal");
/// ```
///
/// # One set of held keys per device
///
/// Every method here takes the calling device's own [`Down`]. One layout serves every keyboard
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
    /// * **A device is gone.** Either it answered a read with a failure, or it was given back so
    ///   that it can be opened again. `held` is then empty, because a device that is gone holds
    ///   nothing. The releases the kernel queued for it are never read, so without this a modifier
    ///   held while it was unplugged stays held for the rest of the process, and every later key
    ///   struck on another keyboard comes out shifted with no way back.
    ///
    /// A key `down` holds and `held` does not is released; a key `held` reports and `down` has not
    /// seen is recorded down with no reading taken. Neither is a key press, because nobody pressed
    /// anything: what changed is what this process knows, so what comes back is at most a change in
    /// the held set.
    ///
    /// **A key the repair recorded reaches no surface where it asks for a terminal.** A chord held
    /// through a terminal switch is held while the devices are opened again, and its release is the
    /// first thing this process hears about it — so the layout is asked what each recorded key means
    /// now, and one that asks for a terminal is swallowed the way a press of it would have been.
    pub fn resynchronise(&mut self, down: &mut Down, held: &BTreeSet<u16>) -> Option<SurfaceEvent> {
        let held: BTreeSet<u16> = held
            .iter()
            .copied()
            .filter(|code| typed(Key::new(*code)))
            .collect();
        let recorded: Vec<u16> = held.difference(&down.keys).copied().collect();
        if let Some(layout) = self.layout.as_mut() {
            for code in down.keys.difference(&held) {
                layout.release(Key::new(*code));
            }
            for code in &recorded {
                layout.hold(Key::new(*code));
            }
        }
        down.keys = held;
        // A key that is no longer down is no longer swallowed, whichever way it left.
        down.swallowed.retain(|code| down.keys.contains(code));
        // After every hold, because what a key means is read under the modifiers the repair has
        // just recorded.
        if let Some(layout) = self.layout.as_ref() {
            for code in recorded {
                let key = Key::new(code);
                if layout.reading(key).terminal.is_some() {
                    down.swallowed.insert(code);
                }
            }
        }
        self.announce()
    }

    /// Translates one batch from one device.
    ///
    /// A batch is one coherent update, and a key event in it is a press, a release or a repeat.
    /// Everything else the batch carries — a relative axis, a scan code, a button — belongs to work
    /// this backend has not done and is left alone.
    ///
    /// `stamps` is how this device's moments are read. It belongs to the device rather than to the
    /// seat because the choice does: a driver may refuse the monotonic clock for one device and
    /// accept it for the next.
    ///
    /// A batch that carries a `SYN_DROPPED` is answered with nothing. The caller then asks the
    /// device what it holds now, and [`Keys::resynchronise`] is the repair.
    ///
    /// Every key in the batch goes through [`Keys::key`], which states what a key that asks for a
    /// terminal does. The terminal this answers with is the last one a key in the batch asked for.
    pub fn batch(&mut self, down: &mut Down, batch: &Batch, stamps: Stamps) -> Translated {
        if dropped(batch) {
            return Translated::nothing();
        }
        let mut translated = Translated::nothing();
        for event in &batch.events {
            let Some(key) = event.key().filter(|key| typed(*key)) else {
                continue;
            };
            translated.and(self.key(down, key, Transition::of(event.value), stamps.at(event.at)));
        }
        translated
    }

    /// Translates one key moving.
    ///
    /// The whole translation, and [`Keys::batch`] is a loop over it. The two callers differ in what
    /// they read a key out of: the kernel's own stream reports a code and a value, and libinput
    /// reports a code and which way it went. Neither difference reaches this far.
    ///
    /// # Keys that ask for a terminal
    ///
    /// The layout says which key asks for one, and both layouts do — `Ctrl+Alt+F1` is its own
    /// keysym under libxkbcommon and a `KT_CONS` entry in a console keymap, so it has never
    /// produced `F1`. Such a key is answered here as the terminal and as no event: it belongs to
    /// the session, the way a key that toggles caps lock belongs to the console driver, and an
    /// application that was told about it as well would act on a chord that is already taking the
    /// screen away.
    ///
    /// **Only a press the device had not already made asks.** A repeat asks for nothing, so a chord
    /// held down asks once rather than thirty times a second. A press of a key this device already
    /// holds asks for nothing either: the layout is told nothing about one, so it is a press nobody
    /// made. Such an event reaches any machine — [`Device::open`](zgui_evdev::Device::open) starts
    /// the kernel queuing events to this client and the read of `EVIOCGKEY` follows it, so a key
    /// struck in between arrives twice — and every resume opens every device again.
    ///
    /// **The press decides, and the key is remembered until it comes up.** The terminal sits on the
    /// level the modifiers select, so a chord taken apart before the finger leaves the key reads as
    /// an ordinary `F1` again, and the release would reach the application with no press behind it.
    /// [`Down`] holds the codes that were swallowed, so the release is swallowed with the press and
    /// an application is never told about half of a chord. A chord held through a terminal switch
    /// comes back as a release with no press behind it, and [`Keys::resynchronise`] swallows that
    /// one for the same reason.
    ///
    /// The transition is recorded either way. A key the layout was never told about would leave the
    /// count of a modifier held over the chord wrong for the rest of the run.
    pub fn key(
        &mut self,
        down: &mut Down,
        key: Key,
        transition: Transition,
        at: Timestamp,
    ) -> Translated {
        // Read before the transition is recorded, because recording a release takes the key out of
        // the swallowed set.
        let swallowed = down.swallows(key);
        let Struck { reading, moved } = self.read(down, key, transition);

        if transition == Transition::Pressed && reading.terminal.is_some() {
            down.swallow(key);
            // No modifier is read here, because no key that asks for a terminal is a modifier: one
            // keymap entry holds one meaning, and this one holds the terminal.
            return Translated {
                events: Vec::new(),
                // A transition the layout was told nothing about is a transition nobody made.
                terminal: if moved { reading.terminal } else { None },
            };
        }
        if swallowed {
            return Translated::nothing();
        }

        let (state, repeat) = match transition {
            Transition::Pressed => (KeyState::Pressed, false),
            Transition::Repeated => (KeyState::Pressed, true),
            Transition::Released => (KeyState::Released, false),
        };
        // The held set is read *after* the transition is recorded, so the press of shift carries
        // shift and its release carries nothing. That is what a browser reports, and what a handler
        // reading the modifiers off a key event expects.
        let modifiers = self
            .layout
            .as_ref()
            .map_or(Modifiers::NONE, |layout| layout.modifiers());

        let mut events = Vec::new();
        // Before the key, because the state a key was struck in is announced before the event that
        // happened in it.
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
            timestamp: at,
        });

        Translated {
            events,
            terminal: None,
        }
    }

    /// Reads the layout for one transition, and records the transition where there is one.
    ///
    /// The device's own [`Down`] decides whether the layout is told at all. A press of a key that
    /// device already has down records nothing, and a release of one it does not have down records
    /// nothing either — so the layout's count follows the keyboard rather than the stream. Whether
    /// it moved is answered as well, because a transition the layout was told nothing about is a
    /// transition nobody made: [`Keys::key`] asks for no terminal on one.
    ///
    /// The press case is reachable on any machine: `zgui_evdev::Device::open` starts the kernel
    /// queuing events to this client, and the grab and the read of `EVIOCGKEY` happen after it, so
    /// a key struck in between arrives through the map *and* through the stream. Counted twice, it
    /// needs two releases and gets one.
    fn read(&mut self, down: &mut Down, key: Key, transition: Transition) -> Struck {
        let moved = down.record(key, transition);
        let Some(layout) = self.layout.as_mut() else {
            // No layout at all. The position still crosses, so a binding written against where the
            // key sits keeps working. No terminal either: which chord asks for one is a layout's
            // answer, and a machine with no layout has none to give.
            return Struck {
                reading: Reading {
                    key: zgui_vocab::Key::Unidentified,
                    without_modifiers: zgui_vocab::Key::Unidentified,
                    terminal: None,
                },
                moved,
            };
        };
        let reading = match transition {
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
        };
        Struck { reading, moved }
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

/// What one node of the device directory amounted to.
#[derive(Debug)]
struct Opened {
    /// What was already held on the device, where one was taken.
    announced: Option<SurfaceEvent>,
    /// Whether the node is an input device this session cannot read yet.
    ///
    /// See [`Unopened::NotYet`]. It is counted rather than reported, because a session waiting for
    /// its terminal answers this for every node on the machine.
    not_yet: bool,
}

impl Opened {
    /// A node that opened no device, and whose reason has been reported.
    const NOTHING: Self = Self {
        announced: None,
        not_yet: false,
    };

    /// A node that is an input device this session cannot read yet.
    const NOT_YET: Self = Self {
        announced: None,
        not_yet: true,
    };
}

/// What a walk of the device directory has to say for itself.
///
/// The decision is here, apart from the walk, because it is arithmetic over three answers and the
/// walk needs a session daemon and a machine full of devices. [`Seat::report`] writes each of these
/// out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    /// Every input device is another session's until somebody switches to this terminal.
    ///
    /// The count is how many nodes answered that way, and it keeps the report to one line in place
    /// of one line for each of them.
    Waiting(usize),
    /// What this walk took, as the two things it can be missing.
    Took {
        /// Whether somebody can type into this program.
        keyboard: bool,
        /// Whether somebody can move the cursor.
        pointer: bool,
    },
}

impl Found {
    /// Decides what a walk that met `waiting` nodes it cannot read yet, and took these devices,
    /// amounts to.
    ///
    /// **A node that answers nothing yet decides the report only where nothing was taken.** A
    /// session waiting for its terminal is handed every one of its devices revoked, so it holds
    /// none at all; a walk that took a keyboard and met a revoked node is a machine where one
    /// device has gone, and the warnings are the right report there.
    ///
    /// The two log levels rest on that difference. A session that is waiting states something that
    /// ends the moment a person switches to this terminal; a session that holds the screen and
    /// found no keyboard states something that lasts for the rest of the run.
    const fn of(waiting: usize, keyboard: bool, pointer: bool) -> Self {
        if waiting > 0 && !keyboard && !pointer {
            return Self::Waiting(waiting);
        }
        Self::Took { keyboard, pointer }
    }
}

/// One device this seat took.
struct Taken {
    /// The device, grabbed for as long as this lives.
    device: Device,
    /// Which of its keys this seat believes are down, and which of them it swallowed.
    down: Down,
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
    /// The directory the devices come from.
    ///
    /// Kept because a session that comes back walks it again. See [`Seat::take_again`] for why a
    /// resume walks the directory rather than a list of the paths that were held.
    directory: PathBuf,
}

impl Seat {
    /// Opens every device the session hands over, takes each one, finds a layout, and starts
    /// watching for the devices that arrive afterwards.
    ///
    /// **Call this after DRM master has been taken.** The grab comes after the card, and that
    /// ordering is the safety interlock: a run on a busy machine fails at the card, so it never
    /// reaches the point where it would take the keyboard from the desktop. The module
    /// documentation states the rest.
    ///
    /// Nothing here fails. A machine with no readable device and no layout is a console that draws
    /// and cannot be typed into, the way this backend was before this existed, and every refusal is
    /// reported through the crate's log rather than turned into an error the frame loop would have
    /// to decide about.
    pub fn open(session: &mut Session, clock: &dyn Clock) -> Self {
        Self::open_in(session, clock, Path::new(zgui_evdev::DIRECTORY))
    }

    /// Takes every device in `directory`.
    ///
    /// [`Seat::open`] is this over the directory the kernel puts input devices in, and that is the
    /// directory a run walks. The directory is a parameter so that a test can hand this one holding
    /// devices it chose: a walk of the real one grabs every keyboard on the machine, and a grab
    /// lasts for as long as the seat does. It is reachable across the crate because the frame
    /// loop's own tests need a seat that holds nothing and takes nothing.
    pub(crate) fn open_in(session: &mut Session, clock: &dyn Clock, directory: &Path) -> Self {
        let found = layout::find();
        info!(
            target: "zgui::platform",
            "this machine's keyboard is {}", found.stated
        );
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
        let watch = match Watch::new_in(directory) {
            Ok(watch) => Some(watch),
            Err(error) => {
                warn!(
                    target: "zgui::platform",
                    "{} cannot be watched, so a keyboard or a mouse plugged in while this runs \
                     reaches nothing: {error}",
                    directory.display()
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
            directory: directory.to_owned(),
        };
        let announced = seat.walk(session);
        seat.pending.extend(announced);
        seat
    }

    /// Opens every node in this seat's directory through the session, and takes what it can.
    ///
    /// The nodes are listed here and opened through the session, rather than opened by the walk
    /// itself. Which devices this run may have is the session's answer: a seated run is handed each
    /// one by the daemon that owns the terminal, and on the ordinary machine that is the only way it
    /// gets one at all, because an `/dev/input/event*` node belongs to the `input` group.
    ///
    /// **A node this seat already holds is left where it is.** Nothing here checks: what refuses
    /// the second open is the session, which holds a device at that path and answers every later
    /// ask for it with a refusal. So a walk over a set that is part taken opens what is left and
    /// repairs nothing that is held — and after a terminal switch what is held is a set of revoked
    /// descriptors, which a walk alone would leave revoked. [`Seat::take_again`] states the
    /// give-back that has to run first.
    ///
    /// What comes back is what was already held on the devices that were taken. Two runs of this
    /// exist: the open, which puts them where the first read will report them, and the resume, which
    /// hands them to the loop.
    fn walk(&mut self, session: &mut Session) -> Vec<SurfaceEvent> {
        let listed = zgui_evdev::nodes_in(&self.directory);
        let mut announced = Vec::new();
        let mut waiting = 0;
        match listed {
            Ok(nodes) => {
                for path in &nodes {
                    let opened = self.take_node(session, path);
                    announced.extend(opened.announced);
                    waiting += usize::from(opened.not_yet);
                }
            }
            Err(error) => warn!(
                target: "zgui::platform",
                "no input device can be found on this machine: {error}"
            ),
        }
        self.report(waiting);
        announced
    }

    /// Says what this walk left the person at the machine with.
    ///
    /// [`Found::of`] is the decision. A session that is waiting for its terminal answers one line
    /// naming the count, at `info`, because that state ends the moment a person switches to this
    /// terminal; a session that holds the screen and found no keyboard answers the warning,
    /// because that is a machine nobody can type into for the rest of the run.
    fn report(&self, waiting: usize) {
        match Found::of(
            waiting,
            self.devices.iter().any(|taken| taken.types),
            self.devices.iter().any(|taken| taken.points.is_some()),
        ) {
            Found::Waiting(nodes) => info!(
                target: "zgui::platform",
                "this run holds no input device at all, and {nodes} of the nodes it was handed \
                 answer nothing yet: a session waiting for its terminal is handed every one of its \
                 devices revoked, and they arrive when somebody switches to this terminal"
            ),
            Found::Took { keyboard, pointer } => {
                if !keyboard {
                    warn!(
                        target: "zgui::platform",
                        "no keyboard on this machine could be taken, so nothing can be typed into \
                         this program"
                    );
                }
                if !pointer {
                    warn!(
                        target: "zgui::platform",
                        "no pointing device on this machine could be taken, so the cursor cannot \
                         be moved"
                    );
                }
            }
        }
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
            down: Down::default(),
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
    fn arrivals(&mut self, session: &mut Session) -> Vec<Report> {
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
            .filter_map(|path| self.take_node(session, path).announced)
            .map(Report::focused)
            .collect()
    }

    /// Opens the node at `path` through the session and takes it, reporting what was already held
    /// on it.
    ///
    /// A refusal is reported and the node is left where it is. Two of them are ordinary. The first
    /// is a node this run may not have, which most of `/dev/input` is on the direct path and which
    /// a session daemon answers for a device it keeps. The second is a node udev has not finished
    /// with, which refuses everybody; the change that says it has finished is one more report the
    /// watch brings, so that one is the first of two tries rather than a device lost.
    ///
    /// **A node this session cannot read yet is counted rather than reported.** It is an input
    /// device, and a session waiting for its terminal is handed every one of them that way, so a
    /// line each states that a machine full of keyboards has none. [`Seat::report`] is the one line
    /// they amount to.
    ///
    /// **A node that opened and was left alone goes straight back.** Most of `/dev/input` is a
    /// device nobody types on and nobody points with — a power button, a lid switch, an
    /// accelerometer — and each one the seat declines is a device the daemon opened. Dropping it
    /// closes the descriptor and leaves the record, so the session is told here.
    fn take_node(&mut self, session: &mut Session, path: &Path) -> Opened {
        let device = match session.open_input(path) {
            Ok(device) => device,
            Err(Unopened::NotYet(_)) => return Opened::NOT_YET,
            Err(Unopened::Refused(error)) => {
                info!(target: "zgui::platform", "{error}");
                return Opened::NOTHING;
            }
        };

        let announced = self.admit(device);
        if !self.holds(path) {
            session.close_input(path);
        }
        Opened {
            announced,
            not_yet: false,
        }
    }

    /// Returns `true` if this seat holds a device at `path`.
    ///
    /// The path identifies a device here, for the reason [`untaken`] states: the kernel gives one
    /// node to one device at a time.
    fn holds(&self, path: &Path) -> bool {
        self.devices.iter().any(|taken| taken.device.path() == path)
    }

    /// Reads every device and reports what a person did, moving `pointer` as they moved it.
    ///
    /// It also reads the watch, so this is where the set of devices changes: one that stopped
    /// answering is dropped and one that has arrived is taken, in that order.
    ///
    /// `screens` is the ground the pointer moves over, so it decides which surface each pointer
    /// event belongs to and how far the pointer can go. It is passed in rather than kept because
    /// it changes with what the application has claimed, and a seat holding a stale copy would
    /// clamp a pointer to a display that is no longer being drawn.
    ///
    /// A device that answers a read with a failure is dropped, and its keys and its buttons are
    /// let go. Any errno is treated that way, because the one that matters cannot be told from the
    /// others by anything this loop could do differently: an unplugged device and a descriptor
    /// `logind` revoked both answer `ENODEV`, and both then answer every later read the same way
    /// while `poll` reports the descriptor permanently ready — so a loop that kept one would spin
    /// at the speed of the processor for as long as it ran. A device dropped over a passing failure
    /// costs a keyboard or a mouse that has to be plugged in again; a device kept costs the whole
    /// program.
    ///
    /// A key that asked for a terminal comes back beside the reports rather than among them. See
    /// [`Heard`], and [`Keys::key`] for why such a key reaches no surface.
    pub fn read(
        &mut self,
        session: &mut Session,
        pointer: &mut Pointer,
        screens: &[Screen],
    ) -> Heard {
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
        let mut terminal = None;
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
                    let translated = keys.batch(&mut taken.down, batch, taken.stamps);
                    ask(&mut terminal, translated.terminal);
                    reports.extend(translated.events.into_iter().map(Report::focused));
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
                                &mut points.down,
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
            reports.extend(let_go_of(&mut gone, keys, pointer, screens));
            let path = gone.device.path().to_owned();
            // The descriptor closes here, and the session is told after it. A daemon told while
            // this process still held a name on the device would release one that is still
            // grabbed.
            drop(gone);
            session.close_input(&path);
        }
        // After the devices that stopped answering have gone. A node removed and made again under
        // the same name is a different device at the same path, and an arrival at a path this seat
        // still holds is refused — so a stale one left here would keep its own replacement out.
        reports.extend(self.arrivals(session));
        Heard { reports, terminal }
    }

    /// Gives every device back to the session, and reports what each one was holding.
    ///
    /// The first half of a terminal switch. Everything is already gone by the time it runs: logind
    /// has revoked every evdev descriptor with `EVIOCREVOKE`, which cannot be undone, so there is
    /// nothing to save and nothing to wait for.
    ///
    /// **The devices go back through the session rather than by being dropped.** Dropping one closes
    /// a descriptor and leaves the daemon holding its record of the device until the seat closes,
    /// which on this path is one record per device per switch.
    ///
    /// Each device goes the way a device that stopped answering goes: its keys come off the layout
    /// and the interactions its buttons were holding open are ended. Without that, a modifier held
    /// while the terminal moved stays held for the rest of the program and a control gripped by the
    /// pointer never lets go.
    ///
    /// On the libinput source the devices go back through the context, and what each was holding is
    /// repaired in the read that follows, so nothing comes back here.
    ///
    /// The watch is untouched. It is this crate's own inotify rather than a device the seat opened,
    /// and no session ever takes it.
    pub fn let_go(
        &mut self,
        session: &mut Session,
        pointer: &Pointer,
        screens: &[Screen],
    ) -> Vec<Report> {
        let mut reports = Vec::new();
        for mut gone in std::mem::take(&mut self.devices) {
            reports.extend(let_go_of(&mut gone, &mut self.keys, pointer, screens));
        }
        // After the loop above, which is where the last descriptor onto each device closed. The
        // session tells the daemon here, and a daemon told while this process still held a
        // descriptor would release a device that is still grabbed.
        session.close_every_input();
        reports
    }

    /// Opens every device again, and reports what is held on each.
    ///
    /// The second half of a terminal switch. Each device arrives the way a device plugged in
    /// arrives, which asks the kernel what is held on it after the grab — so a modifier somebody
    /// kept a finger on is read again rather than counted twice.
    ///
    /// On the libinput source the devices are opened again through the context, and what is held on
    /// each is reported in the read that follows, so nothing comes back here.
    ///
    /// # Order
    ///
    /// [`Seat::let_go`] runs first, on **every** resume, and the two are never folded together.
    /// seatd's `seat_open_device` answers the *same* device id with its reference count raised for
    /// a path the client already holds, so an open that came first would get one id where it
    /// expected two, and the first close would release the device out from under the second. The
    /// grab says the same thing from the kernel's side: it is exclusive, so a second open of a node
    /// this process still holds is a grab that is refused.
    ///
    /// **"Every resume" includes the one where nothing was given back.** A turn that reads a
    /// disable and an enable together is one resume, and it runs with every device still held —
    /// each of them revoked, because logind revokes before it reports. A resume that opened first
    /// there would be refused every path the session holds, take nothing, and leave the program
    /// with no keyboard and no pointer for the rest of the run. The frame loop's `resume` keeps the
    /// order, and it gives back whether or not it holds anything.
    ///
    /// # The walk
    ///
    /// The directory is walked again rather than a list of what was held, and two machines need
    /// that. A run started on a terminal that is not the live one held **nothing** at start-up —
    /// the daemon hands over an evdev node it has already revoked, so the walk took none of them —
    /// and a list of what was held would be empty for the whole run. A keyboard plugged in while
    /// this session was away is in the directory and in no such list.
    ///
    /// What it costs is one open and one give-back per node this seat declines, per switch. That is
    /// the same walk [`Seat::open`] pays for at start-up, once per terminal switch, which is a
    /// person's own action.
    pub fn take_again(&mut self, session: &mut Session) -> Vec<Report> {
        self.walk(session)
            .into_iter()
            .map(Report::focused)
            .collect()
    }
}

/// Lets go of everything a device that is no longer held was holding.
///
/// A device that is gone holds nothing. The releases the kernel queued for it are never read, so
/// this is the only thing that takes its keys back off the layout and ends the interactions its
/// buttons were holding open. Without it a modifier held while its keyboard went stays held for the
/// rest of the program, and a control gripped by a vanished mouse never lets go.
///
/// Two things reach this: a device that answered a read with a failure, and a device given back so
/// that it can be opened again.
fn let_go_of(
    gone: &mut Taken,
    keys: &mut Keys,
    pointer: &Pointer,
    screens: &[Screen],
) -> Vec<Report> {
    let mut reports = Vec::new();
    if let Some(points) = gone.points.as_mut() {
        reports.extend(cancelled(
            &mut points.down,
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
    reports
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
    moved(
        &motion,
        turned,
        stamps.at(batch.at),
        modifiers,
        pointer,
        screens,
    )
}

/// Translates an update that has already been read, and moves the pointer with it.
///
/// [`pointed`] is this over one batch of the kernel's own stream. A source that reports a movement
/// already accelerated and a scroll already in its own unit calls this instead: what a pointer does
/// with an update is the same question however the update was read, and it is answered once.
pub(crate) fn moved(
    motion: &pointer::Motion,
    turned: Option<ScrollDelta>,
    timestamp: Timestamp,
    modifiers: Modifiers,
    pointer: &mut Pointer,
    screens: &[Screen],
) -> Vec<Report> {
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

    let mut reports = Vec::new();
    let report = |action, button, surface, at| {
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
            reports.push(report(PointerAction::Left, None, left, was));
            reports.push(report(PointerAction::Entered, None, surface, at));
        }
        Some((_, was)) if was != at => {
            reports.push(report(PointerAction::Moved, None, surface, at));
        }
        // The pointer stayed where it was. A button or a wheel turn in this batch is still
        // reported below, at the place the pointer is.
        _ => {}
    }
    for (button, action) in &motion.buttons {
        reports.push(report(*action, Some(*button), surface, at));
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
pub(crate) fn cancelled(
    down: &mut BTreeSet<u16>,
    held: &BTreeSet<u16>,
    modifiers: Modifiers,
    stamps: Stamps,
    pointer: &Pointer,
    screens: &[Screen],
) -> Vec<Report> {
    let ended: Vec<u16> = down.difference(held).copied().collect();
    down.retain(|code| held.contains(code));
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
    //!
    //! # The walk's report
    //!
    //! [`Found::of`] is a function of three answers, so every line a walk can write is reachable
    //! here. **The join is not.** A node that answers nothing yet needs a daemon that revokes a
    //! descriptor before it hands it over, and the direct shape a test can build opens each node
    //! itself. So what is asserted here is the decision; that a revoked descriptor produces it is
    //! `zgui-evdev`'s `a_revoked_descriptor_is_an_input_device_that_answers_nothing_yet`, and that
    //! the session reads that refusal as a device arriving later is `crate::session`'s own test.

    use std::collections::BTreeSet;
    use std::ffi::{c_int, c_ulong};
    use std::path::{Path, PathBuf};

    use rustix::fd::{AsFd, AsRawFd, BorrowedFd};
    use rustix::ioctl::{Opcode, opcode};

    use super::{
        Axes, Down, Found, Heard, HighResolution, Keys, Planes, Pointer, Pointing, Report, Screen,
        Seat, Session, Span, Stamps, Transition, Translated, ask, cancelled, focused, pointed,
        types_on, untaken,
    };
    use crate::input::keyboard::layout::{Layout, Reading, Source};
    use crate::session::{Asked, Record};
    use std::time::Duration;
    use zgui_evdev::{
        Absolute, Bitmap, Capabilities, Device, EventType, Key, Reader, Relative, Synchronisation,
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
        ///
        /// `KEY_F1` asks for terminal 1, and that is what both real layouts answer for
        /// `Ctrl+Alt+F1`. Which modifiers reach that answer is each layout's own business — level
        /// five of a `CTRL+ALT` key type under xkb, the map those two bits select in a console
        /// keymap — so this answers it on the key alone, and the tests below drive the translation
        /// over it.
        fn reading_of(&self, key: Key) -> Reading {
            let (upper, lower) = match key {
                Key::KEY_A => ("A", "a"),
                Key::KEY_SPACE => (" ", " "),
                Key::KEY_F1 => {
                    return Reading {
                        key: zgui_vocab::Key::Named(NamedKey::F1),
                        without_modifiers: zgui_vocab::Key::Named(NamedKey::F1),
                        terminal: Some(1),
                    };
                }
                Key::KEY_LEFTSHIFT => {
                    return Reading {
                        key: zgui_vocab::Key::Named(NamedKey::Shift),
                        without_modifiers: zgui_vocab::Key::Named(NamedKey::Shift),
                        terminal: None,
                    };
                }
                _ => {
                    return Reading {
                        key: zgui_vocab::Key::Unidentified,
                        without_modifiers: zgui_vocab::Key::Unidentified,
                        terminal: None,
                    };
                }
            };
            Reading {
                key: zgui_vocab::Key::character(if self.shift > 0 { upper } else { lower }),
                without_modifiers: zgui_vocab::Key::character(lower),
                terminal: None,
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
    fn keys() -> (Keys, Shared, Down) {
        let shared = Shared::default();
        (
            Keys::new(Some(Box::new(shared.clone()))),
            shared,
            Down::default(),
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
    fn translate(keys: &mut Keys, down: &mut Down, bytes: &[u8]) -> Vec<SurfaceEvent> {
        translated(keys, down, bytes).events
    }

    /// The same, with the terminal a key in it asked for.
    fn translated(keys: &mut Keys, down: &mut Down, bytes: &[u8]) -> Translated {
        let mut reader = Reader::new();
        let batches = reader.feed(bytes);
        let mut events = Vec::new();
        let mut terminal = None;
        for batch in &batches {
            let mut answered = keys.batch(down, batch, Stamps::from_origin(SINCE));
            ask(&mut terminal, answered.terminal);
            events.append(&mut answered.events);
        }
        Translated { events, terminal }
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
    fn a_key_that_asks_for_a_terminal_is_answered_as_one_and_reaches_no_surface() {
        // The first half of the switch. A key that asks for a terminal belongs to the session, the
        // way a key that toggles caps lock belongs to the console driver — and an application told
        // about it as well would act on a chord that is already taking the screen away.
        let (mut keys, _, mut down) = keys();

        let asked = translated(&mut keys, &mut down, &moved(SINCE, Key::KEY_F1, 1));
        let ordinary = translated(&mut keys, &mut down, &moved(SINCE, Key::KEY_A, 1));

        assert_eq!(asked.terminal, Some(1), "the terminal the layout read");
        assert!(
            presses(&asked.events).is_empty(),
            "and nothing was dispatched for it: {:?}",
            asked.events
        );
        assert_eq!(
            ordinary.terminal, None,
            "an ordinary key asks for no terminal"
        );
        assert_eq!(
            presses(&ordinary.events).len(),
            1,
            "and reaches the surface: {:?}",
            ordinary.events
        );
    }

    #[test]
    fn a_chord_held_down_asks_for_its_terminal_once() {
        // The kernel repeats a held key thirty times a second, and a switch asked for on each of
        // them is thirty requests to the session daemon for one press of one chord. The release is
        // no ask either, and neither of them reaches a surface: an application told about the
        // release alone would have half a chord.
        let (mut keys, _, mut down) = keys();
        let mut bytes = moved(SINCE, Key::KEY_F1, 1);
        for _ in 0..8 {
            bytes.extend(moved(SINCE, Key::KEY_F1, 2));
        }
        bytes.extend(moved(SINCE, Key::KEY_F1, 0));
        let mut reader = Reader::new();
        let batches = reader.feed(&bytes);

        // One answer per batch, so that a repeat that asked again is visible. Folding them would
        // read the same as asking once.
        let asked: Vec<Translated> = batches
            .iter()
            .map(|batch| keys.batch(&mut down, batch, Stamps::from_origin(SINCE)))
            .collect();

        let terminals: Vec<Option<u32>> = asked.iter().map(|answer| answer.terminal).collect();
        assert_eq!(
            terminals.first(),
            Some(&Some(1)),
            "the press asked: {terminals:?}"
        );
        assert!(
            terminals[1..].iter().all(Option::is_none),
            "and every repeat after it, and the release, asked for nothing: {terminals:?}"
        );
        assert!(
            asked
                .iter()
                .all(|answer| presses(&answer.events).is_empty()),
            "no transition of it reached a surface"
        );
        assert!(
            down.keys.is_empty(),
            "and the key came up on the way, so the layout's count is where it started"
        );
    }

    /// A layout whose `F1` and `F2` ask for a terminal only while control is down.
    ///
    /// Both real layouts read the terminal off the level the modifiers select, and [`Recording`]
    /// answers on the key alone — so a chord taken apart cannot be driven through that one.
    #[derive(Debug, Default)]
    struct Chorded {
        /// How many times control was recorded down, less the times it was recorded up.
        control: i32,
    }

    impl Layout for Chorded {
        fn source(&self) -> Source {
            Source::Xkb
        }

        fn describe(&self) -> String {
            "a layout whose terminal sits under control".to_owned()
        }

        fn press(&mut self, key: Key) -> Reading {
            let reading = self.reading(key);
            if key == Key::KEY_LEFTCTRL {
                self.control += 1;
            }
            reading
        }

        fn reading(&self, key: Key) -> Reading {
            let asked = match key {
                Key::KEY_F1 => Some(1),
                Key::KEY_F2 => Some(2),
                _ => None,
            };
            let named = match key {
                Key::KEY_F1 => zgui_vocab::Key::Named(NamedKey::F1),
                Key::KEY_F2 => zgui_vocab::Key::Named(NamedKey::F2),
                Key::KEY_LEFTCTRL => zgui_vocab::Key::Named(NamedKey::Control),
                _ => zgui_vocab::Key::Unidentified,
            };
            Reading {
                key: named.clone(),
                without_modifiers: named,
                terminal: asked.filter(|_| self.control > 0),
            }
        }

        fn release(&mut self, key: Key) {
            if key == Key::KEY_LEFTCTRL {
                self.control -= 1;
            }
        }

        fn hold(&mut self, key: Key) {
            if key == Key::KEY_LEFTCTRL {
                self.control += 1;
            }
        }

        fn modifiers(&self) -> Modifiers {
            Modifiers::NONE
        }
    }

    /// One keyboard over a layout whose terminal sits under control.
    fn chorded() -> (Keys, Down) {
        (
            Keys::new(Some(Box::new(Chorded::default()))),
            Down::default(),
        )
    }

    #[test]
    fn a_press_this_device_had_already_made_asks_for_no_terminal() {
        // Reachable on any machine, and every resume opens every device again: `Device::open`
        // starts the kernel queuing events to this client before `EVIOCGKEY` is read, so a chord
        // held across the switch arrives through the map *and* through the stream. The layout is
        // told nothing about the second one, so nobody pressed it — and a switch asked for on it
        // would take the screen away again the moment it came back.
        let (mut keys, _, mut down) = keys();
        down.keys.insert(Key::KEY_F1.raw());

        let again = translated(&mut keys, &mut down, &moved(SINCE, Key::KEY_F1, 1));

        assert_eq!(
            again.terminal, None,
            "the device already held the key: {again:?}"
        );
        assert!(
            presses(&again.events).is_empty(),
            "and it reached no surface either: {:?}",
            again.events
        );
    }

    #[test]
    fn a_chord_taken_apart_first_reaches_no_surface_at_all() {
        // The press decides, and this is why it has to. The terminal sits on the level the
        // modifiers select, so letting go of control before the key turns the release back into
        // an ordinary `F1` — and an application that reads key releases would be told about half of
        // a chord it never heard the beginning of.
        let (mut keys, mut down) = chorded();

        translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTCTRL, 1));
        let chord = translated(&mut keys, &mut down, &moved(SINCE, Key::KEY_F1, 1));
        translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTCTRL, 0));
        let up = translated(&mut keys, &mut down, &moved(SINCE, Key::KEY_F1, 0));

        assert_eq!(chord.terminal, Some(1), "the press asked for terminal 1");
        assert!(
            presses(&chord.events).is_empty(),
            "and reached no surface: {:?}",
            chord.events
        );
        assert!(
            presses(&up.events).is_empty(),
            "the release reached none either: {:?}",
            up.events
        );
        assert_eq!(up.terminal, None, "and asked for nothing");
        assert!(
            down.swallowed.is_empty(),
            "the key came up, so nothing about it is remembered: {:?}",
            down.swallowed
        );
    }

    #[test]
    fn a_chord_held_while_the_devices_were_opened_again_reaches_no_surface() {
        // The same edge on the resume. Every resume opens every device again and asks each what is
        // held on it, so a chord held across the switch is a key this process never saw pressed and
        // whose release is the first thing it hears about it.
        let (mut keys, mut down) = chorded();
        let held = BTreeSet::from([Key::KEY_LEFTCTRL.raw(), Key::KEY_F1.raw()]);

        keys.resynchronise(&mut down, &held);
        let up = translated(&mut keys, &mut down, &moved(SINCE, Key::KEY_F1, 0));

        assert!(
            presses(&up.events).is_empty(),
            "the release of a key that asks for a terminal reaches no surface: {:?}",
            up.events
        );
    }

    #[test]
    fn the_last_terminal_asked_for_in_one_batch_is_the_one_answered() {
        // A read carries one terminal and a person can press two chords inside one turn. The last
        // one is the one they asked for, and a fold that kept the first would send them somewhere
        // they had already left.
        let (mut keys, mut down) = chorded();
        translate(&mut keys, &mut down, &moved(SINCE, Key::KEY_LEFTCTRL, 1));
        // One batch holding both, so what is asserted is the fold inside `Keys::batch` rather than
        // the one the helper does over batches.
        let mut bytes = record(SINCE, EventType::EV_KEY, Key::KEY_F1.raw(), 1);
        bytes.extend(record(SINCE, EventType::EV_KEY, Key::KEY_F2.raw(), 1));
        bytes.extend(record(
            SINCE,
            EventType::EV_SYN,
            Synchronisation::SYN_REPORT.raw(),
            0,
        ));

        let asked = translated(&mut keys, &mut down, &bytes);

        assert_eq!(asked.terminal, Some(2), "the second chord is the answer");
    }

    #[test]
    fn the_newest_ask_is_the_one_a_read_answers_with() {
        // The rule both folds run: the one inside a batch, and the one `Seat::read` runs over every
        // batch of every device. An ask of nothing is an ordinary key, and one that cleared the
        // answer would lose a chord pressed earlier in the same turn.
        let mut answered = None;

        ask(&mut answered, Some(2));
        assert_eq!(answered, Some(2));
        ask(&mut answered, None);
        assert_eq!(
            answered,
            Some(2),
            "an ordinary key leaves the ask where it is"
        );
        ask(&mut answered, Some(5));
        assert_eq!(
            answered,
            Some(5),
            "and the last chord is what was asked for"
        );
    }

    /// The cursor planes, as the calls they were asked for.
    ///
    /// Written over a [`Record`] so that a test can hand it the session's own list. The give-back
    /// and the ask are then recorded on one list, in the order they were made, by the two callers
    /// that made them, and that is the only way to assert an order no single object sees.
    struct RecordedPlanes(Record);

    impl Planes for RecordedPlanes {
        fn give_them_back(&mut self) {
            self.0.push(Asked::GavePlanesBack);
        }

        fn take_them_again(&mut self) {
            self.0.push(Asked::TookPlanesAgain);
        }

        fn wait_for_the_switch(&mut self) {
            self.0.push(Asked::WaitedForTheSwitch);
        }
    }

    #[test]
    fn the_terminal_a_key_asked_for_is_what_the_session_is_asked_for() {
        // The other half of the switch, and the join the frame loop cannot drop: the reports leave
        // through this call, so a turn that delivered them made the ask. A read where nobody pressed
        // the chord asks for nothing — a session asked every turn would switch terminal on its own,
        // and would take the pointer off a screen nobody is leaving.
        let mut session = Session::direct();
        // Apart from the session's own, so that what is asserted below is the session and nothing
        // else. `the_cursor_planes_go_back_before_the_session_is_asked_for_a_terminal` is where the
        // two are one list.
        let apart = Record::default();
        let mut planes = RecordedPlanes(apart.clone());
        let quiet = Heard {
            reports: Vec::new(),
            terminal: None,
        };
        let asked = Heard {
            reports: vec![Report::focused(SurfaceEvent::Focused(true))],
            terminal: Some(2),
        };

        assert!(quiet.answered(&mut session, &mut planes).is_empty());
        assert!(
            session.asked().is_empty(),
            "a read where no key asked for a terminal asks for none: {:?}",
            session.asked()
        );
        assert!(
            apart.taken().is_empty(),
            "and takes no pointer off a screen: {:?}",
            apart.taken()
        );

        let reports = asked.answered(&mut session, &mut planes);

        assert_eq!(
            session.asked(),
            [Asked::Switch(2), Asked::Refused],
            "the terminal a key asked for is the one the session is asked for, and this run owns \
             none"
        );
        assert_eq!(
            reports.len(),
            1,
            "and the reports came out of the same call: {reports:?}"
        );
    }

    #[test]
    fn the_cursor_planes_go_back_before_the_session_is_asked_for_a_terminal() {
        // The order is what this pins. A cursor plane keeps whatever was last put on it until
        // somebody names that plane in a commit, and a session that draws its own pointer into the
        // frame names it never — so a plane cleared after the ask is a plane cleared after the
        // terminal has moved and DRM master has gone, and this program's pointer sits over the next
        // session's picture for the rest of the run.
        //
        // Both callers record on one list, so what is read here is the order they ran in rather
        // than two lists that each say their own half happened.
        //
        // A refusal takes them back at once, and waits for nothing: this run owns no terminal, so
        // a bound started here would leave the pointer off this screen for half a second at every
        // press of the chord.
        let mut session = Session::direct();
        let mut planes = RecordedPlanes(session.recording());
        let asked = Heard {
            reports: Vec::new(),
            terminal: Some(2),
        };

        asked.answered(&mut session, &mut planes);

        assert_eq!(
            session.asked(),
            [
                Asked::GavePlanesBack,
                Asked::Switch(2),
                Asked::Refused,
                Asked::TookPlanesAgain,
            ],
            "every plane went back before the ask, and came back after a refusal"
        );
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
    fn one_key_read_on_its_own_is_the_same_translation_as_one_key_in_a_batch() {
        // The two ways in. `Keys::batch` reads the kernel's own stream, and `Keys::key` is the
        // entry for a source that already knows which key moved — libinput reports a code and a
        // direction, and never a batch. They have to be one translation, because everything that
        // makes a key mean something is written once and both would otherwise drift.
        let struck = [
            (Key::KEY_LEFTSHIFT, Transition::Pressed),
            (Key::KEY_A, Transition::Pressed),
            (Key::KEY_A, Transition::Released),
            (Key::KEY_LEFTSHIFT, Transition::Released),
        ];
        let at = Duration::from_millis(250);

        let (mut batched, _, mut batched_down) = keys();
        let mut through_a_batch = Vec::new();
        for (key, transition) in struck {
            let value = match transition {
                Transition::Pressed => 1,
                Transition::Repeated => 2,
                Transition::Released => 0,
            };
            through_a_batch.extend(translate(
                &mut batched,
                &mut batched_down,
                &moved(SINCE + at, key, value),
            ));
        }

        let (mut directly, _, mut direct_down) = keys();
        let mut one_at_a_time = Vec::new();
        for (key, transition) in struck {
            one_at_a_time.extend(
                directly
                    .key(
                        &mut direct_down,
                        key,
                        transition,
                        Stamps::from_origin(SINCE).at(SINCE + at),
                    )
                    .events,
            );
        }

        assert_eq!(
            format!("{one_at_a_time:?}"),
            format!("{through_a_batch:?}"),
            "the same keys through the two entries mean the same thing"
        );
        assert_eq!(directly.modifiers(), batched.modifiers());
    }

    #[test]
    fn the_moment_an_event_carries_is_the_moment_the_kernel_stamped_it() {
        // The kernel timestamps when the key moved. A loop that stamped when it woke would give
        // every event in one wake the same moment, and a double click and a key repeat are
        // measured against the difference between two of them.
        let (mut keys, mut down) = (Keys::new(None), Down::default());
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
        let (mut keys, mut down) = (Keys::new(None), Down::default());

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
        let mut internal = Down::default();
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
        let mut second = Down::default();

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
            &mut points.down,
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
                &mut points.down,
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
            &mut points.down,
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
            &mut points.down,
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
            directory: directory.to_owned(),
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

        let read = seat.read(&mut Session::direct(), &mut Pointer::centred(&[]), &[]);

        assert!(
            read.reports.is_empty(),
            "an empty file is no device: {:?}",
            read.reports
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

    /// Where the kernel puts input devices.
    ///
    /// Written out rather than taken from [`zgui_evdev::DIRECTORY`]. Everything below asks the
    /// machine, and a constant read out of the crate under test would let that crate decide what
    /// the machine has.
    const NODES: &str = "/dev/input";

    /// Where the kernel publishes what each device is.
    const PUBLISHED: &str = "/sys/class/input";

    /// A node this seat takes that nobody types on, or nothing with the reason printed.
    ///
    /// A grab is exclusive and it lasts for as long as the device is held, so a test that took the
    /// keyboard somebody is typing on would take it from the session for as long as it ran.
    ///
    /// **The search is for a mouse**: two *relative* axes, a button, and no key from the block a
    /// person types on. Narrower than [`crate::input::pointer::points_with`], deliberately — that
    /// call accepts a device that reports a position as well, so on a laptop the first match can be
    /// the touchscreen, and a test that grabbed one takes the screen away from whoever is using the
    /// machine.
    ///
    /// # What the machine is asked
    ///
    /// `/dev/input` is walked with `std::fs`, and what each node can report comes from
    /// `/sys/class/input`, which the kernel writes out of the same `input_dev` the ioctls answer
    /// from. Nothing asks `zgui_evdev` and nothing asks [`Seat`].
    ///
    /// That is deliberate. A search that opened each node through the crate under test would send
    /// every test below into this arm exactly when that crate stopped opening devices, and print a
    /// message blaming the machine for it. So a node this answers is a node the tests below
    /// **assert** the seat takes.
    fn takeable_pointer(test: &str) -> Option<PathBuf> {
        let found = readable_nodes()
            .into_iter()
            .find(|path| published(path).is_some_and(|codes| codes.is_a_mouse()));

        if found.is_none() {
            eprintln!(
                "{test}: this machine publishes no mouse, so nothing was asserted. It needs a \
                 readable `/dev/input/event*` with two relative axes, a button, and no key a \
                 person types on. Most nodes belong to the `input` group, so adding this user to \
                 that group is usually what is missing."
            );
        }
        found
    }

    /// The `event*` nodes under `/dev/input` this process can read, in the order the kernel
    /// numbers them.
    ///
    /// Each candidate is opened read-only, as [`Device::open`] opens it. `/dev/input` also holds
    /// `mice`, `mouse0` and `js0`, and `mouse0` is the one that matters: it starts with the same
    /// prefix and is a different interface.
    fn readable_nodes() -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(NODES) else {
            return Vec::new();
        };

        let mut nodes: Vec<(u32, PathBuf)> = entries
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter_map(|path| {
                let number = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_prefix("event"))
                    .and_then(|number| number.parse().ok())?;
                Some((number, path))
            })
            .filter(|(_, path)| std::fs::File::open(path).is_ok())
            .collect();
        nodes.sort();
        nodes.into_iter().map(|(_, path)| path).collect()
    }

    /// What the kernel publishes about one node, as the codes it names.
    struct Published {
        /// Which event types the device emits.
        types: BTreeSet<u16>,
        /// Which keys and buttons it has.
        keys: BTreeSet<u16>,
        /// Which relative axes it has.
        relative: BTreeSet<u16>,
    }

    impl Published {
        /// Returns `true` if this is a device a person points with and nobody types on.
        ///
        /// The three maps and the rule over them are the kernel's own vocabulary, read through the
        /// constants `zgui-evdev` generates from the headers. A key below `BTN_MISC` is a key a
        /// person types on: the kernel puts every one of them below that boundary, and everything
        /// from it upwards is a button or a code behind the buttons.
        fn is_a_mouse(&self) -> bool {
            let emits = |kind: EventType| self.types.contains(&kind.raw());
            let axis = |axis: Relative| self.relative.contains(&axis.raw());

            emits(EventType::EV_KEY)
                && emits(EventType::EV_REL)
                && axis(Relative::REL_X)
                && axis(Relative::REL_Y)
                && self.keys.contains(&Key::BTN_LEFT.raw())
                && !self.keys.iter().any(|code| *code < Key::BTN_MISC.raw())
        }
    }

    /// What the kernel publishes about the node at `path`, where it publishes anything.
    ///
    /// `/sys/class/input/eventN/device` is the `input_dev` behind the node, and the capability
    /// files under it are written from the same maps `EVIOCGBIT` answers.
    fn published(path: &Path) -> Option<Published> {
        let directory = Path::new(PUBLISHED)
            .join(path.file_name()?)
            .join("device/capabilities");
        let map = |name: &str| -> Option<BTreeSet<u16>> {
            Some(published_codes(
                &std::fs::read_to_string(directory.join(name)).ok()?,
            ))
        };

        Some(Published {
            types: map("ev")?,
            keys: map("key")?,
            relative: map("rel")?,
        })
    }

    /// The codes one published capability map names.
    ///
    /// The kernel prints a bitmap as one hexadecimal word per machine word, **most significant
    /// first** and separated by spaces, with the word holding code zero written last. So the
    /// groups are counted from the right.
    fn published_codes(text: &str) -> BTreeSet<u16> {
        let width = usize::BITS;
        text.split_whitespace()
            .rev()
            .enumerate()
            .flat_map(|(group, word)| {
                let bits = u64::from_str_radix(word, 16).unwrap_or(0);
                let base = u32::try_from(group).unwrap_or(0) * width;
                (0..width.min(u64::BITS))
                    .filter(move |bit| bits >> bit & 1 == 1)
                    .filter_map(move |bit| u16::try_from(base + bit).ok())
            })
            .collect()
    }

    /// Held for as long as a test holds the machine's one takeable device.
    ///
    /// The runner runs these on several threads, and an ordinary machine has one mouse, so two
    /// tests that took it at once would meet `EBUSY` in whichever got there second. They take
    /// turns rather than the device.
    static TAKEN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Takes the turn to hold a device.
    ///
    /// A test that panicked while holding it poisons the lock, and the turn is still free: the
    /// grab went with the descriptor the panic dropped.
    fn turn() -> std::sync::MutexGuard<'static, ()> {
        TAKEN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// `EVIOCREVOKE`, computed the way the kernel's header computes it.
    ///
    /// `_IOW('E', 0x91, int)`, built out of the same `rustix` const function `zgui-evdev` builds
    /// its own request numbers with, so no number here is transcribed.
    const REVOKE: Opcode = opcode::write::<c_int>(b'E', 0x91);

    /// Revokes the open file description `fd` names.
    ///
    /// This is what logind does to an input device when another session takes the terminal. The
    /// descriptor stays open and readable, every read on it answers `ENODEV`, and nothing puts it
    /// back — `EVIOCREVOKE` cannot be undone. So it is the one thing a test can do to a device
    /// that a terminal switch also does, and it is how the loop's answer to a device that stopped
    /// answering is asserted with no session to switch away from.
    ///
    /// The argument is the value rather than a pointer to one. `evdev_do_ioctl` refuses a
    /// `EVIOCREVOKE` whose argument is non-null, so a call that pointed at a zero would be
    /// refused.
    fn revoke(fd: BorrowedFd<'_>) {
        // SAFETY: `ioctl` is handed a descriptor this frame borrows for the length of the call, a
        // request number computed for that call, and the integer argument the request reads.
        // Nothing is dereferenced and nothing is written back, so the return value is the only
        // result.
        let answer = unsafe { ioctl(fd.as_raw_fd(), c_ulong::from(REVOKE), 0) };

        assert_eq!(
            answer,
            0,
            "the kernel revokes a descriptor onto a device it still has: {}",
            std::io::Error::last_os_error()
        );
    }

    // The C library's own, for a request this crate makes nowhere else. Declared here rather than
    // reached through a crate, for the reason `tests/support/mod.rs` declares the loader's two:
    // what crosses is stated once, beside the code that calls it.
    unsafe extern "C" {
        /// `ioctl(2)`. Takes the descriptor, the request number, and the argument that request
        /// names.
        fn ioctl(fd: c_int, request: c_ulong, argument: c_int) -> c_int;
    }

    /// How many descriptors this process holds onto `path`.
    ///
    /// `/proc/self/fd` carries one symbolic link per descriptor, and each reads back as what that
    /// descriptor names. One path rather than the whole count, because the test binary runs its
    /// tests on several threads and every one of them opens and closes files of its own.
    fn descriptors_naming(path: &Path) -> usize {
        std::fs::read_dir("/proc/self/fd")
            .expect("this backend runs on Linux, which has `/proc/self/fd` to read")
            .filter_map(std::result::Result::ok)
            .filter(|entry| std::fs::read_link(entry.path()).is_ok_and(|named| named == path))
            .count()
    }

    #[test]
    fn a_suspend_gives_every_device_back_and_the_resume_after_it_takes_them_again() {
        // The two halves of a terminal switch, a round apart, as the frame loop runs them when the
        // session goes away and comes back.
        let test = "a_suspend_gives_every_device_back_and_the_resume_after_it_takes_them_again";
        let _turn = turn();
        let Some(mouse) = takeable_pointer(test) else {
            return;
        };
        // The direct shape, and it is asked for rather than opened: a session that opened a seat
        // would take this terminal for as long as the test ran.
        //
        // **On this shape the give-back does nothing, and that is why the session records what it
        // was asked for.** What a seated run would be asked for, and in which order, is
        // `Session::asked` — written by each call before it reads the shape. That a daemon then
        // releases the device is the other half, and it is asserted from the session's own side by
        // `tests/session_seated.rs`, which owns its process and chooses libseat's noop backend in
        // it.
        //
        // **No test in this tree joins the two halves.** A seat over a seated session needs the
        // backend chosen before the process has a thread, which a test harness cannot do, and
        // `Session::open` on this machine takes the real terminal. So the composition — this seat
        // driving a live seat's devices across a real switch — is asserted by the hardware run in
        // the plan and by nothing here.
        let mut session = Session::direct();
        let root = Scratch::new(test);
        // The directory is this test's own, holding one name for the machine's mouse. A walk of the
        // real `/dev/input` grabs every keyboard on the machine and holds it for as long as the
        // seat lives.
        let path = root.0.join("event0");
        std::os::unix::fs::symlink(&mouse, &path).expect("the mouse is named in this directory");
        let mut seat = watching(&root.0);

        let taken = seat.take_again(&mut session);
        assert!(
            seat.holds(&path),
            "the node the kernel publishes as a mouse is one this seat takes: {taken:?}"
        );
        let watched = seat.descriptors().count();

        // Twice over, because one round proves nothing about the second: a switch that left a
        // descriptor or a record behind carries it into the round after it.
        for round in 0..2 {
            let gone = seat.let_go(&mut session, &Pointer::centred(&[]), &[]);

            assert!(
                !seat.holds(&path),
                "round {round}: the device went, which is what a session that lost the terminal \
                 has to do with one the kernel has already revoked"
            );
            assert_eq!(
                // The node the symbolic link names, because that is what `/proc/self/fd` resolves
                // a descriptor onto it back to.
                descriptors_naming(&mouse),
                0,
                "round {round}: and its descriptor closed, which is what has to happen before a \
                 daemon releases the device"
            );
            assert_eq!(
                seat.descriptors().count(),
                watched - 1,
                "round {round}: the watch is this crate's own inotify rather than a device, so it \
                 is the one descriptor left"
            );
            assert!(
                gone.is_empty(),
                "round {round}: a seat with no layout and nothing held has nothing to say about a \
                 device that went: {gone:?}"
            );

            let back = seat.take_again(&mut session);

            // A grab is exclusive and it is held by the open file description. So a resume that
            // opened the path before the device on it had gone would be refused the grab, the
            // device would be left where it is, and the path would not be here.
            assert!(
                seat.holds(&path),
                "round {round}: the path came back, so what held it had gone before it was asked \
                 for again"
            );
            assert_eq!(
                descriptors_naming(&mouse),
                1,
                "round {round}: and the seat holds one name on the node"
            );
            assert_eq!(
                seat.descriptors().count(),
                watched,
                "round {round}: the loop waits on the device and the watch again"
            );
            assert!(
                back.is_empty(),
                "round {round}: and nothing was held on the device that came back: {back:?}"
            );

            let taken = seat.devices.first_mut().expect("the one device is here");
            assert!(
                taken.device.is_grabbed(),
                "round {round}: the device was taken again rather than only opened"
            );
            assert!(
                taken.points.is_some(),
                "round {round}: and it was read again, so what it does is known"
            );
            taken
                .device
                .read()
                .unwrap_or_else(|error| panic!("round {round}: and it answers a read: {error}"));
        }

        // The session's own half, and the one the direct shape cannot show by counting anything.
        // **The give-back comes before the open, on the same path.** seatd answers a path its
        // client already holds with the same device id and its reference count raised, so a resume
        // that opened first would take that count to zero on the first close and leave the daemon
        // releasing a device this process still holds grabbed.
        assert_eq!(
            session.asked(),
            [
                Asked::Open(path.clone()),
                Asked::CloseEvery,
                Asked::Open(path.clone()),
                Asked::CloseEvery,
                Asked::Open(path.clone()),
            ],
            "each round gave every device back through the session and then asked it for the path \
             again"
        );
    }

    #[test]
    fn a_resume_takes_a_device_a_seat_that_started_away_never_held() {
        // A run started on a terminal nobody is looking at holds **nothing**: the daemon hands over
        // an evdev node it has already revoked, so the walk at start-up takes none of them. A
        // resume that walked a list of what was held would find an empty list and the program would
        // have no keyboard and no mouse for the rest of the run.
        //
        // The same shape covers a keyboard plugged in while the session was away, which is in the
        // directory and in no such list either.
        let test = "a_resume_takes_a_device_a_seat_that_started_away_never_held";
        let _turn = turn();
        let Some(mouse) = takeable_pointer(test) else {
            return;
        };
        let mut session = Session::direct();
        let root = Scratch::new(test);
        let mut seat = watching(&root.0);

        // The directory is empty, which stands for a walk that could take nothing.
        assert!(seat.devices.is_empty());
        assert!(seat.take_again(&mut session).is_empty());
        assert!(
            seat.devices.is_empty(),
            "there was nothing in the directory to take"
        );

        let path = root.0.join("event0");
        std::os::unix::fs::symlink(&mouse, &path).expect("the mouse is named in this directory");
        seat.take_again(&mut session);

        assert!(
            seat.holds(&path),
            "the device arrived while this session was away, and the resume walked the directory"
        );
        assert_eq!(
            session.asked(),
            [Asked::Open(path.clone())],
            "and the one node the directory holds is the one the session was asked for"
        );
    }

    #[test]
    fn a_node_this_seat_declines_goes_back_to_the_session() {
        // Most of `/dev/input` is a device nobody types on and nobody points with — a power
        // button, a lid switch, an accelerometer — and `Seat::open` walks about twenty nodes to
        // find the two or three it wants. Each one it declines was opened by the daemon, and
        // dropping the device closes a descriptor and leaves the daemon's record standing: one
        // record per declined node per run, held until the seat closes.
        //
        // The node here is declined for the other reason the seat declines one: the kernel refuses
        // to hand it over, because this test is holding it. Both reasons reach the same line.
        let test = "a_node_this_seat_declines_goes_back_to_the_session";
        let _turn = turn();
        let Some(path) = takeable_pointer(test) else {
            return;
        };
        let mut session = Session::direct();
        let root = Scratch::new(test);
        let mut seat = watching(&root.0);

        let mut held = Device::open(&path).expect("this process reads the node it just listed");
        held.grab().expect("nothing else holds this device");

        let announced = seat.take_node(&mut session, &path);

        assert!(
            !seat.holds(&path),
            "a device the kernel will not hand over is left where it is"
        );
        assert!(
            announced.announced.is_none(),
            "so nothing was held on it: {announced:?}"
        );
        assert!(
            !announced.not_yet,
            "and the node opened, so it is no device this session is waiting for: {announced:?}"
        );
        assert_eq!(
            session.asked(),
            [Asked::Open(path.clone()), Asked::Close(path.clone())],
            "and the node the session opened went back to it"
        );
    }

    #[test]
    fn a_device_that_stops_answering_goes_back_to_the_session() {
        // What a terminal switch does to every input device this run holds, and what an unplugged
        // device does to one: the descriptor answers `ENODEV` from then on. The loop drops it, and
        // the daemon has to be told — a device dropped and never given back is a record logind
        // holds until the process exits, and on a machine that switches back and forth that is one
        // record per device per switch.
        let test = "a_device_that_stops_answering_goes_back_to_the_session";
        let _turn = turn();
        let Some(path) = takeable_pointer(test) else {
            return;
        };
        let mut session = Session::direct();
        let root = Scratch::new(test);
        let mut seat = watching(&root.0);

        seat.take_node(&mut session, &path);
        assert!(
            seat.holds(&path),
            "the node the kernel publishes as a mouse is one this seat takes"
        );
        revoke(seat.devices[0].device.as_fd());

        let read = seat.read(&mut session, &mut Pointer::centred(&[]), &[]);

        assert!(
            !seat.holds(&path),
            "a device that answers a read with a failure is dropped rather than polled again: \
             {:?}",
            read.reports
        );
        assert_eq!(
            descriptors_naming(&path),
            0,
            "its descriptor closed, which is what has to happen before a daemon releases the \
             device"
        );
        assert_eq!(
            session.asked(),
            [Asked::Open(path.clone()), Asked::Close(path.clone())],
            "and the session was told, so the record of the device goes with it"
        );
    }

    #[test]
    fn every_node_in_the_directory_is_opened_through_the_session() {
        // The correction this milestone is about. `Seat::open` used to open each node itself,
        // which needs the `input` group; a seated run gets its devices from the session, and on
        // the ordinary machine that is the only way this program gets one at all.
        //
        // The directory is this test's own, holding one name for the machine's mouse and one
        // ordinary file. A walk of the real `/dev/input` grabs every keyboard on the machine and
        // holds it for as long as the seat lives.
        let test = "every_node_in_the_directory_is_opened_through_the_session";
        let _turn = turn();
        let Some(path) = takeable_pointer(test) else {
            return;
        };
        let root = Scratch::new(test);
        let node = root.0.join("event0");
        let empty = root.0.join("event1");
        std::os::unix::fs::symlink(&path, &node).expect("the mouse is named in this directory");
        std::fs::write(&empty, []).expect("the file is made");
        let mut session = Session::direct();

        let seat = Seat::open_in(
            &mut session,
            &Aged::started(Duration::from_secs(1)),
            &root.0,
        );

        assert_eq!(
            session.asked(),
            [Asked::Open(node.clone()), Asked::Open(empty.clone())],
            "every node in the directory was asked of the session, in the order the kernel \
             numbers them"
        );
        assert_eq!(
            seat.devices.len(),
            1,
            "the one node that is a device was taken, and the empty file was left"
        );
        assert!(
            seat.holds(&node),
            "the device is named by the path it was opened at"
        );
        assert_eq!(
            seat.descriptors().count(),
            2,
            "which the loop waits on beside the watch this directory got"
        );
    }

    #[test]
    fn a_session_waiting_for_its_terminal_is_one_line_naming_the_count() {
        // The run this was written for started on a terminal that was not the live one, and the
        // daemon handed it twenty-two revoked nodes. It reported each of them, and then that no
        // keyboard on the machine could be taken and that the cursor could not be moved — twenty-
        // four lines, all of them untrue ten seconds later when somebody switched to that terminal.
        assert_eq!(Found::of(22, false, false), Found::Waiting(22));
        assert_eq!(
            Found::of(1, false, false),
            Found::Waiting(1),
            "and one node is the same state, so the count is what varies rather than the answer"
        );
    }

    #[test]
    fn a_session_that_holds_the_screen_and_found_no_keyboard_still_says_so() {
        // The fault the two warnings exist for, and the one this must not lose: a machine where
        // nothing could be taken is a program nobody can type into for the rest of the run.
        assert_eq!(
            Found::of(0, false, false),
            Found::Took {
                keyboard: false,
                pointer: false
            }
        );
        assert_eq!(
            Found::of(0, true, true),
            Found::Took {
                keyboard: true,
                pointer: true
            },
            "and a walk that took both says nothing at all"
        );
        // A device that has gone answers the same way a revoked one does, so a walk that took a
        // keyboard and met one is a machine with a device missing rather than a session waiting.
        // A session waiting for its terminal holds nothing at all, and that tells them apart.
        assert_eq!(
            Found::of(1, true, false),
            Found::Took {
                keyboard: true,
                pointer: false
            }
        );
        assert_eq!(
            Found::of(1, false, true),
            Found::Took {
                keyboard: false,
                pointer: true
            }
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
