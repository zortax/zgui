//! Opening real devices.
//!
//! Everything here needs a node this user may read, so every test looks for one first. What is
//! asserted is that a call *answers*, rather than what it answers: the answer is a fact about the
//! hardware, and the call working is a fact about this crate.

#![cfg(target_os = "linux")]

mod support;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::AsFd;
use zgui_evdev::code::Key;

#[test]
fn a_device_opens_and_says_what_it_is() {
    let devices = support::devices("a_device_opens_and_says_what_it_is");
    if devices.is_empty() {
        return;
    }

    for device in &devices {
        let identity = device.identity();
        println!(
            "{} {:?} bus={:#06x} vendor={:#06x} product={:#06x} version={:#06x} roles={:?}",
            device.path().display(),
            device.name(),
            identity.bus,
            identity.vendor,
            identity.product,
            identity.version,
            device.roles().iter().collect::<Vec<_>>(),
        );
        // A driver that reported no name at all would still open, and every read of it would be
        // of a device nothing can identify.
        assert!(
            !device.name().is_empty(),
            "an input device tells the kernel what it is called"
        );
        // Every input device emits `EV_SYN`, because the kernel ends every report with one. A
        // device whose type map came back empty is a map read out of the wrong request.
        assert!(
            !device.capabilities().types().is_empty(),
            "a device reports at least one event type"
        );
    }
}

#[test]
fn a_device_reports_the_codes_behind_the_types_it_has() {
    let devices = support::devices("a_device_reports_the_codes_behind_the_types_it_has");
    if devices.is_empty() {
        return;
    }

    let mut with_keys = 0;
    for device in &devices {
        let capabilities = device.capabilities();
        // The two maps are read against the type map, so a device that says it has keys and then
        // reports none of them is the case where the second request went wrong.
        if capabilities.has(zgui_evdev::EventType::EV_KEY) {
            with_keys += 1;
            assert!(
                !capabilities.keys().is_empty(),
                "{} says it has keys, so it names some",
                device.path().display()
            );
        } else {
            assert!(
                capabilities.keys().is_empty(),
                "{} was not asked for a map it has no type for",
                device.path().display()
            );
        }
        println!(
            "{}: {} types, {} keys, {} relative axes, {} absolute axes",
            device.path().display(),
            capabilities.types().len(),
            capabilities.keys().len(),
            capabilities.relative().len(),
            capabilities.absolute().len(),
        );
    }

    assert!(
        with_keys > 0,
        "a machine with a readable input device has one with keys or buttons on it"
    );
}

#[test]
fn an_absolute_axis_reports_the_range_it_moves_over() {
    let devices = support::devices("an_absolute_axis_reports_the_range_it_moves_over");
    let with_axes: Vec<_> = devices
        .iter()
        .filter(|device| !device.capabilities().absolute().is_empty())
        .collect();
    if with_axes.is_empty() {
        eprintln!(
            "an_absolute_axis_reports_the_range_it_moves_over: no readable device has an \
             absolute axis, so nothing was asserted"
        );
        return;
    }

    for device in with_axes {
        for axis in device.capabilities().absolute().iter() {
            let range = device
                .axis(axis)
                .expect("a device answers for an axis it says it has");
            println!("{} {axis:?}: {range:?}", device.path().display());
            // An `input_absinfo` read out of the wrong offsets produces a range the wrong way
            // round, and nothing else reports it.
            assert!(
                range.minimum <= range.maximum,
                "an axis moves from its minimum to its maximum"
            );
        }
    }
}

#[test]
fn a_device_says_which_keys_are_held_down() {
    let devices = support::devices("a_device_says_which_keys_are_held_down");
    let with_keys: Vec<_> = devices
        .iter()
        .filter(|device| device.capabilities().has(zgui_evdev::EventType::EV_KEY))
        .collect();
    if with_keys.is_empty() {
        eprintln!(
            "a_device_says_which_keys_are_held_down: no readable device has keys, so nothing was \
             asserted"
        );
        return;
    }

    for device in with_keys {
        let held = device
            .pressed_keys()
            .expect("a device with keys answers which are held");
        // Which keys are down is whatever the person at the machine is doing, so what is asserted
        // is that every one of them is a key the device has. A map read at the wrong length or
        // through the wrong request reports codes the device never had.
        for code in held.iter() {
            assert!(
                device.capabilities().keys().contains(code),
                "{} reports {code:?} held, and says it has no such key",
                device.path().display()
            );
        }
        println!("{}: {} keys held", device.path().display(), held.len());
    }
}

#[test]
fn a_device_timestamps_its_events_on_the_monotonic_clock() {
    let devices = support::devices("a_device_timestamps_its_events_on_the_monotonic_clock");
    if devices.is_empty() {
        return;
    }

    for device in &devices {
        // `EVIOCSCLOCKID` arrived in 2.6.36, so a kernel that refuses it is one from before 2010.
        // On anything newer this is the request working, and it is worth asserting because the
        // alternative is silent: the stream stays on the real clock and every measured interval
        // is wrong only when someone steps the clock.
        assert!(
            device.has_monotonic_timestamps(),
            "{} refused the monotonic clock, which only a kernel older than 2.6.36 does",
            device.path().display()
        );
    }
}

#[test]
fn a_device_hands_out_a_descriptor_that_can_be_polled() {
    let devices = support::devices("a_device_hands_out_a_descriptor_that_can_be_polled");
    let Some(device) = devices.first() else {
        return;
    };

    // A zero timeout is the point: a loop asks whether the device has anything to say and carries
    // on when it has not. A descriptor that could not be polled fails here instead of blocking,
    // because `poll` refuses a bad one.
    let mut watched = [PollFd::new(device, PollFlags::IN)];
    let ready = poll(&mut watched, Some(&Timespec::default())).expect("the descriptor is pollable");

    println!(
        "{}: {ready} descriptor(s) ready with nothing waited for",
        device.path().display()
    );
}

#[test]
fn reading_a_device_that_has_nothing_to_say_waits_for_nothing() {
    let mut devices =
        support::devices("reading_a_device_that_has_nothing_to_say_waits_for_nothing");
    let Some(device) = devices.first_mut() else {
        return;
    };

    // The node is opened non-blocking, so this returns rather than parking. That this test ends
    // at all is the assertion: a read that blocked would hang here until the runner gave up, and
    // no timing check would report it any sooner.
    let batches = device.read().expect("a quiet device reads as nothing");

    for batch in &batches {
        // Whatever arrived came from someone at the keyboard. A batch is still a batch.
        assert!(
            !batch.events.is_empty(),
            "an update the kernel reported has something in it"
        );
    }
}

/// Held for as long as a test holds a grab.
///
/// The runner runs these in parallel, and there is one grabbable device on an ordinary machine, so
/// two tests grabbing at once means the second answers `EBUSY`. Only one client may hold a device,
/// so the tests take turns instead of the device.
static GRAB: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Takes the turn to grab a device.
///
/// A test that panicked while holding it poisons the lock, and the turn is still free: the grab
/// went with the descriptor the panic dropped.
fn turn() -> std::sync::MutexGuard<'static, ()> {
    GRAB.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A readable device nobody is typing on, for the tests that take one away.
///
/// A grab takes the device from everything else for as long as it is held, so taking the keyboard
/// the person at the machine is using is a real cost with an easy alternative.
///
/// The `Keyboard` role is the wrong question to ask here, deliberately. It is udev's `ID_INPUT_KEY`
/// and it is meant to be broad: a remote control is a keyboard under it, and so is the mouse node
/// on the machine this was written on, which advertises `KEY_MACRO27` and its neighbours. What
/// matters is narrower — whether the device has any key from the block a person types on, which is
/// everything under `BTN_MISC`.
fn grabbable(test: &str) -> Option<zgui_evdev::Device> {
    let found = support::devices(test).into_iter().find(|device| {
        !device
            .capabilities()
            .keys()
            .iter()
            .any(|key| key.raw() < Key::BTN_MISC.raw())
    });
    if found.is_none() {
        eprintln!(
            "{test}: every readable device on this machine has keys a person types on, and \
             grabbing one takes it from the session, so nothing was asserted"
        );
    }
    found
}

#[test]
fn dropping_a_device_gives_its_grab_back() {
    // `Device::drop` exists for one case, named in its own doc: a caller that duplicated the
    // descriptor, where closing this one does not close the description the grab is held by. That
    // case is the only reason the implementation is there, so this is the test of it.
    let _turn = turn();
    let Some(mut device) = grabbable("dropping_a_device_gives_its_grab_back") else {
        return;
    };
    let path = device.path().to_owned();

    // `dup` shares the open file description rather than making a new one, so the kernel keeps it
    // alive after the `Device` closes its own descriptor — and the grab with it, unless something
    // gives it back first.
    let duplicate = device
        .as_fd()
        .try_clone_to_owned()
        .expect("the descriptor duplicates");
    device.grab().expect("nothing else holds this device");
    drop(device);

    let mut again = zgui_evdev::Device::open(&path).expect("the device opens again");
    let regrabbed = again.grab();
    // The duplicate is held until here on purpose. Dropping it earlier would close the description
    // and release the grab whatever `Device::drop` did, which is the assertion this is making.
    drop(duplicate);

    regrabbed.expect(
        "a grab left behind by drop is a device that reaches nothing, and no later run puts it back",
    );
    again.release().expect("what was taken is given back");
    println!(
        "{}: the grab survived a dup and was released",
        path.display()
    );
}

#[test]
fn a_device_can_be_grabbed_and_given_back() {
    let _turn = turn();
    let Some(mut device) = grabbable("a_device_can_be_grabbed_and_given_back") else {
        return;
    };

    assert!(!device.is_grabbed(), "a device opens ungrabbed");
    device.grab().expect("nothing else holds this device");
    assert!(device.is_grabbed());
    device.release().expect("what was taken is given back");
    assert!(!device.is_grabbed());

    // Grabbing twice in a row is what proves the release reached the kernel rather than only the
    // flag: a device still held answers the second grab with `EBUSY`.
    device.grab().expect("the release reached the kernel");
    device.release().expect("and so did the second one");
    println!("{}: grabbed and released twice", device.path().display());
}
