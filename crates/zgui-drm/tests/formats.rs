//! What a plane can scan out, and in which memory layouts.
//!
//! A plane's format list says which formats the hardware takes. `IN_FORMATS` says how their pixels
//! may be arranged, and it is the answer a caller needs before it asks a graphics API for an image
//! the display can read as it stands. This is that property against a real driver, and against the
//! plane's own format list, which the kernel builds it from.
//!
//! # What this needs to assert anything
//!
//! A device that opens, and universal planes. Reading properties takes no DRM master, so this runs
//! under a compositor and on any card the user may open. A device without universal planes hides
//! its primary and cursor planes, which is where the interesting layouts are.

mod support;

use zgui_drm::Device;
use zgui_drm::device::Interface;
use zgui_drm::format::Format;
use zgui_drm::property::ObjectKind;

/// The property a plane publishes its scanout layouts under.
///
/// Named here because the crate's own constant is private: a test reaches this crate the way any
/// other caller does.
const IN_FORMATS: &str = "IN_FORMATS";

#[test]
fn every_format_a_plane_publishes_is_a_format_it_lists() {
    let test = "every_format_a_plane_publishes_is_a_format_it_lists";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    let Some(planes) = planes(test, &device) else {
        return;
    };

    // Two kernel interfaces answer the same question, and this is where they have to agree.
    // `MODE_GETPLANE` hands back an array of fourcc codes, `IN_FORMATS` hands back a blob this
    // crate parses by hand, and a parser reading the format table from the wrong place produces
    // codes that are in neither list.
    let mut publishing = 0;
    for id in planes {
        let plane = device.plane(id).expect("a listed plane is readable");
        let Some(published) = device
            .plane_formats(id)
            .expect("a plane answers what it can scan out")
        else {
            continue;
        };
        publishing += 1;

        assert!(
            !published.formats().is_empty(),
            "plane {id} publishes {IN_FORMATS}, so it names at least one format"
        );
        // The kernel builds the blob's list out of the plane's own array, in that order, so the
        // two are the same list. Asserting containment alone would accept a permutation, and a
        // permutation is what a parser reading the format table at a shifted offset produces.
        let publishes: Vec<u32> = published.formats().iter().map(|format| format.0).collect();
        assert_eq!(
            publishes, plane.formats,
            "plane {id} publishes the list it states, in order"
        );
        println!(
            "plane {id} lists {} formats and publishes {}",
            plane.formats.len(),
            published.formats().len()
        );
    }

    if publishing == 0 {
        eprintln!(
            "{test}: no plane on {} publishes {IN_FORMATS}, so nothing was asserted\n\
             the property is optional: name a driver that has it with {}=/dev/dri/cardN",
            device.path().display(),
            support::DEVICE
        );
    }
}

#[test]
fn a_plane_that_publishes_layouts_names_one_for_a_format_it_takes() {
    let test = "a_plane_that_publishes_layouts_names_one_for_a_format_it_takes";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    let Some(planes) = planes(test, &device) else {
        return;
    };

    let mut publishing = 0;
    for id in planes {
        let Some(published) = device
            .plane_formats(id)
            .expect("a plane answers what it can scan out")
        else {
            continue;
        };
        publishing += 1;

        let mut named = 0;
        for format in published.formats() {
            let layouts = published.modifiers(*format);
            named += layouts.len();
            // `parse` answers a set, and a driver naming one pair over two overlapping windows is
            // the case that makes that worth checking against a real blob.
            for (place, layout) in layouts.iter().enumerate() {
                assert!(
                    !layouts[..place].contains(layout),
                    "plane {id} names {:#018x} once for {:#010x}, and holds {layouts:#018x?}",
                    layout.0,
                    format.0
                );
                assert!(
                    published.supports(*format, *layout),
                    "plane {id} supports every layout it names for {:#010x}",
                    format.0
                );
            }
        }
        // Per plane, so that one plane answering for the whole device cannot hide the rest. A
        // plane publishing the property names a layout: the kernel builds the blob out of the
        // modifier list a driver registered its plane with, and an empty one publishes nothing.
        assert!(
            named != 0,
            "plane {id} publishes {IN_FORMATS} over {} formats, so it names a layout for one",
            published.formats().len()
        );

        // The pair a zero-copy scanout is built from, printed for whichever format the caller of
        // this crate will ask for. A plane that takes it in no layout at all says so here.
        println!(
            "plane {id} names {named} layouts, and {:#018x?} for XRGB8888",
            published
                .modifiers(Format::XRGB8888)
                .iter()
                .map(|modifier| modifier.0)
                .collect::<Vec<_>>()
        );
    }

    if publishing == 0 {
        eprintln!(
            "{test}: no plane on {} publishes {IN_FORMATS}, so nothing was asserted\n\
             the property is optional: name a driver that has it with {}=/dev/dri/cardN",
            device.path().display(),
            support::DEVICE
        );
    }
}

#[test]
fn a_plane_that_publishes_no_layouts_is_answered_rather_than_refused() {
    let test = "a_plane_that_publishes_no_layouts_is_answered_rather_than_refused";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    let Some(planes) = planes(test, &device) else {
        return;
    };

    let mut silent = 0;
    for id in planes {
        let properties = device
            .properties(id, ObjectKind::Plane)
            .expect("a plane's properties are readable");
        let publishes = properties.names().any(|name| name == IN_FORMATS);
        // The call itself is the assertion: a plane without the property is a fact about the
        // driver, and it must reach the caller as an answer.
        let published = device
            .plane_formats(id)
            .expect("a plane answers what it can scan out");

        if publishes {
            assert!(
                published.is_some(),
                "plane {id} publishes {IN_FORMATS}, so it is read"
            );
        } else {
            silent += 1;
            assert!(
                published.is_none(),
                "plane {id} publishes no {IN_FORMATS}, so it answers nothing"
            );
        }
    }

    if silent == 0 {
        eprintln!(
            "{test}: every plane on {} publishes {IN_FORMATS}, so the answer for a plane without \
             it was not exercised here\n\
             it is exercised by the unit tests, and on a driver that omits the property: name one \
             with {}=/dev/dri/cardN",
            device.path().display(),
            support::DEVICE
        );
    }
}

/// Returns the planes of `device`, reporting on standard error where it lists none.
///
/// A device opened without universal planes lists its overlay planes and hides the primary and
/// cursor ones, and a device with no plane at all lists nothing. Neither says anything about this
/// crate, so both are reported rather than asserted.
///
/// The card is asked first whether it has an atomic interface, because a short list has a third
/// cause this crate owns. A card with the interface, over a device that hides the primary and
/// cursor planes anyway, is this crate failing to ask for the capability. Every assertion below
/// would still hold there, over the overlay planes alone.
fn planes(test: &str, device: &Device) -> Option<Vec<u32>> {
    if !support::atomic(test, device, "the planes it publishes layouts for") {
        return None;
    }

    let planes = device.planes().expect("a device enumerates its planes");
    if planes.is_empty() {
        eprintln!(
            "{test}: {} lists no plane, so nothing was asserted\n\
             a device hides its primary and cursor planes from a client without universal planes: \
             name a device that has them with {}=/dev/dri/cardN",
            device.path().display(),
            support::DEVICE
        );
        return None;
    }
    Some(planes)
}
