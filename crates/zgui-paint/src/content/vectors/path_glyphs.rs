//! Repeated outlines rasterised once into the monochrome atlas, at every quarter-pixel phase.
//!
//! Each distinct outline gets one atlas tile, a *sheet*, holding 16 *cells*: cell `4 · py + px`
//! is the outline moved by `(px, py) / 4` device pixels. A glyph mark draws every copy of the
//! outline from the cell of its own device phase, so a pan or a moved replay rasterises nothing.
//!
//! A sheet is keyed by the outline's words and its raster style, which already carry the device
//! scale. The keys come from the mask namespace, so a sheet and a mask never share a key.
//!
//! Beside the sheets, the splits of the paths drawn lately are kept by the identity of the path,
//! with the payload lowered from each, so a path that does not change splits once.

use core::hash::Hash;
use std::sync::{Arc, Weak};

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use zgui_atlas::{Atlas, AtlasKey};
use zgui_geom::Size;
use zgui_profile::{Counter, counter};
use zgui_scene::kurbo::BezPath;
use zgui_scene::{MarkPayload, SpriteTile, VectorStroke};

use super::mask::{Budget, Style, VectorMaskStyle, fresh_key, raster_commands, style};
use super::recognitions::evict;
use crate::emit::vector::split::{
    CLOSE, CUBIC, Geometry, LINE, MOVE, QUAD, Split, SplitDeclined, UNITS,
};

/// The widest and tallest a cell may be, in device pixels.
pub(crate) const MAX_CELL: i32 = 64;

/// The most texels the sheets of one request may cover together.
pub(crate) const MAX_ITEM_TEXELS: u64 = 256 * 1024;

/// The most new sheets one frame rasterises.
const BUDGET_SHEETS: u32 = 64;

/// The most texels those new sheets may cover together.
const BUDGET_TEXELS: u64 = 1024 * 1024;

/// How many phases a sheet holds.
pub(crate) const PHASES: usize = 16;

/// What a sheet is kept by: an outline's words and the style it is drawn in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SheetKey {
    /// The outline, in words from its anchor.
    commands: Box<[i32]>,
    /// How its coverage is produced, with every length in device pixels.
    style: Style,
}

/// One phase of an outline in its sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cell {
    /// Where the cell starts in the sheet, in texels.
    at: [u16; 2],
    /// Its extent, in texels.
    size: [u16; 2],
    /// Where it starts from the pixel of the anchor.
    offset: [i32; 2],
}

/// The outlines of one part of a shape, and how they are drawn.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct GlyphRequest<'a> {
    /// The distinct outlines, in device pixels from their anchors.
    pub(crate) geometries: &'a [Geometry],
    /// Whether the outlines are filled or stroked. Stroke lengths are in the units `scale` maps to
    /// device pixels.
    pub(crate) style: VectorMaskStyle<'a>,
    /// Device pixels per stroke unit.
    pub(crate) scale: f64,
}

/// Where the cells of one request are.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphSheets {
    /// The atlas texture every sheet lies in, as [`SpriteTile::texture`] packs it.
    pub texture: u32,
    /// Sixteen words per outline, one per phase: `[x | y << 16, w | h << 16, ox, oy]`, the cell
    /// in atlas texels and where it starts from the pixel of the anchor.
    pub table: Vec<[u32; 4]>,
    /// Per outline, the union of its cells from the pixel of the anchor, as `[x0, y0, x1, y1]`.
    pub reach: Vec<[i32; 4]>,
    /// The atlas keys of the sheets, one per outline.
    pub keys: Vec<AtlasKey>,
}

/// The sheets of every outline drawn lately, the splits of the paths drawn lately, and what this
/// frame has spent.
#[derive(Debug, Default)]
pub(crate) struct PathGlyphs {
    /// What the atlas holds each sheet as. The cells are laid out again from the key.
    sheets: FxHashMap<SheetKey, AtlasKey>,
    /// The splits.
    pub(crate) splits: Splits,
    /// What this frame has spent on new sheets.
    spent: Budget,
}

impl PathGlyphs {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.spent = Budget::default();
        self.splits.begin_frame();
    }

    /// Ends a frame.
    pub(crate) fn end_frame(&mut self) {
        self.splits.end_frame();
    }

    /// Drops the sheets whose tiles the atlas removed.
    pub(crate) fn forget_tiles(&mut self, removed: &[AtlasKey]) {
        if removed.is_empty() || self.sheets.is_empty() {
            return;
        }
        self.sheets.retain(|_, key| !removed.contains(key));
    }

    /// Forgets every sheet and every split.
    pub(crate) fn clear(&mut self) {
        self.sheets.clear();
        self.splits.clear();
    }

    /// How many sheets are held.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.sheets.len()
    }

    /// The sheets of every outline of `request`, rasterising the missing ones, or `None` to
    /// decline.
    ///
    /// Declines a cell over [`MAX_CELL`] on a side, sheets over [`MAX_ITEM_TEXELS`] together,
    /// sheets in more than one texture, and new sheets past the frame's budget of
    /// [`BUDGET_SHEETS`] covering [`BUDGET_TEXELS`].
    pub(crate) fn sheets_for(
        &mut self,
        atlas: &mut Atlas,
        next_handle: &mut u64,
        request: GlyphRequest<'_>,
    ) -> Option<GlyphSheets> {
        let style = style(request.style, request.scale as f32)?;
        let reach = match request.style {
            VectorMaskStyle::Fill(_) => 0.0,
            VectorMaskStyle::Stroke(stroke) => {
                let mut scaled = stroke.clone();
                scaled.width *= request.scale;
                VectorStroke {
                    paint: zgui_scene::PaintRef::NONE,
                    style: scaled,
                }
                .reach()
            }
        };
        let mut layouts: SmallVec<[([Cell; PHASES], [i32; 2]); 4]> = SmallVec::new();
        let mut texels = 0u64;
        for geometry in request.geometries {
            let layout = layout(geometry, reach)?;
            texels += u64::from(layout.1[0].unsigned_abs()) * u64::from(layout.1[1].unsigned_abs());
            layouts.push(layout);
        }
        if texels > MAX_ITEM_TEXELS {
            return None;
        }
        let mut sheets = GlyphSheets {
            texture: 0,
            table: Vec::with_capacity(request.geometries.len() * PHASES),
            reach: Vec::with_capacity(request.geometries.len()),
            keys: Vec::with_capacity(request.geometries.len()),
        };
        for (index, (geometry, (cells, size))) in request.geometries.iter().zip(layouts).enumerate()
        {
            let key = SheetKey {
                commands: geometry.commands.clone(),
                style: style.clone(),
            };
            let held = self
                .sheets
                .get(&key)
                .and_then(|&held| Some((held, atlas.get(held)?)));
            let (atlas_key, tile) = match held {
                Some(found) => found,
                None => {
                    let sheet_texels =
                        u64::from(size[0].unsigned_abs()) * u64::from(size[1].unsigned_abs());
                    if !self
                        .spent
                        .admits(sheet_texels, BUDGET_SHEETS, BUDGET_TEXELS)
                    {
                        counter::bump(Counter::VectorMaskBudgetOverflow);
                        return None;
                    }
                    let atlas_key = fresh_key(next_handle, atlas);
                    let tile = atlas
                        .get_or_insert(atlas_key, Size::new(size[0], size[1]), || {
                            raster_sheet(geometry, &style, &cells, size)
                        })
                        .ok()?;
                    self.spent.spend(sheet_texels);
                    counter::add(Counter::PathGlyphTilesRasterised, PHASES as u64);
                    self.sheets.insert(key, atlas_key);
                    (atlas_key, tile)
                }
            };
            let texture = SpriteTile::of(tile).texture;
            if index == 0 {
                sheets.texture = texture;
            } else if texture != sheets.texture {
                return None;
            }
            let origin = [tile.bounds.origin.x, tile.bounds.origin.y];
            let mut union = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
            for cell in &cells {
                let x = origin[0] as u32 + u32::from(cell.at[0]);
                let y = origin[1] as u32 + u32::from(cell.at[1]);
                sheets.table.push([
                    x | (y << 16),
                    u32::from(cell.size[0]) | (u32::from(cell.size[1]) << 16),
                    cell.offset[0] as u32,
                    cell.offset[1] as u32,
                ]);
                union = [
                    union[0].min(cell.offset[0]),
                    union[1].min(cell.offset[1]),
                    union[2].max(cell.offset[0] + i32::from(cell.size[0])),
                    union[3].max(cell.offset[1] + i32::from(cell.size[1])),
                ];
            }
            sheets.reach.push(union);
            sheets.keys.push(atlas_key);
        }
        Some(sheets)
    }
}

/// The cells of `geometry` grown by `reach`, and the extent of the sheet that holds them, or
/// `None` when a cell is wider or taller than [`MAX_CELL`].
///
/// Cell `4 · py + px` covers the outline moved by `(px, py) / 4`, rounded out to whole pixels.
/// The sheet is a four by four grid of the largest cell.
fn layout(geometry: &Geometry, reach: f32) -> Option<([Cell; PHASES], [i32; 2])> {
    let [x0, y0, x1, y1] = geometry.bounds;
    let mut cells = [Cell::default(); PHASES];
    let mut largest = [1i32, 1i32];
    for (phase, cell) in cells.iter_mut().enumerate() {
        let shift = [(phase % 4) as f32 / 4.0, (phase / 4) as f32 / 4.0];
        let left = (x0 + shift[0] - reach).floor();
        let top = (y0 + shift[1] - reach).floor();
        let right = (x1 + shift[0] + reach).ceil();
        let bottom = (y1 + shift[1] + reach).ceil();
        if ![left, top, right, bottom]
            .iter()
            .all(|edge| edge.is_finite())
        {
            return None;
        }
        let width = (right - left).max(1.0);
        let height = (bottom - top).max(1.0);
        if width > MAX_CELL as f32 || height > MAX_CELL as f32 {
            return None;
        }
        cell.size = [width as u16, height as u16];
        cell.offset = [left as i32, top as i32];
        largest = [largest[0].max(width as i32), largest[1].max(height as i32)];
    }
    for (phase, cell) in cells.iter_mut().enumerate() {
        cell.at = [
            ((phase % 4) as i32 * largest[0]) as u16,
            ((phase / 4) as i32 * largest[1]) as u16,
        ];
    }
    Some((cells, [largest[0] * 4, largest[1] * 4]))
}

/// The 16 cells of `geometry` in one sheet of `size` texels.
fn raster_sheet(
    geometry: &Geometry,
    style: &Style,
    cells: &[Cell; PHASES],
    size: [i32; 2],
) -> Vec<u8> {
    let width = size[0] as usize;
    let mut sheet = vec![0u8; width * size[1] as usize];
    let mut commands = Vec::new();
    for (phase, cell) in cells.iter().enumerate() {
        let shift = [
            (phase % 4) as f64 / 4.0 - f64::from(cell.offset[0]),
            (phase / 4) as f64 / 4.0 - f64::from(cell.offset[1]),
        ];
        commands.clear();
        zeno_commands(&geometry.commands, shift, &mut commands);
        let cell_size = [i32::from(cell.size[0]), i32::from(cell.size[1])];
        let coverage = raster_commands(&commands, style, cell_size);
        let row = cell.size[0] as usize;
        for (y, line) in coverage.chunks_exact(row).enumerate() {
            let at = (usize::from(cell.at[1]) + y) * width + usize::from(cell.at[0]);
            sheet[at..at + row].copy_from_slice(line);
        }
    }
    sheet
}

/// The words of an outline as zeno commands in pixels, moved by `shift`.
fn zeno_commands(words: &[i32], shift: [f64; 2], out: &mut Vec<zeno::Command>) {
    let point = |at: usize| {
        zeno::Point::new(
            (f64::from(words[at]) / UNITS + shift[0]) as f32,
            (f64::from(words[at + 1]) / UNITS + shift[1]) as f32,
        )
    };
    let mut at = 0;
    while let Some(&tag) = words.get(at) {
        match tag {
            MOVE => out.push(zeno::Command::MoveTo(point(at + 1))),
            LINE => out.push(zeno::Command::LineTo(point(at + 1))),
            QUAD => out.push(zeno::Command::QuadTo(point(at + 1), point(at + 3))),
            CUBIC => out.push(zeno::Command::CurveTo(
                point(at + 1),
                point(at + 3),
                point(at + 5),
            )),
            CLOSE => out.push(zeno::Command::Close),
            _ => {}
        }
        at += 1 + match tag {
            MOVE | LINE => 2,
            QUAD => 4,
            CUBIC => 6,
            _ => 0,
        };
    }
}

/// How many frames an entry survives without a lookup, once a lookup has found it.
const KEPT_FRAMES: u32 = 8;

/// How many frames an entry nothing found yet survives.
const UNPROVEN_FRAMES: u32 = 1;

/// How many misses with no hit stop a frame's entries from being kept.
const IDLE_MISSES: u32 = 16;

/// How often, in frames, the cache keeps entries while it is idle.
const PROBE_FRAMES: u32 = 8;

/// The most entries held. A full map drops [`EVICTED_SPLITS`] entries, those touched least
/// recently first.
pub(crate) const MAX_SPLITS: usize = 256;

/// How many entries a full map drops at once, so one scan for the oldest serves this many inserts.
const EVICTED_SPLITS: usize = MAX_SPLITS / 16;

/// One part's payload, with the sheets it names.
#[derive(Debug)]
pub(crate) struct PartPayload {
    /// The atlas keys of the sheets the table was built from.
    pub(crate) keys: Box<[AtlasKey]>,
    /// Whether the part's copies overlap and are drawn as one union.
    pub(crate) union: bool,
    /// The table and the anchors.
    pub(crate) payload: Arc<MarkPayload>,
}

/// One path's split.
#[derive(Debug)]
pub(crate) struct SplitEntry {
    /// The path's allocation, held so no other path takes its address while the entry stands.
    path: Weak<BezPath>,
    /// What the split found, or why it declined.
    pub(crate) outcome: Result<Arc<Split>, SplitDeclined>,
    /// The payload of each part, fill and stroke, lowered from the split.
    pub(crate) payloads: [Option<PartPayload>; 2],
    /// The frame it was last looked up in.
    touched: u32,
    /// Whether a lookup has found it.
    proven: bool,
}

/// Splits of paths, by the address of the path and the bits of the device map.
#[doc(hidden)]
#[derive(Debug)]
pub struct Splits {
    /// The entries.
    entries: FxHashMap<(usize, [u64; 4]), SplitEntry>,
    /// The current frame.
    frame: u32,
    /// Lookups this frame that found an entry.
    hits: u32,
    /// Lookups this frame that found none.
    misses: u32,
    /// Whether this frame keeps what it splits.
    keeping: bool,
}

impl Default for Splits {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            frame: 0,
            hits: 0,
            misses: 0,
            keeping: true,
        }
    }
}

impl Splits {
    /// Starts a frame.
    pub(crate) fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.hits = 0;
        self.misses = 0;
    }

    /// Forgets every entry whose path died, every entry no lookup touched for [`KEPT_FRAMES`]
    /// frames, and every entry no lookup found within [`UNPROVEN_FRAMES`] frames, and decides
    /// whether the next frame keeps entries.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.entries.retain(|_, entry| {
            if entry.path.strong_count() == 0 {
                return false;
            }
            let kept = if entry.proven {
                KEPT_FRAMES
            } else {
                UNPROVEN_FRAMES
            };
            frame.wrapping_sub(entry.touched) < kept
        });
        let idle = self.hits == 0 && self.misses >= IDLE_MISSES;
        self.keeping = !idle || frame.wrapping_add(1).is_multiple_of(PROBE_FRAMES);
    }

    /// The entry of `path` under `linear`, if one is held.
    pub(crate) fn lookup(
        &mut self,
        path: &Arc<BezPath>,
        linear: [f64; 4],
    ) -> Option<&mut SplitEntry> {
        let frame = self.frame;
        match self.entries.get_mut(&key(path, linear)) {
            Some(entry) => {
                entry.touched = frame;
                entry.proven = true;
                self.hits += 1;
                Some(entry)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// Keeps what `path` split into under `linear`, and returns the entry, or `None` when this
    /// frame keeps nothing.
    pub(crate) fn insert(
        &mut self,
        path: &Arc<BezPath>,
        linear: [f64; 4],
        outcome: Result<Arc<Split>, SplitDeclined>,
    ) -> Option<&mut SplitEntry> {
        if !self.keeping {
            return None;
        }
        let key = key(path, linear);
        if self.entries.len() >= MAX_SPLITS && !self.entries.contains_key(&key) {
            evict(&mut self.entries, self.frame, EVICTED_SPLITS, |entry| {
                (entry.touched, false)
            });
        }
        self.entries.insert(
            key,
            SplitEntry {
                path: Arc::downgrade(path),
                outcome,
                payloads: [None, None],
                touched: self.frame,
                proven: false,
            },
        );
        self.entries.get_mut(&key)
    }

    /// How many paths are held.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Forgets everything.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

/// The key of `path` under `linear`.
fn key(path: &Arc<BezPath>, linear: [f64; 4]) -> (usize, [u64; 4]) {
    (Arc::as_ptr(path).addr(), linear.map(f64::to_bits))
}

#[cfg(test)]
mod tests;
