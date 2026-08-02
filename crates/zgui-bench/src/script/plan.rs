//! The sequence itself: what is done, in what order, and why that order.
//!
//! Written out rather than generated. A generated sequence is one nobody can read off the page, and
//! the whole value of this one is that a person can point at the step that broke and say what it
//! was doing — a resize between two notches, a rub while a glide is still carrying, a keystroke
//! into a field the last resize moved.
//!
//! # One sitting, in eight parts
//!
//! The order between the parts is load-bearing: each leaves the engine in a state the next asks its
//! question from, and every part after the first asks its question from a *scrolled* document. That
//! is the whole difference between a sequence that catches a page going blank under a reader and
//! one that reports nothing at four document sizes while it happens.
//!
//! The last part is the exception and says so: it walks the page back to the top first, because the
//! port it scrolls is a box on the page rather than the page itself, and it has to be on the screen
//! to be scrolled at all.
//!
//! Each part is a function named for what it asks rather than for what it presses, and every one of
//! them states the ordering that makes it worth running.

use crate::gallery::WIDTH;
use crate::script::step::Step;

/// The script, which every window in a comparison is driven through event for event.
pub(crate) fn script() -> Vec<Step> {
    let mut steps = Vec::new();
    near_the_top(&mut steps);
    down_the_page(&mut steps);
    room_beneath_the_reader(&mut steps);
    springs(&mut steps);
    across_outputs(&mut steps);
    both_edges(&mut steps);
    past_the_end(&mut steps);
    inside_a_port(&mut steps);
    steps
}

/// Local changes, typing and width-only resizes, all near the top of the document.
fn near_the_top(steps: &mut Vec<Step>) {
    steps.extend([
        Step::Wait,
        Step::Hover(0),
        Step::Click(1),
        Step::Notch(3.0),
        Step::Hover(2),
        Step::Resize(1560.0),
        Step::Notch(3.0),
        Step::Click(3),
        Step::Wait,
        Step::Notch(-3.0),
        Step::Resize(1600.0),
        Step::Hover(1),
        Step::Notch(-3.0),
    ]);
    for letter in ["a", "b", "c", "d"] {
        steps.push(Step::Type(letter));
    }
    steps.extend([
        Step::Notch(3.0),
        Step::Type("e"),
        Step::Resize(1520.0),
        Step::Rub,
        Step::Rub,
        Step::Resize(1600.0),
        Step::Notch(-3.0),
        Step::Hover(3),
        Step::Wait,
    ]);
}

/// Down into the document, and staying there.
///
/// Everything above happens near the top. The savings under test are records that travel with a box
/// and the question is what they do to a box that has been somewhere else, so the rest of the script
/// asks it from a scroll position rather than from the origin: overlays over scrolled content, a
/// theme flip with the page held down the document, and gestures in both directions.
fn down_the_page(steps: &mut Vec<Step>) {
    steps.extend([
        Step::Fling(24.0),
        Step::Drag(-260.0),
        Step::Drag(180.0),
        Step::Fling(-8.0),
        Step::Press("sheet-trigger"),
        Step::Dismiss,
        Step::Fling(16.0),
        Step::Press("popover-trigger"),
        Step::Dismiss,
        Step::Resize(1480.0),
        Step::Theme,
        Step::Notch(-4.0),
        Step::Theme,
        Step::Resize(1600.0),
        Step::Fling(-40.0),
        Step::Wait,
    ]);
}

/// The room beneath the reader, moved while the reader is in it.
///
/// Everything above holds the window's *height* at what it opened with, and the height is the half
/// of the viewport a scroll position is measured against: it is what decides how far the content may
/// be scrolled and therefore what an offset is clamped to. A script that only ever makes the window
/// wider and narrower never asks what the reader's position does when the room beneath them changes,
/// and never lets a resize and a scroll reach one frame together.
fn room_beneath_the_reader(steps: &mut Vec<Step>) {
    steps.extend([
        Step::Fling(30.0),
        Step::Sized(1600.0, 620.0),
        Step::Notch(-3.0),
        Step::Sized(1600.0, 1000.0),
        Step::Notch(3.0),
        Step::GlideResize {
            lines: -20.0,
            after: 4,
            width: 1360.0,
            height: 760.0,
        },
        Step::EdgeDrag {
            from: 1360.0,
            to: 1600.0,
            steps: 12,
        },
        Step::Fling(40.0),
        Step::Sized(1600.0, 400.0),
        Step::Wait,
        Step::Sized(1600.0, 1000.0),
    ]);
}

/// A window changed while an edge is springing back, at both ends of the document.
///
/// The elastic displacement is the one part of a scroll position that exists only while nothing is
/// settled, and it is composed on top of the offset rather than folded into it. So the two have to
/// survive an extent change *together*: an offset re-clamped without its spring, or a spring carried
/// across without its offset, draws the page a spring's worth of pixels away from where it says it
/// is scrolled to — and nothing that reads the offset can see that it happened.
///
/// Both edges, because they are not symmetrical. The top is a displacement against a limit of zero
/// that no resize can move; the end is a displacement against a limit the resize moves under it, so
/// a window made shorter while the end is stretched asks for the offset to be clamped and the
/// stretch to be relaxed in the same frame.
fn springs(steps: &mut Vec<Step>) {
    steps.extend([
        // Held past the end, released, and the window made shorter into the return: the limit moves
        // the wrong way under a container that is already past it.
        Step::Fling(60.0),
        Step::Spring {
            pixels: 320.0,
            after: 3,
            width: 1600.0,
            height: 700.0,
        },
        Step::Wait,
        // And past the top, where the limit cannot move, with the window made narrower into the
        // return so the clamp has nothing to do and the spring still has to arrive.
        Step::Fling(-80.0),
        Step::Spring {
            pixels: -320.0,
            after: 5,
            width: 1440.0,
            height: 1000.0,
        },
        Step::Resize(1600.0),
        Step::Wait,
    ]);
}

/// A window dragged between outputs, with the document scrolled and an overlay over it.
///
/// A change of ratio changes every device-pixel length in the document at once and leaves every CSS
/// one alone, which invalidates every held layout result and re-measures the scrollport in a unit
/// that has just changed size. A scroll offset is a number of *device* pixels, so the same number
/// means a different place in the document on either side of the change: a script that only ever
/// changes the ratio at the top of the page is one where every wrong answer is zero.
///
/// The overlay is opened over the scrolled document at the new ratio, because an overlay is
/// positioned against the scrollport rather than against the page — and it is left up across a
/// resize, which asks whether a surface anchored to a viewport follows a viewport that moves while
/// it is on screen.
fn across_outputs(steps: &mut Vec<Step>) {
    steps.extend([
        Step::Fling(36.0),
        Step::Scale(1.5),
        Step::Notch(-4.0),
        Step::Press("sheet-trigger"),
        Step::Sized(1520.0, 880.0),
        Step::Dismiss,
        Step::Notch(6.0),
        Step::Scale(2.0),
        Step::Notch(-3.0),
        Step::Theme,
        Step::Scale(1.0),
        Step::Sized(1600.0, 1000.0),
        Step::Theme,
        Step::Fling(-40.0),
        Step::Wait,
    ]);
}

/// A whole screenful arriving from each edge in turn, either side of a resize.
///
/// Scrolling far enough that nothing on the screen was on it a moment ago is what makes the fragment
/// pass build rather than translate, and it is the only way content that has never been composed at
/// the current size reaches the screen at all. From the bottom edge and then from the top edge
/// covers both, and doing each of them immediately after a resize covers the one whose records the
/// size change has just thrown away.
fn both_edges(steps: &mut Vec<Step>) {
    steps.extend([
        Step::Fling(80.0),
        Step::Sized(1360.0, 640.0),
        Step::Fling(80.0),
        Step::Wait,
        Step::Fling(-160.0),
        Step::Sized(1600.0, 1000.0),
        Step::Fling(-80.0),
        Step::Wait,
    ]);
}

/// A window that grows taller under a reader who is already at the end of the document.
///
/// The content did not move and there is now more room than it fills, so the position that *was* the
/// end is past it. An offset that is not re-clamped after the extent it is clamped against changes
/// composes the whole document off the top of the window and the page goes blank.
///
/// Then the same growth in the case where the clamp must *not* fire: a reader still inside the
/// document is left exactly where they are, and a notch afterwards moves from there rather than from
/// wherever a re-derived fraction would have put them.
fn past_the_end(steps: &mut Vec<Step>) {
    steps.push(Step::Sized(WIDTH, 800.0));
    for _ in 0..4 {
        steps.push(Step::Fling(400.0));
    }
    steps.push(Step::Sized(WIDTH, 1800.0));
    steps.push(Step::Wait);
    steps.push(Step::Fling(-30.0));
    steps.push(Step::Sized(WIDTH, 1000.0));
    steps.push(Step::Notch(3.0));
    steps.push(Step::Wait);
}

/// A port smaller than the window, scrolled a band at a time, with nothing moving anywhere else.
///
/// Everything above this scrolls the **page**, and the page is the root port. A root port's
/// content is out of the window as well as out of the port, so the frame's damage is cut to the
/// surface before anything reads it and the emit walk never visits what is out of view at all: a
/// row arriving from below the fold arrives with nothing said about it. That is the easy half, and
/// it is the only half a script that wheels over the middle of the window can reach.
///
/// An inner port is where the two differ. Its rows are out of the port and squarely on the surface,
/// so a scroll damages them, the walk enters them on every frame of the glide, and each of those
/// visits is an opportunity to conclude something about a row that is painting nothing — a
/// conclusion the row then carries into view with it. Content that reaches the port and is not
/// drawn is exactly what that looks like, and no step before this one can produce it.
///
/// **Nothing else in the document may be moving while it happens.** Anything that repaints every
/// frame — a spinner, a shimmer, a glide somewhere else — damages the band the arriving row lands
/// in and draws it correctly from a fresh encoding, which makes the fault disappear from the
/// picture while leaving the cause exactly where it was. So the page is brought back to the top
/// first, settled, and then only the port moves.
///
/// A band at a time rather than a fling, because the fault is at the moment of arrival: a fling
/// past a whole screenful lands on content that has to be encoded anyway, and steps over the case
/// where a row that was hidden a frame ago is the row that is now inside the port.
fn inside_a_port(steps: &mut Vec<Step>) {
    // Back to the top, so the probe row is on the screen and the port can be reached at all.
    for _ in 0..3 {
        steps.push(Step::Fling(-400.0));
    }
    steps.push(Step::Wait);
    // Down through the bands, then back up over the ones just left behind: arriving from below and
    // arriving from above are two different records, and only one of them is what a reader
    // scrolling forward sees.
    for _ in 0..4 {
        steps.push(Step::Inside(1.0));
    }
    steps.push(Step::Wait);
    for _ in 0..3 {
        steps.push(Step::Inside(-1.0));
    }
    steps.push(Step::Wait);
}
