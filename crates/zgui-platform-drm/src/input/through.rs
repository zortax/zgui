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
//! # The state a device arrives with
//!
//! libinput reads the keys a device already holds when it opens the device, so a device that
//! arrives here is asked nothing. The other source reads `EVIOCGKEY` when it takes a device,
//! because a modifier held before this process was listening is in the kernel's map and in no
//! event. The repair on this path is the one that runs when a device **goes**.

use std::collections::{BTreeMap, BTreeSet};
use std::os::fd::BorrowedFd;
use std::path::Path;

use tracing::{info, warn};
use zgui_evdev::Key;
use zgui_geom::Size;
use zgui_libinput::{Context, Device, DeviceId, Event, Press, Scrolled};
use zgui_vocab::{PointerAction, ScrollDelta};

use crate::input::lent::{Held, Lent};
use crate::input::pointer::{self, Motion, Pointer, Screen};
use crate::input::seat::{Down, Keys, Opened, Report, Stamps, Transition, ask, cancelled, moved};
use crate::session::Session;

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
}

impl Through {
    /// Returns a source over one libinput context.
    pub(crate) fn new(context: Context) -> Self {
        Self {
            context,
            held: Vec::new(),
            devices: BTreeMap::new(),
            waiting: 0,
        }
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
    ) -> (Vec<Report>, Option<u32>) {
        let Self {
            context,
            held,
            devices,
            waiting,
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
                    arrived(devices, &device);
                }
                Event::DeviceRemoved(device) => {
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
        (reports, terminal)
    }
}

/// Records a device libinput has started reading.
fn arrived(devices: &mut BTreeMap<DeviceId, Reading>, device: &Device) {
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
