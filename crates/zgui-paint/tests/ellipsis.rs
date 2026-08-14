//! What a line cut short by `text-overflow` puts into the scene.

use std::cell::RefCell;

use zgui_atlas::{AtlasTile, TextureId, TextureKind, TileId};
use zgui_css::StyleDraft;
use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};
use zgui_layout::fragment::ParagraphId;
use zgui_paint::emit::text::{self, EllipsisPaint, TextPlacement};
use zgui_paint::{GlyphRun, GlyphSource, PlacedGlyph};
use zgui_scene::{ClipId, OwnSpace, PropertyOwner, Scene, SpatialId};
use zgui_text::GlyphFormat;

/// A source that draws one glyph for whatever it is asked about, and records what that was.
#[derive(Default)]
struct Recorder {
    /// Every `(paragraph, line)` the emitter asked for, in order.
    asked: RefCell<Vec<(u32, u16)>>,
}

impl GlyphSource for Recorder {
    fn visit_line(
        &self,
        paragraph: ParagraphId,
        line: u16,
        _request: zgui_paint::GlyphRequest,
        visit: &mut dyn FnMut(GlyphRun<'_>),
    ) {
        self.asked.borrow_mut().push((paragraph.0, line));
        let glyph = PlacedGlyph {
            resource: AtlasTile {
                texture: TextureId {
                    kind: TextureKind::Mono,
                    index: 0,
                },
                tile: TileId(0),
                bounds: Rect::new(Point::new(0, 0), Size::new(8, 16)),
            }
            .into(),
            bounds: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(8.0), DevicePx(16.0)),
            ),
        };
        visit(GlyphRun {
            content: zgui_paint::RunContent::Tiles(core::slice::from_ref(&glyph)),
            format: GlyphFormat::Mono,
            paint: zgui_scene::PaintSlot(0),
            synthetic_bold: 0.0,
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
        upright: true,
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

/// Emits one line into `scene`, and reports what the source was asked for.
fn emit_into(scene: &mut Scene, placement: TextPlacement) -> Vec<(u32, u16)> {
    let style = zgui_paint::lower(&StyleDraft::initial().build(), 1.0);
    let source = Recorder::default();
    text::emit(
        scene,
        &source,
        ParagraphId(3),
        0,
        &style,
        text::Inherited::default(),
        placement,
    );
    source.asked.borrow().clone()
}

/// The scene a line is emitted into, and what the source was asked for.
fn emit(ellipsis: Option<EllipsisPaint>) -> (Scene, Vec<(u32, u16)>) {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(256, 64));
    let asked = emit_into(&mut scene, placement(ellipsis));
    (scene, asked)
}

/// A line that was not cut draws its own glyphs and nothing else, under its own clip.
///
/// The control. Without it every assertion below would hold just as well against an emitter that
/// drew an ellipsis over every line in the document.
#[test]
fn an_uncut_line_draws_only_itself() {
    let (scene, asked) = emit(None);
    assert_eq!(asked, [(3, 0)]);
    assert_eq!(scene.primitives.mono_sprites.len(), 1);
    assert_eq!(scene.primitives.mono_sprites[0].clip, ClipId::ROOT.0);
}

/// A cut line draws its own glyphs *and* the mark, and the mark is a different paragraph.
#[test]
fn a_cut_line_draws_the_mark_beside_its_own_glyphs() {
    let (scene, asked) = emit(Some(cut()));
    assert_eq!(
        asked,
        [(3, 0), (7, 0)],
        "the line's own paragraph, then the mark's",
    );
    assert_eq!(scene.primitives.mono_sprites.len(), 2);
}

/// The window the line's own glyphs were cut to, and the assertion that the mark was not cut.
///
/// Read by clip rather than by position in the primitive list: two sprites drawn over one another
/// are ordered by the scene, not by the order they were pushed, and a test that assumed otherwise
/// would be asserting about whichever one it happened to pick.
fn window(scene: &Scene) -> (f32, f32) {
    let clips: Vec<u32> = scene
        .primitives
        .mono_sprites
        .iter()
        .map(|sprite| sprite.clip)
        .collect();
    assert_eq!(clips.len(), 2);
    assert_eq!(
        clips.iter().filter(|clip| **clip == ClipId::ROOT.0).count(),
        1,
        "the mark is drawn uncut and the line's own glyphs are not",
    );
    let cut = clips
        .into_iter()
        .find(|clip| *clip != ClipId::ROOT.0)
        .expect("one sprite is cut short");
    let resolved = scene.clips.resolve(zgui_scene::ClipId(cut));
    (resolved.left(), resolved.right())
}

/// The line's own glyphs are clipped to what survived the cut; the mark is not.
///
/// This is the whole mechanism. A glyph's ink may reach past its own advance, so the cut cannot be
/// made by leaving glyphs out — and the mark itself must not be cut, or the thing that says content
/// was hidden would be hidden too.
#[test]
fn the_cut_clips_the_line_and_not_the_mark() {
    let (scene, _) = emit(Some(cut()));
    assert_eq!(
        window(&scene),
        (0.0, 40.0),
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

/// The clip a cut line's own glyphs are drawn through, placed on the device.
///
/// Each link put where the frame's matrices say its coordinate system now is, which is what the
/// shader does to it. A link that names the wrong system resolves somewhere the glyphs are not.
fn placed_window(scene: &Scene) -> (f32, f32) {
    let cut = scene
        .primitives
        .mono_sprites
        .iter()
        .map(|sprite| sprite.clip)
        .find(|clip| *clip != ClipId::ROOT.0)
        .expect("one sprite is cut short");
    let placed = scene
        .clips
        .bounds_placed(zgui_scene::ClipId(cut), &|id| scene.spatial.resolve(id));
    (placed.left().0, placed.right().0)
}

/// A line cut short inside a transformed panel is cut where the line is *drawn*.
///
/// The report this stands against: a revealed secret too long for its cell is drawn as a slice of
/// its middle, pushed against the right-hand edge of the cell, with the rest of the cell empty —
/// and only inside a dialog. The cut states its window in the coordinates the line was laid out
/// in; naming the viewport as the system those coordinates belong to puts the window wherever the
/// panel is *not*, and what is left is whatever the two rectangles happen to overlap in.
#[test]
fn a_cut_line_in_a_transformed_panel_is_cut_where_it_is_drawn() {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(256, 64));
    let panel = panel(&mut scene, 120.0);
    emit_into(
        &mut scene,
        TextPlacement {
            transform: panel,
            ..placement(Some(cut()))
        },
    );

    assert_eq!(
        placed_window(&scene),
        (120.0, 160.0),
        "the cut window landed where the line was laid out rather than where it is drawn",
    );
}

/// A cut at the line's start hides everything before it, and the mark survives.
///
/// The fixture's one glyph sits at the line's own origin, which is on the hidden side of a cut
/// forty pixels along — so it is not drawn at all, and what is left is the mark saying so. That is
/// the property this side of the cut has, stated as what reaches the scene rather than as a
/// rectangle: a clip that hid nothing would leave two sprites here.
#[test]
fn a_cut_at_the_start_hides_the_beginning_of_the_line() {
    let (scene, asked) = emit(Some(EllipsisPaint {
        at_start: true,
        ..cut()
    }));
    assert_eq!(asked, [(3, 0), (7, 0)], "both were visited");
    assert_eq!(
        scene.primitives.mono_sprites.len(),
        1,
        "the glyph before the cut was not drawn",
    );
    assert_eq!(
        scene.primitives.mono_sprites[0].clip,
        ClipId::ROOT.0,
        "and what survived is the mark, which is never cut",
    );
}
