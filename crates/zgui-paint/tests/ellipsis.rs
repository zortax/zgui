//! What a line cut short by `text-overflow` draws.

mod support;

use std::cell::RefCell;

use zgui_css::StyleDraft;
use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};
use zgui_layout::fragment::ParagraphId;
use zgui_paint::GlyphSource;
use zgui_paint::emit::text::{self, EllipsisPaint, TextPlacement};
use zgui_scene::{ClipId, OwnSpace, PropertyOwner, Scene, SpatialId};
use zgui_testkit_compositor::{Drawing, Instance};

use support::chunk::Lone;

/// The one glyph every line holds: at the line's start, twelve pixels down to the baseline.
const GLYPHS: [zgui_text::ShapedGlyph; 1] = [zgui_text::ShapedGlyph {
    glyph: 5,
    x: 0.0,
    y: 12.0,
}];

/// A source that draws one glyph for whatever it is asked about, and records what that was.
#[derive(Default)]
struct Asked {
    /// Every `(paragraph, line)` the emitter asked for, in order.
    asked: RefCell<Vec<(u32, u16)>>,
}

impl GlyphSource for Asked {
    fn visit_line(
        &self,
        paragraph: ParagraphId,
        line: u16,
        visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
    ) {
        self.asked.borrow_mut().push((paragraph.0, line));
        visit(zgui_text::ShapedRun {
            face: zgui_text::FaceId(0),
            size: 16.0,
            coords: &[],
            synthetic_bold: 0.0,
            synthetic_slant: 0.0,
            brush: zgui_scene::PaintSlot(0),
            glyphs: &GLYPHS,
            text_start: 0,
        });
    }
}

/// A line box sixty-four pixels wide at the origin.
fn line() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(64.0), DevicePx(16.0)),
    )
}

/// A placement for that line, cut where `ellipsis` says.
fn placement(ellipsis: Option<EllipsisPaint>) -> TextPlacement {
    TextPlacement {
        line: line(),
        clip: ClipId::ROOT,
        transform: SpatialId::VIEWPORT,
        opaque_target: true,
        subpixel_capable: false,
        scale: 1.0,
        ellipsis,
    }
}

/// A cut forty pixels along, marked by paragraph seven.
fn cut() -> EllipsisPaint {
    EllipsisPaint {
        paragraph: ParagraphId(7),
        cutoff: 40.0,
        width: 8.0,
        at_start: false,
    }
}

/// Records one line drawn through `placement` into a compositor, and reports what the source was
/// asked for.
fn record(scene: &mut Scene, placement: TextPlacement) -> (Lone, Vec<(u32, u16)>) {
    let style = zgui_paint::lower(&StyleDraft::initial().build(), 1.0);
    let source = Asked::default();
    let lone = Lone::record(scene, (placement.transform, ClipId::ROOT), |rec| {
        text::emit(
            rec,
            &source,
            ParagraphId(3),
            0,
            &style,
            text::Inherited::default(),
            placement,
        );
    });
    let asked = source.asked.borrow().clone();
    (lone, asked)
}

/// What a line drawn under `ellipsis` draws, and what the source was asked for.
fn draw(ellipsis: Option<EllipsisPaint>) -> (Drawing, Vec<(u32, u16)>) {
    let mut scene = Scene::new();
    let (mut lone, asked) = record(&mut scene, placement(ellipsis));
    (lone.redraw(), asked)
}

/// The glyph sprites drawn through a clip, and the ones drawn through none.
fn split(drawing: &Drawing) -> (Vec<&Instance>, Vec<&Instance>) {
    drawing
        .glyphs()
        .into_iter()
        .partition(|glyph| glyph.field("clip").is_some())
}

/// A line that was not cut draws its own glyphs and nothing else, under its own clip.
///
/// The control. Without it every assertion below would hold just as well against an emitter that
/// drew an ellipsis over every line in the document.
#[test]
fn an_uncut_line_draws_only_itself() {
    let (drawing, asked) = draw(None);
    assert_eq!(asked, [(3, 0)]);
    let (clipped, free) = split(&drawing);
    assert!(clipped.is_empty(), "{drawing}");
    assert_eq!(free.len(), 1, "{drawing}");
}

/// A cut line draws its own glyphs *and* the mark, and the mark is a different paragraph.
#[test]
fn a_cut_line_draws_the_mark_beside_its_own_glyphs() {
    let (drawing, asked) = draw(Some(cut()));
    assert_eq!(
        asked,
        [(3, 0), (7, 0)],
        "the line's own paragraph, then the mark's",
    );
    assert_eq!(drawing.glyphs().len(), 2, "{drawing}");
}

/// The line's own glyphs are clipped to what survived the cut; the mark is not.
///
/// This is the whole mechanism. A glyph's ink may reach past its own advance, so the cut cannot be
/// made by leaving glyphs out — and the mark itself must not be cut, or the thing that says content
/// was hidden would be hidden too.
#[test]
fn the_cut_clips_the_line_and_not_the_mark() {
    let (drawing, _) = draw(Some(cut()));
    let (clipped, free) = split(&drawing);
    assert_eq!(free.len(), 1, "the mark is drawn uncut: {drawing}");
    let [line] = clipped.as_slice() else {
        panic!("the line's own glyph is cut short: {drawing}");
    };
    assert_eq!(
        line.field("clip"),
        Some("[rect(0, 0, 40, 16)]"),
        "the cut is where the clip ends"
    );
}

/// A coordinate system `by` device pixels to the right of the one it hangs off.
///
/// What a dialog is: laid out in one place and drawn in another, so everything inside it measures
/// its rectangles in the panel's own system while the device sees them through the panel's matrix.
fn panel(scene: &mut Scene, by: f32) -> SpatialId {
    let viewport = scene.spatial.viewport();
    let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    scene.spatial.space_of(
        viewport,
        owner,
        OwnSpace::of(Some(Matrix4::translation(by, 0.0, 0.0)), None, false),
    )
}

/// A line cut short inside a transformed panel is cut where the line is *drawn*.
///
/// The report this stands against: a revealed secret too long for its cell is drawn as a slice of
/// its middle, pushed against the right-hand edge of the cell, with the rest of the cell empty —
/// and only inside a dialog. The cut states its window in the coordinates the line was laid out
/// in, and the window is drawn through the panel's matrix like the glyphs.
#[test]
fn a_cut_line_in_a_transformed_panel_is_cut_where_it_is_drawn() {
    let mut scene = Scene::new();
    let panel = panel(&mut scene, 120.0);
    let (mut lone, _) = record(
        &mut scene,
        TextPlacement {
            transform: panel,
            ..placement(Some(cut()))
        },
    );

    let drawing = lone.redraw();
    let (clipped, _) = split(&drawing);
    assert_eq!(
        clipped[0].field("clip"),
        Some("[rect(0, 0, 40, 16) in=translate[120, 0]]"),
        "the cut window is placed through the panel's matrix: {drawing}",
    );
}

/// The cut window travels with a moved line.
///
/// A scrolled line keeps its chunk and moves it by an offset for as long as nothing else about it
/// changes. The window has to arrive at the same place the glyphs do: left behind, it cuts the
/// line against the position it was recorded at — the glyphs shrink away as the scroll runs, and
/// past one line height only the mark is left.
#[test]
fn the_cut_window_travels_with_a_moved_line() {
    let mut scene = Scene::new();
    let (mut lone, _) = record(&mut scene, placement(Some(cut())));
    lone.offset([0.0, 24.0]);

    let drawing = lone.redraw();
    let (clipped, free) = split(&drawing);
    assert_eq!(free.len(), 1, "the mark survives: {drawing}");
    let [line] = clipped.as_slice() else {
        panic!("the line's glyph survives: {drawing}");
    };
    assert_eq!(
        line.field("clip"),
        Some("[rect(0, 24, 40, 16)]"),
        "the cut moved with the line"
    );
    assert_eq!(
        line.bounds()[1] + line.bounds()[3],
        36.0,
        "and the glyph is drawn where the line is now, on the baseline 12 below its top"
    );
}

/// A cut at the line's start hides everything before it, and the mark survives.
///
/// The fixture's one glyph sits at the line's own origin, which is on the hidden side of a cut
/// forty pixels along. The glyph is drawn only through a window that starts at the cut, and the
/// mark is drawn uncut.
#[test]
fn a_cut_at_the_start_hides_the_beginning_of_the_line() {
    let (drawing, asked) = draw(Some(EllipsisPaint {
        at_start: true,
        ..cut()
    }));
    assert_eq!(asked, [(3, 0), (7, 0)], "both were visited");
    let (clipped, free) = split(&drawing);
    assert_eq!(free.len(), 1, "the mark is never cut: {drawing}");
    for glyph in clipped {
        assert_eq!(
            glyph.field("clip"),
            Some("[rect(40, 0, 24, 16)]"),
            "the glyph before the cut is drawn only past the cut"
        );
    }
}
