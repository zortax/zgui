//! The pointer against the devices and the display this machine actually has.
//!
//! Everything here needs hardware, so every test looks for it first and says on standard error
//! when it is not there. `cargo xtask ledger ignored` forbids switching a test off and states that
//! as the alternative, so a refusal is a fact about the machine printed where it happened.
//!
//! **Nothing here grabs a device.** `EVIOCGRAB` is exclusive and it holds for as long as the
//! descriptor stays open, so a test runner that took one would take the developer's mouse away
//! from the desktop they are running the tests on. What is asserted about a device is asked of its
//! capabilities, which needs the node open and nothing more.

#![cfg(target_os = "linux")]

use zgui_drm::Device;
use zgui_drm::commit;
use zgui_drm::device::Interface;
use zgui_evdev::Role;
use zgui_platform::CursorStyle;
use zgui_platform_drm::cursor::Cursor;
use zgui_platform_drm::input::{pointer, seat};
use zgui_platform_drm::output::Output;

/// Returns the devices this process may read, or nothing with the reason printed.
fn devices(test: &str) -> Option<Vec<zgui_evdev::Device>> {
    let found = match zgui_evdev::discover() {
        Ok(found) => found,
        Err(error) => {
            eprintln!("{test}: /dev/input cannot be read on this machine: {error}");
            return None;
        }
    };
    if found.opened.is_empty() {
        eprintln!(
            "{test}: no input device on this machine can be opened, so nothing was asserted; add \
             this user to the `input` group to run it"
        );
        return None;
    }
    Some(found.opened)
}

#[test]
fn the_narrow_rule_agrees_with_udev_wherever_udev_has_an_opinion() {
    // Against the devices this machine actually has. `Role::Pointer` asks for two relative axes
    // and nothing else, so it is broader in one direction — a device with axes and nothing to
    // press — and narrower in the other, because a touchscreen and a tablet report where they are
    // rather than how far they moved.
    let test = "the_narrow_rule_agrees_with_udev_wherever_udev_has_an_opinion";
    let Some(opened) = devices(test) else {
        return;
    };

    let mut pointers = 0;
    let mut both = 0;
    for device in &opened {
        let points = pointer::points_with(device.capabilities());
        let types = seat::types_on(device.capabilities());
        let udev = device.roles().contains(Role::Pointer);
        println!(
            "{}: {:?} udev-pointer={udev} pointed-with={points} typed-on={types}",
            device.path().display(),
            device.name()
        );
        if points && !udev {
            // The only devices this backend points with that udev does not are the ones that
            // report a position: a touchscreen, a tablet.
            assert!(
                pointer::absolute(device.capabilities()),
                "{} is taken as a pointer by a rule broader than udev's",
                device.path().display()
            );
        }
        pointers += usize::from(points);
        both += usize::from(points && types);
    }

    if pointers == 0 {
        eprintln!(
            "{test}: no device this process may read is one a person points with, so the rule \
             changed no answer on this machine"
        );
    }
    println!(
        "{pointers} of {} devices point, {both} of them also type",
        opened.len()
    );
}

#[test]
fn a_keyboards_own_roller_is_left_with_the_keyboard() {
    // The trap this narrowing exists for, asserted against whatever this machine has: a keyboard
    // node that advertises a relative axis. Read as a pointer's wheel, its roller scrolls a
    // document sideways whenever somebody changes the volume.
    let test = "a_keyboards_own_roller_is_left_with_the_keyboard";
    let Some(opened) = devices(test) else {
        return;
    };

    let mut found = 0;
    for device in &opened {
        let capabilities = device.capabilities();
        if !seat::types_on(capabilities) || !capabilities.has(zgui_evdev::EventType::EV_REL) {
            continue;
        }
        found += 1;
        println!(
            "{}: {:?} types and reports a relative axis; pointed-with={}",
            device.path().display(),
            device.name(),
            pointer::points_with(capabilities)
        );
        // A device that types *and* points is ordinary — a wireless receiver presents one — and it
        // is told apart by the axes rather than by having any axis at all.
        assert_eq!(
            pointer::points_with(capabilities),
            pointer::relative(capabilities) || pointer::absolute(capabilities),
            "{} is read as a pointer on the strength of `EV_REL` alone",
            device.path().display()
        );
    }

    if found == 0 {
        eprintln!(
            "{test}: no readable keyboard here reports a relative axis, so nothing was asserted; \
             the development machine's own keyboard does, through the roller above its keypad"
        );
    }
}

/// Returns a device with DRM master, or nothing with the reason printed.
///
/// Every cursor request needs it: the kernel refuses one from a process that is not the master, and
/// a compositor holding the device is the ordinary reason this cannot be had.
fn master(test: &str) -> Option<Device> {
    let device = match Device::open_first_with(Interface::Preferred) {
        Ok(device) => device,
        Err(error) => {
            eprintln!(
                "{test}: no DRM device on this machine, so nothing was asserted: {error}\n\
                 load the virtual driver with `sudo modprobe vkms` to run it"
            );
            return None;
        }
    };
    if let Err(error) = device.become_master() {
        eprintln!(
            "{test}: this process cannot become DRM master, so nothing was asserted: {error}\n\
             run it from a free virtual terminal, where no compositor holds the device"
        );
        return None;
    }
    Some(device)
}

#[test]
fn a_display_reports_which_of_the_two_cursor_paths_it_takes() {
    // The one thing about the cursor that a real device decides and no test can state: whether the
    // display engine composites the pointer or this backend draws it into the frame. Both are
    // supported and they cost different things, so which one a machine took is printed.
    let test = "a_display_reports_which_of_the_two_cursor_paths_it_takes";
    let Some(device) = master(test) else {
        return;
    };

    let outputs = Output::discover(&device).expect("the device is readable");
    let Some(output) = outputs.first() else {
        eprintln!("{test}: no display is plugged in, so nothing was asserted");
        drop(device.drop_master());
        return;
    };

    let mut taken = Vec::new();
    let mut cursor = Cursor::new(&device, output, &mut taken);
    println!(
        "{}: atomic={} crtc {} at place {}, cursor {}, the device wants {:?}",
        device.path().display(),
        device.is_atomic(),
        output.pipe.crtc,
        output.crtc_index,
        if cursor.on_a_plane() {
            "on a plane"
        } else {
            "drawn into the frame"
        },
        device.cursor_size()
    );
    assert_eq!(
        taken.is_empty(),
        !device.is_atomic() || !cursor.on_a_plane(),
        "a plane taken for one display is recorded, so no second display is given the same one"
    );

    if cursor.on_a_plane() {
        let mut commit = commit::for_device(&device);
        cursor.set_style(CursorStyle::Default);
        cursor.place(Some((100, 100)));
        cursor
            .commit(&device, &mut *commit)
            .expect("the driver takes an image on the cursor plane it offered");
        assert!(!cursor.changed(), "and what was asked for is on the screen");

        cursor.place(Some((140, 160)));
        cursor
            .commit(&device, &mut *commit)
            .expect("a move keeps the image the plane already has");

        // The corner, which is the one position this backend claims about the kernel rather than
        // about itself: a cursor is placed by its top left corner, so a pointer at (0, 0) commits
        // a negative coordinate. A refused commit takes this display off the plane for the rest of
        // the program, and every later frame carries the pointer drawn into it.
        cursor.place(Some((0, 0)));
        cursor.commit(&device, &mut *commit).expect(
            "a pointer at the corner of a display is a negative coordinate, and both \
                     interfaces take one",
        );
        assert!(cursor.on_a_plane(), "and the display kept its plane");

        cursor.set_style(CursorStyle::None);
        cursor
            .commit(&device, &mut *commit)
            .expect("and the plane can be turned off again");
        println!("an image was put on the plane, moved to its corner and taken off again");
    } else {
        eprintln!(
            "{test}: this display has no cursor plane, so no commit was exercised; every \
             para-virtualised driver is here, and so is every device on the legacy interface"
        );
    }

    cursor.release(&device);
    drop(device.drop_master());
}
