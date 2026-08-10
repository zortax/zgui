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
        custom: 0,
        content: 0,
        scale: 1.0f32.to_bits(),
        decorations: 0,
        text_fill: 0,
        anim: 0,
        alpha: 1.0f32.to_bits(),
        highlights: 0,
    }
}

#[test]
fn a_fragment_whose_outside_content_moved_is_encoded_however_still_it_stayed() {
    // A replaced fragment names its node and a drawing names its curves' source, and both names
    // stay put while what they resolve to changes. The revision is the only field that moves.
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let same = fragment(0.0, 0.0);
    cache.encoded(
        &mut scene,
        &same,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    let swapped = crate::walk::replay::Painted {
        content: 1,
        ..painted(0)
    };
    assert_eq!(cache.reuse(&scene, &same, swapped), Reuse::Encode);
    assert_ne!(cache.reuse(&scene, &same, painted(0)), Reuse::Encode);
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
        &mut scene,
        &first,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
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

/// The chunk's bytes are at the position the fragment was encoded at, so a movement's offsets
/// accumulate against that origin. Measuring each replay from the one before it — which is what
/// updating the record's border box on replay would do — translates encode-time bytes by one step
/// of a movement that has taken several, and the fragment is drawn where it was two frames ago.
#[test]
fn a_fragment_moved_twice_replays_with_the_whole_distance() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let first = fragment(0.0, 0.0);
    cache.encoded(
        &mut scene,
        &first,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    let step_one = fragment(0.0, 10.0);
    assert_eq!(
        cache.reuse(&scene, &step_one, painted(0)),
        Reuse::Replay(Size::new(DevicePx(0.0), DevicePx(10.0)))
    );
    cache.replayed(&step_one);

    scene.begin_frame(Size::new(256, 256));
    let step_two = fragment(0.0, 20.0);
    assert_eq!(
        cache.reuse(&scene, &step_two, painted(0)),
        Reuse::Replay(Size::new(DevicePx(0.0), DevicePx(20.0))),
        "the offset is measured from the encoding, so two steps of ten accumulate to twenty"
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
        &mut scene,
        &same,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
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
        &mut scene,
        &same,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
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
        &mut scene,
        &line,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
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
        &mut scene,
        &before,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    let mut wider = fragment(0.0, 0.0);
    wider.border_box.size = Size::new(DevicePx(128.0), DevicePx(24.0));
    assert_eq!(cache.reuse(&scene, &wider, painted(0),), Reuse::Encode);
}

/// A drawing that stayed where it was replays, and the replay re-emits its vector item into the
/// frame's pass planning. A drawing that moved is encoded: its curves are placed in device
/// coordinates and shared by pointer with the rasteriser's encoding cache, so translating them
/// would mean copying the path.
#[test]
fn a_still_drawing_replays_its_vector_item_and_a_moved_one_is_encoded() {
    use std::sync::Arc;

    let mut cache = PaintCache::new();
    let mut scene = scene();
    let mut mark = fragment(0.0, 0.0);
    mark.kind = FragmentKind::Vector;
    let mut path = zgui_scene::kurbo::BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((10.0, 10.0));
    path.line_to((0.0, 10.0));
    path.close_path();
    let item = zgui_scene::VectorItem::filled(
        zgui_scene::VectorId(1),
        Arc::new(path),
        zgui_scene::PaintRef::NONE,
    );
    scene.begin_chunk_capture(cache.take_capture_scratch());
    scene.push_vector(item);
    let chunk = scene.take_chunk_capture();
    assert_eq!(chunk.vectors.len(), 1);
    cache.encoded(
        &mut scene,
        &mark,
        painted(0),
        Encoding {
            chunk,
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));

    let reuse = cache.reuse(&scene, &mark, painted(0));
    assert_eq!(
        reuse,
        Reuse::Replay(Size::new(DevicePx(0.0), DevicePx(0.0))),
        "a still drawing replays"
    );
    let (source, chunk) = cache.chunk(mark.key).expect("recorded");
    let range = scene.replay_chunk(chunk, Size::default(), source);
    assert_eq!(range.len(), 1, "and the replay re-emits the vector item");
    assert_eq!(scene.primitives.vectors.len(), 1);

    let mut moved = fragment(0.0, 40.0);
    moved.kind = FragmentKind::Vector;
    assert_eq!(
        cache.reuse(&scene, &moved, painted(0)),
        Reuse::Encode,
        "a moved drawing is encoded at its new position"
    );
}

/// A chunk captured beyond the clip replays complete on arrival.
///
/// This is the whole of a panel that scrolls into view from below. While it is outside the
/// scroll port every primitive it offers is refused by the frame — and captured anyway, because
/// the capture happens before the cull. When the fragment arrives inside the port, the same
/// chunk replays and the cull admits the part that has come into view.
#[test]
fn a_chunk_captured_beyond_the_clip_replays_complete_on_arrival() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let port = |scene: &mut Scene| crate::walk::replay::Painted {
        clip: scene.clips.only(zgui_scene::ClipLink::rect(Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(256.0), DevicePx(128.0)),
        ))),
        ..painted(0)
    };
    let inside = port(&mut scene);
    let hidden = fragment(0.0, 2000.0);
    // The fragment paints one quad at its own position, refused by the port's clip down there.
    scene.begin_chunk_capture(cache.take_capture_scratch());
    let refused = scene.push_quad(
        zgui_scene::Quad::filled(hidden.border_box, zgui_scene::PaintRef::NONE)
            .clipped(inside.clip),
    );
    let chunk = scene.take_chunk_capture();
    assert!(refused.is_none(), "the quad is outside the port");
    assert_eq!(chunk.quads.len(), 1, "and captured regardless");
    cache.encoded(
        &mut scene,
        &hidden,
        inside,
        Encoding {
            chunk,
            resources: &[],
        },
        &NoResources,
    );
    scene.begin_frame(Size::new(256, 256));
    let inside = port(&mut scene);
    let arrived = fragment(0.0, 100.0);
    let reuse = cache.reuse(&scene, &arrived, inside);
    assert_ne!(
        reuse,
        Reuse::Encode,
        "the chunk is the fragment's complete painting, so arriving at the port replays it"
    );
    let Reuse::Replay(offset) = reuse else {
        unreachable!()
    };
    let (source, chunk) = cache.chunk(arrived.key).expect("recorded");
    let range = scene.replay_chunk(chunk, offset, source);
    assert_eq!(range.len(), 1, "the cull admits the arrived quad");
    assert_eq!(
        scene.primitives.quads[0].bounds[1], 100.0,
        "at the position the fragment arrived at"
    );
}

#[test]
fn a_fragment_nobody_visited_keeps_its_record_and_replays_on_return() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let away = fragment(0.0, 0.0);
    cache.encoded(
        &mut scene,
        &away,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
            resources: &[],
        },
        &NoResources,
    );
    // Many frames pass in which nothing visits the fragment — scrolled out, culled, or simply
    // outside every damage rectangle. The record owns its primitives, so it stands.
    for _ in 0..3 {
        cache.begin_frame();
        cache.end_frame();
        scene.begin_frame(Size::new(256, 256));
    }
    assert_eq!(cache.len(), 1);
    assert_ne!(
        cache.reuse(&scene, &away, painted(0)),
        Reuse::Encode,
        "a record kept across unvisited frames replays on return"
    );
}

#[test]
fn a_retired_fragment_loses_its_record() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let gone = fragment(0.0, 0.0);
    cache.encoded(
        &mut scene,
        &gone,
        painted(0),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
            resources: &[],
        },
        &NoResources,
    );
    assert_eq!(cache.len(), 1);
    cache.retire(&[gone.key], &mut scene, &NoResources);
    assert_eq!(cache.len(), 0);
    // Retiring a name with no record is the ordinary case and costs nothing.
    cache.retire(&[gone.key], &mut scene, &NoResources);
    assert_eq!(cache.reuse(&scene, &gone, painted(0)), Reuse::Encode);
}

/// Pushes one quad through an interned paint and a real clip, and encodes it into a record.
fn encode_one_quad(
    cache: &mut PaintCache,
    scene: &mut Scene,
    fragment: &Fragment,
) -> (ClipId, zgui_scene::PaintId) {
    use zgui_geom::Point;
    let rect = zgui_geom::Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(10.0), DevicePx(10.0)),
    );
    let clip = scene.clips.only(zgui_scene::ClipLink::rect(rect));
    let paint = scene
        .paints
        .solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0));
    scene.begin_chunk_capture(cache.take_capture_scratch());
    scene.push_quad(
        zgui_scene::Quad::filled(rect, zgui_scene::PaintRef::solid(paint)).clipped(clip),
    );
    let chunk = scene.take_chunk_capture();
    cache.encoded(
        scene,
        fragment,
        painted(0),
        Encoding {
            chunk,
            resources: &[],
        },
        &NoResources,
    );
    (clip, paint)
}

#[test]
fn a_record_holds_its_table_entries_until_it_is_dropped() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let one = fragment(0.0, 0.0);
    let (clip, paint) = encode_one_quad(&mut cache, &mut scene, &one);

    assert_eq!(scene.clips.refs(clip), Some(1));
    assert_eq!(scene.paints.refs(paint), Some(1));
    assert!(cache.bytes() > 0);

    // Re-encoding the same fragment retains before it releases, so the counts hold steady.
    encode_one_quad(&mut cache, &mut scene, &one);
    assert_eq!(scene.clips.refs(clip), Some(1));
    assert_eq!(scene.paints.refs(paint), Some(1));

    cache.retire(&[one.key], &mut scene, &NoResources);
    assert_eq!(scene.clips.refs(clip), Some(0));
    assert_eq!(scene.paints.refs(paint), Some(0));
    assert_eq!(cache.bytes(), 0);
}

#[test]
fn eviction_takes_only_cold_records_and_is_a_clean_miss() {
    let mut cache = PaintCache::new();
    let mut scene = scene();
    let one = fragment(0.0, 0.0);
    let (clip, paint) = encode_one_quad(&mut cache, &mut scene, &one);

    // Selected this frame: the working set is never taken, however much was asked for.
    assert_eq!(cache.evict_cold(u64::MAX, &mut scene, &NoResources), 0);
    assert_eq!(cache.len(), 1);

    // A frame later it is cold, and eviction takes it and releases what it held.
    cache.begin_frame();
    let freed = cache.evict_cold(u64::MAX, &mut scene, &NoResources);
    assert!(freed > 0);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bytes(), 0);
    assert_eq!(scene.clips.refs(clip), Some(0));
    assert_eq!(scene.paints.refs(paint), Some(0));
    // The miss is clean: the next visit encodes again, and nothing else remembers the record.
    assert_eq!(cache.reuse(&scene, &one, painted(0)), Reuse::Encode);
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
        &mut scene,
        &label,
        under(space),
        Encoding {
            chunk: zgui_scene::ChunkPrims::default(),
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
