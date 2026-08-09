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

use zgui_platform::CursorStyle;

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

#[cfg(test)]
mod tests {
    //! What each style is drawn as, and where a drawn cursor lands.
    //!
    //! Every one of these is pure: a shape is characters and a frame is a slice, so nothing here
    //! needs a device, a plane or DRM master.

    use super::{BYTES_PER_PIXEL, Image};
    use zgui_platform::CursorStyle;

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
