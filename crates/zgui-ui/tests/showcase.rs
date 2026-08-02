//! The gallery's transformed-text and vector panels, drawn by a real device and read back.
//!
//! # What these are for
//!
//! Every claim these panels make is a claim about colour and shape, and every one of them has a
//! failure that a display list cannot tell apart from a success. A rotated run that was drawn as an
//! upright tile has the same number of items. An asset written with `currentColor` that never took
//! the colour is drawn black in every context, with the same item count each time. A colour
//! illustration flattened to one hue is the same picture as far as the tree is concerned. A gradient
//! heading whose ramp declaration was dropped in the cascade paints a rectangle of ramp and letters
//! of flat text colour, and looks *more* colourful than the correct answer to anything counting
//! items.
//!
//! So nothing here counts anything. The panels are located in the document by what they say, and
//! the composed target is read off the graphics device and asked about colours at coordinates.
//!
//! # One section at a time, on a surface that holds it
//!
//! A page scrolls, and pixels below the fold are pixels nothing drew. Rather than scroll to each
//! panel — which is a second thing that can go wrong between the claim and the picture — a section
//! is drawn at once onto a surface tall enough to hold it.
//!
//! Two things make that a claim rather than an arrangement, and both are checked instead of
//! assumed. A page can grow past the surface, and a region below the last row reads back as the
//! colour of nothing; [`on_surface`] refuses one. And a frame can plan more vector passes than the
//! rasteriser has scratch layers to keep apart, at which point it loses its vector content whole;
//! [`Part`] is why one section is mounted at a time rather than the whole gallery. Both failures
//! look, pixel for pixel, exactly like the failures this file exists to catch, so neither is left
//! to be discovered as one.

mod desktop;
mod device;
#[path = "../examples/gallery/section/mod.rs"]
#[allow(
    dead_code,
    unused_imports,
    reason = "the gallery's sections are one module; these assertions mount three of them"
)]
mod section;
#[path = "../examples/gallery/shell.rs"]
#[allow(
    dead_code,
    reason = "the shell is one module; these assertions use its sheet and its panels"
)]
mod shell;

use std::sync::{Arc, Mutex};

use zgui::geom::{Device, DevicePx, Rect};
use zgui::platform::{AppHandler, PlatformError};
use zgui::reactive::RwSignal;
use zgui::view;
use zgui::view::AnyView;
use zgui_ui_tokens::prelude::{ColorScheme, ThemeProviderProps};

use crate::desktop::census::Census;
use crate::desktop::grab::{self, Grab};
use crate::device::Log;
use crate::device::frame::Frame;
use crate::section::{ArtworkProps, StyledTextProps, SvgProps};

/// How wide the surface is drawn, in device pixels.
const WIDTH: f32 = 1600.0;

/// How tall, which is enough for any one section at this width with nothing below the fold.
///
/// Well under the 8192 a graphics device commonly stops at, so there is room for a section to grow
/// before the ceiling is the thing that decides what this measures.
const HEIGHT: f32 = 4000.0;

/// One device pixel per CSS pixel, so every coordinate in this file is the one the sheet wrote.
const SCALE: f64 = 1.0;

/// One section of the gallery, drawn on its own.
///
/// **On its own, and that is the point.** The engine rasterises a frame's vector content into a
/// scratch texture with one layer per pass, and there is a fixed ceiling on the layers; a frame
/// planning more passes than that loses its vector content entirely rather than partly. Three
/// sections' worth of paths on one page is over the ceiling, and every assertion in this file would
/// then be reading a surface nothing drew on — which is indistinguishable, pixel for pixel, from
/// each of the failures these assertions exist to catch. One section at a time is under it, so what
/// is measured here is the drawing and not the ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// Text a glyph atlas cannot serve.
    Text,
    /// Vector documents, and how they are fitted into a box.
    Svg,
    /// Colour illustrations, and what a context does and does not tint.
    Artwork,
}

impl Part {
    /// Every section, which is what an assertion spanning all of them walks.
    const ALL: [Self; 3] = [Self::Text, Self::Svg, Self::Artwork];

    /// The regions this section's assertions are asked inside, named by the text identifying them.
    ///
    /// Each is the outermost node saying exactly that — the swatch and its caption, or the cell and
    /// its label — so a region holds one drawing and one small line of grey text and nothing else.
    const fn regions(self) -> &'static [&'static str] {
        match self {
            Self::Text => &[
                "Hlturn 0",
                "Hlturn 30",
                "Hlturn 60",
                "Hlturn 90",
                "Hlturn 135",
                "Agskew",
                "Agscale wide",
                "Agscale tall",
                "Ramped",
            ],
            Self::Svg => &[
                "star on plain",
                "star on rose",
                "star on teal",
                "star on ink",
                "facet on rose",
                "banner in a square",
                "banner in a wide box",
                "banner in a tall box",
                "aspect xMidYMid meet",
                "aspect none",
                "aspect xMinYMid meet",
            ],
            Self::Artwork => &[
                "pair on rose",
                "pair on teal",
                "scene on plain",
                "scene on rose",
                "scene on teal",
                "scene on ink",
            ],
        }
    }

    /// The panels this section's captures show, by the heading each one carries.
    const fn panels(self) -> &'static [&'static str] {
        match self {
            Self::Text => &[
                "Turned type",
                "Text on a turned card",
                "Display and gradient",
                "Spacing",
                "Decoration",
                "Turned and still selectable",
            ],
            Self::Svg => &[
                "One asset, four colours",
                "Fitting a view box",
                "A ramp and a clip",
            ],
            Self::Artwork => &["A palette of its own", "Not tinted by its context"],
        }
    }

    /// What this section is called in a capture's file name.
    const fn slug(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Svg => "svg",
            Self::Artwork => "artwork",
        }
    }

    /// The section itself, under the theme the gallery ships.
    fn view(self) -> AnyView {
        let scheme = RwSignal::new_local(ColorScheme::Light);
        match self {
            Self::Text => AnyView::new(view! {
                ThemeProvider(scheme = scheme) { column(class = "page") { StyledText() } }
            }),
            Self::Svg => AnyView::new(view! {
                ThemeProvider(scheme = scheme) { column(class = "page") { Svg() } }
            }),
            Self::Artwork => AnyView::new(view! {
                ThemeProvider(scheme = scheme) { column(class = "page") { Artwork() } }
            }),
        }
    }
}

/// How many regions all three sections hold between them.
const REGION_COUNT: usize = 26;

/// One run of one section: what it drew, and where the named regions ended up.
struct Opened {
    /// Every frame, in the order they were drawn.
    frames: Vec<Frame>,
    /// Each region of [`Part::regions`] that was found, by name.
    regions: Vec<(String, Rect<DevicePx, Device>)>,
}

/// One section's run, or nothing on a machine with no graphics device.
macro_rules! opened {
    ($part:expr) => {
        match run($part) {
            Some(opened) => opened,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// Drives the application over buffers until it settles, and records where the regions are.
fn buffers(
    part: Part,
    regions: &Mutex<Vec<(String, Rect<DevicePx, Device>)>>,
) -> impl FnOnce(Box<dyn AppHandler>) -> Result<(), PlatformError> + '_ {
    move |handler| {
        let mut harness = zgui_platform_headless::Harness::new(handler);
        harness.deliver_to_first(zgui::platform::SurfaceEvent::ScaleFactorChanged {
            scale_factor: SCALE,
            size: zgui::geom::Size::new(zgui::geom::DevicePx(WIDTH), zgui::geom::DevicePx(HEIGHT)),
        });
        harness.settle(160);
        // Read while the window is still open: the document goes with it.
        if let Some(handles) = grab::taken() {
            let census = Census::take(&handles);
            let mut found = regions.lock().unwrap_or_else(|held| held.into_inner());
            for name in part.regions() {
                if let Some(rect) = census.outermost(name).and_then(|node| node.rect) {
                    on_surface(name, rect);
                    found.push(((*name).to_owned(), rect));
                }
            }
            for title in part.panels() {
                if let Some(rect) = census.panel(title).and_then(|node| node.rect) {
                    on_surface(title, rect);
                    found.push(((*title).to_owned(), rect));
                }
            }
        }
        harness.shut_down();
        Ok(())
    }
}

/// Checks that `rect` is somewhere the surface has pixels.
///
/// A region below the last row is laid out, is found by name, and has a perfectly sensible
/// rectangle — and every pixel read inside it is the colour of a surface nothing reached. That
/// reads exactly like a rasteriser that planned the drawing and never ran it, which is a real
/// failure this file exists to catch, so the two are told apart here rather than in each assertion.
///
/// # Panics
///
/// Panics when the page has outgrown [`HEIGHT`], naming the region that fell off it.
fn on_surface(name: &str, rect: Rect<DevicePx, Device>) {
    let bottom = rect.origin.y.0 + rect.size.height.0;
    let right = rect.origin.x.0 + rect.size.width.0;
    assert!(
        bottom <= HEIGHT && right <= WIDTH,
        "{name:?} was laid out at {rect:?}, which is off a {WIDTH}x{HEIGHT} surface — the page \
         has grown past the surface it is drawn onto, and nothing below that is drawn at all"
    );
}

/// One section, drawn once however many assertions read it.
///
/// One cache per section rather than one for all three: an assertion asks for the section it names,
/// and a section nothing asks about is never drawn.
fn run(part: Part) -> Option<&'static Opened> {
    static OPENED: [std::sync::OnceLock<Option<Opened>>; Part::ALL.len()] =
        [const { std::sync::OnceLock::new() }; Part::ALL.len()];
    OPENED[part as usize]
        .get_or_init(|| {
            if !device::available() {
                return None;
            }
            let _guard = device::device_lock();
            grab::forget();
            let log: Log = Arc::new(Mutex::new(Vec::new()));
            let regions = Mutex::new(Vec::new());
            zgui::app()
                .with_title("gallery")
                .with_size(WIDTH, HEIGHT)
                .with_stylesheet(crate::shell::SHEET)
                .with_renderer(Box::new(device::factory(&log)))
                .run_on(buffers(part, &regions), move || (Grab, part.view()))
                .expect("the gallery ran");
            let frames = core::mem::take(&mut *log.lock().unwrap_or_else(|held| held.into_inner()));
            assert!(!frames.is_empty(), "the gallery drew no frame at all");
            Some(Opened {
                frames,
                regions: regions
                    .into_inner()
                    .unwrap_or_else(|held| held.into_inner()),
            })
        })
        .as_ref()
}

/// The last frame that held any vector content at all.
///
/// Not simply the last frame: once the page has settled the application draws frames that damage
/// nothing, and their display lists are empty, so every question asked of one comes back the same
/// way whether the gallery draws or not.
fn settled(opened: &Opened) -> &Frame {
    opened
        .frames
        .iter()
        .rev()
        .find(|frame| !frame.drawings.is_empty())
        .expect("some frame of the gallery held a drawing")
}

/// Where the region named `name` ended up.
///
/// # Panics
///
/// Panics when the run did not find it, because a rectangle that was never located would make every
/// assertion about what is inside it vacuously true.
fn region(opened: &Opened, name: &str) -> Rect<DevicePx, Device> {
    opened
        .regions
        .iter()
        .find(|(found, _)| found == name)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the run laid out no region saying {name:?}"))
}

/// Whether `inner`'s middle is inside `outer`.
///
/// The middle rather than the whole rectangle. A stroked outline's ink reaches half a stroke width
/// past the box it was fitted into, so a drawing that belongs to a region can stick a pixel out of
/// it — and a containment test would then quietly drop exactly the outlines that are hardest to
/// draw, leaving the measurement to the shapes that are easy.
fn within(outer: Rect<DevicePx, Device>, inner: Rect<DevicePx, Device>) -> bool {
    let x = inner.origin.x.0 + inner.size.width.0 / 2.0;
    let y = inner.origin.y.0 + inner.size.height.0 / 2.0;
    x >= outer.origin.x.0
        && y >= outer.origin.y.0
        && x <= outer.origin.x.0 + outer.size.width.0
        && y <= outer.origin.y.0 + outer.size.height.0
}

/// Every pixel coordinate inside a region, clamped to the surface.
fn every(
    frame: &Frame,
    rect: Rect<DevicePx, Device>,
) -> impl Iterator<Item = (i32, i32)> + use<'_> {
    let size = frame.pixels.size();
    let left = (rect.origin.x.0.floor() as i32).max(0);
    let top = (rect.origin.y.0.floor() as i32).max(0);
    let right = ((rect.origin.x.0 + rect.size.width.0).ceil() as i32).min(size.width);
    let bottom = ((rect.origin.y.0 + rect.size.height.0).ceil() as i32).min(size.height);
    (top..bottom).flat_map(move |y| (left..right).map(move |x| (x, y)))
}

/// How far apart two colours are, as the largest single-channel difference.
fn apart(left: [u8; 3], right: [u8; 3]) -> i32 {
    (0..3)
        .map(|channel| (i32::from(left[channel]) - i32::from(right[channel])).abs())
        .max()
        .unwrap_or(0)
}

/// How many pixels inside `rect` are within `tolerance` of `colour`.
fn count_of(frame: &Frame, rect: Rect<DevicePx, Device>, colour: [u8; 3], tolerance: i32) -> usize {
    every(frame, rect)
        .filter(|(x, y)| {
            let [r, g, b, _] = frame.pixels.rgba(*x, *y);
            apart([r, g, b], colour) <= tolerance
        })
        .count()
}

/// The colour most of `rect` is made of, which is whatever is behind whatever is drawn on it.
fn background(frame: &Frame, rect: Rect<DevicePx, Device>) -> [u8; 3] {
    let mut counts: rustc_hash::FxHashMap<[u8; 3], u32> = rustc_hash::FxHashMap::default();
    for (x, y) in every(frame, rect) {
        let [r, g, b, _] = frame.pixels.rgba(x, y);
        *counts.entry([r, g, b]).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or([0, 0, 0], |(colour, _)| colour)
}

/// The rectangle the ink inside `rect` covers, measured from the pixels themselves.
///
/// Every pixel far enough from the rectangle's own most common colour counts as ink. This is how a
/// run that stayed on the glyph atlas is measured — it produces no vector item to ask — so the two
/// halves of the same panel can be compared with each other.
fn ink_extent(frame: &Frame, rect: Rect<DevicePx, Device>) -> Option<(f32, f32, f32, f32)> {
    let behind = background(frame, rect);
    let mut found: Option<(i32, i32, i32, i32)> = None;
    for (x, y) in every(frame, rect) {
        let [r, g, b, _] = frame.pixels.rgba(x, y);
        if apart([r, g, b], behind) <= 24 {
            continue;
        }
        found = Some(match found {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        });
    }
    found.map(|(x0, y0, x1, y1)| (x0 as f32, y0 as f32, (x1 + 1) as f32, (y1 + 1) as f32))
}

/// The square cell at the top of a turned-type region, without the label under it.
fn cell(rect: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
    Rect::new(
        rect.origin,
        zgui::geom::Size::new(rect.size.width, rect.size.width),
    )
}

/// The width and height of the ink in one turned-type cell, measured from pixels.
///
/// # Panics
///
/// Panics when nothing was drawn there, which is the answer these assertions exist to notice.
fn run_size(frame: &Frame, opened: &Opened, name: &str) -> (f32, f32) {
    let rect = cell(region(opened, name));
    let extent = ink_extent(frame, rect)
        .unwrap_or_else(|| panic!("no ink at all landed inside the {name:?} cell {rect:?}"));
    (extent.2 - extent.0, extent.3 - extent.1)
}

/// The rectangle every drawing inside `rect` covers between them, in whole pixels.
///
/// Only the *vector* items, which is what a promoted run and a vector document both produce. Small
/// upright labels are served from the glyph atlas and are not drawings, so a caption under a cell
/// cannot widen the answer.
fn drawn_extent(frame: &Frame, rect: Rect<DevicePx, Device>) -> Option<(f32, f32, f32, f32)> {
    let mut found: Option<(f32, f32, f32, f32)> = None;
    for drawing in frame.drawings.iter().filter(|item| within(rect, item.ink)) {
        let ink = drawing.ink;
        let next = (
            ink.origin.x.0,
            ink.origin.y.0,
            ink.origin.x.0 + ink.size.width.0,
            ink.origin.y.0 + ink.size.height.0,
        );
        found = Some(match found {
            None => next,
            Some(held) => (
                held.0.min(next.0),
                held.1.min(next.1),
                held.2.max(next.2),
                held.3.max(next.3),
            ),
        });
    }
    found
}

/// The width and height of what was drawn inside a region.
///
/// # Panics
///
/// Panics when nothing was drawn there, which is the answer these assertions exist to notice.
fn drawn_size(frame: &Frame, opened: &Opened, name: &str) -> (f32, f32) {
    let rect = region(opened, name);
    let extent = drawn_extent(frame, rect)
        .unwrap_or_else(|| panic!("no drawing at all landed inside the {name:?} region {rect:?}"));
    (extent.2 - extent.0, extent.3 - extent.1)
}

/// Whether a drawing's own rectangle is any different from the page it is on.
///
/// Two ways, because two different pictures are wrong. An outline — a letter, a ring — leaves most
/// of its rectangle showing whatever is behind it, so it is caught by the rectangle not being one
/// flat colour. A solid shape fills its rectangle edge to edge and *is* one flat colour, so it is
/// caught by that colour differing from what the region around it is mostly made of. A drawing that
/// was planned, counted, composited from a scratch nobody wrote and never rasterised fails both.
fn marks_its_rectangle(frame: &Frame, ink: Rect<DevicePx, Device>, behind: [u8; 3]) -> bool {
    device::ink::fraction(&frame.pixels, ink) > 0.0 || apart(background(frame, ink), behind) > 8
}

#[test]
fn every_drawing_in_the_new_panels_marks_the_pixels_it_claimed() {
    let mut blank = Vec::new();
    let mut counted = 0;
    for part in Part::ALL {
        let opened = opened!(part);
        let frame = settled(opened);
        for name in part.regions() {
            let rect = region(opened, name);
            let behind = background(frame, rect);
            for drawing in frame.drawings.iter().filter(|item| within(rect, item.ink)) {
                counted += 1;
                if !marks_its_rectangle(frame, drawing.ink, behind) {
                    blank.push((*name, drawing.order, drawing.ink));
                }
            }
        }
    }

    assert!(
        counted >= REGION_COUNT,
        "only {counted} drawings landed inside {REGION_COUNT} regions, so this is measuring the \
         wrong document rather than the wrong pixels"
    );
    assert!(
        blank.is_empty(),
        "{} drawings changed nothing at all inside their own rectangle — a box of exactly the \
         right size with nothing in it: {blank:?}",
        blank.len()
    );
}

#[test]
fn a_run_turned_a_quarter_turn_has_the_upright_one_s_extents_transposed() {
    let opened = opened!(Part::Text);
    let frame = settled(opened);

    let (flat_width, flat_height) = run_size(frame, opened, "Hlturn 0");
    let (turned_width, turned_height) = run_size(frame, opened, "Hlturn 90");

    assert!(
        flat_width > flat_height,
        "the upright run is {flat_width}x{flat_height}, which is not the shape `Hl` has — so the \
         measurement is of something else"
    );
    assert!(
        (turned_width - flat_height).abs() <= 3.0 && (turned_height - flat_width).abs() <= 3.0,
        "a quarter turn has to transpose the run: upright {flat_width}x{flat_height}, turned \
         {turned_width}x{turned_height}. A run drawn as an upright tile under a turned transform \
         comes back the same shape it started"
    );
}

#[test]
fn a_run_turned_part_way_is_wider_and_taller_than_the_upright_one() {
    let opened = opened!(Part::Text);
    let frame = settled(opened);

    let (flat_width, flat_height) = run_size(frame, opened, "Hlturn 0");
    for name in ["Hlturn 30", "Hlturn 60", "Hlturn 135"] {
        let (width, height) = run_size(frame, opened, name);
        assert!(
            height > flat_height + 2.0,
            "{name} is {width}x{height} and the upright run is {flat_width}x{flat_height}: a run \
             turned off the axis covers more rows than one on it, and this one does not"
        );
    }
}

#[test]
fn a_skew_widens_a_run_without_making_it_taller_and_a_scale_does_the_opposite() {
    let opened = opened!(Part::Text);
    let frame = settled(opened);

    let (wide_width, wide_height) = run_size(frame, opened, "Agscale wide");
    let (tall_width, tall_height) = run_size(frame, opened, "Agscale tall");
    let (skew_width, skew_height) = run_size(frame, opened, "Agskew");

    assert!(
        wide_width > tall_width * 1.6 && tall_height > wide_height * 1.6,
        "a run scaled 2.2 across and one scaled 2.2 down should be each other transposed: \
         {wide_width}x{wide_height} beside {tall_width}x{tall_height}"
    );
    assert!(
        skew_width > wide_width / 2.0 && (skew_height - tall_height / 2.2).abs() < tall_height,
        "a skew leans the run over, which widens it and leaves its height alone: \
         {skew_width}x{skew_height}"
    );
}

#[test]
fn a_gradient_heading_paints_a_ramp_on_the_letters_rather_than_a_rectangle_behind_them() {
    let opened = opened!(Part::Text);
    let frame = settled(opened);
    let rect = region(opened, "Ramped");

    // The four stops the sheet wrote, sampled generously: what has to be true is that letters of
    // several different hues are on the page, which no flat fill in any single colour produces.
    let stops = [
        [0xf4, 0x3f, 0x5e],
        [0xf5, 0x9e, 0x0b],
        [0x22, 0xc5, 0x5e],
        [0x63, 0x66, 0xf1],
    ];
    let found: Vec<usize> = stops
        .iter()
        .map(|stop| count_of(frame, rect, *stop, 40))
        .collect();
    assert!(
        found.iter().all(|count| *count >= 8),
        "the heading is meant to run through {stops:?}; the pixel counts near those colours are \
         {found:?}. A dropped ramp declaration paints the letters in one flat colour"
    );

    // And it is the letters, not the box. A ramp painting the background would cover the whole
    // heading; letters cover a fraction of the line box they sit on.
    let painted: usize = found.iter().sum();
    let area = (rect.size.width.0 * rect.size.height.0) as usize;
    assert!(
        painted * 2 < area,
        "{painted} of the {area} pixels in the heading's region are ramp, which is a filled \
         rectangle rather than letters cut out of one"
    );
}

#[test]
fn one_monochrome_asset_takes_the_colour_of_each_context_it_is_put_in() {
    let opened = opened!(Part::Svg);
    let frame = settled(opened);

    // Each context, and the colour its sheet sets. The caption under each swatch is grey, so the
    // only thing inside a region that can be this colour is the drawing.
    let contexts = [
        ("star on rose", [0xbe, 0x12, 0x3c]),
        ("star on teal", [0x0f, 0x76, 0x6e]),
        ("star on ink", [0xfd, 0xe6, 0x8a]),
    ];
    for (name, colour) in contexts {
        let rect = region(opened, name);
        let own = count_of(frame, rect, colour, 24);
        assert!(
            own >= 40,
            "{own} pixels of {colour:?} inside {name}: an asset written with `currentColor` that \
             never took it is a black silhouette in every context"
        );
        // And it is not every colour at once: the other two contexts' colours are not in it.
        for (other, wrong) in contexts {
            if other == name {
                continue;
            }
            let strays = count_of(frame, rect, wrong, 24);
            assert!(
                strays * 8 < own,
                "{strays} pixels of {other}'s colour {wrong:?} turned up inside {name}, against \
                 {own} of its own"
            );
        }
    }
}

#[test]
fn a_colour_illustration_keeps_its_palette_whatever_it_is_put_on() {
    let opened = opened!(Part::Artwork);
    let frame = settled(opened);

    // Four fills chosen from four corners of the picture, and far apart from each other, so a
    // picture flattened to one hue cannot satisfy more than one of them.
    let palette = [
        ("sun", [0xf5, 0x9e, 0x0b]),
        ("river", [0x25, 0x63, 0xeb]),
        ("roof", [0xdc, 0x26, 0x26]),
        ("hill", [0x15, 0x80, 0x3d]),
    ];
    let places = [
        "scene on plain",
        "scene on rose",
        "scene on teal",
        "scene on ink",
    ];

    let mut counts: Vec<(&str, Vec<usize>)> = Vec::new();
    for name in places {
        let rect = region(opened, name);
        let found: Vec<usize> = palette
            .iter()
            .map(|(_, colour)| count_of(frame, rect, *colour, 28))
            .collect();
        assert!(
            found.iter().all(|count| *count >= 20),
            "the illustration on {name} shows {found:?} pixels of {:?}; a picture that took the \
             colour of what it sits on has lost most of them",
            palette.map(|(what, _)| what)
        );
        counts.push((name, found));
    }

    // Identical, not merely present: the same picture on four backgrounds is the same pixels.
    let (first_name, first) = &counts[0];
    for (name, found) in counts.iter().skip(1) {
        for (index, (what, _)) in palette.iter().enumerate() {
            let a = first[index] as f32;
            let b = found[index] as f32;
            assert!(
                (a - b).abs() <= a.max(b) * 0.12,
                "the {what} of the illustration covers {a} pixels on {first_name} and {b} on \
                 {name}, so the picture is being tinted by what it sits on"
            );
        }
    }
}

#[test]
fn a_colour_asset_and_a_monochrome_one_side_by_side_answer_their_context_differently() {
    let opened = opened!(Part::Artwork);
    let frame = settled(opened);

    let rose = region(opened, "pair on rose");
    let teal = region(opened, "pair on teal");

    // The star follows the context.
    let star_rose = count_of(frame, rose, [0xbe, 0x12, 0x3c], 24);
    let star_teal = count_of(frame, teal, [0x0f, 0x76, 0x6e], 24);
    assert!(
        star_rose >= 60 && star_teal >= 60,
        "the icon beside the illustration did not take its context's colour: {star_rose} rose, \
         {star_teal} teal"
    );
    assert!(
        count_of(frame, rose, [0x0f, 0x76, 0x6e], 24) * 8 < star_rose,
        "the rose pair holds teal ink, so the icon is not following its context at all"
    );

    // The illustration does not.
    for (name, rect) in [("rose", rose), ("teal", teal)] {
        for (what, colour) in [
            ("sun", [0xf5, 0x9e, 0x0b]),
            ("river", [0x25, 0x63, 0xeb]),
            ("roof", [0xdc, 0x26, 0x26]),
        ] {
            let count = count_of(frame, rect, colour, 28);
            assert!(
                count >= 40,
                "the illustration's {what} is {count} pixels on the {name} pane, so the colour \
                 asset took its context's colour after all"
            );
        }
    }
}

#[test]
fn a_document_with_a_ramp_and_a_clip_draws_the_ramp_and_only_inside_the_clip() {
    let opened = opened!(Part::Svg);
    let frame = settled(opened);
    let rect = region(opened, "facet on rose");
    let ink = drawn_extent(frame, rect).expect("the facet drew something");

    // The ramp runs from the top-left corner of the document to the bottom-right, so across the
    // middle of the drawing red falls and blue rises. Sampling that rather than looking for the
    // stop colours themselves is deliberate: the clip is a diamond, so the two corners the ramp
    // *starts and ends* at are exactly the parts that were cut away, and a fixture hunting for them
    // would be asking the clip to have failed.
    let span = ink.2 - ink.0;
    let down = ink.3 - ink.1;
    let samples: Vec<[u8; 3]> = [0.32_f32, 0.41, 0.5, 0.59, 0.68]
        .iter()
        .map(|along| {
            let [r, g, b, _] = frame
                .pixels
                .rgba((ink.0 + span * along) as i32, (ink.1 + down * along) as i32);
            [r, g, b]
        })
        .collect();
    assert!(
        samples[0][0] > samples[4][0] + 40,
        "red has to fall across the ramp: {samples:?}"
    );
    assert!(
        samples[4][2] > samples[0][2] + 40,
        "and blue has to rise: {samples:?}"
    );

    // The clip is a diamond inside a square drawing, so the drawing's own corners are the tinted
    // pane behind it and not the ramp.
    for (x, y) in [
        (ink.0 + 2.0, ink.1 + 2.0),
        (ink.2 - 3.0, ink.1 + 2.0),
        (ink.0 + 2.0, ink.3 - 3.0),
    ] {
        let [r, g, b, _] = frame.pixels.rgba(x as i32, y as i32);
        assert!(
            apart([r, g, b], [0xff, 0xe4, 0xe6]) <= 24,
            "the corner of the facet is {:?} rather than the rose pane behind it, so the \
             document's clip did not cut its corners off",
            [r, g, b]
        );
    }
}

#[test]
fn one_drawing_fitted_into_three_shapes_of_box_keeps_its_own_proportions() {
    let opened = opened!(Part::Svg);
    let frame = settled(opened);

    // The asset is a space forty-eight units across for sixteen down. Fitted into a box it is
    // scaled by whichever of the two ratios is smaller and centred in what is left over, so the
    // drawing's own shape never changes — which is the difference between fitting and stretching.
    //
    // The reported ink is a little larger than the geometry in both directions, because a stroked
    // outline covers half a stroke width past its own path and the ink is the rectangle a renderer
    // may write into. That margin is the same on both axes, so what is compared is how much wider
    // than tall the ink is: a stretched drawing in a square box is as tall as it is wide.
    for (name, box_width, box_height) in [
        ("banner in a square", 96.0_f32, 96.0_f32),
        ("banner in a wide box", 144.0, 48.0),
        ("banner in a tall box", 48.0, 120.0),
    ] {
        let scale = (box_width / 48.0).min(box_height / 16.0);
        let expected = 48.0 * scale - 16.0 * scale;
        let (width, height) = drawn_size(frame, opened, name);
        assert!(
            ((width - height) - expected).abs() <= 3.0,
            "{name} drew ink {width}x{height}: {} wider than tall, against the {expected} the fit \
             rule asks for",
            width - height
        );
    }

    // And the three are not the same size, or the boxes were not three different shapes.
    let square = drawn_size(frame, opened, "banner in a square").0;
    let wide = drawn_size(frame, opened, "banner in a wide box").0;
    let tall = drawn_size(frame, opened, "banner in a tall box").0;
    assert!(
        wide > square && square > tall,
        "the three boxes should scale the drawing differently: {wide}, {square}, {tall} across"
    );
}

#[test]
fn a_documents_own_aspect_rule_decides_what_shape_its_contents_come_out() {
    let opened = opened!(Part::Svg);
    let frame = settled(opened);

    // A square `viewBox` in a document twice as wide as it is tall. `meet` keeps the contents square
    // inside that wider extent; `none` stretches them across it. Both documents then go into the
    // same element box, so what differs on the page is the aspect rule and nothing else.
    let (met_width, met_height) = drawn_size(frame, opened, "aspect xMidYMid meet");
    let (none_width, none_height) = drawn_size(frame, opened, "aspect none");
    let (left_width, left_height) = drawn_size(frame, opened, "aspect xMinYMid meet");

    assert!(
        (met_width - met_height).abs() <= 4.0,
        "a meeting square stays square: {met_width}x{met_height}"
    );
    assert!(
        none_width > met_width * 1.6,
        "`none` stretches the contents across the document's own extent: \
         {none_width}x{none_height} against {met_width}x{met_height}"
    );
    assert!(
        (left_width - met_width).abs() <= 4.0 && (left_height - met_height).abs() <= 4.0,
        "`xMinYMid meet` is the same shape as `xMidYMid meet`, only further left: \
         {left_width}x{left_height}"
    );

    // Further left: the two differ by where the square sits, which is what `xMin` says.
    let met = drawn_extent(frame, region(opened, "aspect xMidYMid meet")).expect("meet drew");
    let left = drawn_extent(frame, region(opened, "aspect xMinYMid meet")).expect("left drew");
    let met_offset = met.0 - region(opened, "aspect xMidYMid meet").origin.x.0;
    let left_offset = left.0 - region(opened, "aspect xMinYMid meet").origin.x.0;
    assert!(
        met_offset > left_offset + 8.0,
        "a centred square sits further into its box than a left-aligned one: {met_offset} \
         against {left_offset}"
    );
}

/// Writes the regions out as pictures, for the record beside these assertions.
///
/// Does nothing unless a directory is named, so an ordinary test run writes no files. What is
/// written is the very buffer every assertion above read.
#[test]
fn the_panels_are_captured_when_a_capture_directory_is_asked_for() {
    if crate::device::shot::directory().is_none() {
        return;
    }
    for part in Part::ALL {
        let opened = opened!(part);
        let frame = settled(opened);
        let slug = part.slug();

        crate::device::shot::whole(&frame.pixels, &format!("00-whole-page-{slug}"))
            .expect("the page was written");
        for (index, title) in part.panels().iter().enumerate() {
            let rect = region(opened, title);
            let file = format!("panel-{slug}-{:02}-{}", index + 1, title.replace(' ', "-"));
            crate::device::shot::crop(&frame.pixels, rect, &file).expect("the panel was written");
        }
        for (index, name) in part.regions().iter().enumerate() {
            let rect = region(opened, name);
            let file = format!("part-{slug}-{:02}-{}", index + 1, name.replace(' ', "-"));
            crate::device::shot::crop(&frame.pixels, rect, &file).expect("the region was written");
        }
    }
}
