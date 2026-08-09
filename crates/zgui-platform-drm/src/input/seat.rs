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
//! something else.
//!
//! # Order against DRM master
//!
//! The frame loop takes master first and fails to start while a compositor holds it. That ordering
//! is the interlock: a run on a busy machine cannot take the keyboard from the desktop, because it
//! never reaches the point where it would ask for one. [`Seat::open`] has to be called from the
//! loop after `become_master`, and nothing else keeps it.
//!
//! # Which devices are keyboards
//!
//! [`Role::Keyboard`](zgui_evdev::Role) is udev's `ID_INPUT_KEY`, and it is meant to be broad — a
//! remote control is a keyboard under it, and so is a gaming mouse that advertises `KEY_MACRO27`
//! and its neighbours. The narrower question is whether the device has a key from the block a
//! person types on, and [`types_on`] asks it.

use std::collections::BTreeSet;
use std::time::Duration;

use rustix::fd::{AsFd, BorrowedFd};
use tracing::{info, warn};
use zgui_evdev::{Batch, Capabilities, Device, EventType, Key, Synchronisation};
use zgui_platform::{Clock, SurfaceEvent, SurfaceId};
use zgui_vocab::{KeyState, Modifiers, Timestamp};

use crate::input::keyboard::layout::{Layout, Reading};
use crate::input::keyboard::{code, layout};

/// Returns `true` if a person types on this device.
///
/// A key **below `BTN_MISC`** is the block a keyboard sends: the letters, the digits, the
/// modifiers, the function keys and the media keys. A device with none of them is a mouse or a lid
/// switch, whatever udev's broader rule calls it.
///
/// `KEY_RESERVED` is code zero and says nothing, so it is excluded by
/// [`Key::is_key`](zgui_evdev::Key::is_key) and by this.
pub fn types_on(capabilities: &Capabilities) -> bool {
    capabilities.has(EventType::EV_KEY)
        && capabilities
            .keys()
            .iter()
            .any(|key| key.is_key() && key.raw() < Key::BTN_MISC.raw())
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

/// Where the frame loop's clock and the kernel's meet.
///
/// `zgui-evdev` asks every device for `CLOCK_MONOTONIC`, and a [`Timestamp`] counts from the moment
/// the application started. One reading of the kernel's clock, taken beside one reading of the
/// loop's, puts the two on the same zero. Both run at the same rate, so the anchor holds for as
/// long as the program does.
#[derive(Clone, Copy, Debug)]
pub struct Stamps {
    /// What the kernel's clock read at the loop's own origin.
    origin: Duration,
}

impl Stamps {
    /// Returns the anchor between the two clocks, read now.
    pub fn anchored(clock: &dyn Clock) -> Self {
        Self {
            origin: monotonic().saturating_sub(clock.timestamp().since_origin()),
        }
    }

    /// Returns the anchor a caller states, for a test with no clock in it.
    pub const fn from_origin(origin: Duration) -> Self {
        Self { origin }
    }

    /// Returns the moment `at` is, in the contract's numbering.
    ///
    /// Saturating, because a device may report an event stamped a hair before the anchor was
    /// taken: the kernel timestamps when the key moved and the anchor is read afterwards.
    ///
    /// On a stream stamped when it was read, `at` is discarded: every event of that read carries
    /// the moment the loop read it.
    pub fn at(self, at: Duration) -> Timestamp {
        Timestamp::from_origin(at.saturating_sub(self.origin))
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
pub struct Keys {
    /// What a key means, or nothing on a machine with no layout source at all.
    ///
    /// With no layout a press still reaches a document: the position is the kernel's own and needs
    /// no layout, so a binding written against where a key sits keeps working and only what the
    /// key *types* is lost.
    layout: Option<Box<dyn Layout>>,
    /// The held set as it was last reported, so a change is announced once.
    modifiers: Modifiers,
    /// Where the two clocks meet.
    stamps: Stamps,
    /// Which keys this seat believes are down.
    ///
    /// Kept for one job: putting the layout back in step after the kernel says it dropped part of
    /// an update. See [`Keys::resynchronise`].
    down: BTreeSet<u16>,
}

impl Keys {
    /// A translation over `layout`, stamping against `stamps`.
    pub fn new(layout: Option<Box<dyn Layout>>, stamps: Stamps) -> Self {
        Self {
            layout,
            modifiers: Modifiers::NONE,
            stamps,
            down: BTreeSet::new(),
        }
    }

    /// Returns the modifiers held, as this translation last reported them.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// Records the keys that were already down when the seat opened.
    ///
    /// `EVIOCGKEY` is what reports them: a modifier held before this process was listening is in
    /// the kernel's own map of held keys and in no event, so without this it is invisible until it
    /// is released and pressed again. Each reaches the layout as a **down transition with no
    /// reading**, balanced by the release the kernel will send when the finger comes up.
    pub fn hold(&mut self, keys: impl IntoIterator<Item = Key>) -> Option<SurfaceEvent> {
        for key in keys.into_iter().filter(|key| typed(*key)) {
            self.down.insert(key.raw());
            if let Some(layout) = self.layout.as_mut() {
                layout.hold(key);
            }
        }
        self.announce()
    }

    /// Puts the layout back in step with the keys the kernel says are down.
    ///
    /// The other half of what a `SYN_DROPPED` asks for. The kernel's rule is to discard everything
    /// up to the next `SYN_REPORT` — which [`Keys::batch`] does by answering with nothing — and
    /// then to ask the device what its state is now. A key the layout believes is down and the
    /// device does not is released; a key the device reports and the layout has not seen is held.
    /// Neither is a key press, because nobody pressed anything: what changed is what this process
    /// knows.
    pub fn resynchronise(&mut self, held: &BTreeSet<u16>) -> Option<SurfaceEvent> {
        let held: BTreeSet<u16> = held
            .iter()
            .copied()
            .filter(|code| typed(Key::new(*code)))
            .collect();
        if let Some(layout) = self.layout.as_mut() {
            for code in self.down.difference(&held) {
                layout.release(Key::new(*code));
            }
            for code in held.difference(&self.down) {
                layout.hold(Key::new(*code));
            }
        }
        self.down = held;
        self.announce()
    }

    /// What one batch of events amounts to.
    ///
    /// A batch is one coherent update, and a key event in it is a press, a release or a repeat.
    /// Everything else the batch carries — a relative axis, a scan code, a button — belongs to
    /// work this backend has not done and is left alone.
    pub fn batch(&mut self, batch: &Batch) -> Vec<SurfaceEvent> {
        if dropped(batch) {
            return Vec::new();
        }
        let mut events = Vec::new();
        for event in &batch.events {
            let Some(key) = event.key().filter(|key| typed(*key)) else {
                continue;
            };
            let transition = Transition::of(event.value);
            let reading = self.read(key, transition);
            let (state, repeat) = match transition {
                Transition::Pressed => (KeyState::Pressed, false),
                Transition::Repeated => (KeyState::Pressed, true),
                Transition::Released => (KeyState::Released, false),
            };
            // The held set is read *after* the transition is recorded, so the press of shift
            // carries shift and its release carries nothing — which is what a browser reports and
            // what a handler reading the modifiers off a key event expects.
            let modifiers = self.layout.as_ref().map_or(Modifiers::NONE, |layout| {
                layout.modifiers()
            });
            // Before the key, because the state a key was struck in is announced before the event
            // that happened in it.
            if modifiers != self.modifiers {
                self.modifiers = modifiers;
                events.push(SurfaceEvent::ModifiersChanged(modifiers));
            }
            events.push(SurfaceEvent::Key {
                state,
                event: crate::input::keyboard::event(
                    code::physical(key),
                    reading.key,
                    reading.without_modifiers,
                    repeat,
                ),
                modifiers,
                timestamp: self.stamps.at(event.at),
            });
        }
        events
    }

    /// Reads the layout for one transition, and records the transition where there is one.
    fn read(&mut self, key: Key, transition: Transition) -> Reading {
        match transition {
            Transition::Pressed => {
                self.down.insert(key.raw());
            }
            Transition::Released => {
                self.down.remove(&key.raw());
            }
            Transition::Repeated => {}
        }
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
            Transition::Pressed => layout.press(key),
            // A repeat is no transition at all.
            Transition::Repeated => layout.reading(key),
            // A release reports what the key meant while it was down, so it is read first.
            Transition::Released => {
                let reading = layout.reading(key);
                layout.release(key);
                reading
            }
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

/// Every keyboard on this machine, taken.
pub struct Seat {
    /// The devices, each grabbed for as long as this lives.
    keyboards: Vec<Device>,
    /// The layout and the translation over it.
    keys: Keys,
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

        let mut keys = Keys::new(found.layout, Stamps::anchored(clock));
        let mut keyboards = Vec::new();
        match zgui_evdev::discover() {
            Ok(discovery) => {
                for skipped in &discovery.skipped {
                    info!(
                        target: "zgui::platform",
                        "{} cannot be read: {}", skipped.path.display(), skipped.reason
                    );
                }
                for device in discovery.opened {
                    if let Some(device) = take(device) {
                        // After the grab, so that a key pressed between the two does not arrive
                        // through both the map and the stream.
                        match device.pressed_keys() {
                            Ok(held) => {
                                let _ = keys.hold(held.iter());
                            }
                            Err(error) => warn!(
                                target: "zgui::platform",
                                "{} will not say which keys are held, so a modifier held now stays \
                                 invisible until it is pressed again: {error}",
                                device.path().display()
                            ),
                        }
                        keyboards.push(device);
                    }
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
        Self { keyboards, keys }
    }

    /// Returns the descriptors the frame loop waits on beside the device and the wake channel.
    pub fn descriptors(&self) -> impl Iterator<Item = BorrowedFd<'_>> {
        self.keyboards.iter().map(AsFd::as_fd)
    }

    /// Which modifiers are held, as this seat last reported them.
    pub fn modifiers(&self) -> Modifiers {
        self.keys.modifiers()
    }

    /// Reads every keyboard and reports what a person did.
    ///
    /// A device that answers a read with a failure is dropped rather than read again: `ENODEV` is
    /// what an unplugged device and a revoked descriptor both answer, and both then answer every
    /// later read the same way while `poll` reports the descriptor permanently ready — so a loop
    /// that kept one would spin at the speed of the processor for as long as it ran.
    pub fn read(&mut self) -> Vec<SurfaceEvent> {
        let Self { keyboards, keys } = self;
        let mut events = Vec::new();
        let mut resynchronise = false;
        let mut lost = Vec::new();
        for (index, device) in keyboards.iter_mut().enumerate() {
            match device.read() {
                Ok(batches) => {
                    for batch in &batches {
                        resynchronise |= dropped(batch);
                        events.append(&mut keys.batch(batch));
                    }
                }
                Err(error) => {
                    warn!(
                        target: "zgui::platform",
                        "{} stopped answering and is no longer watched: {error}",
                        device.path().display()
                    );
                    lost.push(index);
                }
            }
        }
        for index in lost.into_iter().rev() {
            keyboards.remove(index);
        }
        if resynchronise {
            let held = keyboards
                .iter()
                .filter_map(|device| device.pressed_keys().ok())
                .flat_map(|map| map.iter().map(Key::raw).collect::<Vec<_>>())
                .collect();
            events.extend(keys.resynchronise(&held));
        }
        events
    }
}

/// Opens the grab on a device somebody types on, or answers with nothing.
///
/// A device nobody types on is left alone. So is one the kernel refuses to hand over: a grab is
/// exclusive, so another client already holding it is the ordinary reason, and the device is left
/// alone either way.
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
                "{} is held by something else and is left with it: {error}",
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
    use zgui_platform::{SurfaceEvent, SurfaceId};
    use zgui_vocab::{KeyCode, KeyState, Modifiers, NamedKey, PhysicalKey};

    /// A layout that records what it was told, and holds shift the way a real one would.
    ///
    /// libxkbcommon counts a modifier's transitions, so what matters is the *count*: a caller that
    /// recorded a repeat as a press would leave it above zero and shift would never come up again.
    /// This counts the same way, so a test can assert the balance without the library.
    #[derive(Debug, Default)]
    struct Recording {
        /// How many times shift was recorded down, less the times it was recorded up.
        shift: i32,
        /// Every call that recorded something, in order.
        recorded: Vec<(&'static str, u16)>,
        /// Every call that read without recording, in order.
        read: Vec<u16>,
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
            self.recorded.push(("press", key.raw()));
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
            self.recorded.push(("release", key.raw()));
            if key == Key::KEY_LEFTSHIFT {
                self.shift -= 1;
            }
        }

        fn hold(&mut self, key: Key) {
            self.recorded.push(("hold", key.raw()));
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
            self.0.borrow_mut().read.push(key.raw());
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

    /// A translation over a recording layout, and the record it writes.
    fn keys() -> (Keys, Shared) {
        let shared = Shared::default();
        (
            Keys::new(Some(Box::new(shared.clone())), Stamps::from_origin(SINCE)),
            shared,
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

    /// What a stream of bytes turns into, through the whole translation.
    fn translate(keys: &mut Keys, bytes: &[u8]) -> Vec<SurfaceEvent> {
        let mut reader = Reader::new();
        reader
            .feed(bytes)
            .iter()
            .flat_map(|batch| keys.batch(batch))
            .collect()
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
        let (mut keys, _) = keys();
        let mut bytes = moved(SINCE, Key::KEY_A, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 0));

        let events = translate(&mut keys, &bytes);

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
    fn a_press_carries_all_three_readings_of_it() {
        let (mut keys, _) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 1));

        let events = translate(&mut keys, &bytes);

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
        let (mut keys, _) = keys();
        let mut bytes = moved(SINCE, Key::KEY_A, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 2));

        let events = translate(&mut keys, &bytes);

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
        let (mut keys, layout) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        for _ in 0..8 {
            bytes.extend(moved(SINCE, Key::KEY_LEFTSHIFT, 2));
        }
        bytes.extend(moved(SINCE, Key::KEY_LEFTSHIFT, 0));
        bytes.extend(moved(SINCE, Key::KEY_A, 1));

        let events = translate(&mut keys, &bytes);

        assert_eq!(
            layout.0.borrow().recorded,
            [
                ("press", Key::KEY_LEFTSHIFT.raw()),
                ("release", Key::KEY_LEFTSHIFT.raw()),
                ("press", Key::KEY_A.raw()),
            ],
            "one press and one release reached the layout, whatever came between them"
        );
        assert_eq!(
            layout.0.borrow().read.len(),
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
        let (mut keys, _) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        bytes.extend(moved(SINCE, Key::KEY_LEFTSHIFT, 0));

        let events = translate(&mut keys, &bytes);

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
        let (mut keys, _) = keys();
        let mut bytes = moved(SINCE, Key::KEY_LEFTSHIFT, 1);
        bytes.extend(moved(SINCE, Key::KEY_A, 1));

        let events = translate(&mut keys, &bytes);

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
        let (mut keys, layout) = keys();

        let events = translate(&mut keys, &moved(SINCE, Key::BTN_LEFT, 1));

        assert!(events.is_empty(), "{events:?}");
        assert!(
            layout.0.borrow().recorded.is_empty(),
            "and the layout was never told about it"
        );
    }

    #[test]
    fn the_code_that_sends_nothing_is_not_a_key_either() {
        // `KEY_RESERVED` is code zero. A driver that reports it has said nothing.
        let (mut keys, _) = keys();

        assert!(
            translate(&mut keys, &moved(SINCE, Key::KEY_RESERVED, 1)).is_empty()
        );
    }

    #[test]
    fn an_event_that_is_not_a_key_at_all_is_left_alone() {
        // A wheel and a scan code both arrive in a keyboard's own batches. Reading one as a key
        // would press whichever key the axis number happens to name.
        let (mut keys, _) = keys();
        let mut bytes = record(SINCE, EventType::EV_REL, Relative::REL_WHEEL.raw(), 1);
        bytes.extend(record(SINCE, EventType::EV_MSC, 4, 0x0007_0004));
        bytes.extend(record(
            SINCE,
            EventType::EV_SYN,
            Synchronisation::SYN_REPORT.raw(),
            0,
        ));

        assert!(translate(&mut keys, &bytes).is_empty());
    }

    #[test]
    fn a_batch_the_kernel_dropped_part_of_reports_nothing() {
        // What arrives after a `SYN_DROPPED` is the tail of an update whose beginning no longer
        // exists. Delivering it would press a key nobody pressed, and release one nobody released.
        let (mut keys, layout) = keys();
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

        let events = translate(&mut keys, &bytes);

        assert!(events.is_empty(), "{events:?}");
        assert!(layout.0.borrow().recorded.is_empty());
    }

    #[test]
    fn a_resynchronisation_puts_the_layout_back_in_step_without_pressing_anything() {
        // The other half of what a `SYN_DROPPED` asks for. Shift went down before the overflow and
        // came up during it, so the layout believes it is held and the device says it is not.
        let (mut keys, layout) = keys();
        translate(&mut keys, &moved(SINCE, Key::KEY_LEFTSHIFT, 1));
        assert_eq!(keys.modifiers(), Modifiers::SHIFT);

        let announced = keys.resynchronise(&BTreeSet::new());

        assert!(
            matches!(announced, Some(SurfaceEvent::ModifiersChanged(held)) if held.is_empty()),
            "the change is announced, and no key press is invented: {announced:?}"
        );
        assert_eq!(
            layout.0.borrow().recorded.last(),
            Some(&("release", Key::KEY_LEFTSHIFT.raw())),
            "the layout was told the key came up"
        );
        assert_eq!(keys.modifiers(), Modifiers::NONE);
    }

    #[test]
    fn a_resynchronisation_records_a_key_the_kernel_says_is_down() {
        let (mut keys, layout) = keys();

        let announced = keys.resynchronise(&BTreeSet::from([Key::KEY_LEFTSHIFT.raw()]));

        assert!(matches!(
            announced,
            Some(SurfaceEvent::ModifiersChanged(_))
        ));
        assert_eq!(
            layout.0.borrow().recorded,
            [("hold", Key::KEY_LEFTSHIFT.raw())],
            "a key that was down through the overflow is held rather than pressed"
        );
        assert_eq!(keys.modifiers(), Modifiers::SHIFT);
    }

    #[test]
    fn a_key_already_down_when_the_seat_opened_reaches_the_layout() {
        // `EVIOCGKEY` says shift is under a finger. Nothing later in the stream says so, so
        // without this it stays invisible until it is released and pressed again.
        let (mut keys, layout) = keys();

        let announced = keys.hold([Key::KEY_LEFTSHIFT, Key::BTN_LEFT]);

        assert!(
            matches!(announced, Some(SurfaceEvent::ModifiersChanged(held)) if held == Modifiers::SHIFT)
        );
        assert_eq!(
            layout.0.borrow().recorded,
            [("hold", Key::KEY_LEFTSHIFT.raw())],
            "a button is no key, and a held key is recorded with no reading taken"
        );

        // And the release the kernel sends when the finger comes up balances it.
        let events = translate(&mut keys, &moved(SINCE, Key::KEY_LEFTSHIFT, 0));
        assert_eq!(keys.modifiers(), Modifiers::NONE);
        assert!(!events.is_empty());
    }

    #[test]
    fn the_moment_an_event_carries_is_the_moment_the_kernel_stamped_it() {
        // The kernel timestamps when the key moved. A loop that stamped when it woke would give
        // every event in one wake the same moment, and a double click and a key repeat are
        // measured against the difference between two of them.
        let mut keys = Keys::new(None, Stamps::from_origin(SINCE));
        let mut bytes = moved(SINCE + Duration::from_millis(250), Key::KEY_A, 1);
        bytes.extend(moved(SINCE + Duration::from_millis(400), Key::KEY_A, 0));

        let events = translate(&mut keys, &bytes);

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
        let mut keys = Keys::new(None, Stamps::from_origin(SINCE));

        let events = translate(&mut keys, &moved(SINCE, Key::KEY_A, 1));

        let SurfaceEvent::Key { event, .. } = &events[0] else {
            panic!("a press arrived: {events:?}");
        };
        assert_eq!(event.physical, PhysicalKey::Code(KeyCode::KeyA));
        assert_eq!(event.key, zgui_vocab::Key::Unidentified);
        assert_eq!(event.key.inserted_text(), None);
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
