//! The vocabulary a script is written in.

/// One step of the script both windows are driven through.
///
/// Every variant is one thing a person does, and the ones that carry more than a number carry it
/// because the *interleaving* is what they exist to exercise: a resize that lands part-way through
/// a glide, an edge held down for a dozen configures, a gesture released past the end of a list
/// with the window changing while it springs back. None of those is expressible as two steps in a
/// row, because between two steps the engine is allowed to settle and settling is precisely what
/// hides them.
#[derive(Clone, Debug)]
pub(crate) enum Step {
    /// Move the pointer onto one of the probe swatches.
    Hover(usize),
    /// Press and release on one of them.
    Click(usize),
    /// One wheel notch, and every frame of the glide it starts.
    Notch(f32),
    /// One wheel notch delivered over the probe's own scroll port, which is not the root scroller.
    ///
    /// A [`Step::Notch`] lands in the middle of the window and scrolls the page, and the page is
    /// the *root* port: everything below it is below the surface as well, so the frame cuts the
    /// damage away and nothing that is out of view is visited at all. An inner port is the case
    /// that leaves — its rows are out of the port and still on the surface, so they are walked on
    /// every frame while painting nothing, and a conclusion the walk reaches about a row down there
    /// is one the row carries with it when it arrives.
    ///
    /// So this is a notch by *name* rather than by position: the port is found by its
    /// `data-testid` each time, because the page it sits on moves under it.
    Inside(f32),
    /// One character into whatever has focus.
    Type(&'static str),
    /// One backspace.
    Rub,
    /// A window this many CSS pixels wide.
    Resize(f32),
    /// A window this many CSS pixels wide and this many tall.
    ///
    /// A width-only resize keeps the viewport's height, and the height is the half of the viewport
    /// a scroll position is measured against: it decides how far the content may be scrolled, so a
    /// script that never moves it never asks what an offset does when the room under it changes.
    Sized(f32, f32),
    /// The surface moves to a new device pixel ratio without changing what it shows.
    ///
    /// The extent in CSS pixels is held fixed, so this is a window dragged between two outputs of
    /// the same logical size: everything measured in device pixels changes and nothing measured in
    /// CSS pixels does, which is the one event that invalidates every held layout result at once.
    Scale(f32),
    /// One refresh interval of nothing at all.
    Wait,
    /// A held pointer gesture that drags the page this many CSS pixels, in eight moves.
    ///
    /// A gesture is not a notch repeated: it moves the offset directly under the user's hand
    /// instead of starting the animation a detent starts, so the frames it produces are the ones a
    /// glide never runs.
    Drag(f32),
    /// A notch large enough to bring a screenful of content in from an edge.
    Fling(f32),
    /// Press the control named by this test id, which is how an overlay is opened.
    Press(&'static str),
    /// Escape, which closes whatever the topmost overlay is.
    Dismiss,
    /// A notch, and a resize delivered part-way through the glide it started.
    ///
    /// A glide writes a scroll offset on every frame while a resize rewrites the extent the offset
    /// is clamped against, so the two arriving together is the one ordering neither of them is
    /// written for. The resize lands after `after` frames of the glide, and the rest of the glide
    /// is carried at the new size.
    GlideResize {
        /// How far the notch asks the page to travel.
        lines: f32,
        /// How many frames of the glide run before the configure arrives.
        after: u32,
        /// The width the configure reports, in CSS pixels.
        width: f32,
        /// The height it reports.
        height: f32,
    },
    /// A gesture dragged past an edge and released, with a resize delivered into the spring back.
    ///
    /// An elastic displacement is not a scroll offset and is not held in the same place: it is a
    /// transient composed on top at paint time, it has a speed of its own, and it is the one
    /// quantity in the scroll state that is nonzero only while nothing else is settled. So a resize
    /// during the return asks whether the *composed* position and the clamped one are carried
    /// across an extent change together — the offset alone being right is a page drawn a spring's
    /// worth of pixels away from where it is scrolled to.
    ///
    /// Positive `pixels` push the offset down, so a positive pull past the end of the document and
    /// a negative one past the top are the two edges this reaches.
    Spring {
        /// How far the held gesture pushes, in CSS pixels, over eight moves.
        pixels: f32,
        /// How many frames of the return run before the configure arrives.
        after: u32,
        /// The width the configure reports, in CSS pixels.
        width: f32,
        /// The height it reports.
        height: f32,
    },
    /// An edge held and dragged, one configure per frame, from the current width to `to`.
    ///
    /// A person resizing a window does not deliver one configure: they deliver one per frame for as
    /// long as the button is down, and the pace gate answers only some of them. What the gate skips
    /// is what a single configure never exercises.
    EdgeDrag {
        /// The width the drag starts at, in CSS pixels.
        from: f32,
        /// The width it ends at.
        to: f32,
        /// How many configures the drag is made of.
        steps: u32,
    },
    /// Flip the colour scheme, which replaces every custom property the sheet resolves against.
    Theme,
}
