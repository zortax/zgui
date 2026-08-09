//! The open devices, the grab, and turning batches into surface events.
//!
//! A seat is the keyboards one person is typing on. It opens every device somebody could type on,
//! takes each one away from everything else, pushes the keys that were already down into the
//! layout, and turns what the kernel reports into what a surface is told.
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

use std::collections::BTreeSet;
use std::time::Duration;

use rustix::fd::{AsFd, BorrowedFd};
use tracing::{info, warn};
use zgui_evdev::{Batch, Capabilities, Device, EventType, Key, Synchronisation};
use zgui_platform::{Clock, SurfaceEvent, SurfaceId};
use zgui_vocab::{KeyState, Modifiers, Timestamp};

use crate::input::keyboard;
use crate::input::keyboard::layout::{Layout, Reading};
use crate::input::keyboard::{code, layout};

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
/// A button belongs to a pointer. Everything else in the kernel's three key ranges is delivered,
/// including the blocks it added behind the buttons: a keyboard with media keys sends those and a
/// person pressed them.
fn typed(key: Key) -> bool {
    key.is_key()
}

/// Which surface a key press reaches.
///
/// A console has no window manager, so focus is a decision this backend makes, and this is it:
/// **the first surface the application claimed**. It is a placeholder. When the pointer arrives it
/// becomes "the display the pointer is over", and this function is the whole of what changes.
///
/// A program that claimed no display is told about no key. There is nowhere for one to go.
pub fn focused(claimed: &[SurfaceId]) -> Option<SurfaceId> {
    claimed.first().copied()
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

/// One keyboard this seat took.
struct Keyboard {
    /// The device, grabbed for as long as this lives.
    device: Device,
    /// Which of its keys this seat believes are down.
    down: BTreeSet<u16>,
    /// How its moments are read.
    stamps: Stamps,
}

/// Every keyboard on this machine, taken.
pub struct Seat {
    /// The devices, each grabbed for as long as this lives.
    keyboards: Vec<Keyboard>,
    /// The layout and the translation over it.
    keys: Keys,
    /// What this seat has to say before it has read anything.
    ///
    /// The modifiers that were already held when the devices were taken. Nothing has been told
    /// about them yet — the loop asks for events, and this is the first answer — and a caller left
    /// to work them out from key events alone would believe none were held while every key event
    /// said otherwise.
    pending: Vec<SurfaceEvent>,
}

impl Seat {
    /// Opens every keyboard this process may read, takes each one, and finds a layout.
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

        let anchored = Stamps::anchored(clock);
        let mut keys = Keys::new(found.layout);
        let mut keyboards = Vec::new();
        let mut pending = Vec::new();
        match zgui_evdev::discover() {
            Ok(discovery) => {
                for skipped in &discovery.skipped {
                    info!(
                        target: "zgui::platform",
                        "{} cannot be read: {}", skipped.path.display(), skipped.reason
                    );
                }
                for device in discovery.opened {
                    let Some(device) = take(device) else {
                        continue;
                    };
                    // Asked rather than assumed. A driver that refused `EVIOCSCLOCKID` leaves this
                    // device's stream on the wall clock, which shares no zero with the frame loop's
                    // own reading, so its events are stamped when they are read instead.
                    let stamps = if device.has_monotonic_timestamps() {
                        anchored
                    } else {
                        warn!(
                            target: "zgui::platform",
                            "{} refused the monotonic clock, so its keys are stamped when the loop \
                             reads them rather than when they moved",
                            device.path().display()
                        );
                        Stamps::Read(clock.timestamp())
                    };
                    let mut keyboard = Keyboard {
                        device,
                        down: BTreeSet::new(),
                        stamps,
                    };
                    // After the grab, so that nothing else can change what is held between the two.
                    match keyboard.device.pressed_keys() {
                        Ok(held) => {
                            let held = held.iter().map(Key::raw).collect();
                            pending.extend(keys.resynchronise(&mut keyboard.down, &held));
                        }
                        Err(error) => warn!(
                            target: "zgui::platform",
                            "{} will not say which keys are held, so a modifier held now stays \
                             invisible until it is pressed again: {error}",
                            keyboard.device.path().display()
                        ),
                    }
                    keyboards.push(keyboard);
                }
            }
            Err(error) => warn!(
                target: "zgui::platform",
                "no input device can be found on this machine: {error}"
            ),
        }
        if keyboards.is_empty() {
            warn!(
                target: "zgui::platform",
                "no keyboard on this machine could be taken, so nothing can be typed into this \
                 program"
            );
        }
        Self {
            keyboards,
            keys,
            pending,
        }
    }

    /// Returns the descriptors the frame loop waits on beside the device and the wake channel.
    pub fn descriptors(&self) -> impl Iterator<Item = BorrowedFd<'_>> {
        self.keyboards
            .iter()
            .map(|keyboard| keyboard.device.as_fd())
    }

    /// Reads every keyboard and reports what a person did.
    ///
    /// A device that answers a read with a failure is dropped and its keys are released. Any errno
    /// is treated that way, because the one that matters cannot be told from the others by anything
    /// this loop could do differently: `ENODEV` is what an unplugged device and a descriptor
    /// `logind` revoked both answer, and both then answer every later read the same way while
    /// `poll` reports the descriptor permanently ready — so a loop that kept one would spin at the
    /// speed of the processor for as long as it ran. A device dropped over a passing failure costs
    /// a keyboard that has to be plugged in again; a device kept costs the whole program.
    pub fn read(&mut self) -> Vec<SurfaceEvent> {
        let Self {
            keyboards,
            keys,
            pending,
        } = self;
        let mut events = std::mem::take(pending);
        let mut lost = Vec::new();
        for (index, keyboard) in keyboards.iter_mut().enumerate() {
            let batches = match keyboard.device.read() {
                Ok(batches) => batches,
                Err(error) => {
                    warn!(
                        target: "zgui::platform",
                        "{} stopped answering and is no longer watched: {error}",
                        keyboard.device.path().display()
                    );
                    lost.push(index);
                    continue;
                }
            };
            let mut resynchronise = false;
            for batch in &batches {
                resynchronise |= dropped(batch);
                events.append(&mut keys.batch(&mut keyboard.down, batch, keyboard.stamps));
            }
            if resynchronise {
                match keyboard.device.pressed_keys() {
                    Ok(held) => {
                        let held = held.iter().map(Key::raw).collect();
                        events.extend(keys.resynchronise(&mut keyboard.down, &held));
                    }
                    // What is believed is left alone. Repairing against nothing would release every
                    // key the person is holding, which is worse than carrying a stale belief until
                    // the next answer.
                    Err(error) => warn!(
                        target: "zgui::platform",
                        "{} will not say which keys are held, so what this loop believes is down \
                         stays as it was: {error}",
                        keyboard.device.path().display()
                    ),
                }
            }
        }
        for index in lost.into_iter().rev() {
            let mut gone = keyboards.remove(index);
            // A device that is gone holds nothing. The releases the kernel queued for it are never
            // read, so this is the only thing that takes its keys back off the layout.
            events.extend(keys.resynchronise(&mut gone.down, &BTreeSet::new()));
        }
        events
    }
}

/// Opens the grab on a device somebody types on, or answers with nothing.
///
/// A device nobody types on is left alone. So is one the kernel refuses to hand over: a grab is
/// exclusive, so another client already holding it is the ordinary reason, and either way the
/// device stays with whatever has it.
fn take(mut device: Device) -> Option<Device> {
    if !types_on(device.capabilities()) {
        return None;
    }
    match device.grab() {
        Ok(()) => {
            info!(
                target: "zgui::platform",
                "typing on {} ({})", device.name(), device.path().display()
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

    use super::{Keys, Stamps, Transition, focused, types_on};
    use crate::input::keyboard::layout::{Layout, Reading, Source};
    use std::time::Duration;
    use zgui_evdev::{
        Absolute, Bitmap, Capabilities, EventType, Key, Reader, Relative, Synchronisation,
    };
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
        // A keyboard with a trackpoint reports its buttons on the same node. A button belongs to a
        // pointer, and the pointer is later work.
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

    #[test]
    fn the_anchor_puts_the_kernels_clock_and_the_loops_on_one_zero() {
        // The one piece of arithmetic that joins two clocks, and the only thing here that reads a
        // real one. An anchor with its sign inverted, or one taken against the wrong origin, lands
        // decades away rather than microseconds — and every stamp downstream is beside frame stamps
        // measured in seconds, where nothing could tell.
        let clock = crate::clock::SystemClock::new();
        let stamps = Stamps::anchored(&clock);

        let now = super::monotonic();
        let mapped = stamps.at(now).since_origin();
        let reads = clock.timestamp().since_origin();

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
    fn the_first_surface_the_application_claimed_is_the_focused_one() {
        // A placeholder, and named as one where it is written. A console has no window manager, so
        // this is a decision rather than an answer.
        assert_eq!(
            focused(&[SurfaceId::new(1), SurfaceId::new(2)]),
            Some(SurfaceId::new(1))
        );
        assert_eq!(
            focused(&[]),
            None,
            "a program that claimed no display is told about no key"
        );
    }
}
