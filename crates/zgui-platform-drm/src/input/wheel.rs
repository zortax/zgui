//! Scrolling, in the unit the device measures in and facing the way this framework counts.
//!
//! # The high-resolution axes
//!
//! A wheel that reports `REL_WHEEL_HI_RES` reports `REL_WHEEL` for the same physical movement, in
//! the same batch. The high-resolution axis counts in **one hundred and twentieths of a detent**
//! and the other counts whole detents, so a reader that took both would scroll exactly twice as
//! far as the wheel was turned. It would do so smoothly, by a plausible amount and in the right
//! direction, so nothing about it looks like a defect.
//!
//! [`HighResolution`] decides, per device, from what the device says it has. A device that
//! advertises the fine axis is read on the fine axis alone. libinput does the same with the same
//! two codes. The answer belongs to the device rather than to one update: a free-spinning wheel
//! reports fine movement in every batch and a whole detent only when it has accumulated one, so a
//! rule written per batch would take both in the batch where they meet.
//!
//! # The unit
//!
//! A detent stays a detent. How far one is meant to travel depends on the used line height of the
//! element being scrolled, which is unknown here. So [`ScrollDelta::Lines`] leaves this backend,
//! and [`zgui_platform::scroll`] holds the rest of the answer.
//!
//! # The signs
//!
//! The kernel and this framework describe a scroll from opposite ends on the vertical axis and
//! from the same end on the horizontal one, so one axis is negated and the other keeps its sign.
//!
//! `REL_WHEEL` is positive when the wheel is pushed **away** from the person, which reveals content
//! further **up**, which is a **smaller** scroll offset. So the vertical axis is negated.
//! `REL_HWHEEL` is positive when the wheel is tilted **right**, which reveals content further right,
//! which is a larger offset. So the horizontal axis keeps its sign.
//!
//! libinput applies the same pair of signs to these two codes, and the windowing backend reaches
//! the same place through its own library: a detent pushed away from the person leaves both
//! backends as a negative block delta.
//!
//! # The scroll settings
//!
//! Three lines to a detent, a detent that arrives whole, and the direction the hardware turned.
//! The first two are what the kernel delivers and what every application on a Linux desktop moves.
//! The third is worth stating: [`ScrollDirection::Inverted`] exists for a backend that reads the
//! device raw, which this one does, and it means "natural scrolling, always on". A console has
//! nobody to ask which the person wanted. So what leaves here is which way the hardware turned,
//! and [`PlatformCx::scroll_settings`] stays at the contract's own answer.
//!
//! [`ScrollDirection::Inverted`]: zgui_platform::scroll::ScrollDirection::Inverted
//! [`PlatformCx::scroll_settings`]: zgui_platform::PlatformCx::scroll_settings

use zgui_evdev::{Batch, Capabilities, Relative};
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::{PointerId, PointerKind, ScrollDelta, ScrollPhase, WheelEvent};

/// How many of the fine axis's steps make one detent.
///
/// The kernel's own number for `REL_WHEEL_HI_RES` and `REL_HWHEEL_HI_RES`, stated in
/// `Documentation/input/event-codes.rst`.
// Transcribed rather than read from a header: no header this crate binds carries it.
const STEPS_PER_DETENT: f32 = 120.0;

/// Which axes a device reports in hundred-and-twentieths of a detent.
///
/// Read from the device rather than from a batch. See the head of this module for why the two are
/// different questions.
///
/// ```
/// use zgui_evdev::{Absolute, Bitmap, Capabilities, EventType, Key, Relative};
/// use zgui_platform_drm::input::wheel::HighResolution;
///
/// let wheel = Capabilities::new(
///     Bitmap::from_codes([EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL]),
///     Bitmap::from_codes([Key::BTN_LEFT]),
///     Bitmap::from_codes([Relative::REL_WHEEL, Relative::REL_WHEEL_HI_RES]),
///     Bitmap::<Absolute>::default(),
/// );
///
/// let fine = HighResolution::of(&wheel);
///
/// assert!(fine.vertical);
/// assert!(!fine.horizontal, "one axis can be fine while the other is not");
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HighResolution {
    /// Whether `REL_WHEEL_HI_RES` is what this device's vertical wheel reports.
    pub vertical: bool,
    /// Whether `REL_HWHEEL_HI_RES` is what its horizontal wheel reports.
    pub horizontal: bool,
}

impl HighResolution {
    /// Returns the fine axes this device says it reports.
    pub fn of(capabilities: &Capabilities) -> Self {
        Self {
            vertical: capabilities.relative().contains(Relative::REL_WHEEL_HI_RES),
            horizontal: capabilities
                .relative()
                .contains(Relative::REL_HWHEEL_HI_RES),
        }
    }
}

/// Returns how far one batch asked to scroll, or nothing where it asked for none.
///
/// Detents, whole or fractional. A high-resolution wheel reports a fraction of one per step and a
/// notched one reports 120 steps per click, so both arrive here in the same unit.
///
/// ```
/// use std::time::Duration;
/// use zgui_evdev::{Batch, Event, EventType, Relative};
/// use zgui_platform_drm::input::wheel::{HighResolution, delta};
/// use zgui_vocab::ScrollDelta;
///
/// fn turned(axes: &[(Relative, i32)]) -> Batch {
///     Batch {
///         at: Duration::from_secs(1),
///         events: axes
///             .iter()
///             .map(|(axis, value)| Event {
///                 at: Duration::from_secs(1),
///                 kind: EventType::EV_REL,
///                 code: axis.raw(),
///                 value: *value,
///             })
///             .collect(),
///     }
/// }
///
/// let coarse = HighResolution::default();
///
/// // Pushed away from the person, which is a smaller offset here.
/// assert_eq!(
///     delta(&turned(&[(Relative::REL_WHEEL, 1)]), coarse),
///     Some(ScrollDelta::Lines { x: 0.0, y: -1.0 })
/// );
/// // Tilted right, which is a larger offset in both vocabularies.
/// assert_eq!(
///     delta(&turned(&[(Relative::REL_HWHEEL, 1)]), coarse),
///     Some(ScrollDelta::Lines { x: 1.0, y: 0.0 })
/// );
///
/// // A device with the fine axis reports both codes for one movement, and the fine one alone
/// // is read: one detent, rather than two.
/// let fine = HighResolution { vertical: true, horizontal: false };
/// assert_eq!(
///     delta(
///         &turned(&[(Relative::REL_WHEEL, 1), (Relative::REL_WHEEL_HI_RES, 120)]),
///         fine,
///     ),
///     Some(ScrollDelta::Lines { x: 0.0, y: -1.0 })
/// );
/// ```
pub fn delta(batch: &Batch, resolution: HighResolution) -> Option<ScrollDelta> {
    let mut x = 0.0;
    let mut y = 0.0;
    for event in &batch.events {
        let Some(axis) = event.relative() else {
            continue;
        };
        let value = event.value as f32;
        match axis {
            Relative::REL_WHEEL if !resolution.vertical => y -= value,
            Relative::REL_HWHEEL if !resolution.horizontal => x += value,
            Relative::REL_WHEEL_HI_RES if resolution.vertical => y -= value / STEPS_PER_DETENT,
            Relative::REL_HWHEEL_HI_RES if resolution.horizontal => x += value / STEPS_PER_DETENT,
            _ => continue,
        }
    }
    (x != 0.0 || y != 0.0).then_some(ScrollDelta::Lines { x, y })
}

/// Returns one scroll, at the pointer's own place.
///
/// A wheel turn carries no position of its own on any device, so the position is the one the
/// pointer is at. That position is what routes the turn to whatever is under the pointer.
///
/// Every scroll here is [`ScrollPhase::Discrete`]. A gesture has a beginning and an end, and a
/// touchpad is what has one. This backend reads no touchpad gestures, and a wheel — fine or
/// notched — is a stream of self-contained turns with nothing around them.
///
/// ```
/// use zgui_geom::{CssPx, Point};
/// use zgui_platform_drm::input::wheel::event;
/// use zgui_vocab::{PointerId, PointerKind, ScrollDelta, ScrollPhase};
///
/// let at = Point::new(CssPx(120.0), CssPx(80.0));
/// let turn = event(ScrollDelta::Lines { x: 0.0, y: -1.0 }, at);
///
/// assert_eq!(turn.position, at);
/// assert_eq!(turn.phase, ScrollPhase::Discrete);
/// assert_eq!(turn.id, PointerId::MOUSE);
/// assert_eq!(turn.kind, PointerKind::Mouse);
/// ```
pub fn event(delta: ScrollDelta, position: Point<CssPx, Css>) -> WheelEvent {
    WheelEvent {
        delta,
        phase: ScrollPhase::Discrete,
        position,
        id: PointerId::MOUSE,
        kind: PointerKind::Mouse,
    }
}

#[cfg(test)]
mod tests {
    //! The double report, the two signs, and the unit.

    use super::{HighResolution, delta};
    use zgui_evdev::{Absolute, Bitmap, Capabilities, EventType, Key, Reader, Relative};
    use zgui_vocab::ScrollDelta;

    /// The bytes of one record, as the kernel lays out `input_event`.
    fn record(kind: EventType, code: u16, value: i32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_i64.to_ne_bytes());
        bytes.extend_from_slice(&0_i64.to_ne_bytes());
        bytes.extend_from_slice(&kind.raw().to_ne_bytes());
        bytes.extend_from_slice(&code.to_ne_bytes());
        bytes.extend_from_slice(&value.to_ne_bytes());
        bytes
    }

    /// What one update of these axis readings asked for.
    fn turned(resolution: HighResolution, axes: &[(Relative, i32)]) -> Option<ScrollDelta> {
        let mut bytes = Vec::new();
        for (axis, value) in axes {
            bytes.extend(record(EventType::EV_REL, axis.raw(), *value));
        }
        bytes.extend(record(
            EventType::EV_SYN,
            zgui_evdev::Synchronisation::SYN_REPORT.raw(),
            0,
        ));
        let mut reader = Reader::new();
        let batches = reader.feed(&bytes);
        let [read] = &batches[..] else {
            panic!("one report is one batch: {batches:?}");
        };
        delta(read, resolution)
    }

    /// A device with these relative axes.
    fn wheel(axes: &[Relative]) -> Capabilities {
        Capabilities::new(
            Bitmap::from_codes([EventType::EV_SYN, EventType::EV_KEY, EventType::EV_REL]),
            Bitmap::from_codes([Key::BTN_LEFT]),
            Bitmap::from_codes(axes.iter().copied()),
            Bitmap::<Absolute>::default(),
        )
    }

    #[test]
    fn a_detent_pushed_away_from_the_person_scrolls_towards_the_top() {
        // The sign the kernel writes, and the one this framework reads. Without the negation a
        // document scrolls backwards — smoothly, by the right distance, and nothing anywhere
        // fails.
        assert_eq!(
            turned(HighResolution::default(), &[(Relative::REL_WHEEL, 1)]),
            Some(ScrollDelta::Lines { x: 0.0, y: -1.0 })
        );
        assert_eq!(
            turned(HighResolution::default(), &[(Relative::REL_WHEEL, -1)]),
            Some(ScrollDelta::Lines { x: 0.0, y: 1.0 })
        );
    }

    #[test]
    fn a_wheel_tilted_right_scrolls_right_and_keeps_its_sign() {
        // The asymmetry worth writing down: the kernel and this framework agree about which way is
        // right and disagree about which way is up. A backend that negated both would scroll every
        // table backwards, which nobody notices until the first horizontally scrolling one.
        assert_eq!(
            turned(HighResolution::default(), &[(Relative::REL_HWHEEL, 1)]),
            Some(ScrollDelta::Lines { x: 1.0, y: 0.0 })
        );
    }

    #[test]
    fn a_high_resolution_wheel_is_not_taken_twice() {
        // Why this module reads a device before it reads a batch. Both codes describe one physical
        // movement and arrive together, so taking both scrolls exactly twice as far.
        let fine = HighResolution {
            vertical: true,
            horizontal: true,
        };

        let turn = turned(
            fine,
            &[
                (Relative::REL_WHEEL, 1),
                (Relative::REL_WHEEL_HI_RES, 120),
                (Relative::REL_HWHEEL, 1),
                (Relative::REL_HWHEEL_HI_RES, 120),
            ],
        );

        assert_eq!(
            turn,
            Some(ScrollDelta::Lines { x: 1.0, y: -1.0 }),
            "one detent each way, rather than two"
        );
    }

    #[test]
    fn the_fine_axis_counts_in_hundred_and_twentieths_of_a_detent() {
        let fine = HighResolution {
            vertical: true,
            horizontal: false,
        };

        assert_eq!(
            turned(fine, &[(Relative::REL_WHEEL_HI_RES, 15)]),
            Some(ScrollDelta::Lines { x: 0.0, y: -0.125 }),
            "an eighth of a detent, as a free-spinning wheel reports"
        );
    }

    #[test]
    fn a_device_without_the_fine_axis_is_read_on_the_coarse_one() {
        // The fine codes are ignored where the device does not advertise them, so a driver that
        // sends one anyway cannot move a document twice.
        let coarse = HighResolution::default();

        assert_eq!(turned(coarse, &[(Relative::REL_WHEEL_HI_RES, 120)]), None);
        assert_eq!(
            turned(coarse, &[(Relative::REL_WHEEL, 3)]),
            Some(ScrollDelta::Lines { x: 0.0, y: -3.0 })
        );
    }

    #[test]
    fn which_axes_are_fine_comes_from_the_device_itself() {
        assert_eq!(
            HighResolution::of(&wheel(&[Relative::REL_X, Relative::REL_Y])),
            HighResolution::default(),
            "a mouse with no wheel at all reports neither"
        );
        assert_eq!(
            HighResolution::of(&wheel(&[Relative::REL_WHEEL, Relative::REL_WHEEL_HI_RES])),
            HighResolution {
                vertical: true,
                horizontal: false
            },
            "and one axis can be fine while the other is not"
        );
        assert_eq!(
            HighResolution::of(&wheel(&[
                Relative::REL_WHEEL_HI_RES,
                Relative::REL_HWHEEL_HI_RES
            ])),
            HighResolution {
                vertical: true,
                horizontal: true
            }
        );
    }

    #[test]
    fn a_delta_stays_in_detents_and_is_never_guessed_into_pixels() {
        // A line height invented here would be wrong for every element that has another one, and
        // nothing downstream could tell that it had been invented.
        let turn = turned(HighResolution::default(), &[(Relative::REL_WHEEL, 1)])
            .expect("a detent asked for something");

        assert!(turn.is_lines());
    }

    #[test]
    fn an_update_that_turned_nothing_asks_for_nothing() {
        // A pointer moving reports `REL_X` and `REL_Y` in a batch of its own, and a scroll of no
        // distance is an event a document would be dispatched for no reason.
        assert_eq!(
            turned(
                HighResolution::default(),
                &[(Relative::REL_X, 5), (Relative::REL_Y, 5)]
            ),
            None
        );
        assert_eq!(turned(HighResolution::default(), &[]), None);
        assert_eq!(
            turned(HighResolution::default(), &[(Relative::REL_WHEEL, 0)]),
            None,
            "and a driver that reported a turn of nothing said nothing"
        );
    }

    #[test]
    fn one_axis_reported_twice_in_an_update_accumulates() {
        assert_eq!(
            turned(
                HighResolution::default(),
                &[(Relative::REL_WHEEL, 1), (Relative::REL_WHEEL, 2)]
            ),
            Some(ScrollDelta::Lines { x: 0.0, y: -3.0 })
        );
    }
}
