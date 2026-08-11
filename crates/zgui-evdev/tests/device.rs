//! Opening real devices, and building one over a descriptor somebody else opened.
//!
//! Everything here needs a node this user may read, so every test looks for one first. Which nodes
//! those are is `support`'s answer, and it is read off the machine — see that module for why the
//! search may not go through this crate.
//!
//! Most of what is asserted is that a call *answers*, rather than what it answers: the answer is a
//! fact about the hardware, and the call working is a fact about this crate. Two facts are held to
//! an exact value anyway, because the kernel publishes them a second way under `/sys/class/input`:
//! what a device calls itself and what the hardware says it is. A request skipped or issued
//! through the wrong number reads as zeros, and zeros are what the comparisons below refuse.

#![cfg(target_os = "linux")]
// `EVIOCREVOKE` is the one request this crate issues nowhere, and it is the request a session
// daemon makes on an evdev descriptor. So it is declared and issued here, beside the test that
// needs it.
#![allow(unsafe_code)]

mod support;

use std::collections::BTreeSet;
use std::ffi::{c_int, c_ulong};
use std::path::{Path, PathBuf};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use rustix::fs::{Mode, OFlags};
use rustix::ioctl::{Opcode, opcode};
use zgui_evdev::{Code, Device, EventType, Key};

/// A descriptor onto `path`, opened the way a session daemon hands one over.
///
/// **Blocking**, because neither logind nor seatd states which status flags a client is handed, so
/// what this crate does with a descriptor that has none of them is the case worth covering.
///
/// **Read-write**, because that is what logind opens with, where [`Device::open`] asks for
/// read-only. A machine whose nodes this user may read and not write says so and hands back a
/// read-only descriptor, which covers everything here except that the mode reaches nothing.
fn handed_over(test: &str, path: &Path) -> OwnedFd {
    match rustix::fs::open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()) {
        Ok(fd) => fd,
        Err(errno) => {
            eprintln!(
                "{test}: {} opens for reading and not for writing on this machine ({errno}), so \
                 the descriptor below is read-only and that a device works either way was not \
                 asserted. Add this user to the group the node belongs to to run it.",
                path.display()
            );
            rustix::fs::open(path, OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty())
                .unwrap_or_else(|errno| panic!("a node that opened once opens again: {errno}"))
        }
    }
}

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
fn a_device_answers_the_name_and_the_identity_the_kernel_publishes() {
    // The one place a value is asserted rather than printed. `/sys/class/input/eventN/device` is
    // the same `input_dev` the ioctls read, published by the kernel a second way, so this holds
    // `EVIOCGNAME` and `EVIOCGID` up against something outside this crate. A request that is
    // skipped, or issued through a number the kernel reads as another request, answers a name of
    // nothing and an identity of four zeros — and every comparison in this file that is between
    // two devices is satisfied by both of them being wrong.
    let test = "a_device_answers_the_name_and_the_identity_the_kernel_publishes";
    let devices = support::devices(test);
    if devices.is_empty() {
        return;
    }

    let mut compared = 0;
    for device in &devices {
        let Some(published) = support::published(test, device.path()) else {
            continue;
        };
        compared += 1;

        assert_eq!(
            device.name(),
            published.name,
            "{} calls itself what the kernel publishes for it",
            device.path().display()
        );
        let identity = device.identity();
        assert_eq!(
            (
                identity.bus,
                identity.vendor,
                identity.product,
                identity.version
            ),
            (
                published.bus,
                published.vendor,
                published.product,
                published.version
            ),
            "{} reports the bus, the vendor, the product and the version the kernel publishes",
            device.path().display()
        );
    }

    if compared == 0 {
        eprintln!(
            "{test}: this kernel publishes nothing under /sys/class/input, so what each device \
             says it is was read from the device alone and asserted against nothing"
        );
    }
}

#[test]
fn a_device_answers_the_capability_maps_the_kernel_publishes() {
    // The other half of the same anchor. Every map is `EVIOCGBIT` with the event type in the
    // request number and a length this crate chose, so a map read at the wrong length or through
    // the wrong type is a device that does the wrong job — and a comparison between two of this
    // crate's own devices sees none of it.
    let test = "a_device_answers_the_capability_maps_the_kernel_publishes";
    let devices = support::devices(test);
    if devices.is_empty() {
        return;
    }

    let mut compared = 0;
    for device in &devices {
        let Some(published) = support::published(test, device.path()) else {
            continue;
        };
        compared += 1;

        let capabilities = device.capabilities();
        let named = |published: &BTreeSet<u16>| published.iter().copied().collect::<Vec<_>>();
        assert_eq!(
            raw(capabilities.types().iter()),
            named(&published.types),
            "{} emits the event types the kernel publishes",
            device.path().display()
        );
        assert_eq!(
            raw(capabilities.keys().iter()),
            named(&published.keys),
            "{} has the keys the kernel publishes",
            device.path().display()
        );
        assert_eq!(
            raw(capabilities.relative().iter()),
            named(&published.relative),
            "{} has the relative axes the kernel publishes",
            device.path().display()
        );
        assert_eq!(
            raw(capabilities.absolute().iter()),
            named(&published.absolute),
            "{} has the absolute axes the kernel publishes",
            device.path().display()
        );
    }

    if compared == 0 {
        eprintln!(
            "{test}: this kernel publishes nothing under /sys/class/input, so what each device can \
             report was read from the device alone and asserted against nothing"
        );
    }
}

/// The kernel's own numbers for a map's codes, in order.
fn raw<C: Code>(codes: impl Iterator<Item = C>) -> Vec<u16> {
    codes.map(Code::raw).collect()
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
        if capabilities.has(EventType::EV_KEY) {
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
        .filter(|device| device.capabilities().has(EventType::EV_KEY))
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
fn grabbable(test: &str) -> Option<Device> {
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

    let mut again = Device::open(&path).expect("the device opens again");
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

#[test]
fn a_device_over_a_descriptor_reads_the_same_device_open_reads() {
    let test = "a_device_over_a_descriptor_reads_the_same_device_open_reads";
    let devices = support::devices(test);
    let Some(opened) = devices.first() else {
        return;
    };
    let path = opened.path().to_owned();

    let over = Device::over(handed_over(test, &path), &path)
        .expect("a device is built over an open descriptor");

    // The identity and the capabilities are what a caller classifies a device by, so a constructor
    // that read one of them through the wrong request, or skipped it, hands back a device that
    // does no job at all.
    assert_eq!(over.name(), opened.name(), "one node has one name");
    assert_eq!(over.identity(), opened.identity());
    assert_eq!(over.capabilities(), opened.capabilities());
    assert_eq!(
        over.roles().iter().collect::<Vec<_>>(),
        opened.roles().iter().collect::<Vec<_>>(),
        "so both answer the same jobs"
    );
    assert!(
        !over.capabilities().types().is_empty(),
        "and the maps were read rather than left empty"
    );
    assert_eq!(
        over.has_monotonic_timestamps(),
        opened.has_monotonic_timestamps(),
        "the clock is asked for on a descriptor handed over as well"
    );
    assert!(!over.is_grabbed(), "a device arrives ungrabbed");
    println!(
        "{}: {:?} over a descriptor this test opened",
        over.path().display(),
        over.name()
    );
}

#[test]
fn a_device_over_a_descriptor_answers_the_path_its_caller_named() {
    let test = "a_device_over_a_descriptor_answers_the_path_its_caller_named";
    let devices = support::devices(test);
    let Some(opened) = devices.first() else {
        return;
    };
    let path = opened.path().to_owned();
    // A name nothing can open, over a descriptor onto a device. The path is carried for messages,
    // and this test says so: a call that opened it would be refused here.
    let named = PathBuf::from("/dev/input/event-the-session-named");

    let over = Device::over(handed_over(test, &path), &named)
        .expect("a device is built over an open descriptor");

    assert_eq!(over.path(), named);
}

#[test]
fn a_device_over_a_blocking_descriptor_reads_rather_than_waiting() {
    let test = "a_device_over_a_blocking_descriptor_reads_rather_than_waiting";
    let devices = support::devices(test);
    let Some(opened) = devices.first() else {
        return;
    };
    let path = opened.path().to_owned();

    // The duplicate stays here, so what reached the shared open file description can be read after
    // the fact.
    let blocking = handed_over(test, &path);
    let kept = blocking
        .try_clone()
        .expect("a descriptor onto an open node duplicates");
    let mut over =
        Device::over(blocking, &path).expect("a device is built over an open descriptor");

    // The flag lives on the open file description, so the copy this test kept reports it too. A
    // session daemon's own descriptor is another name for that same
    // description, and this is what such a daemon sees.
    //
    // **This is asserted before anything reads the device, and that ordering is what keeps the
    // test bounded.** Nobody is asked to press a key, so a read of a descriptor that stayed
    // blocking waits for an event that may never come — which at run time is a frame loop stopping
    // dead with nothing printed, and here would be a test that never returns. A read made after
    // the flag is known to be on cannot wait: the kernel answers `EAGAIN` where there is nothing
    // to report.
    let flags = rustix::fs::fcntl_getfl(&kept).expect("a descriptor reports its status flags");
    assert!(
        flags.contains(OFlags::NONBLOCK),
        "the flag reached the description behind the descriptor that was handed over, and the \
         descriptor a session daemon kept is still blocking: {flags:?}"
    );

    // The observable behind that flag, read on this thread because the assertion above says it
    // answers at once.
    let batches = over.read().expect("a quiet device reads as nothing");

    // Whatever arrived came from somebody at the keyboard, so the count is not the assertion. That
    // the call came back is.
    println!(
        "{}: a blocking descriptor read {} batches and returned",
        path.display(),
        batches.len()
    );
}

/// `EVIOCREVOKE`, computed the way the kernel's header computes it.
///
/// `_IOW('E', 0x91, int)`, built out of the same `rustix` const function this crate builds its own
/// request numbers with, so no number here is transcribed.
const REVOKE: Opcode = opcode::write::<c_int>(b'E', 0x91);

/// Revokes the open file description `fd` names.
///
/// This is what logind does to an evdev descriptor it hands to a session that is waiting for its
/// terminal, and what it does to every one of them when another session takes the screen. The
/// descriptor stays open, every request on it answers `ENODEV`, and nothing puts it back.
///
/// The description this revokes is the one the test opened, so the device itself and every other
/// client of it are untouched.
///
/// The argument is the value rather than a pointer to one: `evdev_do_ioctl` refuses an
/// `EVIOCREVOKE` whose argument is non-null, so a call that pointed at a zero would be refused.
fn revoke(fd: BorrowedFd<'_>) {
    // SAFETY: `ioctl` is handed a descriptor this frame borrows for the length of the call, a
    // request number computed for that call, and the integer argument the request reads. Nothing
    // is dereferenced and nothing is written back, so the return value is the only result.
    let answer = unsafe { ioctl(fd.as_raw_fd(), c_ulong::from(REVOKE), 0) };

    assert_eq!(
        answer,
        0,
        "the kernel revokes a descriptor onto a device it still has: {}",
        std::io::Error::last_os_error()
    );
}

// The C library's own, for a request this crate issues nowhere. Declared here instead of reached
// through a crate, for the reason `support` declares what it declares: what crosses is stated once,
// beside the code that calls it.
unsafe extern "C" {
    /// `ioctl(2)`. Takes the descriptor, the request number, and the argument that request names.
    fn ioctl(fd: c_int, request: c_ulong, argument: c_int) -> c_int;
}

#[test]
fn a_revoked_descriptor_is_an_input_device_that_answers_nothing_yet() {
    // What logind hands a session that is waiting for its terminal: an evdev node with the
    // descriptor already revoked. The input driver then answers `ENODEV` for every request,
    // including the one this crate probes with — so the probe has to read it as a device this run
    // gets when a person switches to the terminal.
    let test = "a_revoked_descriptor_is_an_input_device_that_answers_nothing_yet";
    let devices = support::devices(test);
    let Some(opened) = devices.first() else {
        return;
    };
    let path = opened.path().to_owned();

    let handed = handed_over(test, &path);
    // The refusal takes the descriptor with it, so what it was left in is read through a second
    // name for the same open file description.
    let kept = handed.try_clone().expect("a descriptor duplicates");
    revoke(handed.as_fd());

    let error = Device::over(handed, &path).expect_err("a revoked descriptor answers no request");

    assert!(
        matches!(error, zgui_evdev::Error::Revoked { .. }),
        "the node is an input device this run cannot read yet: {error:?}"
    );
    assert!(
        error.to_string().contains(&path.display().to_string()),
        "the refusal names the path its caller gave: {error}"
    );
    // The same rule the other refusal keeps: identify, then change. A revoked node goes back to the
    // daemon that opened it, and the daemon's own descriptor names the same open file description.
    let flags = rustix::fs::fcntl_getfl(&kept).expect("a descriptor reports its status flags");
    assert!(
        !flags.contains(OFlags::NONBLOCK),
        "a descriptor this crate refuses goes back to its owner with the flags it arrived with, \
         and this one reads {flags:?}"
    );
    println!(
        "{}: revoked, and refused as a device that answers nothing yet",
        path.display()
    );
}

#[test]
fn a_descriptor_that_names_no_input_device_is_refused_rather_than_built_over() {
    // A session hands out graphics cards over the call it hands out input devices with, so a
    // descriptor onto something that is not a node is a mistake a caller can make. `/dev/null` is
    // the one every machine has, and it refuses an input request number exactly as a card does.
    let other = rustix::fs::open("/dev/null", OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
        .expect("/dev/null opens");
    // The refusal takes the descriptor with it, so what it was left in is read through a second
    // name for the same open file description.
    let kept = other.try_clone().expect("a descriptor duplicates");
    let named = PathBuf::from("/dev/input/event-that-is-no-device");

    let error = Device::over(other, &named)
        .expect_err("a descriptor onto something other than an input device is refused");

    assert!(
        matches!(error, zgui_evdev::Error::Unusable(_)),
        "the refusal says the descriptor cannot be used: {error:?}"
    );
    assert!(
        error.to_string().contains("event-that-is-no-device"),
        "the refusal names the path its caller gave: {error}"
    );
    // Identify, then change. The status flags belong to the open file description, so raising one
    // reaches every name for it — including the daemon's own descriptor onto a device this call is
    // about to hand straight back.
    let flags = rustix::fs::fcntl_getfl(&kept).expect("a descriptor reports its status flags");
    assert!(
        !flags.contains(OFlags::NONBLOCK),
        "a descriptor this crate refuses goes back to its owner with the flags it arrived with, \
         and this one reads {flags:?}"
    );

    // The other direction, so that the check above is a check rather than a refusal of everything.
    let test = "a_descriptor_that_names_no_input_device_is_refused_rather_than_built_over";
    let devices = support::devices(test);
    let Some(opened) = devices.first() else {
        return;
    };
    let path = opened.path().to_owned();
    Device::over(handed_over(test, &path), &path)
        .unwrap_or_else(|error| panic!("a descriptor onto {} is taken: {error}", path.display()));
}
