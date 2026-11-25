//! The recorder emitters write a fragment's painting into, as compositor items.
//!
//! Items refer to compositor nodes and slots. The recorder turns zgui's clip chains, coordinate
//! systems and paints into them as the emitters name them, so a fragment's items are complete
//! once its emitters return.

mod image;
mod items;
mod recording;

pub use self::image::{ImageKey, ImagePixels, ShownTiles};
pub(crate) use self::items::{Items, Ref};
pub use self::recording::Recording;

use std::sync::Arc;

use crate::Tables;
use iceblink::{
    Backdrop, ClipRef, ClipShape, ExternalQuad, Filter, Glyph, GlyphRun, Line, LocalClip,
    LocalParent, MarkPayload, Marks, PaintRef, ParamsBlock, PathItem, Quad, ShaderQuad, Shadow,
    SpaceRef, TextureId,
};
use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Rect, Vec2};
use zgui_space::{ClipId, ClipLink, SpatialId};

use crate::sync::Resolver;

/// Where emitters record one chunk's items, and the tables they read.
///
/// The chunk has a default coordinate system and a default clip chain, which are the fragment's
/// own. An item drawn in them refers to them as `INHERIT`.
///
/// Without a compositor, the items are built and nothing refers to a node or a slot.
pub struct Recorder<'a> {
    tables: &'a mut Tables,
    items: &'a mut Items,
    resolver: Option<Resolver<'a>>,
}

impl<'a> Recorder<'a> {
    /// Creates a recorder that empties `items` and records into them, with `space` and `clip` as
    /// the chunk's defaults.
    pub(crate) fn new(
        tables: &'a mut Tables,
        items: &'a mut Items,
        resolver: Option<Resolver<'a>>,
        space: SpatialId,
        clip: ClipId,
    ) -> Self {
        items.reset(space, clip);
        Self {
            tables,
            items,
            resolver,
        }
    }

    /// Returns a recorder into the same items for a shorter time.
    pub(crate) fn reborrow(&mut self) -> Recorder<'_> {
        Recorder {
            tables: self.tables,
            items: self.items,
            resolver: self.resolver.as_mut().map(Resolver::reborrow),
        }
    }

    /// Returns the tables the emitters read and intern into.
    pub fn tables(&mut self) -> &mut Tables {
        self.tables
    }

    /// Returns the tables for reading.
    pub fn tables_ref(&self) -> &Tables {
        self.tables
    }

    /// Returns the reference an item drawn in the coordinate system `space` holds.
    pub fn space(&mut self, space: SpatialId) -> SpaceRef {
        self.space_slot(space.index())
    }

    /// Returns the reference a scrollbar thumb drawn in the coordinate system `space` holds, when
    /// the compositor moves it with the scroll offset of the content in `content`.
    ///
    /// The thumb moves `free` pixels along `axis` (0 for x, 1 for y) over the whole scroll range,
    /// and is drawn where it sits at offset zero. Returns `None` when nothing moves it, so the
    /// thumb is drawn where it sits now.
    pub fn thumb_space(
        &mut self,
        space: SpatialId,
        content: SpatialId,
        axis: usize,
        free: f32,
    ) -> Option<SpaceRef> {
        let resolver = self.resolver.as_mut()?;
        let id = resolver.thumb(self.tables, (space.index(), content.index()), axis, free)?;
        refer(self.items, Ref::Space(id));
        Some(SpaceRef(id.slot()))
    }

    /// Returns the reference an item drawn in the coordinate system in `slot` holds.
    pub(crate) fn space_slot(&mut self, slot: u32) -> SpaceRef {
        if slot == self.items.space.index() {
            return SpaceRef::INHERIT;
        }
        let Some(resolver) = &mut self.resolver else {
            return SpaceRef::INHERIT;
        };
        let id = resolver.space(self.tables, slot);
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
        match resolver.clip(self.tables, clip) {
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
        let reference = self.clip(parent);
        match self.local_clip_in(reference, link) {
            Some(clip) => clip,
            None => {
                let chain = self.tables.clips.push(parent, link);
                self.clip(chain)
            }
        }
    }

    /// Adds a clip of `link` inside the clip `parent` refers to, which moves with the chunk, and
    /// returns the reference items drawn through it hold.
    ///
    /// Returns `None` for a link measured in another coordinate system than the chunk's default.
    pub fn local_clip_in(&mut self, parent: ClipRef, link: ClipLink) -> Option<ClipRef> {
        let ClipLink::RoundedRect {
            rect,
            radii,
            shape,
            space,
        } = link;
        if space != self.items.space {
            return None;
        }
        let parent = match parent {
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
        Some(ClipRef::local(index))
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
    pub fn paint(&mut self, paint: crate::paints::PaintRef) -> PaintRef {
        let Some(id) = paint.id() else {
            return PaintRef::NONE;
        };
        if let Some(crate::paints::Paint::Solid(color)) = self.tables.paints.get(id) {
            return self.color(*color);
        }
        self.slot(paint)
    }

    /// Returns the reference to the slot of `paint`, whose value a later frame may rewrite.
    ///
    /// An image paint, which the compositor has no slot for, paints nothing.
    pub fn slot(&mut self, paint: crate::paints::PaintRef) -> PaintRef {
        let Some(id) = paint.id() else {
            return PaintRef::NONE;
        };
        let Some(resolver) = &mut self.resolver else {
            return PaintRef::NONE;
        };
        let Some((slot, kind)) = resolver.paint(self.tables, id) else {
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
        self.items.push_quad(quad);
    }

    /// Returns the slots of the declared shader `shader` and of a parameter block holding `params`,
    /// in that order.
    ///
    /// Returns `None` for a handle no program was declared under. Without a compositor both slots
    /// are zero.
    pub fn shader_slots(
        &mut self,
        shader: zgui_css::shader::ShaderId,
        params: ParamsBlock,
    ) -> Option<(u32, u32)> {
        let Some(resolver) = &mut self.resolver else {
            return Some((0, 0));
        };
        let shader = resolver.shader(shader)?;
        let params = resolver.params(params);
        refer(self.items, Ref::Shader(shader));
        refer(self.items, Ref::Params(params));
        Some((shader.slot(), params.slot()))
    }

    /// Returns the slots of the declared shader `shader` and of the parameter slot of `params`, in
    /// that order.
    ///
    /// Returns `None` for a handle no program was declared under. Without a compositor both slots
    /// are zero.
    pub fn shader_slots_shared(
        &mut self,
        shader: zgui_css::shader::ShaderId,
        params: &crate::content::shader::SharedParams,
    ) -> Option<(u32, u32)> {
        let Some(resolver) = &mut self.resolver else {
            return Some((0, 0));
        };
        let shader = resolver.shader(shader)?;
        let params = resolver.shared_params(params);
        refer(self.items, Ref::Shader(shader));
        refer(self.items, Ref::Params(params));
        Some((shader.slot(), params.slot()))
    }

    /// Appends a shader quad, whose shader and parameter block are slots
    /// [`Recorder::shader_slots`] returned.
    pub fn shader_quad(&mut self, quad: ShaderQuad) {
        self.items.push_shader_quad(quad);
    }

    /// Returns the compositor filter that runs `step`, whose parameter block the chunk holds.
    ///
    /// Returns `None` without a compositor and for a shader no program was declared under.
    pub fn shader_filter(&mut self, step: crate::emit::shader::ShaderStep) -> Option<Filter> {
        self.resolver.as_mut()?.shader_filter(step)
    }

    /// Appends an external quad drawing the texture `texture`, whose slot replaces the quad's.
    pub fn external(&mut self, texture: TextureId, mut quad: ExternalQuad) {
        quad.texture = texture.slot();
        if self.resolver.is_some() {
            refer(self.items, Ref::Texture(texture));
        }
        self.items.push_external(quad);
    }

    /// Appends a shadow.
    pub fn shadow(&mut self, shadow: Shadow) {
        self.items.push_shadow(shadow);
    }

    /// Appends a decoration line.
    pub fn line(&mut self, line: Line) {
        self.items.push_line(line);
    }

    /// Appends a glyph run of `glyphs` at the normalised variation coordinates `coords`, empty
    /// for the default instance of its font.
    ///
    /// The run's glyph and coordinate ranges are replaced.
    pub fn glyph_run(&mut self, mut run: GlyphRun, glyphs: &[Glyph], coords: &[i16]) {
        run.glyphs = append(&mut self.items.glyphs, glyphs);
        run.coords = append(&mut self.items.coords, coords);
        self.items.push_glyph_run(run);
    }

    /// Appends a marks item drawing from `payload`, whose index in the chunk's payload list
    /// replaces the item's.
    pub fn marks(&mut self, mut marks: Marks, payload: Arc<MarkPayload>) {
        marks.payload = self.items.payload(payload);
        self.items.push_marks(marks);
    }

    /// Appends a path, unless a number in its geometry, its maps or its stroke is not finite.
    ///
    /// Returns whether it was appended.
    pub fn path(&mut self, path: PathItem) -> bool {
        if !finite_path(&path) {
            return false;
        }
        self.items.push_path(path);
        true
    }

    /// Appends a backdrop filtered by `chain`, whose index in the chunk's filter-chain list
    /// replaces the item's.
    pub fn backdrop(&mut self, mut backdrop: Backdrop, chain: Arc<[Filter]>) {
        backdrop.filters = self.items.filters(chain);
        self.items.push_backdrop(backdrop);
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

/// Appends `values` to `arena` and returns the range they take.
fn append<T: Copy>(arena: &mut Vec<T>, values: &[T]) -> iceblink::ArenaRange {
    let start = arena.len() as u32;
    arena.extend_from_slice(values);
    iceblink::ArenaRange {
        start,
        len: values.len() as u32,
    }
}

/// Adds `reference` to the ids `items` refer to, unless it is there.
pub(crate) fn refer(items: &mut Items, reference: Ref) {
    if !items.refs.contains(&reference) {
        items.refs.push(reference);
    }
}

/// Reports whether every number of `item` is finite.
fn finite_path(item: &PathItem) -> bool {
    let finite = |values: &[f64]| values.iter().all(|value| value.is_finite());
    let stroke = item.stroke.as_ref().is_none_or(|stroke| {
        let style = &stroke.style;
        finite(&[style.width, style.miter_limit, style.dash_offset]) && finite(&style.dash_pattern)
    });
    let geometry = |path: &kurbo::BezPath| {
        use kurbo::PathEl;

        let point = |point: &kurbo::Point| point.x.is_finite() && point.y.is_finite();
        path.elements().iter().all(|element| match element {
            PathEl::MoveTo(a) | PathEl::LineTo(a) => point(a),
            PathEl::QuadTo(a, b) => point(a) && point(b),
            PathEl::CurveTo(a, b, c) => point(a) && point(b) && point(c),
            PathEl::ClosePath => true,
        })
    };
    finite(&item.placement.as_coeffs())
        && finite(&item.brush.as_coeffs())
        && stroke
        && geometry(&item.path)
        && item.clips.iter().all(|clip| geometry(&clip.path))
}

/// Returns a borderless quad over `bounds` filled with `fill`, in the chunk's default space and
/// clip.
pub fn filled(bounds: Rect<DevicePx, Device>, radii: [f32; 8], fill: PaintRef) -> Quad {
    Quad {
        bounds: local_rect(bounds),
        radii,
        shapes: iceblink::ROUND,
        border: [0.0; 4],
        fill,
        stroke: PaintRef::NONE,
        paint_origin: [0.0; 2],
        clip: ClipRef::INHERIT,
        space: SpaceRef::INHERIT,
        style: iceblink::BorderStyle::SOLID,
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
