//! The recorder emitters write a fragment's painting into, as compositor items.
//!
//! Items refer to compositor nodes and slots. The recorder turns zgui's clip chains, coordinate
//! systems and paints into them as the emitters name them, so a fragment's items are complete
//! once its emitters return.

mod items;

pub(crate) use self::items::{Items, Ref};

use compositor_types::{
    ClipRef, ClipShape, Glyph, GlyphRun, Line, LocalClip, LocalParent, PaintRef, Quad, Shadow,
    SpaceRef,
};
use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Rect, Vec2};
use zgui_scene::{ClipId, ClipLink, Scene, SpatialId};

use crate::sync::Resolver;

/// One chunk's items recorded without a compositor, for reading back.
///
/// Nothing refers to a node or a slot. Items name the chunk's default space and clip, colours are
/// carried in their references, and every other paint is none.
#[derive(Debug, Default)]
pub struct Recording {
    items: Items,
}

impl Recording {
    /// Creates an empty recording.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a recorder that empties the recording and records into it, with `space` and `clip`
    /// as the chunk's defaults.
    pub fn recorder<'a>(
        &'a mut self,
        scene: &'a mut Scene,
        space: SpatialId,
        clip: ClipId,
    ) -> Recorder<'a> {
        Recorder::new(scene, &mut self.items, None, space, clip)
    }

    /// Returns the quads, in record order.
    pub fn quads(&self) -> &[Quad] {
        &self.items.quads
    }

    /// Returns the shadows, in record order.
    pub fn shadows(&self) -> &[Shadow] {
        &self.items.shadows
    }

    /// Returns the decoration lines, in record order.
    pub fn lines(&self) -> &[Line] {
        &self.items.lines
    }

    /// Returns the glyph runs, in record order.
    pub fn glyph_runs(&self) -> &[GlyphRun] {
        &self.items.glyph_runs
    }

    /// Returns the glyphs of `run`.
    pub fn glyphs(&self, run: &GlyphRun) -> &[Glyph] {
        &self.items.glyphs[run.glyphs.indices()]
    }
}

/// Where emitters record one chunk's items, and the scene whose tables they read.
///
/// The chunk has a default coordinate system and a default clip chain, which are the fragment's
/// own. An item drawn in them refers to them as `INHERIT`.
///
/// Without a compositor, the items are built and nothing refers to a node or a slot.
pub struct Recorder<'a> {
    scene: &'a mut Scene,
    items: &'a mut Items,
    resolver: Option<Resolver<'a>>,
}

impl<'a> Recorder<'a> {
    /// Creates a recorder that empties `items` and records into them, with `space` and `clip` as
    /// the chunk's defaults.
    pub(crate) fn new(
        scene: &'a mut Scene,
        items: &'a mut Items,
        resolver: Option<Resolver<'a>>,
        space: SpatialId,
        clip: ClipId,
    ) -> Self {
        items.reset(space, clip);
        Self {
            scene,
            items,
            resolver,
        }
    }

    /// Returns the scene, whose tables the emitters read and whose display list receives the
    /// content the recorder does not take.
    pub fn scene(&mut self) -> &mut Scene {
        self.scene
    }

    /// Returns the scene for reading.
    pub fn scene_ref(&self) -> &Scene {
        self.scene
    }

    /// Returns the reference an item drawn in the coordinate system `space` holds.
    pub fn space(&mut self, space: SpatialId) -> SpaceRef {
        self.space_slot(space.index())
    }

    /// Returns the reference an item drawn in the coordinate system in `slot` holds.
    pub(crate) fn space_slot(&mut self, slot: u32) -> SpaceRef {
        if slot == self.items.space.index() {
            return SpaceRef::INHERIT;
        }
        let Some(resolver) = &mut self.resolver else {
            return SpaceRef::INHERIT;
        };
        let id = resolver.space(self.scene, slot);
        refer(self.items, Ref::Space(id));
        SpaceRef(id.slot())
    }

    /// Returns the reference an item drawn through the clip chain `clip` holds.
    ///
    /// The root chain and a chain the compositor cannot express clip nothing.
    pub fn clip(&mut self, clip: ClipId) -> ClipRef {
        if clip == self.items.clip {
            return ClipRef::INHERIT;
        }
        if clip.is_root() {
            return ClipRef::NONE;
        }
        let Some(resolver) = &mut self.resolver else {
            return ClipRef::INHERIT;
        };
        match resolver.clip(self.scene, clip) {
            Some(id) => {
                refer(self.items, Ref::Clip(id));
                ClipRef(id.slot())
            }
            None => ClipRef::NONE,
        }
    }

    /// Adds a clip of `link` inside the chain `parent` that moves with the chunk, and returns
    /// the reference items drawn through it hold.
    ///
    /// The link is measured in the chunk's default coordinate system. A link in another system,
    /// or a mask, is mirrored as part of a clip chain instead, which does not move with the chunk.
    pub fn local_clip(&mut self, parent: ClipId, link: ClipLink) -> ClipRef {
        let ClipLink::RoundedRect {
            rect,
            radii,
            shape,
            space,
        } = link
        else {
            let chain = self.scene.clips.push(parent, link);
            return self.clip(chain);
        };
        if space != self.items.space {
            let chain = self.scene.clips.push(parent, link);
            return self.clip(chain);
        }
        let parent = match self.clip(parent) {
            ClipRef::INHERIT => LocalParent::Chunk,
            ClipRef::NONE => LocalParent::None,
            reference => match reference.local_index() {
                Some(index) => LocalParent::Local(index),
                None => self.node_of(reference),
            },
        };
        let index = self.items.local_clips.len() as u32;
        self.items.local_clips.push(LocalClip {
            parent,
            shape: ClipShape::RoundedRect {
                rect: local_rect(rect),
                radii: flat_radii(radii),
                shapes: [shape.get(); 4],
            },
        });
        ClipRef::local(index)
    }

    /// Returns the reference that paints `color`.
    pub fn color(&self, color: Color) -> PaintRef {
        if color.alpha() == 0.0 {
            return PaintRef::NONE;
        }
        PaintRef::color(rgba8(color.to_premultiplied_srgb()))
    }

    /// Returns the reference that paints the interned paint `paint`.
    ///
    /// A solid colour is carried in the reference. Every other paint refers to a slot.
    pub fn paint(&mut self, paint: zgui_scene::PaintRef) -> PaintRef {
        let Some(id) = paint.id() else {
            return PaintRef::NONE;
        };
        if let Some(zgui_scene::Paint::Solid(color)) = self.scene.paints.get(id) {
            return self.color(*color);
        }
        self.slot(paint)
    }

    /// Returns the reference to the slot of `paint`, whose value a later frame may rewrite.
    ///
    /// An image paint, which the compositor has no slot for, paints nothing.
    pub fn slot(&mut self, paint: zgui_scene::PaintRef) -> PaintRef {
        let Some(id) = paint.id() else {
            return PaintRef::NONE;
        };
        let Some(resolver) = &mut self.resolver else {
            return PaintRef::NONE;
        };
        let Some((slot, kind)) = resolver.paint(self.scene, id) else {
            return PaintRef::NONE;
        };
        refer(self.items, Ref::Paint(kind, slot));
        PaintRef {
            kind,
            _pad: [0; 3],
            index: slot.slot(),
        }
    }

    /// Appends a quad.
    pub fn quad(&mut self, quad: Quad) {
        self.items.push_quad(quad, self.scene.captured());
    }

    /// Appends a shadow.
    pub fn shadow(&mut self, shadow: Shadow) {
        self.items.push_shadow(shadow, self.scene.captured());
    }

    /// Appends a decoration line.
    pub fn line(&mut self, line: Line) {
        self.items.push_line(line, self.scene.captured());
    }

    /// Appends a glyph run of `glyphs`, at the default instance of its font.
    ///
    /// The run's glyph and coordinate ranges are replaced. `instanced` marks a run shaped at a
    /// variation instance, which is drawn at the default instance.
    pub fn glyph_run(&mut self, mut run: GlyphRun, glyphs: &[Glyph], instanced: bool) {
        let start = self.items.glyphs.len() as u32;
        self.items.glyphs.extend_from_slice(glyphs);
        run.glyphs = compositor_types::ArenaRange {
            start,
            len: glyphs.len() as u32,
        };
        run.coords = compositor_types::ArenaRange::default();
        self.items.instanced += usize::from(instanced);
        self.items.push_glyph_run(run, self.scene.captured());
    }

    /// Returns the chunk's items.
    pub(crate) fn items_mut(&mut self) -> &mut Items {
        self.items
    }

    /// Returns the clip node a node reference was taken from.
    fn node_of(&self, reference: ClipRef) -> LocalParent {
        self.items
            .refs
            .iter()
            .find_map(|held| match held {
                Ref::Clip(id) if id.slot() == reference.0 => Some(LocalParent::Node(*id)),
                _ => None,
            })
            .unwrap_or(LocalParent::Chunk)
    }
}

impl core::fmt::Debug for Recorder<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Recorder")
            .field("items", &self.items.len())
            .field("compositor", &self.resolver.is_some())
            .finish()
    }
}

/// Adds `reference` to the ids `items` refer to, unless it is there.
fn refer(items: &mut Items, reference: Ref) {
    if !items.refs.contains(&reference) {
        items.refs.push(reference);
    }
}

/// Returns a borderless quad over `bounds` filled with `fill`, in the chunk's default space and
/// clip.
pub fn filled(bounds: Rect<DevicePx, Device>, radii: [f32; 8], fill: PaintRef) -> Quad {
    Quad {
        bounds: local_rect(bounds),
        radii,
        shapes: compositor_types::ROUND,
        border: [0.0; 4],
        fill,
        stroke: PaintRef::NONE,
        paint_origin: [0.0; 2],
        clip: ClipRef::INHERIT,
        space: SpaceRef::INHERIT,
        style: compositor_types::BorderStyle::SOLID,
        _pad: [0; 3],
    }
}

/// Returns `rect` as `[x, y, width, height]`.
pub fn local_rect(rect: Rect<DevicePx, Device>) -> [f32; 4] {
    [
        rect.origin.x.0,
        rect.origin.y.0,
        rect.size.width.0,
        rect.size.height.0,
    ]
}

/// Returns the eight-float form of four elliptical radii, clockwise from the top left.
pub fn flat_radii(radii: Corners<Vec2<DevicePx>>) -> [f32; 8] {
    [
        radii.top_left.x.0,
        radii.top_left.y.0,
        radii.top_right.x.0,
        radii.top_right.y.0,
        radii.bottom_right.x.0,
        radii.bottom_right.y.0,
        radii.bottom_left.x.0,
        radii.bottom_left.y.0,
    ]
}

/// Returns a colour of channels in `[0, 1]` as eight bits per channel.
fn rgba8(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}
