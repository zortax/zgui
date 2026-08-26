//! Every device, read through libinput.
//!
//! This source hands the nodes to libinput and reads what libinput decided. It gains the policy
//! nothing in this tree is going to write: acceleration curves, touchpad tap and two-finger
//! scrolling, palm rejection, button debouncing, and a database of quirks for named hardware.
//! [`seat`](crate::input::seat) is the other source, and it reads the kernel's own stream and
//! decides everything about that stream in this tree.
//!
//! # What both sources share
//!
//! Everything above the device. libinput reports the kernel's own key and button codes, so the
//! layout, the modifiers, the terminal chord, where the pointer is and what a scroll is worth are
//! the same answers written in the same place. The two sources differ in where an update is read.
//!
//! # The device classes
//!
//! Which device is a keyboard and which is a pointer is libinput's answer here, taken from the
//! evdev bits, its own quirks database and udev's properties. `types_on` and `points_with` read the
//! evdev bits and decide, and they belong to the other source alone.
//!
//! # The repeats
//!
//! libinput drops the kernel's auto-repeat — `ignore kernel key repeat` in its own `evdev.c` — for
//! the reason it takes no grab: how long a key is held before it repeats is a decision about a
//! person rather than about a device. A reader of the kernel's own stream gets those repeats for
//! nothing, so the other source makes none. This source makes its own.
//!
//! One key repeats at a time, as every keyboard does: pressing a second key takes the repeat over,
//! and releasing a key that is not the repeating one leaves it alone. The rate comes from the
//! device, read with `EVIOCGREP`, so a key repeats exactly as it would have.
//!
//! # The state a device arrives with
//!
//! libinput reads the keys a device already holds when it opens the device, so a device that
//! arrives here is asked nothing. The other source reads `EVIOCGKEY` when it takes a device,
//! because a modifier held before this process was listening is in the kernel's map and in no
//! event. The repair on this path is the one that runs when a device **goes**.

use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::BorrowedFd;
use std::path::Path;
use std::time::Duration;

use tracing::{info, warn};
use zgui_evdev::Key;
use zgui_geom::Size;
use zgui_libinput::{Context, Device, DeviceId, Event, Press, Scrolled};
use zgui_vocab::{PointerAction, ScrollDelta};

use crate::input::lent::{Held, Lent};
use crate::input::pointer::{self, Motion, Pointer, Screen};
use crate::input::seat::{Down, Keys, Opened, Report, Stamps, Transition, ask, cancelled, moved};
use crate::session::Session;
use zgui_vocab::Timestamp;

/// How many of libinput's wheel steps make one detent.
///
/// The same number the kernel's own high-resolution wheel axis counts in, and libinput reports in
/// it for the same reason: a free-spinning wheel reports fine movement continuously and a whole
/// detent only when it has accumulated one.
const STEPS_PER_DETENT: f32 = 120.0;

/// What one device libinput reports needs remembered between events.
#[derive(Debug, Default)]
struct Reading {
    /// Which of its keys this source believes are down, and which of them it swallowed.
    ///
    /// The device's own rather than the source's, for the reason [`Keys`] gives: one layout serves
    /// every keyboard, and shift held on two of them is two transitions needing two releases.
    down: Down,
    /// Which of its buttons this source believes are down.
    buttons: BTreeSet<u16>,
    /// Whether somebody types on it.
    types: bool,
    /// Whether somebody points with it.
    points: bool,
    /// How long it waits before repeating a key, and how often after that.
    ///
    /// Read once when the device arrived, because it is the device's own setting and does not move
    /// while it is open. Nothing where the device is no keyboard.
    repeat: Option<(Duration, Duration)>,
}

/// The key that is repeating, and when its next repeat is owed.
#[derive(Debug, Clone, Copy)]
struct Repeating {
    /// The device it is held on, so that its release is the one that stops this.
    device: DeviceId,
    /// The key itself.
    key: Key,
    /// When the next repeat is owed.
    due: Timestamp,
    /// How long between repeats after the first.
    period: Duration,
}

/// libinput, and what this source remembers about the devices it reports.
pub(crate) struct Through {
    /// The context every device is given to and read from.
    context: Context,
    /// Every device opened for it, under the descriptor libinput was given.
    held: Vec<Held>,
    /// What each device it reports needs remembered.
    devices: BTreeMap<DeviceId, Reading>,
    /// How many nodes the last add could not read yet, which the session answers.
    waiting: usize,
    /// The one key that is repeating, where one is.
    repeating: Option<Repeating>,
}

impl Through {
    /// Returns a source over one libinput context.
    pub(crate) fn new(context: Context) -> Self {
        Self {
            context,
            held: Vec::new(),
            devices: BTreeMap::new(),
            waiting: 0,
            repeating: None,
        }
    }

    /// Returns when the next key repeat is owed, where a key is repeating.
    ///
    /// Nothing on a console wakes a loop for this, so a loop that waits without cutting its wait to
    /// this moment repeats at the speed of whatever else happens to arrive.
    pub(crate) fn due(&self) -> Option<Timestamp> {
        self.repeating.map(|repeating| repeating.due)
    }

    /// Returns the descriptor a loop waits on.
    ///
    /// One, rather than one for each device: libinput reads every device it holds and reports
    /// through this.
    pub(crate) fn descriptor(&self) -> BorrowedFd<'_> {
        self.context.descriptor()
    }

    /// Gives libinput one node to read, and says whether it took it.
    ///
    /// A node already held, one that is not an evdev node, and one this process may not open are
    /// each refused. The device itself arrives in the next read.
    pub(crate) fn add(&mut self, session: &mut Session, path: &Path) -> Opened {
        let Self {
            context,
            held,
            waiting,
            ..
        } = self;
        *waiting = 0;
        let taken = context.add(&mut Lent::new(session, held, waiting), path);
        Opened {
            // Nothing is announced here: libinput reads what a device already holds when it opens
            // one. The other source reads that state itself and says so.
            announced: None,
            not_yet: !taken && *waiting > 0,
        }
    }

    /// Returns every node this source is reading.
    pub(crate) fn held(&self) -> Vec<&Path> {
        self.held.iter().map(Held::path).collect()
    }

    /// Returns `true` if this source is already reading the device at `path`.
    pub(crate) fn holds(&self, path: &Path) -> bool {
        self.held.iter().any(|held| held.path() == path)
    }

    /// Gives every device back, for a terminal switch.
    ///
    /// Each one is reported gone in the same read that follows, so what a caller believes each was
    /// holding is repaired through the one path rather than a second one written for this.
    pub(crate) fn let_go(&mut self, session: &mut Session) {
        let Self {
            context,
            held,
            waiting,
            ..
        } = self;
        context.suspend(&mut Lent::new(session, held, waiting));
    }

    /// Opens every node again, on the way back from a terminal switch.
    ///
    /// libinput asks for each path it remembers, through the session, so a node that no longer
    /// opens is forgotten and the same path can arrive again later.
    pub(crate) fn take_again(&mut self, session: &mut Session) {
        let Self {
            context,
            held,
            waiting,
            ..
        } = self;
        if let Err(error) = context.resume(&mut Lent::new(session, held, waiting)) {
            warn!(
                target: "zgui::platform",
                "the input devices could not be opened again after the terminal came back: {error}"
            );
        }
    }

    /// Returns everything the devices have done since the last read.
    ///
    /// The terminal a key asked for comes back with the reports, for the reason the other source
    /// gives: the reports leave through one call, so a turn that delivered them asked for it.
    pub(crate) fn read(
        &mut self,
        session: &mut Session,
        keys: &mut Keys,
        pointer: &mut Pointer,
        screens: &[Screen],
        stamps: Stamps,
        now: Timestamp,
    ) -> (Vec<Report>, Option<u32>) {
        let Self {
            context,
            held,
            devices,
            waiting,
            repeating,
        } = self;

        if let Err(error) = context.dispatch(&mut Lent::new(session, held, waiting)) {
            warn!(
                target: "zgui::platform",
                "the input devices could not be read, so nothing they did is known: {error}"
            );
            return (Vec::new(), None);
        }

        let mut reports = Vec::new();
        let mut terminal = None;
        while let Some(event) = context.next_event() {
            match event {
                Event::DeviceAdded(device) => {
                    if context.tap_to_click(device.id()) {
                        info!(
                            target: "zgui::platform",
                            "{} taps to click", device.path().display()
                        );
                    }
                    arrived(devices, &device, held);
                }
                Event::DeviceRemoved(device) => {
                    // Nothing is held on a device that has gone, so a key repeating on it stops.
                    repeating.take_if(|held| held.device == device.id());
                    reports.extend(went(devices, &device, keys, pointer, screens, stamps));
                }
                Event::Key {
                    device,
                    key,
                    press,
                    at,
                } => {
                    let Some(reading) = devices.get_mut(&device) else {
                        continue;
                    };
                    if !reading.types {
                        continue;
                    }
                    let Ok(code) = u16::try_from(key) else {
                        continue;
                    };
                    let translated = keys.key(
                        &mut reading.down,
                        Key::new(code),
                        match press {
                            Press::Down => Transition::Pressed,
                            Press::Up => Transition::Released,
                        },
                        stamps.at(at),
                    );
                    ask(&mut terminal, translated.terminal);
                    reports.extend(translated.events.into_iter().map(Report::focused));

                    let struck = Key::new(code);
                    match press {
                        // The last key struck is the one that repeats, and it takes the repeat over
                        // from whatever had it. That is what a keyboard does: holding `a` and then
                        // striking `b` repeats `b`.
                        Press::Down if keys.repeats(struck) => {
                            *repeating = reading.repeat.map(|(delay, period)| Repeating {
                                device,
                                key: struck,
                                due: after(stamps.at(at), delay),
                                period,
                            });
                        }
                        // A key that does not repeat still stops whatever was repeating: a person
                        // who takes hold of shift has stopped holding the letter down.
                        Press::Down => *repeating = None,
                        // Only the repeating key's own release stops it. Letting go of anything
                        // else leaves it where it is.
                        Press::Up => {
                            repeating.take_if(|held| held.device == device && held.key == struck);
                        }
                    }
                }
                // A pointer button is a keyboard's own rule here: taking hold of a mouse is not
                // holding a letter down.
                Event::Button { .. } => {
                    *repeating = None;
                    reports.extend(pointed(
                        devices,
                        &event,
                        keys.modifiers(),
                        pointer,
                        screens,
                        stamps,
                    ));
                }
                pointing => {
                    reports.extend(pointed(
                        devices,
                        &pointing,
                        keys.modifiers(),
                        pointer,
                        screens,
                        stamps,
                    ));
                }
            }
        }

        // After the queue, so that a release read on this turn has already stopped whatever it was
        // holding down, and a repeat is never paid for a key that is already up.
        reports.extend(repeats(repeating, devices, keys, now));

        (reports, terminal)
    }
}

/// Pays every repeat that is owed by `now`.
///
/// More than one is owed on a turn that took longer than the period — a frame that missed, or a
/// wait that something else ended. Each is delivered rather than collapsed, because a person
/// holding a key down expects the count to follow the time they held it.
fn repeats(
    repeating: &mut Option<Repeating>,
    devices: &mut BTreeMap<DeviceId, Reading>,
    keys: &mut Keys,
    now: Timestamp,
) -> Vec<Report> {
    let mut reports = Vec::new();
    loop {
        let Some(held) = *repeating else {
            return reports;
        };
        if held.due.since_origin() > now.since_origin() {
            return reports;
        }
        let Some(reading) = devices.get_mut(&held.device) else {
            // The device went without its removal being read, which cannot happen — a removal is
            // what takes the repeat off. Stopping is the safe answer either way: a repeat with no
            // device behind it is one nobody is holding.
            *repeating = None;
            return reports;
        };

        let translated = keys.key(&mut reading.down, held.key, Transition::Repeated, held.due);
        reports.extend(translated.events.into_iter().map(Report::focused));

        // From the moment it was owed rather than from now, so that a late turn does not push every
        // later repeat late with it.
        *repeating = Some(Repeating {
            due: after(held.due, held.period),
            ..held
        });
    }
}

/// Returns the moment `later` after `moment`.
fn after(moment: Timestamp, later: Duration) -> Timestamp {
    Timestamp::from_origin(moment.since_origin().saturating_add(later))
}

/// Records a device libinput has started reading.
fn arrived(devices: &mut BTreeMap<DeviceId, Reading>, device: &Device, held: &[Held]) {
    let capabilities = device.capabilities();
    info!(
        target: "zgui::platform",
        "{} is read through libinput as {}",
        device.path().display(),
        describe(capabilities.keyboard(), capabilities.pointer())
    );
    devices.insert(
        device.id(),
        Reading {
            down: Down::default(),
            buttons: BTreeSet::new(),
            types: capabilities.keyboard(),
            points: capabilities.pointer(),
            // The device's own rate, read once here: libinput drops the repeats the kernel makes
            // from it, and this source makes its own at the same rate.
            repeat: held
                .iter()
                .find(|held| held.path() == device.path())
                .and_then(Held::repeat),
        },
    );
}

/// Returns what a device this source took is read as.
fn describe(types: bool, points: bool) -> &'static str {
    match (types, points) {
        (true, true) => "a keyboard and a pointer",
        (true, false) => "a keyboard",
        (false, true) => "a pointer",
        (false, false) => "neither a keyboard nor a pointer",
    }
}

/// Ends everything a device that has gone was holding open.
///
/// A device holds nothing once it is gone, so its keys come off the layout and its buttons end the
/// interactions they were holding. Without the first, a modifier held while its keyboard was
/// unplugged stays held for the rest of the program and every later letter comes out shifted; and
/// a terminal switch gives every device back, so this runs for all of them on the way out.
fn went(
    devices: &mut BTreeMap<DeviceId, Reading>,
    device: &Device,
    keys: &mut Keys,
    pointer: &Pointer,
    screens: &[Screen],
    stamps: Stamps,
) -> Vec<Report> {
    let Some(mut reading) = devices.remove(&device.id()) else {
        return Vec::new();
    };
    let nothing = BTreeSet::new();
    let mut reports = cancelled(
        &mut reading.buttons,
        &nothing,
        keys.modifiers(),
        stamps,
        pointer,
        screens,
    );
    reports.extend(
        keys.resynchronise(&mut reading.down, &nothing)
            .map(Report::focused),
    );
    reports
}

/// Reads one thing done with a pointing device.
fn pointed(
    devices: &mut BTreeMap<DeviceId, Reading>,
    event: &Event,
    modifiers: zgui_vocab::Modifiers,
    pointer: &mut Pointer,
    screens: &[Screen],
    stamps: Stamps,
) -> Vec<Report> {
    let Some(reading) = devices.get_mut(&event.device()) else {
        return Vec::new();
    };
    if !reading.points {
        return Vec::new();
    }
    let Some(at) = event.at() else {
        return Vec::new();
    };

    let mut motion = Motion::default();
    let mut turned = None;
    match event {
        Event::Motion { dx, dy, .. } => {
            motion.by = Some((*dx as f32, *dy as f32));
        }
        Event::MotionAbsolute { x, y, .. } => {
            motion.to = Some((*x as f32, *y as f32));
        }
        Event::Button { button, press, .. } => {
            let Ok(code) = u16::try_from(*button) else {
                return Vec::new();
            };
            let Some(named) = pointer::button(Key::new(code)) else {
                return Vec::new();
            };
            // The device's own set decides what is delivered, for the reason the other source
            // gives: a press of a button it already has down, or a release of one it has not,
            // would leave a control holding a press it never sees released.
            let changed = match press {
                Press::Down => reading.buttons.insert(code),
                Press::Up => reading.buttons.remove(&code),
            };
            if !changed {
                return Vec::new();
            }
            let action = match press {
                Press::Down => PointerAction::Pressed,
                Press::Up => PointerAction::Released,
            };
            motion.buttons.push((named, action));
        }
        Event::Scroll {
            source,
            vertical,
            horizontal,
            ..
        } => {
            turned = scrolled(*source, *vertical, *horizontal);
        }
        _ => return Vec::new(),
    }

    moved(&motion, turned, stamps.at(at), modifiers, pointer, screens)
}

/// Returns one scroll, in the unit this framework carries.
///
/// The sign is libinput's already. `REL_WHEEL` is positive when the wheel is pushed away from the
/// person, which reveals content further up, which is a smaller offset, so the other source
/// negates it. libinput reports a scroll **down** as positive, which is a larger offset and the
/// direction this framework means. So the sign is carried through here, and negating it a second
/// time would scroll the wrong way.
fn scrolled(
    source: Scrolled,
    vertical: Option<f64>,
    horizontal: Option<f64>,
) -> Option<ScrollDelta> {
    let x = horizontal.unwrap_or(0.0) as f32;
    let y = vertical.unwrap_or(0.0) as f32;
    if vertical.is_none() && horizontal.is_none() {
        return None;
    }
    match source {
        Scrolled::Wheel => Some(ScrollDelta::Lines {
            x: x / STEPS_PER_DETENT,
            y: y / STEPS_PER_DETENT,
        }),
        // A finger and a continuous source measure in pixels, and a pixel here is a CSS pixel: what
        // libinput reports is already scaled to the device's own idea of one.
        Scrolled::Finger | Scrolled::Continuous => {
            Some(ScrollDelta::Pixels(Size::new(x.into(), y.into())))
        }
    }
}

#[cfg(test)]
mod tests {
    //! When a held key repeats, and when it stops.
    //!
    //! None of this needs libinput or a device: what is under test is the arithmetic that decides
    //! when the next repeat is owed, and every input to it is written by hand.

    use super::*;

    /// A moment `after` the origin.
    fn at(after: Duration) -> Timestamp {
        Timestamp::from_origin(after)
    }

    /// One device, holding one key down.
    fn holding(
        key: Key,
        since: Duration,
        period: Duration,
    ) -> (Option<Repeating>, BTreeMap<DeviceId, Reading>) {
        let device = DeviceId::new(1);
        let mut devices = BTreeMap::new();
        devices.insert(
            device,
            Reading {
                types: true,
                ..Reading::default()
            },
        );
        (
            Some(Repeating {
                device,
                key,
                due: at(since),
                period,
            }),
            devices,
        )
    }

    #[test]
    fn a_repeat_that_is_not_owed_yet_is_not_paid() {
        let (mut repeating, mut devices) = holding(
            Key::KEY_A,
            Duration::from_millis(250),
            Duration::from_millis(33),
        );
        let mut keys = Keys::new(None);

        let paid = repeats(
            &mut repeating,
            &mut devices,
            &mut keys,
            at(Duration::from_millis(249)),
        );

        assert!(paid.is_empty(), "the delay has not passed");
        assert_eq!(
            repeating.map(|held| held.due),
            Some(at(Duration::from_millis(250))),
            "and the moment it is owed at has not moved"
        );
    }

    #[test]
    fn a_repeat_that_is_owed_is_paid_once_and_owed_again_a_period_later() {
        let (mut repeating, mut devices) = holding(
            Key::KEY_A,
            Duration::from_millis(250),
            Duration::from_millis(33),
        );
        let mut keys = Keys::new(None);

        let paid = repeats(
            &mut repeating,
            &mut devices,
            &mut keys,
            at(Duration::from_millis(250)),
        );

        assert_eq!(paid.len(), 1, "one repeat for one period");
        assert_eq!(
            repeating.map(|held| held.due),
            Some(at(Duration::from_millis(283))),
            "and the next is owed a period after the one just paid"
        );
    }

    #[test]
    fn a_turn_that_ran_late_pays_every_repeat_it_owes() {
        // A frame that missed, or a wait something else ended. A person holding a key down expects
        // the count to follow the time they held it, so the ones that came due are delivered rather
        // than collapsed into one.
        let (mut repeating, mut devices) = holding(
            Key::KEY_A,
            Duration::from_millis(250),
            Duration::from_millis(33),
        );
        let mut keys = Keys::new(None);

        let paid = repeats(
            &mut repeating,
            &mut devices,
            &mut keys,
            at(Duration::from_millis(350)),
        );

        // Owed at 250, 283, 316, 349 — four of them by 350.
        assert_eq!(paid.len(), 4, "every repeat the turn was late for");
        assert_eq!(
            repeating.map(|held| held.due),
            Some(at(Duration::from_millis(382))),
            "and the next follows the one owed at 349, not the turn that paid it"
        );
    }

    #[test]
    fn the_repeats_do_not_drift_with_a_turn_that_ran_late() {
        // Counting the next repeat from `now` rather than from the moment it was owed would push
        // every later repeat late by however long that one turn ran over, for the rest of the hold.
        let period = Duration::from_millis(33);
        let (mut repeating, mut devices) = holding(Key::KEY_A, Duration::from_millis(250), period);
        let mut keys = Keys::new(None);

        // One turn, a hair late.
        let _ = repeats(
            &mut repeating,
            &mut devices,
            &mut keys,
            at(Duration::from_millis(270)),
        );

        assert_eq!(
            repeating.map(|held| held.due),
            Some(at(Duration::from_millis(283))),
            "the next is owed 33ms after the one that was owed, not 33ms after the late turn"
        );
    }

    #[test]
    fn a_repeat_on_a_device_that_is_gone_stops() {
        let (mut repeating, mut devices) = holding(
            Key::KEY_A,
            Duration::from_millis(250),
            Duration::from_millis(33),
        );
        devices.clear();
        let mut keys = Keys::new(None);

        let paid = repeats(
            &mut repeating,
            &mut devices,
            &mut keys,
            at(Duration::from_millis(500)),
        );

        assert!(paid.is_empty(), "a key nobody is holding repeats nothing");
        assert!(
            repeating.is_none(),
            "and it stops rather than being owed for ever"
        );
    }
}
