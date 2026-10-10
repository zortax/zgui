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
A pan or a zoom through it builds no new geometry: recognised shapes and series keep their
payloads, and the renderer uploads none.

```rust,ignore
let handle = CanvasHandle::new();
handle.draw(|scene| scene.push_series(series));
// On every drag step:
handle.set_transform(kurbo::Affine::translate((dx, 0.0)));
```

The transform stays when a draw closure clears the scene for its next run.

## Series

A `Series` holds plot data in data space as an `Arc<[[f32; 2]]>`, with a `to_canvas` matrix:

- `Series::Points` draws one marker per point: a circle or a square, with a fill, a stroke or both.
- `Series::Line` draws one polyline through the points, with round joins.

Marker sizes and line widths are CSS pixels. No canvas transform scales them: not `to_canvas`,
not the view transform and not the view-box fit. A CSS transform on the element scales them with
the rest of the element. A NaN point ends a line run, and a points series skips it.

A series is drawn as one union mark per part. Its payload is built once per data allocation, so
keep the data in one `Arc` and change the view to move it. Data far from the origin keeps its
precision: positions are stored relative to the centre of the data.

`push_series` places the series above the shapes pushed so far. Series take no part in hit
testing.

## Clipping

A canvas does not clip its own drawing. To keep a plot inside its box, put the canvas in a box with
`overflow: hidden`.
