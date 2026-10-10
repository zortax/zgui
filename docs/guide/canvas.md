# Drawing on a canvas

A canvas element draws a retained scene: shapes and series that an application builds and changes.
The scene lives in a registry on the paint side. The element carries only a token, a revision and a
view counter, so a change to the scene is a property change of one element and repaints only it.

## Two ways to draw

| | Use when | What a change costs |
|---|---|---|
| `canvas().draw(\|cx\| …)` | the picture follows signals or the element's size | one run of the closure, into a cleared scene |
| `canvas().scene(&handle)` with `CanvasHandle::draw` | the application keeps the scene itself | one edit |

Both move the scene's revision on every edit. A scene can also be edited from another thread
through `CanvasHandle::scene`, which is a `SceneHandle`.

## Shapes

A shape is an outline with its own fill, stroke and clips, built with `ShapeBuilder`. Shapes are in
canvas units: CSS pixels from the content box's top left corner, or view-box units when the
element has a view box. Stroke widths are canvas units too, so they scale with the view.

Keep a path in one `Arc` across edits (`ShapeBuilder::shared`) when its geometry does not change.
Recognition and the mark payloads are keyed by that allocation.

## The view transform

`set_transform(affine)` on `CanvasHandle`, `SceneHandle` or the scene in a draw closure sets one
matrix, applied before the view-box fit. It moves every shape and series and moves no revision.
A pan or a zoom through it recognises no shape again. The series and the shapes that become
analytic quads or marks keep their payloads, and the renderer uploads none for them. A turn or a
stretch keeps only the series payloads. A shape on the general route keeps its encoding under
every view, because the view only changes where the renderer places that encoding. A shape on the
path glyph route keeps its payload under a pan and rasterises its outlines again under a new zoom.
A shape on the mask route rasterises again under each new view.

```rust,ignore
let handle = CanvasHandle::new();
handle.draw(|scene| scene.push_series(series));
// On every drag step:
handle.set_transform(kurbo::Affine::translate((dx, 0.0)));
```

The transform stays when a draw closure clears the scene for its next run.

## Series

A `Series` holds plot data in data space as an `Arc<[[f32; 2]]>`, with a `to_canvas` matrix:

- `Series::Points` draws one marker per point: a circle, a square or any outline
  (`Marker::Path`), with a fill, a stroke or both.
- `Series::Line` draws one polyline through the points, with round joins.

Marker sizes and line widths are CSS pixels. No canvas transform scales them: not `to_canvas`,
not the view transform and not the view-box fit. A CSS transform on the element scales them with
the rest of the element. A NaN point ends a line run, and a points series skips it.

A series is drawn as one union mark per part. Its payload is built once per data allocation, so
keep the data in one `Arc` and change the view to move it. Data far from the origin keeps its
precision: positions are stored relative to the centre of the data.

`push_series` places the series above the shapes pushed so far. Series take no part in hit
testing.

A long line can be drawn at a lower level of detail. `push_series_lod(series, Lod::Columns)`, or
`--zgui-vector-lod: columns` on the element for all its line series, reduces a `Series::Line`
whose points run left to right, with more than four points per device column, to the first, lowest,
highest and last point of each column. A pan builds nothing new, and a zoom builds the reduction
again only past twice or half the scale. Under antialiasing the reduced line is lighter where the
data is noise denser than a pixel, so the reduction is never automatic.

A `Marker::Path` is an outline in CSS pixels with its origin on the point. It is drawn as path
glyphs (see below), so a pan or a zoom of the view rasterises nothing and uploads no payload. A
marker more than 64 device pixels across, a filled marker that crosses itself, and a marker under
an element transform that turns or skews, draw through the general route.

## Path glyphs

A shape whose subpaths repeat a few outlines, such as a scatter of triangles, crosses or stars, is
drawn like text. Each distinct outline is rasterised once per device scale into the monochrome
atlas, at 16 quarter-pixel positions, and each subpath draws the one nearest its own position. A
pan rasterises nothing, and a position is off by at most an eighth of a pixel. The route takes a
shape when all of these hold:

- No analytic or marks route takes it. Circles, rectangles and simple strokes go there first.
- It has at least 8 subpaths. At most 16 distinct outlines are allowed among the first 64
  subpaths, and at most one eighth of all subpaths (and never more than 64) are distinct.
- Each subpath is at most 64 device pixels from its first point.
- The paint is a solid or inherited colour, with no clip and no dash pattern.
- The element's transform keeps the axes: a scale, a mirror or a quarter turn. A stroke needs the
  same scale on both axes.
- Subpaths that overlap are painted as one union. Overlapping subpaths under the even-odd rule,
  turning both ways, or crossing themselves (a figure-eight), take another route, because an
  overlap can be a hole.

## CPU layers

A drawing with no series, and with a gradient or a clip that the analytic and marks routes cannot
draw, can be drawn as one CPU layer. The paint stage rasterises the whole drawing into one tile of
the image atlas and draws the tile as one sprite. The sprite takes the element's clip, transform
and opacity. A scroll or a move by whole pixels draws the same sprite again. A static page of such
drawings does not build the general rasteriser. Vector documents take the same route.

- The element's transform must be a scale with two positive axes. A turn, a skew or a mirror
  uses the general route.
- One tile is exact for one scale and one sixteenth-pixel position. During a zoom, the tile of the
  nearest scale (from half to twice) is stretched. Three frames after the zoom stops, the drawing
  is rasterised again at its new scale. Two scales of one drawing are kept.
- While the general rasteriser is not built, a frame rasterises tiles for about 8 ms of estimated
  work. A drawing that does not fit waits at most two frames.
- After that, one frame makes at most three new tiles in about 2 ms. A new drawing waits for two
  stable frames unless it is cheap. A drawing that is rasterised three times in 30 frames gets no
  new tile for the next 30 frames.
- A drawing larger than 2048 pixels on a side or 4 MiB is cut into tiles of 512 pixels, one sprite
  each. Only the tiles that a frame redraws are rasterised, together with the tiles next to them
  while budget is left. A tile that does not fit the frame's budget is drawn at most two frames
  later. A move or a scroll keeps every tile. An edit of one canvas shape rasterises again only the
  tiles the shape meets.
- The tiles are held to four surfaces of RGBA8, from 16 MiB to 128 MiB.

## Clipping

A canvas does not clip its own drawing. To keep a plot inside its box, put the canvas in a box with
`overflow: hidden`.
