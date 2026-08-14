//! Small solid vector shapes rasterised into the shared monochrome atlas.
//!
//! The cache deliberately names geometry rather than colour. A monochrome tile is coverage, so
//! recolouring an icon only changes the sprite instance and never rasterises the outline again.

use core::hash::{Hash, Hasher};

use rustc_hash::{FxHashMap, FxHasher};
use zgui_atlas::{Atlas, AtlasKey, AtlasTile, TextureKind};
use zgui_geom::{Device, Rect, Size};
use zgui_scene::kurbo::{self, BezPath, PathEl};
use zgui_scene::peniko;

/// How a path's coverage is produced.
#[derive(Clone, Copy, Debug)]
pub enum VectorMaskStyle<'a> {
    /// The path's interior under this fill rule.
    Fill(peniko::Fill),
    /// The outline produced by this stroke style.
    Stroke(&'a kurbo::Stroke),
}

/// The geometry needed to request one coverage mask.
#[derive(Clone, Copy, Debug)]
pub struct VectorMaskRequest<'a> {
    /// The outline in the coordinates its own box is measured in.
    pub path: &'a BezPath,
    /// Whether the outline is filled or stroked.
    pub style: VectorMaskStyle<'a>,
    /// Mask texels per unit of each of the path's own axes.
    ///
    /// One and one where the shape is drawn under nothing but a translation, which is the ordinary
    /// case and the one that makes a tile shared between two placements of the same icon. A shape
    /// under a scale is rasterised at the density the scale asks for, so what reaches the screen is
    /// coverage the rasteriser produced rather than coverage a sampler stretched. A rotation is a
    /// density of one on both axes, so every angle of a turning shape shares one tile.
    pub density: [f32; 2],
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// Integer pixel bounds of the raster, in the same space the density measures.
    pub bounds: Rect<i32, Device>,
}

/// One cached coverage raster and the key that keeps it alive across replay.
#[derive(Clone, Copy, Debug)]
pub struct VectorMask {
    /// Where the coverage lives now.
    pub tile: AtlasTile,
    /// What the atlas knows the coverage as.
    pub key: AtlasKey,
}

/// Paint-side source of small vector coverage masks.
pub trait VectorMaskSource {
    /// Returns a cached mask, rasterising it on a miss.
    fn vector_mask(&self, request: VectorMaskRequest<'_>) -> Option<VectorMask>;
}

/// A source that declines every mask request.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoVectorMasks;

impl VectorMaskSource for NoVectorMasks {
    fn vector_mask(&self, _request: VectorMaskRequest<'_>) -> Option<VectorMask> {
        None
    }
}

/// Geometry identity independent of its integer translation and tint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Fingerprint {
    commands: Box<[Command]>,
    style: Style,
    scale: u32,
    size: [i32; 2],
}

/// Raster style encoded without borrowing the source shape.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Style {
    Fill(bool),
    Stroke {
        width: u32,
        join: u8,
        miter_limit: u32,
        start_cap: u8,
        end_cap: u8,
        dashes: Box<[u32]>,
        dash_offset: u32,
    },
}

/// One path command encoded in exact `f32` bits after integer-origin normalisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Command {
    Move([u32; 2]),
    Line([u32; 2]),
    Quad([u32; 4]),
    Curve([u32; 6]),
    Close,
}

/// The sparse metadata beside monochrome atlas entries used by vector masks.
#[derive(Debug)]
pub(crate) struct VectorMaskCache {
    entries: FxHashMap<Fingerprint, AtlasKey>,
    next_handle: u64,
}

/// A disjoint namespace from glyph handles in the monochrome atlas.
const MASK_NAMESPACE: u64 = 0x7E00_0000_0000_0000;
const HANDLE_BITS: u64 = 0x00FF_FFFF_FFFF_FFFF;

impl Default for VectorMaskCache {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            next_handle: MASK_NAMESPACE,
        }
    }
}

impl VectorMaskCache {
    /// Looks up or builds one coverage tile.
    pub(crate) fn tile_for(
        &mut self,
        atlas: &mut Atlas,
        request: VectorMaskRequest<'_>,
    ) -> Option<VectorMask> {
        let width = request.bounds.size.width;
        let height = request.bounds.size.height;
        if width <= 0 || height <= 0 {
            return None;
        }
        let commands = commands(
            request.path,
            request.density,
            request.bounds.origin.x,
            request.bounds.origin.y,
        )?;
        // The density needs no field of its own. It is already in the scaled commands, in the size
        // they were measured to produce and in the scaled stroke width, so two requests that agree
        // on all three ask for the same raster whatever densities they arrived at it by.
        let fingerprint = Fingerprint {
            commands: commands.into_boxed_slice(),
            style: style(request.style, request.density[0])?,
            scale: request.scale.to_bits(),
            size: [width, height],
        };

        let key = if let Some(key) = self.entries.get(&fingerprint).copied() {
            key
        } else {
            let key = self.fresh_key(atlas);
            self.entries.insert(fingerprint.clone(), key);
            key
        };
        let tile = atlas
            .get_or_insert(key, Size::new(width, height), || raster(&fingerprint))
            .ok()?;
        Some(VectorMask { tile, key })
    }

    /// Drops metadata whose atlas content was evicted.
    pub(crate) fn forget_tiles(&mut self, removed: &[AtlasKey]) {
        if removed.is_empty() {
            return;
        }
        self.entries.retain(|_, key| !removed.contains(key));
    }

    /// Forgets every geometry identity after the atlas itself is cleared.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.next_handle = MASK_NAMESPACE;
    }

    fn fresh_key(&mut self, atlas: &Atlas) -> AtlasKey {
        loop {
            let key = AtlasKey::new(self.next_handle, TextureKind::Mono);
            self.next_handle = MASK_NAMESPACE | (self.next_handle.wrapping_add(1) & HANDLE_BITS);
            if !atlas.contains(key) {
                return key;
            }
        }
    }
}

fn commands(
    path: &BezPath,
    density: [f32; 2],
    origin_x: i32,
    origin_y: i32,
) -> Option<Vec<Command>> {
    let x = f64::from(origin_x);
    let y = f64::from(origin_y);
    let kx = f64::from(density[0]);
    let ky = f64::from(density[1]);
    let point = |point: zgui_scene::kurbo::Point| {
        let point = [(point.x * kx - x) as f32, (point.y * ky - y) as f32];
        (point[0].is_finite() && point[1].is_finite())
            .then_some([point[0].to_bits(), point[1].to_bits()])
    };
    path.elements()
        .iter()
        .map(|element| match *element {
            PathEl::MoveTo(p) => Some(Command::Move(point(p)?)),
            PathEl::LineTo(p) => Some(Command::Line(point(p)?)),
            PathEl::QuadTo(a, b) => {
                let a = point(a)?;
                let b = point(b)?;
                Some(Command::Quad([a[0], a[1], b[0], b[1]]))
            }
            PathEl::CurveTo(a, b, c) => {
                let a = point(a)?;
                let b = point(b)?;
                let c = point(c)?;
                Some(Command::Curve([a[0], a[1], b[0], b[1], c[0], c[1]]))
            }
            PathEl::ClosePath => Some(Command::Close),
        })
        .collect()
}

fn raster(fingerprint: &Fingerprint) -> Vec<u8> {
    let commands: Vec<zeno::Command> = fingerprint
        .commands
        .iter()
        .map(|command| match *command {
            Command::Move(p) => zeno::Command::MoveTo(point(p)),
            Command::Line(p) => zeno::Command::LineTo(point(p)),
            Command::Quad(p) => zeno::Command::QuadTo(point([p[0], p[1]]), point([p[2], p[3]])),
            Command::Curve(p) => zeno::Command::CurveTo(
                point([p[0], p[1]]),
                point([p[2], p[3]]),
                point([p[4], p[5]]),
            ),
            Command::Close => zeno::Command::Close,
        })
        .collect();
    let mut mask = zeno::Mask::new(commands.as_slice());
    let dashes;
    match &fingerprint.style {
        Style::Fill(even_odd) => {
            mask.style(if *even_odd {
                zeno::Fill::EvenOdd
            } else {
                zeno::Fill::NonZero
            });
        }
        Style::Stroke {
            width,
            join,
            miter_limit,
            start_cap,
            end_cap,
            dashes: dash_bits,
            dash_offset,
        } => {
            dashes = dash_bits
                .iter()
                .map(|value| f32::from_bits(*value))
                .collect::<Vec<_>>();
            mask.style(zeno::Stroke {
                width: f32::from_bits(*width),
                join: zeno_join(*join),
                miter_limit: f32::from_bits(*miter_limit),
                start_cap: zeno_cap(*start_cap),
                end_cap: zeno_cap(*end_cap),
                dashes: &dashes,
                offset: f32::from_bits(*dash_offset),
                scale: true,
            });
        }
    }
    mask.size(
        fingerprint.size[0].max(0) as u32,
        fingerprint.size[1].max(0) as u32,
    );
    mask.render().0
}

/// Converts the public, borrowed style into exact bits for the cache and raster closure.
///
/// Every length of a stroke is measured along the outline, so `density` scales all of them. It is
/// one number rather than two because stroking does not commute with a map that scales the two axes
/// differently, and the caller declines the mask rather than ask this to draw the wrong outline.
/// The miter limit is a ratio and is left alone.
fn style(style: VectorMaskStyle<'_>, density: f32) -> Option<Style> {
    match style {
        VectorMaskStyle::Fill(rule) => Some(Style::Fill(rule == peniko::Fill::EvenOdd)),
        VectorMaskStyle::Stroke(stroke) => {
            let finite = [stroke.width, stroke.miter_limit, stroke.dash_offset]
                .into_iter()
                .chain(stroke.dash_pattern.iter().copied())
                .all(f64::is_finite);
            if !finite || stroke.width <= 0.0 {
                return None;
            }
            let scaled = |value: f64| ((value * f64::from(density)) as f32).to_bits();
            Some(Style::Stroke {
                width: scaled(stroke.width),
                join: kurbo_join(stroke.join),
                miter_limit: (stroke.miter_limit as f32).to_bits(),
                start_cap: kurbo_cap(stroke.start_cap),
                end_cap: kurbo_cap(stroke.end_cap),
                dashes: stroke
                    .dash_pattern
                    .iter()
                    .map(|value| scaled(*value))
                    .collect(),
                dash_offset: scaled(stroke.dash_offset),
            })
        }
    }
}

fn kurbo_join(join: kurbo::Join) -> u8 {
    match join {
        kurbo::Join::Bevel => 0,
        kurbo::Join::Miter => 1,
        kurbo::Join::Round => 2,
    }
}

fn zeno_join(join: u8) -> zeno::Join {
    match join {
        0 => zeno::Join::Bevel,
        2 => zeno::Join::Round,
        _ => zeno::Join::Miter,
    }
}

fn kurbo_cap(cap: kurbo::Cap) -> u8 {
    match cap {
        kurbo::Cap::Butt => 0,
        kurbo::Cap::Square => 1,
        kurbo::Cap::Round => 2,
    }
}

fn zeno_cap(cap: u8) -> zeno::Cap {
    match cap {
        1 => zeno::Cap::Square,
        2 => zeno::Cap::Round,
        _ => zeno::Cap::Butt,
    }
}

fn point(bits: [u32; 2]) -> zeno::Point {
    zeno::Point::new(f32::from_bits(bits[0]), f32::from_bits(bits[1]))
}

/// A stable diagnostic fingerprint for tests and cache instrumentation.
#[allow(dead_code)]
fn geometry_hash(fingerprint: &Fingerprint) -> u64 {
    let mut hasher = FxHasher::default();
    fingerprint.hash(&mut hasher);
    hasher.finish()
}
