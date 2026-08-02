//! What a record decides, and what it owns while it stands.

use zgui_geom::{DevicePx, Point, Rect, Size};
use zgui_layout::fragment::ParagraphId;
use zgui_layout::{Fragment, FragmentKind};
use zgui_scene::{ClipId, Scene, SpatialId};

use crate::lower::cache::PaintStyleRef;
use crate::walk::replay::hold::NoResources;
use crate::walk::replay::{Encoding, PaintCache, Reuse};

/// A minted key, for a test that needs a name and not a stored value.
fn key<T>(index: u32) -> zgui_arena::Key<T> {
    zgui_arena::Key::new(
        index,
        zgui_arena::Generation::new(1).expect("one is a generation"),
        zgui_arena::DomainId::FIRST,
    )
}

/// A fragment at the given origin, sixty-four by twenty-four, painting exactly its own box.
fn fragment(x: f32, y: f32) -> Fragment {
    let mut fragment = Fragment::new(key(1), key(1), FragmentKind::Box);
    fragment.border_box = Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(64.0), DevicePx(24.0)),
    );
    fragment.ink = fragment.border_box;
    // The build always fills both readings; a fixture that left the local one at zero would slip
    // past the cut-range check for any position at all.
    fragment.local_ink = fragment.border_box;
    fragment
}

/// The painting these fixtures record: the initial style, no clip, no transform, no decoration.
fn painted(style: u32) -> crate::walk::replay::Painted {
    crate::walk::replay::Painted {
        style: PaintStyleRef::new(0, style),
        clip: ClipId::ROOT,
        transform: SpatialId::VIEWPORT,
        // What the viewport's own coordinate system resolves to, which is what a fragment drawn
        // straight into the window carries.
        transform_hash: zgui_scene::Content::content_hash(&zgui_geom::Matrix4::IDENTITY),
        decorations: 0,
        text_fill: 0,
        anim: 0,
        alpha: 1.0f32.to_bits(),
        highlights: 0,
    }
}

/// A scene with one frame's worth of retained operations.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(256, 256));
    scene
}

#[test]
fn a_fragment_with_no_record_is_encoded() {
    let cache = PaintCache::new();
    let scene = scene();
    assert_eq!(
        cache.reuse(&scene, &fragment(0.0, 0.0), painted(0),),
        Reuse::Encode
    );
}

#[test]
fn a_moved_fragment_replays_with_the_distance_it_moved() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let first = fragment(0.0, 0.0);
    cache.encoded(
        &scene,
        &first,
        painted(0),
        Encoding {
            ops: 0..0,
            whole: true,
            resources: &[],
        },
        &NoResources,
    );
    // A second frame, so the range recorded is in the retained log rather than this frame's.
    scene.begin_frame(Size::new(256, 256));
    let moved = fragment(0.0, -120.0);
    assert_eq!(
        cache.reuse(&scene, &moved, painted(0),),
        Reuse::Replay(Size::new(DevicePx(0.0), DevicePx(-120.0)))
    );
}

#[test]
fn a_fragment_under_a_changed_folded_alpha_is_encoded_again() {
    // An ancestor's opacity is folded into a descendant's colours rather than composited
    // through a target whenever their ink is disjoint, and *nothing else in this record moves*
    // when it does: the descendant's own style, clip, transform and animation are all exactly
    // what they were. Without the alpha here a panel fading out is a panel whose contents never
    // fade, replayed at the alpha they had when the fade began.
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let same = fragment(0.0, 0.0);
    cache.encoded(
        &scene,
        &same,
        painted(0),
        Encoding {
            ops: 0..0,
            whole: true,
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    let dimmed = crate::walk::replay::Painted {
        alpha: 0.5f32.to_bits(),
        ..painted(0)
    };
    assert_eq!(cache.reuse(&scene, &same, dimmed), Reuse::Encode);
    // And the same alpha still replays, or every fragment of every document is encoded twice.
    assert_ne!(cache.reuse(&scene, &same, painted(0)), Reuse::Encode);
}

#[test]
fn a_restyled_fragment_is_encoded_however_still_it_stayed() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let same = fragment(0.0, 0.0);
    cache.encoded(
        &scene,
        &same,
        painted(0),
        Encoding {
            ops: 0..0,
            whole: true,
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    assert_eq!(
        cache.reuse(&scene, &same, painted(1),),
        Reuse::Encode,
        "a hover that changes only a colour must not replay last frame's colour"
    );
}

/// A line that is still the same line, holding different characters, is encoded again.
///
/// The name is kept across a change of paragraph on purpose — destroying it would unregister a
/// hit entry and force the painting order to be derived again for the whole document — so the
/// record is what has to notice. Nothing else can: a digit replaced by one of the same width
/// leaves the style, the chain, the transform and the size exactly as they were.
#[test]
fn a_line_whose_paragraph_changed_is_encoded_however_still_it_stayed() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let mut line = fragment(0.0, 0.0);
    line.kind = FragmentKind::Line {
        paragraph: ParagraphId(0),
        line: 0,
    };
    cache.encoded(
        &scene,
        &line,
        painted(0),
        Encoding {
            ops: 0..0,
            whole: true,
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    line.kind = FragmentKind::Line {
        paragraph: ParagraphId(1),
        line: 0,
    };
    assert_eq!(
        cache.reuse(&scene, &line, painted(0),),
        Reuse::Encode,
        "one character changed for one of the same width and the previous glyphs would have \
         been replayed"
    );
}

#[test]
fn a_resized_fragment_is_encoded_rather_than_stretched() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let before = fragment(0.0, 0.0);
    cache.encoded(
        &scene,
        &before,
        painted(0),
        Encoding {
            ops: 0..0,
            whole: true,
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    let mut wider = fragment(0.0, 0.0);
    wider.border_box.size = Size::new(DevicePx(128.0), DevicePx(24.0));
    assert_eq!(cache.reuse(&scene, &wider, painted(0),), Reuse::Encode);
}

/// A drawing paints one thing — a vector item — and a vector item is not in the operation log,
/// so there is nothing for a replay to re-emit. Replaying one is a fragment that draws nothing
/// into pixels the damage has already cleared.
#[test]
fn a_drawing_is_encoded_however_still_it_stayed() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let mut mark = fragment(0.0, 0.0);
    mark.kind = FragmentKind::Vector;
    cache.encoded(
        &scene,
        &mark,
        painted(0),
        Encoding {
            ops: 0..0,
            whole: true,
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    assert_eq!(
        cache.reuse(&scene, &mark, painted(0)),
        Reuse::Encode,
        "a replayed drawing emits no vector item and the icon vanishes"
    );
}

/// A fragment whose painting the clip cut short is encoded again wherever it moves to.
///
/// This is the whole of a panel that scrolls into view from below. While it is outside the
/// scroll port every primitive it offers is refused, so what it records is an empty range; the
/// style, the clip, the transform and the size are all exactly what they were when it arrives
/// inside the port, so without this the empty range is replayed and the panel never appears.
#[test]
fn a_painting_the_clip_cut_short_is_encoded_again_where_the_fragment_paints() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    // The port is re-entered on the second frame as a real walk re-enters it, because the clip
    // tables are per-frame: a chain nobody pushed this frame resolves to nothing, and a
    // fragment tested against nothing looks like a fragment that paints nowhere.
    let port = |scene: &mut Scene| crate::walk::replay::Painted {
        clip: scene.clips.only(zgui_scene::ClipLink::rect(Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(256.0), DevicePx(128.0)),
        ))),
        ..painted(0)
    };
    let inside = port(&mut scene);
    let hidden = fragment(0.0, 2000.0);
    cache.encoded(
        &scene,
        &hidden,
        inside,
        Encoding {
            ops: 0..0,
            whole: false,
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    let inside = port(&mut scene);
    let arrived = fragment(0.0, 100.0);
    assert_eq!(
        cache.reuse(&scene, &arrived, inside),
        Reuse::Encode,
        "the range was recorded outside the port and holds none of what the row paints in it"
    );
    // Still outside it, the same empty range is the whole of what the row paints there, so it
    // stands: a list far longer than its port must not re-encode every row of it every frame.
    assert_ne!(
        cache.reuse(&scene, &fragment(0.0, 1900.0), inside),
        Reuse::Encode
    );
}

#[test]
fn a_fragment_nobody_visited_loses_its_record() {
    let mut cache = PaintCache::new();
    let scene = scene();
    cache.encoded(
        &scene,
        &fragment(0.0, 0.0),
        painted(0),
        Encoding {
            ops: 0..0,
            whole: true,
            resources: &[],
        },
        &NoResources,
    );
    assert_eq!(cache.len(), 1);
    cache.begin_frame();
    cache.end_frame(&NoResources);
    assert_eq!(
        cache.len(),
        0,
        "a record for a fragment that is gone would be replayed"
    );
}

/// A coordinate system whose slot has been handed to an unrelated box is not the one a record was
/// taken under, and everything cached under it has to be encoded again.
///
/// This is the one failure a structural name introduces that an interned one could not have. An
/// interned identifier was safe under an equality test because its identity *was* its value; a slot
/// is a place, the places come back, and the box handed one next is not the box the record was
/// written for. Every other field of the record is what it was — the fixture gives the stranger the
/// *same matrix*, so even the fingerprint of what the name resolves to agrees — and the counter in
/// the name is the whole of the difference.
///
/// What it costs to get this wrong is a fragment replayed through a matrix belonging to somebody
/// else, with content that looks right and geometry that looks right.
#[test]
fn a_recycled_spatial_slot_reencodes_the_chunks_that_named_it() {
    use zgui_geom::Matrix4;
    use zgui_scene::{Content, OwnSpace, PropertyOwner, SpatialId};

    let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
    let moved = Matrix4::translation(10.0, 0.0, 0.0);
    let own = OwnSpace::of(Some(moved), None, false);
    let under = |space: SpatialId| crate::walk::replay::Painted {
        transform: space,
        transform_hash: moved.content_hash(),
        ..painted(0)
    };

    let mut cache = PaintCache::new();
    let mut scene = scene();
    let viewport = scene.spatial.viewport();
    let card = owner(2);
    let space = scene.spatial.space_of(viewport, card, own);

    // A fragment inside the card, drawn in the card's coordinate system rather than one of its own.
    // It survives the card: what goes away is the box above it.
    let label = fragment(0.0, 0.0);
    cache.encoded(
        &scene,
        &label,
        under(space),
        Encoding {
            ops: 0..0,
            whole: true,
            resources: &[],
        },
        &NoResources,
    );

    scene.begin_frame(Size::new(256, 256));
    scene.spatial.release(card);
    scene.spatial.recycle();
    let stranger = scene.spatial.space_of(viewport, owner(3), own);
    assert_eq!(
        stranger.index(),
        space.index(),
        "the slot came back, which is the premise of the whole case",
    );
    assert_ne!(stranger, space);
    assert_eq!(
        under(stranger).transform_hash,
        under(space).transform_hash,
        "the stranger is at the same place, so nothing but the name itself has moved",
    );

    assert_eq!(
        cache.reuse(&scene, &label, under(stranger)),
        Reuse::Encode,
        "a range recorded under the card was replayed through a slot the card no longer owns",
    );
}
