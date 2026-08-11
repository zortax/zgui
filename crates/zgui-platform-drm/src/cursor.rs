//! The pointer's own picture: a shape, and where it goes.
//!
//! A console draws nothing a person did not ask for, so the cursor is this backend's to make. There
//! is no cursor theme to read — a theme is an X11 or a Wayland convention with a library and a
//! search path behind it, and neither exists here — so the shapes are drawn in code.
//!
//! # The shapes
//!
//! Six, and the seventeen [`CursorStyle`] variants that ask for a cursor share them. Each is
//! written as a silhouette, one character to a pixel, and the black outline around it is computed
//! rather than drawn: an outline stated by hand is one that has to be kept right by hand every time
//! a shape changes, and a cursor with a gap in its outline is invisible over anything the same
//! colour as its fill.
//!
//! A style with no shape of its own falls back to the arrow. [`CursorStyle`] states that rule —
//! a missing cursor is a cosmetic problem, and an error return here would be checked by nobody.
//!
//! # The bytes
//!
//! `ARGB8888`, which reaches memory as blue, green, red, alpha. The legacy cursor request carries
//! no format and the kernel reads that one whatever it is told, so an image in any other is
//! *reinterpreted* rather than refused — `XRGB8888`, the format everything else in this tree scans
//! out, has its unused byte read as alpha and is completely transparent. `zgui-drm` refuses an
//! image it would misread, and this is where the right one is made.
//!
//! That byte order also makes the fallback cheap. `XRGB8888` puts blue, green and red in the same
//! three places, so compositing a cursor into a frame reads the alpha and copies the other three
//! bytes rather than converting anything.
//!
//! **Nothing in this crate can check that order.** Every pixel drawn here is white, black or
//! clear, so exchanging the blue and the red byte changes no test and no picture. What the order
//! is for is the day a shape has a colour in it, and until then it is stated rather than checked.
//! The format itself is checked: `zgui-drm` refuses an image whose format is not the one the
//! legacy request reads, which is the substitution that turns a cursor invisible.
//!
//! # The two ways a cursor reaches a screen
//!
//! **A plane.** The display engine composites the image, and moving it is two numbers, so a
//! pointer costs one `DRM_IOCTL_MODE_CURSOR2` per motion and no frame at all. That request is the
//! cheaper one on both interfaces: the kernel gives it a shortcut an atomic property commit has no
//! way to ask for, and [`Commit::move_cursor`] sets that out. A real device on the atomic interface
//! has such a plane, and so does every device on the legacy interface — the legacy request names
//! the CRTC and reads no plane, so [`Device::cursor_plane`] answering `None` there says nothing
//! about the hardware.
//!
//! **The frame.** A device that offers neither has the image drawn into the picture as it is
//! copied for scanout, which costs a whole frame per motion. So the two paths differ in what the
//! loop does with a pointer that moved: a display on a plane commits, and a display on the
//! fallback asks its surface to be drawn again.
//!
//! On the atomic interface the para-virtualised drivers are on the fallback. vmwgfx, qxl, virtio
//! and virtualbox hide their cursor plane from a client that has not set
//! `DRM_CLIENT_CAP_CURSOR_PLANE_HOTSPOT`, which `zgui-drm` does not ask for, so **a virtual machine
//! on one of those drivers has no hardware cursor here**.

use tracing::warn;
use zgui_drm::buffer::DumbBuffer;
use zgui_drm::commit::Commit;
use zgui_drm::cursor::{CursorImage, CursorPlane, CursorSize};
use zgui_drm::framebuffer::Framebuffer;
use zgui_drm::{Device, Error};
use zgui_platform::CursorStyle;

use crate::output::Output;

/// How many bytes one pixel takes, in a cursor image and in a scanout buffer alike.
const BYTES_PER_PIXEL: usize = 4;

/// What a silhouette's filled pixels are written as.
const FILLED: u8 = b'#';

/// The ordinary arrow.
///
/// The hotspot is its tip. The left edge is vertical and the right edge falls away at forty-five
/// degrees, which is the shape every desktop draws and the one a person reads as "here".
const ARROW: &[&str] = &[
    "#",
    "##",
    "###",
    "####",
    "#####",
    "######",
    "#######",
    "########",
    "#########",
    "##########",
    "###########",
    "############",
    "########",
    "###  ###",
    "##    ###",
    "#      ###",
    "        ###",
    "         ##",
];

/// The pointing hand, over something that activates.
///
/// The hotspot is the tip of the finger rather than the middle of the shape, because that is what
/// a person aims with.
const HAND: &[&str] = &[
    "    ##",
    "    ##",
    "    ##",
    "    ##",
    "    ##",
    "    #####",
    "    ########",
    "    ###########",
    "  #############",
    " ##############",
    " ##############",
    " ##############",
    " #############",
    "  ###########",
    "   #########",
    "    ########",
];

/// The text bar, over something that can be selected or typed into.
///
/// Transposed for vertical text, which is the same bar lying down.
const BEAM: &[&str] = &[
    "#####", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ",
    "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "#####",
];

/// The crosshair, over something being aimed at.
const CROSS: &[&str] = &[
    "       #       ",
    "       #       ",
    "       #       ",
    "       #       ",
    "       #       ",
    "       #       ",
    "       #       ",
    "###############",
    "       #       ",
    "       #       ",
    "       #       ",
    "       #       ",
    "       #       ",
    "       #       ",
    "       #       ",
];

/// The resize arrow that points along one axis, drawn lying down.
///
/// Transposed for the other axis, which is the same arrow standing up.
const RESIZE: &[&str] = &[
    "    #       #    ",
    "   ##       ##   ",
    "  ###       ###  ",
    " ############### ",
    "#################",
    " ############### ",
    "  ###       ###  ",
    "   ##       ##   ",
    "    #       #    ",
];

/// The resize arrow that points from the top left corner to the bottom right one.
///
/// Mirrored for the other diagonal. The heads are square brackets rather than triangles: a
/// triangle drawn across a diagonal band at this size is a shape whose rows come apart, and an
/// icon set uses a bracket for the same reason.
const DIAGONAL: &[&str] = &[
    "####",
    "####",
    "####",
    "#####",
    "   ###",
    "    ###",
    "     ###",
    "      ###",
    "       ###",
    "        ###",
    "         ###",
    "          #####",
    "           ####",
    "           ####",
    "           ####",
];

/// How a shape is turned before it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Turn {
    /// Drawn as it is written.
    Keep,
    /// Rows for columns, which stands a shape drawn lying down on its end.
    Transposed,
    /// Left for right, which turns one diagonal into the other.
    Mirrored,
}

/// One shape, turned, with the pixel a person aims with.
#[derive(Debug, Clone, Copy)]
struct Shape {
    /// The silhouette, one character a pixel, `#` where it is filled.
    rows: &'static [&'static str],
    /// Where the pointer points, in the silhouette's own pixels before it is turned.
    hotspot: (i32, i32),
    /// How the silhouette is turned before it is drawn.
    turn: Turn,
}

/// Returns the shape a style is drawn with.
///
/// Nothing for [`CursorStyle::None`], which is a style that asks for no cursor at all rather than
/// for a shape this backend does not have.
///
/// Everything else that has no shape here falls back to the arrow, which is the rule
/// [`CursorStyle`] states for every platform: a busy indicator, a no-entry sign, an open hand and a
/// four-way arrow are drawings rather than translations, and a milestone spent on them is a
/// milestone not spent on the pointer underneath.
fn shape(style: CursorStyle) -> Option<Shape> {
    let (rows, hotspot, turn) = match style {
        CursorStyle::None => return None,
        CursorStyle::Pointer => (HAND, (5, 0), Turn::Keep),
        CursorStyle::Text => (BEAM, (2, 8), Turn::Keep),
        CursorStyle::VerticalText => (BEAM, (2, 8), Turn::Transposed),
        CursorStyle::Crosshair => (CROSS, (7, 7), Turn::Keep),
        CursorStyle::ResizeColumn | CursorStyle::ResizeEastWest => (RESIZE, (8, 4), Turn::Keep),
        CursorStyle::ResizeRow | CursorStyle::ResizeNorthSouth => {
            (RESIZE, (8, 4), Turn::Transposed)
        }
        CursorStyle::ResizeNorthWestSouthEast => (DIAGONAL, (7, 7), Turn::Keep),
        CursorStyle::ResizeNorthEastSouthWest => (DIAGONAL, (7, 7), Turn::Mirrored),
        // The arrow, for `Default` and for every style this backend has no shape of its own for.
        // The wildcard is what `#[non_exhaustive]` costs, and it is the right arm here: a variant
        // added to the contract tomorrow is one this backend does not have either.
        _ => (ARROW, (0, 0), Turn::Keep),
    };
    Some(Shape {
        rows,
        hotspot,
        turn,
    })
}

/// A cursor image: the pixels, and where in them the pointer points.
///
/// Tight rather than the size of a cursor plane. The plane's buffer is whatever extent the device
/// asked for and the shape sits in a corner of it, so an image kept at the plane's extent would
/// make the fallback composite sixty-five thousand pixels a frame to draw twenty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The pixels, four bytes each, blue first, tightly packed.
    pixels: Vec<u8>,
    /// How wide, in pixels.
    width: u32,
    /// How tall, in pixels.
    height: u32,
    /// Where the pointer points, in pixels right of the left edge.
    hotspot_x: i32,
    /// Where the pointer points, in pixels below the top edge.
    hotspot_y: i32,
}

impl Image {
    /// Returns the image a style is drawn as, or nothing where the style asks for no cursor.
    ///
    /// ```
    /// use zgui_platform::CursorStyle;
    /// use zgui_platform_drm::cursor::Image;
    ///
    /// let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");
    ///
    /// // Four bytes a pixel, tightly packed, and a one-pixel border for the outline.
    /// assert_eq!(arrow.bytes().len() as u32, arrow.width() * arrow.height() * 4);
    ///
    /// // A style with no shape of its own is the arrow.
    /// assert_eq!(Image::of(CursorStyle::Wait).as_ref(), Some(&arrow));
    /// // And the one style that asks for no cursor is drawn as nothing.
    /// assert!(Image::of(CursorStyle::None).is_none());
    /// ```
    pub fn of(style: CursorStyle) -> Option<Self> {
        let shape = shape(style)?;
        let silhouette = turned(shape);
        Some(drawn(&silhouette, shape.hotspot, shape.turn))
    }

    /// Returns how wide the image is, in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns how tall the image is, in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns where the pointer points, in pixels right of the left edge.
    pub const fn hotspot_x(&self) -> i32 {
        self.hotspot_x
    }

    /// Returns where the pointer points, in pixels below the top edge.
    pub const fn hotspot_y(&self) -> i32 {
        self.hotspot_y
    }

    /// Returns the bytes, four to a pixel, blue first.
    pub fn bytes(&self) -> &[u8] {
        &self.pixels
    }

    /// Writes this image into the top left corner of a cursor plane's buffer, clearing the rest.
    ///
    /// The whole buffer is written because whatever it held is the last shape, and a plane scans
    /// out every pixel of it. `stride` is the buffer's own, which a driver may have rounded up past
    /// four bytes a pixel.
    ///
    /// A buffer too small for the image takes the part that fits. The extent came from the device
    /// and the shapes here are twenty pixels across, so that is a device asking for a cursor
    /// smaller than any shape rather than a mistake above.
    pub fn fill(&self, into: &mut [u8], stride: usize, height: u32) {
        for (row, bytes) in into.chunks_mut(stride).take(height as usize).enumerate() {
            bytes.fill(0);
            let Some(source) = self.row(row) else {
                continue;
            };
            let width = source.len().min(bytes.len());
            bytes[..width].copy_from_slice(&source[..width]);
        }
    }

    /// Draws this image into a frame, with its top left corner at `x`, `y`.
    ///
    /// This is the fallback for a display with no cursor plane. The frame is `XRGB8888`, which
    /// stores blue, green and red where this image stores them, so a pixel is blended rather than
    /// converted — and the scanout ignores the fourth byte, so it is left alone.
    ///
    /// Every part of the image outside the frame is left out, so a pointer near an edge is drawn
    /// half on the screen rather than wrapped around to the other side.
    pub fn draw(&self, into: &mut [u8], stride: usize, width: u32, height: u32, x: i32, y: i32) {
        for row in 0..self.height {
            let Ok(top) = usize::try_from(y + row as i32) else {
                continue;
            };
            if top >= height as usize {
                continue;
            }
            let Some(source) = self.row(row as usize) else {
                continue;
            };
            for column in 0..self.width {
                let Ok(left) = usize::try_from(x + column as i32) else {
                    continue;
                };
                if left >= width as usize {
                    continue;
                }
                let from = column as usize * BYTES_PER_PIXEL;
                let at = top * stride + left * BYTES_PER_PIXEL;
                let (Some(pixel), Some(target)) = (
                    source.get(from..from + BYTES_PER_PIXEL),
                    into.get_mut(at..at + BYTES_PER_PIXEL),
                ) else {
                    continue;
                };
                blend(pixel, target);
            }
        }
    }

    /// Returns one row of the image, or nothing past its last.
    fn row(&self, row: usize) -> Option<&[u8]> {
        let stride = self.width as usize * BYTES_PER_PIXEL;
        self.pixels.get(row * stride..(row + 1) * stride)
    }
}

/// Draws `pixel` over `target`, in the order both store their channels.
///
/// Source-over, with the source's alpha. The shapes here are opaque or clear and nothing between,
/// so what this really does today is copy or skip — it is written as a blend anyway, because a
/// shape with a soft edge would otherwise leave a black square around every cursor and nothing
/// would report it.
fn blend(pixel: &[u8], target: &mut [u8]) {
    let alpha = u32::from(pixel[3]);
    if alpha == 0 {
        return;
    }
    for channel in 0..3 {
        // The image is opaque where it is drawn at all, so the source is already multiplied by its
        // own alpha and the target keeps what is left over.
        let over = u32::from(pixel[channel]) + u32::from(target[channel]) * (255 - alpha) / 255;
        target[channel] = u8::try_from(over.min(255)).unwrap_or(255);
    }
}

/// Returns the silhouette, turned the way the shape asks for.
fn turned(shape: Shape) -> Vec<Vec<bool>> {
    let height = shape.rows.len();
    let width = shape.rows.iter().map(|row| row.len()).max().unwrap_or(0);
    let filled = |x: usize, y: usize| {
        shape
            .rows
            .get(y)
            .and_then(|row| row.as_bytes().get(x))
            .is_some_and(|byte| *byte == FILLED)
    };
    match shape.turn {
        Turn::Keep => (0..height)
            .map(|y| (0..width).map(|x| filled(x, y)).collect())
            .collect(),
        Turn::Transposed => (0..width)
            .map(|y| (0..height).map(|x| filled(y, x)).collect())
            .collect(),
        Turn::Mirrored => (0..height)
            .map(|y| (0..width).map(|x| filled(width - 1 - x, y)).collect())
            .collect(),
    }
}

/// Returns the image a turned silhouette draws as, with its outline computed and a one-pixel
/// border round it.
///
/// The border is where the outline goes. A silhouette that touches its own left edge — the arrow
/// does, along its whole vertical side — has no room for one otherwise, and an outline with a side
/// missing is a cursor that disappears against anything its fill colour.
fn drawn(silhouette: &[Vec<bool>], hotspot: (i32, i32), turn: Turn) -> Image {
    let height = silhouette.len();
    let width = silhouette.iter().map(Vec::len).max().unwrap_or(0);
    let filled = |x: i32, y: i32| {
        usize::try_from(y)
            .ok()
            .and_then(|y| silhouette.get(y))
            .and_then(|row| usize::try_from(x).ok().and_then(|x| row.get(x)))
            .copied()
            .unwrap_or(false)
    };
    let outlines = |x: i32, y: i32| {
        (-1..=1).any(|dy| (-1..=1).any(|dx| (dx, dy) != (0, 0) && filled(x + dx, y + dy)))
    };

    let mut pixels = Vec::with_capacity((width + 2) * (height + 2) * BYTES_PER_PIXEL);
    for y in 0..height as i32 + 2 {
        for x in 0..width as i32 + 2 {
            // The silhouette sits one pixel in, so that its outline has somewhere to be.
            let (inside_x, inside_y) = (x - 1, y - 1);
            let pixel = if filled(inside_x, inside_y) {
                // Blue, green, red, alpha. White, so that a cursor is read against a dark
                // background and its outline against a light one.
                [0xFF, 0xFF, 0xFF, 0xFF]
            } else if outlines(inside_x, inside_y) {
                [0x00, 0x00, 0x00, 0xFF]
            } else {
                [0x00, 0x00, 0x00, 0x00]
            };
            pixels.extend_from_slice(&pixel);
        }
    }

    let (hotspot_x, hotspot_y) = match turn {
        Turn::Keep => hotspot,
        // The shape's rows became its columns, so the two halves of the hotspot swap with them.
        Turn::Transposed => (hotspot.1, hotspot.0),
        Turn::Mirrored => (width as i32 - 1 - hotspot.0, hotspot.1),
    };
    Image {
        pixels,
        width: width as u32 + 2,
        height: height as u32 + 2,
        // Moved with the silhouette, which sits one pixel in.
        hotspot_x: hotspot_x + 1,
        hotspot_y: hotspot_y + 1,
    }
}

/// The buffer a cursor plane scans an image out of.
#[derive(Debug)]
struct Held {
    /// The buffer itself, mapped when an image is written into it.
    buffer: DumbBuffer,
    /// The framebuffer the atomic interface names it by, where one was registered.
    ///
    /// `None` on the legacy interface, which names the GEM handle and reads no framebuffer. One
    /// registered there would cost a kernel object for nothing and an `ADDFB2` that fails wherever
    /// no plane advertises the format.
    framebuffer: Option<Framebuffer>,
}

/// What putting a cursor on its plane would take.
///
/// Three, because the two interfaces both charge differently for them: an image is a whole buffer
/// and a position, a move is two numbers, and taking the cursor off is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    /// Take the cursor off the plane, because there is nothing to show.
    Hide,
    /// Move the image the plane already holds to this corner.
    Move(i32, i32),
    /// Put this style's image on the plane, at this corner.
    Set(CursorStyle, i32, i32),
}

/// What this cursor last asked the screen for.
///
/// Three states rather than two, because "nothing is up there" and "what is up there is another
/// program's" ask for different things. A plane holding nothing and wanting nothing is finished; a
/// plane another session has had and wants nothing has to be cleared, or that session's pointer
/// stays on this display for as long as the program runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    /// Nothing has been asked for, so no image of this program's is up.
    ///
    /// Which is where a cursor starts, and where a hide leaves it.
    Nothing,
    /// This style, at this corner, was asked for.
    For(CursorStyle, (i32, i32)),
    /// Another session has had the screen, so what is up is unknown.
    ///
    /// [`Cursor::forget_the_plane`] puts a cursor here, and everything is asked for again from it:
    /// an image where one is wanted, and a hide where none is.
    Unknown,
}

/// The cursor on one display.
///
/// Both halves of this backend read it. The frame loop decides what it looks like and where it is;
/// a display on the fallback draws it into the frame as one is presented. Both run on the loop's
/// thread, one at a time.
#[derive(Debug)]
pub struct Cursor {
    /// The CRTC that shows it, and the plane the atomic interface puts it on.
    plane: CursorPlane,
    /// Whether this display has a cursor plane at all.
    ///
    /// Not the same question as [`CursorPlane::id`] being `None`. The legacy interface names the
    /// CRTC and has a hardware cursor with no plane object anywhere.
    hardware: bool,
    /// The buffer the plane scans out of, where there is one.
    held: Option<Held>,
    /// The picture, or nothing where the style asks for no cursor.
    image: Option<Image>,
    /// The style the picture was drawn from.
    style: CursorStyle,
    /// Where the pointer is on this display, in device pixels, while it is on this one.
    at: Option<(i32, i32)>,
    /// What this cursor was last *asked* to take, so that nothing is asked twice.
    ///
    /// Asked rather than shown, and the two differ on one of the two paths. A commit blocks until
    /// the kernel has applied it, so on a plane this is what is on the screen. On the fallback it
    /// is a frame that was requested and may not arrive: [`Cursor::asked_for`] says which of the
    /// two ways that happens repairs itself.
    asked: Asked,
}

impl Cursor {
    /// Creates the cursor for `output`, taking a plane for it where the device has one to give.
    ///
    /// `taken` is the cursor planes already given to other displays, and this adds the one it
    /// takes. A plane drives one CRTC at a time, so a second display given the first one's plane
    /// takes the first one's cursor away with nothing reported.
    ///
    /// Nothing here fails. A device that offers no plane, a driver that refuses the buffer and a
    /// buffer the legacy interface would misread all leave a cursor drawn into the frame instead,
    /// which is slower and correct. The refusal is written to the log where it happened.
    pub fn new(device: &Device, output: &Output, taken: &mut Vec<u32>) -> Self {
        let size = device.cursor_size();
        // Asked only on the atomic interface. The legacy one hides every plane from a client that
        // did not ask for universal planes, so a `None` there says nothing about the hardware.
        let id = if device.is_atomic() {
            match device.cursor_plane(output.crtc_index, taken) {
                Ok(id) => id,
                Err(error) => {
                    warn!(
                        "the cursor plane for CRTC {} could not be read, so the pointer is drawn \
                         into the frame instead: {error}",
                        output.pipe.crtc
                    );
                    None
                }
            }
        } else {
            None
        };
        let plane = CursorPlane {
            crtc: output.pipe.crtc,
            id,
        };
        // The legacy interface has a cursor and no plane object, so its request is tried and a
        // refusal says the CRTC has none. `Cursor::commit` is where that answer arrives.
        let offered = !device.is_atomic() || id.is_some();
        let held = offered.then(|| allocate(device, size)).flatten();
        if offered && held.is_none() {
            warn!(
                "CRTC {} has a cursor plane and no buffer to put on it, so the pointer is drawn \
                 into the frame instead",
                output.pipe.crtc
            );
        }
        taken.extend(reserved(id, held.is_some()));
        Self {
            plane,
            hardware: held.is_some(),
            held,
            image: Image::of(CursorStyle::default()),
            style: CursorStyle::default(),
            at: None,
            asked: Asked::Nothing,
        }
    }

    /// Returns `true` if the display engine composites this cursor.
    ///
    /// The loop reads it to decide what a pointer that moved costs: a commit where this is true,
    /// and a whole frame where it is false.
    pub const fn on_a_plane(&self) -> bool {
        self.hardware
    }

    /// Gives it a new shape.
    pub fn set_style(&mut self, style: CursorStyle) {
        if style == self.style {
            return;
        }
        self.style = style;
        self.image = Image::of(style);
    }

    /// Puts it where the pointer is on this display, or nowhere while the pointer is on another.
    pub fn place(&mut self, at: Option<(i32, i32)>) {
        self.at = at;
    }

    /// Returns `true` if what was last asked for is no longer what this cursor is.
    ///
    /// A cursor that has forgotten the plane has always changed, whatever it wants. What is up
    /// there belongs to another program, so even a display the pointer is on no part of has
    /// something to do about it.
    pub fn changed(&self) -> bool {
        match self.asked {
            Asked::Unknown => true,
            Asked::Nothing => self.wanted().is_some(),
            Asked::For(style, at) => self.wanted() != Some((style, at)),
        }
    }

    /// Records that what this cursor is has been asked for.
    ///
    /// The plane path calls it for itself, and what it records is what the kernel took: a cursor
    /// request returns once the plane's state holds it, and the display engine shows it at the next
    /// scanout without anything else being asked for. So a request that returned needs no second
    /// one, which is the property this record is here for.
    ///
    /// The loop calls it for the fallback path, after it has asked the surface to be drawn — and
    /// there asking is not showing. Without it every later turn would ask again and a pointer that
    /// moved once would redraw for ever, so the request is recorded rather than the frame. Two
    /// things can stop that frame arriving, and they end differently:
    ///
    /// * **A flip is still on its way**, so [`Scanout::present`](crate::Scanout::present) declines
    ///   the frame. The contract has the caller ask for another when the completion arrives, so
    ///   this repairs itself one refresh later.
    /// * **The runtime draws nothing for the request.** Nothing asks again, so the pointer stays
    ///   one motion behind until anything else moves it — while every click still lands where the
    ///   pointer really is, because the position an event carries is never this record.
    pub fn asked_for(&mut self) {
        self.asked = self
            .wanted()
            .map_or(Asked::Nothing, |(style, at)| Asked::For(style, at));
    }

    /// Forgets what the plane is holding, because another session has had it.
    ///
    /// A session that has been away calls this before it commits. What [`Cursor::asked_for`]
    /// records is what **this** cursor put on the plane, and the session that owned the screen in
    /// between has put its own there — so a cursor that kept the record would plan a move, which
    /// keeps whatever image the plane already holds, and the pointer would come back as another
    /// program's shape or as nothing at all.
    ///
    /// **A display the pointer is on no part of is the half that costs a screen.** Such a cursor
    /// wants nothing, so a record cleared to "nothing is up there" would plan nothing and the image
    /// the other session left would stay on the plane for the rest of the run — a second pointer,
    /// on a display this program never draws one on. So the record says *unknown* instead, and a
    /// display that wants no cursor asks for the plane to be cleared.
    ///
    /// A display on the fallback is covered by the same call: the record there is a frame that was
    /// asked for, and the frame that carried the pointer is one another session drew over.
    pub fn forget_the_plane(&mut self) {
        self.asked = Asked::Unknown;
    }

    /// Returns what putting this cursor on its plane would take, or nothing where it would take
    /// nothing.
    ///
    /// The only part of the plane path that runs without a device. What matters here is the split
    /// between [`Plan::Set`] and [`Plan::Move`]: a move keeps the image the plane already has,
    /// which is why it costs two numbers — and a move where a set was needed leaves the wrong
    /// picture on the plane with every ioctl reporting success, so hovering a button would keep
    /// showing the arrow.
    ///
    /// Keyed on the style rather than on the picture. Two styles that fall back to the same shape
    /// therefore cost one image commit where a move would have done, which is the safe direction.
    ///
    /// A cursor that has forgotten its plane plans an image where one is wanted and a
    /// [`Plan::Hide`] where none is. See [`Cursor::forget_the_plane`] for what the second one is
    /// worth.
    fn plan(&self) -> Option<Plan> {
        if !self.changed() {
            return None;
        }
        let Some((style, (x, y))) = self.wanted() else {
            return Some(Plan::Hide);
        };
        if matches!(self.asked, Asked::For(was, _) if was == style) {
            Some(Plan::Move(x, y))
        } else {
            Some(Plan::Set(style, x, y))
        }
    }

    /// Puts what this cursor is on its plane.
    ///
    /// Answers at once on a display with no plane, and on one where the plane already holds what
    /// is wanted. The loop calls it once a turn rather than once per motion: a move can still wait
    /// for an outstanding flip, and a change of shape is a property commit that waits for up to
    /// two refreshes — and a loop that reads flips, deadlines and input on one thread does none of
    /// the three while it waits.
    ///
    /// **A refusal takes this display off the plane for the rest of the program**, and the pointer
    /// is drawn into its frames from then on. It has to: the loop asks again whenever the pointer
    /// moves, so a display that kept a plane it cannot commit to would reissue the same failing
    /// ioctl every turn — and would draw no pointer either, because a display on a plane draws
    /// none into its frames. That is an invisible pointer and one warning a turn, which is the
    /// loudest and least useful way a console can fail.
    ///
    /// [`Cursor::new`] catches every refusal it can see before the first frame. This is the one it
    /// cannot see: which configurations a driver takes is known only by asking it, and the plane
    /// path is the part of this backend that a machine holding DRM master has never run.
    ///
    /// # Errors
    ///
    /// Returns whatever the kernel refused the image, the move or the hiding with. The display has
    /// already fallen back by then, so a caller logs it rather than acting on it.
    pub fn commit(&mut self, device: &Device, commit: &mut dyn Commit) -> Result<(), Error> {
        if !self.hardware {
            return Ok(());
        }
        let Some(plan) = self.plan() else {
            return Ok(());
        };
        let refused = match plan {
            Plan::Hide => commit.hide_cursor(device, self.plane),
            Plan::Move(x, y) => commit.move_cursor(device, self.plane, x, y),
            Plan::Set(_, x, y) => self
                .write(device)
                .and_then(|image| commit.set_cursor(device, self.plane, image, x, y)),
        };
        if let Err(error) = refused {
            self.hardware = false;
            // Whatever the plane is holding has to go, or the image left on it and the one drawn
            // into every later frame are two pointers on one screen. The plane may hold nothing —
            // a refused `set_cursor` puts nothing there — and hiding nothing is not an error.
            drop(commit.hide_cursor(device, self.plane));
            self.asked = Asked::Nothing;
            return Err(error);
        }
        self.asked_for();
        Ok(())
    }

    /// Draws it into a frame, for a display with no plane to put it on.
    ///
    /// Answers by drawing nothing where the pointer is on another display, where the style asks
    /// for no cursor, and where the display engine is compositing it already.
    pub fn draw(&self, into: &mut [u8], stride: usize, width: u32, height: u32) {
        if self.hardware {
            return;
        }
        let (Some(image), Some((x, y))) = (self.image.as_ref(), self.wanted().map(|(_, at)| at))
        else {
            return;
        };
        image.draw(into, stride, width, height, x, y);
    }

    /// Gives the buffer back.
    ///
    /// Taken by value, because the plane is dead afterwards. A refusal is reported through the log
    /// rather than returned: this runs while a program is shutting down, and the kernel releases
    /// the buffer when the device closes either way.
    ///
    /// The cursor is not taken off its plane first. A hide here would be one more blocking commit
    /// while the program is shutting down.
    ///
    /// A direct run on an atomic device has the plane cleared for it: closing its own descriptor is
    /// the last handle on the device, so the kernel's client restores from `drm_lastclose` and
    /// `drm_client_modeset_commit_atomic` disables every plane that is not primary. The legacy
    /// commit disables none, and a seated run reaches neither — the daemon holds a duplicate of this
    /// descriptor, so the process exiting closes no last handle.
    ///
    /// **So a seated shutdown leaves the image on the plane.** What takes a pointer off a seated
    /// run's screen is [`Cursor::give_the_plane_back`] before the switch, and a run that stops
    /// without ever switching leaves it up for the next session.
    pub fn release(self, device: &Device) {
        let Some(held) = self.held else {
            return;
        };
        if let Some(framebuffer) = held.framebuffer
            && let Err(error) = device.remove_framebuffer(framebuffer)
        {
            warn!("a cursor framebuffer could not be removed: {error}");
        }
        if let Err(error) = device.destroy_dumb_buffer(held.buffer) {
            warn!("a cursor buffer could not be released: {error}");
        }
    }

    /// Returns the style that should be on the screen and the corner its image goes at.
    ///
    /// The corner rather than the pointer: both interfaces place a cursor by its top left corner,
    /// so the caller subtracts the hotspot. A pointer near the left or the top edge lands at a
    /// negative coordinate, which both interfaces take.
    fn wanted(&self) -> Option<(CursorStyle, (i32, i32))> {
        let (x, y) = self.at?;
        let image = self.image.as_ref()?;
        Some((self.style, (x - image.hotspot_x(), y - image.hotspot_y())))
    }

    /// Writes the picture into the buffer and names it the way both interfaces need.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unusable`] when there is no buffer to write into, and whatever the driver
    /// refused the mapping with.
    fn write(&mut self, device: &Device) -> Result<CursorImage, Error> {
        let (Some(held), Some(image)) = (self.held.as_mut(), self.image.as_ref()) else {
            return Err(Error::Unusable(
                "this display has no cursor buffer to put an image in".to_owned(),
            ));
        };
        let (width, height) = (held.buffer.width(), held.buffer.height());
        let stride = held.buffer.stride();
        image.fill(held.buffer.bytes(device)?, stride as usize, height);
        Ok(CursorImage {
            framebuffer: held.framebuffer,
            handle: held.buffer.handle(),
            // The buffer's extent rather than the picture's: the plane scans out every pixel of
            // it, and the picture sits in its top left corner with the rest cleared.
            width,
            height,
            stride,
            format: CursorImage::LEGACY_FORMAT,
            hotspot_x: image.hotspot_x(),
            hotspot_y: image.hotspot_y(),
        })
    }
}

/// Allocates one buffer of the extent this device asked for, registered where the atomic interface
/// needs it.
///
/// Answers with nothing where the driver refuses, and where the buffer it gave back is one the
/// legacy interface would read as something else. Both leave the pointer drawn into the frame.
fn allocate(device: &Device, size: CursorSize) -> Option<Held> {
    let buffer =
        match device.create_dumb_buffer(size.width, size.height, CursorImage::LEGACY_FORMAT) {
            Ok(buffer) => buffer,
            Err(error) => {
                warn!(
                    "a {}x{} cursor buffer was refused: {error}",
                    size.width, size.height
                );
                return None;
            }
        };
    if !legible(device.is_atomic(), size.width, buffer.stride()) {
        warn!(
            "this driver gave a {}-pixel cursor buffer rows of {} bytes, and the legacy cursor \
             request reads {} — so the pointer is drawn into the frame instead",
            size.width,
            buffer.stride(),
            CursorImage::legacy_stride(size.width)
        );
        drop(device.destroy_dumb_buffer(buffer));
        return None;
    }
    let framebuffer = if device.is_atomic() {
        match device.add_framebuffer(&buffer, CursorImage::LEGACY_FORMAT) {
            Ok(framebuffer) => Some(framebuffer),
            Err(error) => {
                warn!("a cursor buffer could not be registered for scanout: {error}");
                drop(device.destroy_dumb_buffer(buffer));
                return None;
            }
        }
    } else {
        None
    };
    Some(Held {
        buffer,
        framebuffer,
    })
}

/// Returns the plane this display keeps other displays away from.
///
/// Only one it has a buffer on. A plane recorded for a display that fell back is a plane the next
/// display cannot have either, so one refused allocation would cost two displays their hardware
/// cursor rather than one.
fn reserved(id: Option<u32>, allocated: bool) -> Option<u32> {
    id.filter(|_| allocated)
}

/// Returns `true` if the interface in use reads a buffer of this extent the way it was laid out.
///
/// The legacy request carries no stride and the kernel reads four bytes a pixel with no rounding,
/// so a buffer whose rows the driver rounded up is scanned out sheared while every ioctl reports
/// success. The atomic path reads the stride off the framebuffer and takes any layout the plane
/// advertises, so it asks nothing.
///
/// Asked at allocation rather than at the commit: a display that cannot use its plane has to fall
/// back before its first frame rather than fail on every motion.
fn legible(atomic: bool, width: u32, stride: u32) -> bool {
    atomic || u64::from(stride) == CursorImage::legacy_stride(width)
}

#[cfg(test)]
mod tests {
    //! What each style is drawn as, and where a drawn cursor lands.
    //!
    //! Every one of these is pure: a shape is characters and a frame is a slice, so nothing here
    //! needs a device, a plane or DRM master.

    use super::{
        Asked, BYTES_PER_PIXEL, Cursor, Image, Plan, Shape, Turn, drawn, legible, reserved, turned,
    };
    use zgui_drm::cursor::CursorPlane;
    use zgui_drm::{Device, commit};
    use zgui_platform::CursorStyle;

    /// A cursor on a display with no plane to put one on, which a virtual machine has.
    ///
    /// Built here rather than through [`Cursor::new`], which needs a device, DRM master and a
    /// driver that hands over a plane. What it holds afterwards is the same state the real one
    /// holds on the fallback path: no buffer, and a picture drawn into the frame.
    fn fallback(style: CursorStyle) -> Cursor {
        Cursor {
            // A CRTC the kernel numbers nothing. One test below issues a real request against this
            // plane, and a plausible id would be a real display's — so an id that names no object
            // keeps a test run from taking somebody's cursor away.
            plane: CursorPlane { crtc: 0, id: None },
            hardware: false,
            held: None,
            image: Image::of(style),
            style,
            at: None,
            asked: Asked::Nothing,
        }
    }

    /// Every style the contract names.
    const EVERY: &[CursorStyle] = &[
        CursorStyle::Default,
        CursorStyle::Pointer,
        CursorStyle::Text,
        CursorStyle::VerticalText,
        CursorStyle::Crosshair,
        CursorStyle::Grab,
        CursorStyle::Grabbing,
        CursorStyle::Wait,
        CursorStyle::Progress,
        CursorStyle::NotAllowed,
        CursorStyle::Move,
        CursorStyle::ResizeColumn,
        CursorStyle::ResizeRow,
        CursorStyle::ResizeEastWest,
        CursorStyle::ResizeNorthSouth,
        CursorStyle::ResizeNorthEastSouthWest,
        CursorStyle::ResizeNorthWestSouthEast,
        CursorStyle::None,
    ];

    /// The four bytes at `(x, y)` of an image.
    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let at = (y * image.width() + x) as usize * BYTES_PER_PIXEL;
        image.bytes()[at..at + BYTES_PER_PIXEL]
            .try_into()
            .expect("four bytes a pixel")
    }

    #[test]
    fn every_style_the_contract_names_is_drawn_as_something() {
        // A style a platform does not have falls back to the default rather than failing, which is
        // the rule the contract states. Only the style that asks for no cursor at all answers with
        // nothing.
        for style in EVERY {
            let drawn = Image::of(*style);
            assert_eq!(
                drawn.is_some(),
                *style != CursorStyle::None,
                "{style:?} is drawn as {drawn:?}"
            );
        }
    }

    #[test]
    fn a_style_with_no_shape_of_its_own_is_the_arrow() {
        let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");
        for style in [
            CursorStyle::Grab,
            CursorStyle::Grabbing,
            CursorStyle::Wait,
            CursorStyle::Progress,
            CursorStyle::NotAllowed,
            CursorStyle::Move,
        ] {
            assert_eq!(
                Image::of(style).as_ref(),
                Some(&arrow),
                "{style:?} falls back to the arrow"
            );
        }
    }

    #[test]
    fn the_shapes_a_person_tells_apart_are_different_pictures() {
        // A mapping that answered the arrow for everything would pass every other test here.
        let shapes = [
            CursorStyle::Default,
            CursorStyle::Pointer,
            CursorStyle::Text,
            CursorStyle::VerticalText,
            CursorStyle::Crosshair,
            CursorStyle::ResizeEastWest,
            CursorStyle::ResizeNorthSouth,
            CursorStyle::ResizeNorthWestSouthEast,
            CursorStyle::ResizeNorthEastSouthWest,
        ];
        for (index, style) in shapes.iter().enumerate() {
            for other in &shapes[index + 1..] {
                assert_ne!(
                    Image::of(*style),
                    Image::of(*other),
                    "{style:?} and {other:?} are the same picture"
                );
            }
        }
    }

    #[test]
    fn a_shape_drawn_lying_down_and_the_same_shape_standing_up_swap_their_axes() {
        let across = Image::of(CursorStyle::ResizeEastWest).expect("the arrow is drawn");
        let along = Image::of(CursorStyle::ResizeNorthSouth).expect("the arrow is drawn");

        assert_eq!(across.width(), along.height());
        assert_eq!(across.height(), along.width());
        assert_eq!(across.hotspot_x(), along.hotspot_y());
        assert_eq!(across.hotspot_y(), along.hotspot_x());
        for y in 0..across.height() {
            for x in 0..across.width() {
                assert_eq!(pixel(&across, x, y), pixel(&along, y, x), "at ({x}, {y})");
            }
        }
    }

    #[test]
    fn one_diagonal_is_the_other_one_read_backwards() {
        let falling = Image::of(CursorStyle::ResizeNorthWestSouthEast).expect("it is drawn");
        let rising = Image::of(CursorStyle::ResizeNorthEastSouthWest).expect("it is drawn");

        assert_eq!(falling.width(), rising.width());
        for y in 0..falling.height() {
            for x in 0..falling.width() {
                assert_eq!(
                    pixel(&falling, x, y),
                    pixel(&rising, falling.width() - 1 - x, y),
                    "at ({x}, {y})"
                );
            }
        }
    }

    /// The image a shape is drawn as, the way [`Image::of`] draws one.
    fn image_of(shape: Shape) -> Image {
        drawn(&turned(shape), shape.hotspot, shape.turn)
    }

    #[test]
    fn mirroring_a_shape_carries_its_hotspot_across_with_it() {
        // The arithmetic that moves a hotspot when a shape is read backwards, over a shape written
        // here. The one shape this backend mirrors is fifteen wide and points at its middle
        // column, so the move lands where it started and the diagonal's own tests cannot tell the
        // expression from one that dropped it. A silhouette that is not symmetrical, pointed at
        // off centre, can.
        const CORNER: &[&str] = &["#", "#", "####"];

        let upright = image_of(Shape {
            rows: CORNER,
            hotspot: (0, 0),
            turn: Turn::Keep,
        });
        let mirrored = image_of(Shape {
            rows: CORNER,
            hotspot: (0, 0),
            turn: Turn::Mirrored,
        });

        assert_eq!((upright.hotspot_x(), upright.hotspot_y()), (1, 1));
        assert_eq!(
            (mirrored.hotspot_x(), mirrored.hotspot_y()),
            (4, 1),
            "the top of the upright is on the other side once the shape is read backwards"
        );
        assert_eq!(
            pixel(&mirrored, 4, 1),
            [0xFF, 0xFF, 0xFF, 0xFF],
            "and it is still a pixel of the picture rather than a hole beside it"
        );
    }

    #[test]
    fn every_shapes_hotspot_is_on_the_shape() {
        // A hotspot is where a person aims, so it has to be a pixel of the picture rather than a
        // hole in it. All six shapes satisfy this today; what this catches is a hotspot that
        // drifts off one when a silhouette is edited, which shows up as clicks landing a few
        // pixels from where they were aimed.
        for style in EVERY {
            let Some(drawn) = Image::of(*style) else {
                continue;
            };
            let (x, y) = (drawn.hotspot_x(), drawn.hotspot_y());
            assert!(
                x >= 0 && y >= 0 && (x as u32) < drawn.width() && (y as u32) < drawn.height(),
                "{style:?} points at ({x}, {y}), which is outside its own {}x{} picture",
                drawn.width(),
                drawn.height()
            );
            let [_, _, _, alpha] = pixel(&drawn, x as u32, y as u32);
            assert_ne!(
                alpha, 0,
                "{style:?} points at ({x}, {y}), where it has drawn nothing"
            );
        }
    }

    #[test]
    fn the_arrows_tip_is_where_it_points() {
        // A hotspot in the middle of the shape puts every click a dozen pixels below and to the
        // right of where the person aimed, which reads as a document that ignores its own edges.
        let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");

        assert_eq!((arrow.hotspot_x(), arrow.hotspot_y()), (1, 1));
        assert_eq!(
            pixel(&arrow, 1, 1),
            [0xFF, 0xFF, 0xFF, 0xFF],
            "the tip is the shape itself"
        );
        assert_eq!(
            pixel(&arrow, 0, 0),
            [0x00, 0x00, 0x00, 0xFF],
            "and the outline is the pixel above and to the left of it"
        );
    }

    #[test]
    fn every_shape_is_outlined_on_every_side() {
        // The arrow's left edge is vertical and touches column zero of its own silhouette, so a
        // drawing with no border round it has no outline there — and a white cursor with one side
        // unoutlined disappears against anything white. Every neighbour of a filled pixel has to
        // be inside the image and has to be drawn, which says the border is wide enough and the
        // outline has no gap in it.
        for style in EVERY {
            let Some(drawn) = Image::of(*style) else {
                continue;
            };
            for y in 0..drawn.height() {
                for x in 0..drawn.width() {
                    if pixel(&drawn, x, y) != [0xFF, 0xFF, 0xFF, 0xFF] {
                        continue;
                    }
                    for dy in -1..=1_i32 {
                        for dx in -1..=1_i32 {
                            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                            assert!(
                                nx >= 0
                                    && ny >= 0
                                    && (nx as u32) < drawn.width()
                                    && (ny as u32) < drawn.height(),
                                "{style:?} reaches its own edge at ({x}, {y}), so it has no \
                                 outline there"
                            );
                            let [_, _, _, alpha] = pixel(&drawn, nx as u32, ny as u32);
                            assert_ne!(
                                alpha, 0,
                                "{style:?} has a hole in its outline beside ({x}, {y})"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_filled_pixel_is_white_and_a_pixel_beside_it_is_black() {
        let cross = Image::of(CursorStyle::Crosshair).expect("the crosshair is drawn");
        // The silhouette's centre, one pixel in from the border.
        assert_eq!(pixel(&cross, 8, 8), [0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(
            pixel(&cross, 8, 0),
            [0x00, 0x00, 0x00, 0xFF],
            "the pixel above the top of the upright is the outline"
        );
        assert_eq!(
            pixel(&cross, 0, 0),
            [0x00, 0x00, 0x00, 0x00],
            "and a corner nothing reaches is clear"
        );
    }

    #[test]
    fn a_plane_with_no_buffer_on_it_is_left_for_the_next_display() {
        assert_eq!(reserved(Some(31), true), Some(31));
        assert_eq!(
            reserved(Some(31), false),
            None,
            "a display that fell back holds no plane, so the next one can still have it"
        );
        assert_eq!(reserved(None, true), None);
    }

    #[test]
    fn a_refused_commit_takes_the_display_off_its_plane_for_good() {
        // The refusal `Cursor::new` cannot see. Which configurations a driver takes is known only
        // by asking it, so a display can be offered a plane and refused the first image on it —
        // and a display that kept a plane it cannot commit to would reissue the same failing
        // ioctl every turn for the rest of the program while drawing no pointer into its frames
        // either. That is an invisible pointer and one warning a turn.
        let test = "a_refused_commit_takes_the_display_off_its_plane_for_good";
        let Ok(device) = Device::open_first() else {
            eprintln!(
                "{test}: no DRM device on this machine, so nothing was asserted\n\
                 load the virtual driver with `sudo modprobe vkms` to run it"
            );
            return;
        };
        let mut commit = commit::for_device(&device);

        // A display that was offered a plane and has no buffer for it. Writing the image is what
        // fails, before any request reaches the driver, so this is the refusal path without a
        // driver having to be talked into refusing anything — and without DRM master, which a
        // machine running a compositor cannot give.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.hardware = true;
        cursor.place(Some((10, 10)));

        assert!(cursor.commit(&device, &mut *commit).is_err());
        assert!(
            !cursor.on_a_plane(),
            "the display is off its plane, so its frames carry the pointer from now on"
        );

        cursor.place(Some((11, 11)));
        assert!(
            cursor.commit(&device, &mut *commit).is_ok(),
            "and it never asks the driver again"
        );
        let (width, height) = (64_u32, 64_u32);
        let mut into = frame(width, height);
        cursor.draw(&mut into, width as usize * BYTES_PER_PIXEL, width, height);
        assert!(
            into.iter().any(|byte| *byte != 0x40),
            "the pointer is drawn into the frame now, rather than nowhere at all"
        );
    }

    #[test]
    fn a_cursor_is_drawn_with_the_pixel_a_person_aims_with_under_the_pointer() {
        // Both interfaces place a cursor by its top left corner, so the caller subtracts the
        // hotspot. Left out, every cursor sits below and to the right of where it points and every
        // click lands where the person was not aiming.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.place(Some((40, 50)));
        let (width, height) = (64_u32, 64_u32);
        let mut into = frame(width, height);

        cursor.draw(&mut into, width as usize * BYTES_PER_PIXEL, width, height);

        assert_eq!(
            dot(&into, width, 40, 50),
            [0xFF, 0xFF, 0xFF, 0x40],
            "the tip of the arrow is exactly where the pointer is"
        );
    }

    #[test]
    fn a_pointer_on_another_display_draws_nothing_on_this_one() {
        // Two displays and one pointer. A display that drew the cursor whether or not the pointer
        // was on it would show one on every screen at once.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.place(None);
        let (width, height) = (64_u32, 64_u32);
        let mut into = frame(width, height);

        cursor.draw(&mut into, width as usize * BYTES_PER_PIXEL, width, height);

        assert!(into.iter().all(|byte| *byte == 0x40));
    }

    #[test]
    fn a_style_that_asks_for_no_cursor_draws_none() {
        let mut cursor = fallback(CursorStyle::Default);
        cursor.place(Some((10, 10)));
        cursor.asked_for();
        cursor.set_style(CursorStyle::None);
        let (width, height) = (64_u32, 64_u32);
        let mut into = frame(width, height);

        cursor.draw(&mut into, width as usize * BYTES_PER_PIXEL, width, height);

        assert!(into.iter().all(|byte| *byte == 0x40));
        assert!(
            cursor.changed(),
            "and taking a cursor away is itself a change the screen has not seen yet"
        );
    }

    #[test]
    fn a_display_on_a_plane_draws_nothing_into_its_frames() {
        // The display engine composites the image, so a backend that also drew it would show two
        // cursors, one of which never moves between frames.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.hardware = true;
        cursor.place(Some((10, 10)));
        let (width, height) = (64_u32, 64_u32);
        let mut into = frame(width, height);

        cursor.draw(&mut into, width as usize * BYTES_PER_PIXEL, width, height);

        assert!(into.iter().all(|byte| *byte == 0x40));
    }

    #[test]
    fn a_cursor_that_has_been_asked_for_asks_for_nothing_more() {
        // What stops the fallback asking for a frame every turn for the rest of the program.
        let mut cursor = fallback(CursorStyle::Default);
        assert!(!cursor.changed(), "a pointer on no display shows nothing");

        cursor.place(Some((10, 10)));
        assert!(cursor.changed());
        cursor.asked_for();
        assert!(!cursor.changed());

        cursor.place(Some((10, 11)));
        assert!(cursor.changed(), "a pointer that moved one pixel has moved");
        cursor.asked_for();

        cursor.set_style(CursorStyle::Text);
        assert!(cursor.changed(), "and so has one that changed shape");
        cursor.asked_for();
        assert!(!cursor.changed());
    }

    #[test]
    fn a_style_that_falls_back_to_the_one_already_shown_is_still_a_change_of_style() {
        // `Wait` and `Grab` are both drawn as the arrow. What is committed is keyed on the style
        // rather than on the picture, so this costs one image commit where a move would have done
        // — which is the safe direction: the other way round leaves the wrong picture on a plane.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.place(Some((10, 10)));
        cursor.asked_for();

        cursor.set_style(CursorStyle::Wait);

        assert!(cursor.changed());
    }

    #[test]
    fn a_shape_that_is_not_on_the_plane_is_put_on_it_rather_than_moved() {
        // The one decision on the plane path that a device cannot help with, and the one that
        // fails silently: a move keeps whatever image the plane already holds, so a move where a
        // set was needed leaves the arrow showing while the pointer is over a button, with every
        // ioctl reporting success.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.hardware = true;
        cursor.place(Some((10, 10)));

        assert_eq!(
            cursor.plan(),
            Some(Plan::Set(CursorStyle::Default, 9, 9)),
            "nothing is on the plane yet, so the image has to go on it"
        );
        cursor.asked_for();

        cursor.place(Some((20, 30)));
        assert_eq!(
            cursor.plan(),
            Some(Plan::Move(19, 29)),
            "the same shape somewhere else is two numbers"
        );
        cursor.asked_for();

        cursor.set_style(CursorStyle::Text);
        let Some(Plan::Set(style, ..)) = cursor.plan() else {
            panic!(
                "a different shape needs a different image: {:?}",
                cursor.plan()
            );
        };
        assert_eq!(style, CursorStyle::Text);
        cursor.asked_for();

        cursor.set_style(CursorStyle::None);
        assert_eq!(
            cursor.plan(),
            Some(Plan::Hide),
            "and a style that asks for no cursor takes the plane's own away"
        );
        cursor.asked_for();
        assert_eq!(cursor.plan(), None, "then there is nothing left to do");
    }

    #[test]
    fn a_plane_another_session_has_had_is_written_again_rather_than_moved() {
        // What a resume does before it commits. The record says the arrow is already up there at
        // this very corner, and it is worth nothing: the session that owned the screen in between
        // put its own image on the plane. A cursor that kept the record would plan a move, which
        // keeps whatever the plane holds, and this program's pointer would come back as another
        // program's shape — with every ioctl reporting success.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.hardware = true;
        cursor.place(Some((10, 10)));
        cursor.asked_for();
        assert_eq!(cursor.plan(), None, "this cursor believes it is already up");

        cursor.forget_the_plane();

        assert_eq!(
            cursor.plan(),
            Some(Plan::Set(CursorStyle::Default, 9, 9)),
            "so the image goes on the plane again, at the corner it was already at"
        );
    }

    #[test]
    fn a_plane_another_session_has_had_is_cleared_where_no_pointer_is_wanted() {
        // A display the pointer is on no part of wants no cursor, so a record cleared to "nothing
        // is up there" plans nothing — and the image the other session left stays on the plane for
        // the rest of the run. That is a second pointer, sitting still, on a display this program
        // never draws one on.
        //
        // The same shape covers a display whose pointer has not been placed yet, which is every
        // display of a run that started on a terminal nobody was looking at.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.hardware = true;
        cursor.place(None);
        assert_eq!(
            cursor.plan(),
            None,
            "a display with no pointer on it asks for nothing in the ordinary course"
        );

        cursor.forget_the_plane();

        assert_eq!(
            cursor.plan(),
            Some(Plan::Hide),
            "and after another session has had the plane, clearing it is what is left to do"
        );
        cursor.asked_for();
        assert_eq!(
            cursor.plan(),
            None,
            "once, so the loop asks the driver for nothing every turn afterwards"
        );
    }

    #[test]
    fn a_plan_puts_the_image_at_the_corner_rather_than_at_the_pointer() {
        // Both interfaces place a cursor by its top left corner, so the hotspot is subtracted
        // once, here. A pointer at the very corner of a display therefore commits a negative
        // coordinate, which is the ordinary case rather than an edge one.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.hardware = true;
        cursor.place(Some((0, 0)));

        assert_eq!(cursor.plan(), Some(Plan::Set(CursorStyle::Default, -1, -1)));
    }

    #[test]
    fn a_style_that_falls_back_to_the_shape_on_the_plane_is_still_put_on_it() {
        // `Wait` and `Default` are drawn the same. The plan is keyed on the style rather than on
        // the picture, so this costs one image commit where a move would have done — which is the
        // safe direction, because the other way round leaves the wrong picture on a plane.
        let mut cursor = fallback(CursorStyle::Default);
        cursor.hardware = true;
        cursor.place(Some((10, 10)));
        cursor.asked_for();

        cursor.set_style(CursorStyle::Wait);

        assert!(matches!(cursor.plan(), Some(Plan::Set(..))));
    }

    #[test]
    fn a_display_with_no_plane_plans_nothing_for_one() {
        let mut cursor = fallback(CursorStyle::Default);
        cursor.place(Some((10, 10)));

        // The plan is the same either way. `commit` reads it, and it answers at once where
        // there is no plane — so the fallback is decided by one field rather than by two that could
        // disagree.
        assert!(cursor.plan().is_some());
        assert!(!cursor.on_a_plane());
    }

    #[test]
    fn the_legacy_interface_refuses_a_buffer_whose_rows_it_would_misread() {
        // `drm_mode_cursor2` carries no stride and the kernel reads four bytes a pixel with no
        // rounding, so a buffer whose rows a driver rounded up is scanned out sheared while every
        // ioctl reports success. The atomic path reads the stride off the framebuffer and asks
        // nothing.
        assert!(
            legible(false, 64, 64 * 4),
            "four bytes a pixel is what it reads"
        );
        assert!(
            !legible(false, 64, 64 * 4 + 64),
            "and a row the driver rounded up is one it would shear"
        );
        assert!(
            legible(true, 64, 64 * 4 + 64),
            "the atomic path takes any layout the plane advertises"
        );
    }

    /// A frame of `width` by `height`, filled with a colour no cursor writes.
    fn frame(width: u32, height: u32) -> Vec<u8> {
        vec![0x40; (width * height) as usize * BYTES_PER_PIXEL]
    }

    /// The four bytes at `(x, y)` of a frame.
    fn dot(frame: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = (y * width + x) as usize * BYTES_PER_PIXEL;
        frame[at..at + BYTES_PER_PIXEL]
            .try_into()
            .expect("four bytes a pixel")
    }

    #[test]
    fn a_cursor_drawn_into_a_frame_lands_where_it_was_put() {
        // The fallback path, over a slice. A display with no cursor plane is every virtual machine
        // and every device on the legacy interface, so this is the path most runs take.
        let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");
        let (width, height) = (64, 64);
        let mut into = frame(width, height);

        arrow.draw(
            &mut into,
            width as usize * BYTES_PER_PIXEL,
            width,
            height,
            10,
            20,
        );

        assert_eq!(
            dot(&into, width, 11, 21),
            [0xFF, 0xFF, 0xFF, 0x40],
            "the tip is one pixel in from the corner it was put at, and the frame's fourth byte is \
             left alone"
        );
        assert_eq!(
            dot(&into, width, 10, 20),
            [0x00, 0x00, 0x00, 0x40],
            "with its outline above and to the left"
        );
        assert_eq!(
            dot(&into, width, 9, 19),
            [0x40; 4],
            "and nothing outside the shape was touched"
        );
    }

    #[test]
    fn a_cursor_at_an_edge_is_drawn_as_far_as_the_frame_goes() {
        // A pointer at the top left corner puts its image at a negative coordinate, because the
        // position is the corner of the image and the hotspot is inside it. Wrapping instead of
        // clipping would put half a cursor at the opposite edge of the screen.
        let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");
        let (width, height) = (32, 32);
        let mut into = frame(width, height);

        arrow.draw(
            &mut into,
            width as usize * BYTES_PER_PIXEL,
            width,
            height,
            -1,
            -1,
        );

        assert_eq!(
            dot(&into, width, 0, 0),
            [0xFF, 0xFF, 0xFF, 0x40],
            "the tip landed at the corner, and the outline that would sit above it is gone"
        );
        assert_eq!(
            dot(&into, width, width - 1, height - 1),
            [0x40; 4],
            "and nothing wrapped round"
        );
    }

    #[test]
    fn a_cursor_partly_past_the_right_edge_is_cut_rather_than_wrapped() {
        // A row is a run of bytes and the next row follows it, so a column index past the width is
        // an index into the row below. Without the guard a pointer within its own width of the
        // right edge smears cursor fragments down the left of the screen, one scanline lower.
        let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");
        let (width, height) = (32_u32, 32_u32);
        let mut into = frame(width, height);

        arrow.draw(
            &mut into,
            width as usize * BYTES_PER_PIXEL,
            width,
            height,
            30,
            0,
        );

        assert_eq!(
            dot(&into, width, 30, 0),
            [0x00, 0x00, 0x00, 0x40],
            "the two columns that fit are drawn"
        );
        assert_eq!(
            dot(&into, width, 0, 1),
            [0x40; 4],
            "and the third did not become the first pixel of the row below"
        );
    }

    #[test]
    fn a_cursor_off_the_frame_altogether_draws_nothing() {
        let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");
        let (width, height) = (32, 32);
        let mut into = frame(width, height);

        arrow.draw(
            &mut into,
            width as usize * BYTES_PER_PIXEL,
            width,
            height,
            100,
            100,
        );
        arrow.draw(
            &mut into,
            width as usize * BYTES_PER_PIXEL,
            width,
            height,
            -100,
            -100,
        );

        assert!(into.iter().all(|byte| *byte == 0x40), "nothing was drawn");
    }

    #[test]
    fn a_plane_buffer_holds_the_shape_in_its_corner_and_nothing_else() {
        // The whole buffer is written because whatever it held is the last shape, and a plane
        // scans out every pixel of it. A driver rounds the stride up, so the padding past each row
        // is written too.
        let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");
        let (width, height) = (64_u32, 64_u32);
        let stride = width as usize * BYTES_PER_PIXEL + 16;
        let mut buffer = vec![0xAA; stride * height as usize];

        arrow.fill(&mut buffer, stride, height);

        assert_eq!(
            &buffer[BYTES_PER_PIXEL..BYTES_PER_PIXEL * 2],
            [0x00, 0x00, 0x00, 0xFF],
            "the outline above the tip is at the top left corner"
        );
        assert!(
            buffer[stride * 40..].iter().all(|byte| *byte == 0),
            "and every row past the shape is clear rather than whatever was there"
        );
    }

    #[test]
    fn a_plane_buffer_smaller_than_the_shape_takes_what_fits() {
        // A device that asked for a cursor smaller than any shape here. The alternative is an
        // index past the end, which in a frame loop is a panic per style change.
        let arrow = Image::of(CursorStyle::Default).expect("the arrow is drawn");
        let stride = 8 * BYTES_PER_PIXEL;
        let mut buffer = vec![0xAA; stride * 8];

        arrow.fill(&mut buffer, stride, 8);

        assert_eq!(&buffer[..BYTES_PER_PIXEL], [0x00, 0x00, 0x00, 0xFF]);
    }
}
