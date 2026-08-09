//! Where the pointer is, what moved it, and which button changed.
//!
//! A console has no pointer of its own. The kernel reports what a device did — a mouse says how
//! far it moved, a touchscreen says where it is — and the position between those reports belongs
//! to this backend. [`Pointer`] is that position, and [`Screen`] is the ground it moves over.
//!
//! # The one pointer
//!
//! Every device this seat holds drives the same [`Pointer`], and every event carries
//! [`PointerId::MOUSE`] and [`PointerKind::Mouse`]. There is one visible cursor here, so there is
//! one pointer to move. The multi-touch protocol is absent: `ABS_MT_SLOT` and the contacts under
//! it are read by nothing, so two fingers on a touchscreen are one pointer that jumps between
//! them, and no event reports a pressure.
//!
//! # Which devices point
//!
//! [`Role::Pointer`](zgui_evdev::Role) answers a different question, and its answer is too narrow
//! in one direction and too broad in the other. It asks for `REL_X` and `REL_Y`, so a touchscreen
//! and a graphics tablet fail it, and this backend reads both. It asks for no button, so a device
//! that reports two relative axes and nothing to press passes it.
//!
//! [`points_with`] asks for direct evidence of both halves, in the spirit of
//! [`types_on`](crate::input::seat::types_on): a pair of axes to say where, and a button to press
//! with. A device can be a keyboard and a pointer at once, and several are.

use std::collections::BTreeSet;

use zgui_evdev::{Absolute, AxisRange, Batch, Capabilities, EventType, Key, Relative};
use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{Surface, SurfaceId};
use zgui_vocab::{PointerAction, PointerButton, PointerEvent, PointerId, PointerKind};

/// The buttons that are evidence somebody presses with this device.
///
/// The mouse's own button, the touchscreen's contact, and the two a tablet's stylus has.
// These are the codes udev's `input_id` builtin looks for when it decides that a device is a mouse,
// a touchscreen or a tablet, so the same question is asked of the same codes.
const PRESSED_WITH: &[Key] = &[
    Key::BTN_LEFT,
    Key::BTN_TOUCH,
    Key::BTN_STYLUS,
    Key::BTN_TOOL_PEN,
];

/// How a device says where the pointer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axes {
    /// It says how far it moved, and this backend owns the position.
    Relative,
    /// It says where it is, inside the ranges the driver reported.
    Absolute {
        /// The range `ABS_X` reads in.
        x: Span,
        /// The range `ABS_Y` reads in.
        y: Span,
    },
}

/// The ends of one absolute axis, as `EVIOCGABS` reported them.
///
/// A touchscreen counts in its own units — nought to 4095 is ordinary — and nothing above knows
/// what one of them is. So a reading crosses as a fraction of the range, and the range has to be
/// asked for: a device measured against a guessed one puts the pointer at a fraction of where the
/// finger is.
///
/// ```
/// use zgui_platform_drm::input::pointer::Span;
///
/// let axis = Span { minimum: 0, maximum: 4095 };
///
/// assert_eq!(axis.fraction(0), 0.0);
/// assert_eq!(axis.fraction(4095), 1.0);
/// assert_eq!(axis.fraction(9000), 1.0, "a reading past the stated end is pulled back inside it");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// The smallest value the axis reports.
    pub minimum: i32,
    /// The largest value it reports.
    pub maximum: i32,
}

impl Span {
    /// Returns the range `reported` describes.
    pub const fn of(reported: AxisRange) -> Self {
        Self {
            minimum: reported.minimum,
            maximum: reported.maximum,
        }
    }

    /// Returns where `value` sits in this range, from nought to one.
    ///
    /// A driver that reports one value for both ends answers with the left edge rather than with a
    /// division by zero, and a reading outside the range it stated is pulled back inside it.
    pub fn fraction(self, value: i32) -> f32 {
        let span = self.maximum as f32 - self.minimum as f32;
        if span <= 0.0 {
            return 0.0;
        }
        ((value as f32 - self.minimum as f32) / span).clamp(0.0, 1.0)
    }
}

/// Returns `true` if a person points with this device.
///
/// **The device has to have a pair of axes and something to press.** Either half on its own is met
/// by devices nobody points with, and taking one of those costs the session a function with no way
/// to get it back while the program runs — the same trade
/// [`types_on`](crate::input::seat::types_on) makes, running the same way.
///
/// The axes alone are met by a keyboard: this machine's own keyboard advertises `REL_HWHEEL` and
/// `REL_HWHEEL_HI_RES` for the roller above its keypad, and a rule written on `EV_REL` would read
/// that roller as a pointer's wheel and scroll a document sideways whenever somebody changed the
/// volume. The button alone is met by anything with a `BTN_*` code at all, which includes every
/// gamepad.
///
/// ```
/// use zgui_evdev::{Absolute, Bitmap, Capabilities, EventType, Key, Relative};
/// use zgui_platform_drm::input::pointer::points_with;
///
/// let mouse = Capabilities::new(
///     Bitmap::from_codes([EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL]),
///     Bitmap::from_codes([Key::BTN_LEFT, Key::BTN_RIGHT]),
///     Bitmap::from_codes([Relative::REL_X, Relative::REL_Y, Relative::REL_WHEEL]),
///     Bitmap::<Absolute>::default(),
/// );
/// let keyboard = Capabilities::new(
///     Bitmap::from_codes([EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL]),
///     Bitmap::from_codes([Key::KEY_A, Key::KEY_LEFTSHIFT]),
///     Bitmap::from_codes([Relative::REL_HWHEEL]),
///     Bitmap::<Absolute>::default(),
/// );
///
/// assert!(points_with(&mouse));
/// assert!(!points_with(&keyboard), "the roller above a keypad is no pointer");
/// ```
pub fn points_with(capabilities: &Capabilities) -> bool {
    pressed_with(capabilities) && (relative(capabilities) || absolute(capabilities))
}

/// Returns `true` if this device has a button somebody points and presses with.
fn pressed_with(capabilities: &Capabilities) -> bool {
    capabilities.has(EventType::EV_KEY)
        && PRESSED_WITH
            .iter()
            .any(|button| capabilities.keys().contains(*button))
}

/// Returns `true` if this device reports a change on both axes.
pub fn relative(capabilities: &Capabilities) -> bool {
    capabilities.has(EventType::EV_REL)
        && capabilities.relative().contains(Relative::REL_X)
        && capabilities.relative().contains(Relative::REL_Y)
}

/// Returns `true` if this device reports a position on both axes.
pub fn absolute(capabilities: &Capabilities) -> bool {
    capabilities.has(EventType::EV_ABS)
        && capabilities.absolute().contains(Absolute::ABS_X)
        && capabilities.absolute().contains(Absolute::ABS_Y)
}

/// Returns which button `key` is, when it is one a pointer has.
///
/// The named five cross to their own names, and they are the five the windowing backend reaches
/// through its own library for these same codes.
///
/// The rest of the kernel's mouse block keeps its own number rather than collapsing into the
/// primary button: a mouse with eight buttons is a mouse somebody bound all eight of. **That
/// number is not portable, and it is the kernel's here.** A Wayland session hands winit the evdev
/// code unchanged, so a button beyond the named five carries the same number on both backends;
/// an X11 session hands it the X11 button index instead, so the same physical button is a
/// different number there. Nothing at this layer can reconcile the two — the vocabulary carries
/// one opaque number and each backend fills it in with what its own platform said — so a shortcut
/// bound to an unnamed button is a shortcut bound per session type.
///
/// A tool code answers with nothing. `BTN_TOOL_PEN` says a stylus came within range of a tablet
/// and nobody pressed anything, so reading it as a press would click wherever the pen was pointing
/// as it approached.
///
/// ```
/// use zgui_evdev::Key;
/// use zgui_platform_drm::input::pointer::button;
/// use zgui_vocab::PointerButton;
///
/// assert_eq!(button(Key::BTN_LEFT), Some(PointerButton::Primary));
/// assert_eq!(button(Key::BTN_TOUCH), Some(PointerButton::Primary), "a contact is a press");
/// assert_eq!(
///     button(Key::BTN_TASK),
///     Some(PointerButton::Other(Key::BTN_TASK.raw())),
///     "past the named five, the kernel's own code is carried"
/// );
/// assert_eq!(button(Key::BTN_TOOL_PEN), None, "a tool is no button");
/// assert_eq!(button(Key::KEY_A), None);
/// ```
pub fn button(key: Key) -> Option<PointerButton> {
    match key {
        // A contact is a press of the primary button, and the windowing backend reports the same
        // for a finger and for a pen tip.
        Key::BTN_LEFT | Key::BTN_TOUCH => Some(PointerButton::Primary),
        Key::BTN_RIGHT | Key::BTN_STYLUS => Some(PointerButton::Secondary),
        Key::BTN_MIDDLE | Key::BTN_STYLUS2 => Some(PointerButton::Middle),
        Key::BTN_SIDE | Key::BTN_BACK => Some(PointerButton::Back),
        Key::BTN_EXTRA | Key::BTN_FORWARD => Some(PointerButton::Forward),
        // `BTN_0` to `BTN_9` and the rest of the mouse block, up to where the joystick's buttons
        // begin. A gamepad's or a tablet's own codes are past that and are nobody's pointer.
        key if (Key::BTN_MISC.raw()..Key::BTN_JOYSTICK.raw()).contains(&key.raw()) => {
            Some(PointerButton::Other(key.raw()))
        }
        _ => None,
    }
}

/// What one coherent update from a pointing device amounts to.
///
/// A batch is one update, so `REL_X` and `REL_Y` in it are one diagonal motion. Read as two events
/// they are two motions along one axis each, and a cursor moves along one axis and then the other.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Motion {
    /// How far it moved, in device pixels, where it says how far.
    pub by: Option<(f32, f32)>,
    /// Where it is, as a fraction of each axis, where it says where.
    pub to: Option<(f32, f32)>,
    /// The buttons that changed, in the order the kernel reported them.
    pub buttons: Vec<(PointerButton, PointerAction)>,
}

impl Motion {
    /// Returns `true` if this update moved nothing and pressed nothing.
    pub fn is_empty(&self) -> bool {
        self.by.is_none() && self.to.is_none() && self.buttons.is_empty()
    }
}

/// Returns what one batch from one pointing device amounts to.
///
/// `down` is that device's own set of held buttons, and it decides what is delivered: a press of a
/// button the device already has down and a release of one it does not are both dropped. The
/// kernel reports a button that was already held when the device was taken through `EVIOCGKEY`
/// *and* through the stream, and delivered twice it leaves a control holding a press it never sees
/// released.
///
/// A relative axis accumulates rather than replacing, because a batch may carry `REL_X` twice.
pub fn batch(axes: Axes, down: &mut BTreeSet<u16>, batch: &Batch) -> Motion {
    let mut motion = Motion::default();
    let mut absolute = (None, None);
    for event in &batch.events {
        if let Some(axis) = event.relative() {
            let (x, y) = motion.by.unwrap_or((0.0, 0.0));
            motion.by = match axis {
                Relative::REL_X => Some((x + event.value as f32, y)),
                Relative::REL_Y => Some((x, y + event.value as f32)),
                _ => continue,
            };
        } else if let Some(axis) = event.absolute() {
            match axis {
                Absolute::ABS_X => absolute.0 = Some(event.value),
                Absolute::ABS_Y => absolute.1 = Some(event.value),
                _ => continue,
            }
        } else if let Some(key) = event.key() {
            let Some(button) = button(key) else {
                continue;
            };
            // A button repeats on no device this crate has met. A value the kernel does not write
            // records nothing, so it cannot unbalance the held set either.
            let action = match event.value {
                0 if down.remove(&key.raw()) => PointerAction::Released,
                1 if down.insert(key.raw()) => PointerAction::Pressed,
                _ => continue,
            };
            motion.buttons.push((button, action));
        }
    }

    // Both axes or neither. A reading that carries one axis says nothing about the other, and
    // completing the pair from where the pointer already is would put the finger where it is not.
    // So such an update moves the pointer nowhere, and the device reports both axes again.
    if let Axes::Absolute {
        x: span_x,
        y: span_y,
    } = axes
    {
        motion.to = match absolute {
            (Some(x), Some(y)) => Some((span_x.fraction(x), span_y.fraction(y))),
            _ => None,
        };
    }
    motion
}

/// One display the pointer can be on.
///
/// A console arranges nothing: every display is driven from its own framebuffer and the kernel
/// says nothing about where any of them is. So this backend arranges them, and the arrangement is
/// the order the connectors came back in, left to right, each with its top edge at zero. It is a
/// decision rather than an answer, and it is the only arrangement that lets a pointer cross from
/// one display to the next at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    /// Which surface this display is seen as.
    pub id: SurfaceId,
    /// Where its left edge is, in device pixels from the left edge of the first display.
    pub left: f32,
    /// How wide it is, in device pixels.
    pub width: f32,
    /// How tall it is, in device pixels.
    pub height: f32,
    /// How many device pixels there are to a CSS pixel on it.
    pub scale: f64,
}

impl Screen {
    /// Returns the claimed displays side by side, in the order they were found.
    ///
    /// ```
    /// use zgui_geom::{DevicePx, Size};
    /// use zgui_platform::{Surface, SurfaceId};
    /// use zgui_platform_drm::input::pointer::Screen;
    /// use zgui_platform_headless::OffscreenSurface;
    ///
    /// let first = OffscreenSurface::new(
    ///     SurfaceId::new(1),
    ///     Size::new(DevicePx(1920.0), DevicePx(1080.0)),
    /// );
    /// let second = OffscreenSurface::new(
    ///     SurfaceId::new(2),
    ///     Size::new(DevicePx(1280.0), DevicePx(720.0)),
    /// );
    ///
    /// let row = Screen::row([&first as &dyn Surface, &second as &dyn Surface]);
    ///
    /// assert_eq!(row[0].left, 0.0);
    /// assert_eq!(row[1].left, 1920.0, "the second display begins where the first ends");
    /// ```
    pub fn row<'a>(displays: impl IntoIterator<Item = &'a dyn Surface>) -> Vec<Self> {
        let mut left = 0.0;
        displays
            .into_iter()
            .map(|display| {
                let size = display.size();
                let screen = Self {
                    id: display.id(),
                    left,
                    width: size.width.0,
                    height: size.height.0,
                    scale: display.scale_factor(),
                };
                left += size.width.0;
                screen
            })
            .collect()
    }
}

/// Where the pointer is, across every display the application claimed.
///
/// The position belongs to this backend: a mouse reports how far it moved and never where it is,
/// so something has to hold the answer between reports. An absolute device writes into the same
/// position, so a machine with a mouse and a touchscreen has one pointer that both of them move.
///
/// ```
/// use zgui_platform::SurfaceId;
/// use zgui_platform_drm::input::pointer::{Pointer, Screen};
///
/// let screens = [
///     Screen { id: SurfaceId::new(1), left: 0.0, width: 1920.0, height: 1080.0, scale: 1.0 },
///     Screen { id: SurfaceId::new(2), left: 1920.0, width: 1280.0, height: 720.0, scale: 1.0 },
/// ];
///
/// let mut pointer = Pointer::centred(&screens);
/// assert_eq!(pointer.union(), (960.0, 540.0));
///
/// pointer.moved_by(1000.0, 0.0, &screens);
/// let (id, _at) = pointer.position(&screens).expect("it is on a display");
/// assert_eq!(id, SurfaceId::new(2), "it crossed to the second display");
///
/// pointer.moved_by(10_000.0, 10_000.0, &screens);
/// assert_eq!(
///     pointer.union(),
///     (3199.0, 719.0),
///     "the last column and the last row of the display it is on"
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    /// Where it is, in device pixels from the left edge of the first display.
    x: f32,
    /// Where it is, in device pixels from the top edge of the display it is on.
    y: f32,
}

impl Pointer {
    /// Returns a pointer in the middle of the first display.
    ///
    /// The middle rather than a corner: a pointer that starts at the origin is one a person has to
    /// find, and a console draws no cursor until this backend puts one there.
    pub fn centred(screens: &[Screen]) -> Self {
        let (x, y) = screens.first().map_or((0.0, 0.0), |screen| {
            (screen.width / 2.0, screen.height / 2.0)
        });
        Self::at(x, y, screens)
    }

    /// Returns a pointer at `x`, `y`, pulled inside the displays.
    pub fn at(x: f32, y: f32, screens: &[Screen]) -> Self {
        let mut pointer = Self { x, y };
        pointer.clamp(screens);
        pointer
    }

    /// Returns where it is, in device pixels from the left edge of the first display and from the
    /// top edge of the display it is on.
    pub const fn union(self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// Moves it `dx`, `dy` device pixels, and keeps it inside the displays.
    pub fn moved_by(&mut self, dx: f32, dy: f32, screens: &[Screen]) {
        self.x += dx;
        self.y += dy;
        self.clamp(screens);
    }

    /// Puts it at `fx`, `fy` of the way across the displays, where a device says where it is.
    ///
    /// The horizontal fraction spans every display and the vertical one spans the display it lands
    /// on. A tablet or a touchscreen states no display of its own — there is no session daemon
    /// here to bind one to an output — so it drives the whole arrangement, as a compositor does
    /// with a tablet nobody has configured. The cost is that a device physically stuck
    /// to one display of two reaches both and matches neither.
    pub fn moved_to(&mut self, fx: f32, fy: f32, screens: &[Screen]) {
        let across = screens
            .last()
            .map_or(0.0, |screen| screen.left + screen.width);
        self.x = fx * across;
        self.clamp(screens);
        let height = self.on(screens).map_or(0.0, |screen| screen.height);
        self.y = fy * height;
        self.clamp(screens);
    }

    /// Returns which display it is on.
    ///
    /// Nothing while the application has claimed none, because a program that asked for no
    /// display has nowhere to put a pointer.
    pub fn on(self, screens: &[Screen]) -> Option<&Screen> {
        screens
            .iter()
            .rev()
            .find(|screen| self.x >= screen.left)
            .or_else(|| screens.first())
    }

    /// Returns which display it is on and where it is on it, in the space a layout is written in.
    pub fn position(self, screens: &[Screen]) -> Option<(SurfaceId, Point<CssPx, Css>)> {
        let screen = self.on(screens)?;
        Some((
            screen.id,
            position(self.x - screen.left, self.y, screen.scale),
        ))
    }

    /// Pulls the position back inside the displays.
    ///
    /// One pixel short of the far edge on each axis, which is where the last column and the last
    /// row of a display are. A pointer at the width itself is on no display at all, and every
    /// answer that reads one would then need a second meaning.
    fn clamp(&mut self, screens: &[Screen]) {
        // A value that is not a number would poison every later comparison, and the arrangement
        // below decides which display a person is looking at. Neither axis is left holding one.
        if !self.x.is_finite() {
            self.x = 0.0;
        }
        if !self.y.is_finite() {
            self.y = 0.0;
        }
        let across = screens
            .last()
            .map_or(0.0, |screen| screen.left + screen.width);
        self.x = self.x.clamp(0.0, (across - 1.0).max(0.0));
        let height = self.on(screens).map_or(0.0, |screen| screen.height);
        self.y = self.y.clamp(0.0, (height - 1.0).max(0.0));
    }
}

/// Returns where the pointer is on one display, in the space a layout is written in.
///
/// The kernel measures in device pixels because that is what the hardware has; everything above is
/// written in CSS pixels because that is what a stylesheet is written in. The surface's own scale
/// crosses between them, and it is the surface's rather than the display's: the two are one on
/// this backend today, and the contract allows a surface presented at another scale.
///
/// ```
/// use zgui_geom::{CssPx, Point};
/// use zgui_platform_drm::input::pointer::position;
///
/// assert_eq!(position(300.0, 150.0, 1.5), Point::new(CssPx(200.0), CssPx(100.0)));
/// // A scale of zero would place every pointer at infinity, so it is read as one.
/// assert_eq!(position(12.0, 30.0, 0.0), Point::new(CssPx(12.0), CssPx(30.0)));
/// ```
pub fn position(x: f32, y: f32, scale_factor: f64) -> Point<CssPx, Css> {
    // A scale of zero is no reason to place every pointer at infinity. Dividing by it would do
    // that, and nothing downstream would survive it.
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    Point::new(
        CssPx((f64::from(x) / scale) as f32),
        CssPx((f64::from(y) / scale) as f32),
    )
}

/// Returns the pointer at `position`, carrying the button that was used.
///
/// One pointer, reported as the mouse. See the head of this module for what that leaves out.
pub fn event(position: Point<CssPx, Css>, button: Option<PointerButton>) -> PointerEvent {
    PointerEvent {
        id: PointerId::MOUSE,
        kind: PointerKind::Mouse,
        primary: true,
        position,
        button,
        // No device this backend reads reports one. Absent rather than zero: one says the device
        // cannot tell, and the other says the pen is not touching the glass.
        pressure: None,
    }
}

#[cfg(test)]
mod tests {
    //! The translation, over bytes written here and displays described here.
    //!
    //! No device and no display. `zgui_evdev::Reader::feed` turns bytes into batches with no
    //! descriptor anywhere, and a screen is four numbers — so the cases that matter are the ones a
    //! working mouse rarely produces: a diagonal, a crossing, an edge, a keyboard's roller.

    use std::collections::BTreeSet;
    use std::time::Duration;

    use super::{
        Axes, Motion, Pointer, Screen, Span, batch, button, points_with, position, relative,
    };
    use zgui_evdev::{
        Absolute, Bitmap, Capabilities, EventType, Key, Reader, Relative, Synchronisation,
    };
    use zgui_geom::{CssPx, DevicePx, Point, Size};
    use zgui_platform::{Surface, SurfaceId};
    use zgui_platform_headless::OffscreenSurface;
    use zgui_vocab::{PointerAction, PointerButton};

    /// The capabilities of a device with these types, keys and axes.
    fn capabilities(
        types: &[EventType],
        keys: &[Key],
        relative: &[Relative],
        absolute: &[Absolute],
    ) -> Capabilities {
        Capabilities::new(
            Bitmap::from_codes(types.iter().copied()),
            Bitmap::from_codes(keys.iter().copied()),
            Bitmap::from_codes(relative.iter().copied()),
            Bitmap::from_codes(absolute.iter().copied()),
        )
    }

    /// The bytes of one record, as the kernel lays out `input_event`.
    fn record(kind: EventType, code: u16, value: i32) -> Vec<u8> {
        let at = Duration::from_secs(1);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_i64.to_ne_bytes());
        bytes.extend_from_slice(&i64::from(at.subsec_micros()).to_ne_bytes());
        bytes.extend_from_slice(&kind.raw().to_ne_bytes());
        bytes.extend_from_slice(&code.to_ne_bytes());
        bytes.extend_from_slice(&value.to_ne_bytes());
        bytes
    }

    /// The bytes that end one coherent update.
    fn report() -> Vec<u8> {
        record(EventType::EV_SYN, Synchronisation::SYN_REPORT.raw(), 0)
    }

    /// What one update of these records amounts to.
    fn update(axes: Axes, records: &[Vec<u8>]) -> Motion {
        let mut bytes: Vec<u8> = records.concat();
        bytes.extend(report());
        let mut reader = Reader::new();
        let batches = reader.feed(&bytes);
        let mut down = BTreeSet::new();
        let [read] = &batches[..] else {
            panic!("one report is one batch: {batches:?}");
        };
        batch(axes, &mut down, read)
    }

    /// Two displays side by side, the second smaller than the first.
    ///
    /// Written out rather than built from surfaces, so that what a test states is the arrangement
    /// itself. [`Screen::row`] is what builds one from the displays, and it has a test of its own.
    fn screens() -> Vec<Screen> {
        vec![
            Screen {
                id: SurfaceId::new(1),
                left: 0.0,
                width: 1920.0,
                height: 1080.0,
                scale: 1.0,
            },
            Screen {
                id: SurfaceId::new(2),
                left: 1920.0,
                width: 1280.0,
                height: 720.0,
                scale: 1.0,
            },
        ]
    }

    #[test]
    fn the_displays_are_laid_out_left_to_right_in_the_order_they_were_found() {
        // A console arranges nothing, so this arrangement is the backend's own decision. It is the
        // only one that lets a pointer cross from one display to the next at all.
        let first = OffscreenSurface::new(
            SurfaceId::new(1),
            Size::new(DevicePx(1920.0), DevicePx(1080.0)),
        );
        let second = OffscreenSurface::new(
            SurfaceId::new(2),
            Size::new(DevicePx(1280.0), DevicePx(720.0)),
        );

        let row = Screen::row([&first as &dyn Surface, &second as &dyn Surface]);

        assert_eq!(row, screens());
    }

    #[test]
    fn a_mouse_is_a_device_a_person_points_with() {
        // The Razer mouse node on the development machine: five buttons, two axes and a wheel.
        let mouse = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL],
            &[Key::BTN_LEFT, Key::BTN_RIGHT, Key::BTN_MIDDLE],
            &[Relative::REL_X, Relative::REL_Y, Relative::REL_WHEEL],
            &[],
        );

        assert!(points_with(&mouse));
    }

    #[test]
    fn a_keyboards_roller_is_not_a_pointer() {
        // The Razer keyboard node on the development machine, as `/proc/bus/input/devices`
        // reports it: a full key map and `REL_HWHEEL` for the roller above the keypad, with no
        // `REL_X` and no `REL_Y`. A rule written on `EV_REL` reads that roller as a pointer's
        // wheel, and a document scrolls sideways whenever somebody changes the volume.
        let keyboard = capabilities(
            &[
                EventType::EV_SYN,
                EventType::EV_KEY,
                EventType::EV_REL,
                EventType::EV_ABS,
            ],
            &[Key::KEY_A, Key::KEY_LEFTSHIFT],
            &[Relative::REL_HWHEEL, Relative::REL_HWHEEL_HI_RES],
            &[Absolute::ABS_VOLUME, Absolute::ABS_MISC],
        );

        assert!(!points_with(&keyboard));
        assert!(!relative(&keyboard), "it reports neither axis");
    }

    #[test]
    fn a_touchscreen_points_even_though_it_has_no_relative_axis() {
        // `Role::Pointer` asks for `REL_X` and `REL_Y`, so it says no to every device that reports
        // where it is rather than how far it moved. Those are the ones this backend reads through
        // `EVIOCGABS`.
        let touchscreen = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_ABS],
            &[Key::BTN_TOUCH],
            &[],
            &[Absolute::ABS_X, Absolute::ABS_Y],
        );

        assert!(
            !touchscreen.roles().contains(zgui_evdev::Role::Pointer),
            "udev's own rule calls this no pointer, which is why the narrower question exists"
        );
        assert!(points_with(&touchscreen));
    }

    #[test]
    fn a_gamepad_is_not_a_pointer() {
        // Two absolute axes for the stick and a block of buttons. Nobody points with it, and a
        // grab would take it away from whatever is using it.
        let gamepad = capabilities(
            &[EventType::EV_SYN, EventType::EV_KEY, EventType::EV_ABS],
            &[Key::BTN_SOUTH, Key::BTN_NORTH, Key::BTN_TL],
            &[],
            &[Absolute::ABS_X, Absolute::ABS_Y],
        );

        assert!(!points_with(&gamepad));
    }

    #[test]
    fn a_device_with_axes_and_nothing_to_press_is_left_alone() {
        let axes_only = capabilities(
            &[EventType::EV_SYN, EventType::EV_REL],
            &[],
            &[Relative::REL_X, Relative::REL_Y],
            &[],
        );

        assert!(!points_with(&axes_only));
    }

    #[test]
    fn every_button_crosses_to_its_own_button() {
        // The five the windowing backend names too. Its Wayland path matches these codes one for
        // one; its X11 path reads an X11 button index and reaches the same five names by another
        // route, so a shortcut bound to a *named* button is the same button everywhere.
        let pairs = [
            (Key::BTN_LEFT, PointerButton::Primary),
            (Key::BTN_RIGHT, PointerButton::Secondary),
            (Key::BTN_MIDDLE, PointerButton::Middle),
            (Key::BTN_SIDE, PointerButton::Back),
            (Key::BTN_BACK, PointerButton::Back),
            (Key::BTN_EXTRA, PointerButton::Forward),
            (Key::BTN_FORWARD, PointerButton::Forward),
            (Key::BTN_TOUCH, PointerButton::Primary),
        ];
        for (code, crossed) in pairs {
            assert_eq!(button(code), Some(crossed), "{code:?} crossed wrongly");
        }
    }

    #[test]
    fn a_button_beyond_the_named_ones_keeps_its_own_number() {
        // A mouse with eight buttons is a mouse somebody bound all eight of, and the number is the
        // kernel's own code. It agrees with the windowing backend on a Wayland session, where the
        // compositor passes the evdev code through, and not on an X11 one, where that backend
        // reports the X11 button index instead. The number is opaque above this layer, so nothing
        // could reconcile them; what this asserts is that it is the kernel's rather than an index
        // of this backend's own invention.
        assert_eq!(
            button(Key::BTN_TASK),
            Some(PointerButton::Other(Key::BTN_TASK.raw()))
        );
        assert_eq!(
            button(Key::BTN_0),
            Some(PointerButton::Other(Key::BTN_0.raw()))
        );
    }

    #[test]
    fn a_key_and_a_tool_are_not_buttons() {
        // `KEY_MACRO27` is what the Razer mouse node reports beside its buttons, and a person
        // typed it. `BTN_TOOL_PEN` says a stylus came within range of a tablet and nobody pressed
        // anything, so reading it as a press clicks wherever the pen was pointing.
        assert_eq!(button(Key::KEY_MACRO27), None);
        assert_eq!(button(Key::KEY_A), None);
        assert_eq!(button(Key::BTN_TOOL_PEN), None);
        assert_eq!(button(Key::BTN_TOOL_FINGER), None);
        assert_eq!(
            button(Key::BTN_SOUTH),
            None,
            "a gamepad's button is nobody's"
        );
    }

    #[test]
    fn a_diagonal_move_is_one_motion_rather_than_two() {
        // Why a batch is the unit. Read as two events this pointer moves along one axis and then
        // the other, and a person sees the cursor take a corner where they drew a diagonal.
        let moved = update(
            Axes::Relative,
            &[
                record(EventType::EV_REL, Relative::REL_X.raw(), 4),
                record(EventType::EV_REL, Relative::REL_Y.raw(), -3),
            ],
        );

        assert_eq!(moved.by, Some((4.0, -3.0)));
        assert_eq!(moved.to, None);
    }

    #[test]
    fn one_axis_reported_twice_in_an_update_accumulates() {
        let moved = update(
            Axes::Relative,
            &[
                record(EventType::EV_REL, Relative::REL_X.raw(), 4),
                record(EventType::EV_REL, Relative::REL_X.raw(), 3),
            ],
        );

        assert_eq!(moved.by, Some((7.0, 0.0)), "one update moved seven pixels");
    }

    #[test]
    fn a_wheel_in_a_pointers_batch_moves_the_pointer_nowhere() {
        // `REL_WHEEL` is an axis and it is not a position. Reading every relative code as motion
        // walks the pointer up the screen with every notch.
        let moved = update(
            Axes::Relative,
            &[record(EventType::EV_REL, Relative::REL_WHEEL.raw(), 1)],
        );

        assert!(moved.is_empty(), "{moved:?}");
    }

    #[test]
    fn an_absolute_device_reports_where_it_is_as_a_fraction_of_its_own_range() {
        let axes = Axes::Absolute {
            x: Span {
                minimum: 0,
                maximum: 4095,
            },
            y: Span {
                minimum: 0,
                maximum: 4095,
            },
        };

        let moved = update(
            axes,
            &[
                record(EventType::EV_ABS, Absolute::ABS_X.raw(), 4095),
                record(EventType::EV_ABS, Absolute::ABS_Y.raw(), 0),
            ],
        );

        assert_eq!(moved.to, Some((1.0, 0.0)));
        assert_eq!(
            moved.by, None,
            "an absolute device says nothing about how far"
        );
    }

    #[test]
    fn an_absolute_reading_with_one_axis_missing_moves_nothing() {
        // A device that reported one axis said nothing about the other, and completing the pair
        // from the pointer's own place would put the finger where it is not.
        let axes = Axes::Absolute {
            x: Span {
                minimum: 0,
                maximum: 4095,
            },
            y: Span {
                minimum: 0,
                maximum: 4095,
            },
        };

        let moved = update(
            axes,
            &[record(EventType::EV_ABS, Absolute::ABS_X.raw(), 2048)],
        );

        assert_eq!(moved.to, None);
    }

    #[test]
    fn an_axis_whose_ends_are_the_same_value_reads_as_the_left_edge() {
        // A driver that states no range at all. The alternative is a division by zero, which is a
        // position of infinity and a pointer nothing can bring back.
        let span = Span {
            minimum: 7,
            maximum: 7,
        };
        assert_eq!(span.fraction(7), 0.0);
        assert_eq!(span.fraction(9), 0.0);
    }

    #[test]
    fn a_reading_outside_the_stated_range_is_pulled_back_inside_it() {
        let span = Span {
            minimum: 100,
            maximum: 200,
        };
        assert_eq!(span.fraction(50), 0.0);
        assert_eq!(span.fraction(250), 1.0);
        assert_eq!(span.fraction(150), 0.5);
    }

    #[test]
    fn a_press_and_its_release_cross_once_each() {
        let mut down = BTreeSet::new();
        let mut reader = Reader::new();
        let mut bytes = record(EventType::EV_KEY, Key::BTN_LEFT.raw(), 1);
        bytes.extend(report());
        bytes.extend(record(EventType::EV_KEY, Key::BTN_LEFT.raw(), 0));
        bytes.extend(report());

        let crossed: Vec<_> = reader
            .feed(&bytes)
            .iter()
            .flat_map(|read| batch(Axes::Relative, &mut down, read).buttons)
            .collect();

        assert_eq!(
            crossed,
            [
                (PointerButton::Primary, PointerAction::Pressed),
                (PointerButton::Primary, PointerAction::Released),
            ]
        );
        assert!(down.is_empty(), "and nothing is left held");
    }

    #[test]
    fn a_press_of_a_button_the_device_already_has_down_crosses_nothing() {
        // The kernel reports a button held when the device was taken through `EVIOCGKEY` and
        // through the stream. Delivered twice it leaves a control holding a press it never sees
        // released.
        let mut down = BTreeSet::from([Key::BTN_LEFT.raw()]);
        let mut reader = Reader::new();
        let mut bytes = record(EventType::EV_KEY, Key::BTN_LEFT.raw(), 1);
        bytes.extend(report());

        let crossed: Vec<_> = reader
            .feed(&bytes)
            .iter()
            .flat_map(|read| batch(Axes::Relative, &mut down, read).buttons)
            .collect();

        assert!(crossed.is_empty(), "{crossed:?}");
    }

    #[test]
    fn a_release_of_a_button_that_was_never_down_crosses_nothing() {
        let mut down = BTreeSet::new();
        let mut reader = Reader::new();
        let mut bytes = record(EventType::EV_KEY, Key::BTN_LEFT.raw(), 0);
        bytes.extend(report());

        let crossed: Vec<_> = reader
            .feed(&bytes)
            .iter()
            .flat_map(|read| batch(Axes::Relative, &mut down, read).buttons)
            .collect();

        assert!(crossed.is_empty(), "{crossed:?}");
    }

    #[test]
    fn a_pointer_starts_in_the_middle_of_the_first_display() {
        // A pointer at the origin is one a person has to find, and nothing here drew a cursor
        // before this backend put one on the screen.
        let screens = screens();
        let pointer = Pointer::centred(&screens);

        assert_eq!(pointer.union(), (960.0, 540.0));
        assert_eq!(
            pointer.on(&screens).map(|screen| screen.id),
            Some(SurfaceId::new(1))
        );
    }

    #[test]
    fn a_pointer_crosses_from_one_display_to_the_next() {
        let screens = screens();
        let mut pointer = Pointer::at(1900.0, 300.0, &screens);
        assert_eq!(
            pointer.on(&screens).map(|screen| screen.id),
            Some(SurfaceId::new(1))
        );

        pointer.moved_by(100.0, 0.0, &screens);

        let (id, at) = pointer.position(&screens).expect("it is on a display");
        assert_eq!(id, SurfaceId::new(2), "it crossed");
        assert_eq!(
            at,
            Point::new(CssPx(80.0), CssPx(300.0)),
            "and its place is measured from the left edge of the display it is on"
        );
    }

    #[test]
    fn a_pointer_stays_inside_the_displays() {
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);

        pointer.moved_by(-10_000.0, -10_000.0, &screens);
        assert_eq!(pointer.union(), (0.0, 0.0));

        pointer.moved_by(10_000.0, 10_000.0, &screens);
        assert_eq!(
            pointer.union(),
            (3199.0, 719.0),
            "the last column of the last display, and the last row of it"
        );
    }

    #[test]
    fn a_pointer_over_a_short_display_cannot_stand_below_it() {
        // The two displays are different heights, so the ground is not a rectangle. A pointer
        // taken across at the bottom of the tall one would otherwise be under the short one, where
        // no display would answer for it.
        let screens = screens();
        let mut pointer = Pointer::at(100.0, 1000.0, &screens);

        pointer.moved_by(2000.0, 0.0, &screens);

        assert_eq!(pointer.union(), (2100.0, 719.0));
        assert_eq!(
            pointer.on(&screens).map(|screen| screen.id),
            Some(SurfaceId::new(2))
        );
    }

    #[test]
    fn a_pointer_with_no_display_to_be_on_is_on_none() {
        // What a program that claimed no display looks like. Reported rather than placed at the
        // origin of a display that is not there.
        let pointer = Pointer::centred(&[]);

        assert_eq!(pointer.union(), (0.0, 0.0));
        assert!(pointer.on(&[]).is_none());
        assert!(pointer.position(&[]).is_none());
    }

    #[test]
    fn an_absolute_device_reaches_every_display() {
        let screens = screens();
        let mut pointer = Pointer::centred(&screens);

        pointer.moved_to(1.0, 1.0, &screens);
        assert_eq!(pointer.union(), (3199.0, 719.0));

        pointer.moved_to(0.0, 0.0, &screens);
        assert_eq!(pointer.union(), (0.0, 0.0));
    }

    #[test]
    fn a_position_crosses_out_of_device_pixels_by_the_surfaces_own_scale() {
        assert_eq!(
            position(200.0, 100.0, 2.0),
            Point::new(CssPx(100.0), CssPx(50.0))
        );
    }

    #[test]
    fn a_scale_of_zero_is_treated_as_one_rather_than_producing_infinities() {
        // A backend answering zero is no reason to place every pointer at infinity. Dividing by
        // it would do that, and nothing downstream would survive it.
        assert_eq!(position(8.0, 8.0, 0.0), Point::new(CssPx(8.0), CssPx(8.0)));
    }
}
