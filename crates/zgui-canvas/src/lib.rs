//! Retained shape lists an application draws into, reachable from the paint stage by token.
//!
//! A canvas element's shapes cannot travel through the document: a property carries scalars and
//! text by contract, and a shape list is neither. What travels instead is a *token* — the name of
//! a [`CanvasScene`] registered here — plus a revision that moves on every mutation, so the
//! document sees exactly what it needs for invalidation and nothing else. The paint stage
//! resolves the token back through [`resolve`] when it reaches the element's fragment.
//!
//! This is the precedent replaced content set, applied to shapes: *"the painting half is a
//! separate hook installed beside the scene."* The shapes are the painting half; the property is
//! the name.
//!
//! # What a scene holds
//!
//! [`zgui_svg::Shape`]s, exactly as a parsed vector document resolves to them: a path behind an
//! [`Arc`], its own fill and stroke, its own clips. Sharing the vocabulary is what makes a canvas
//! cost nothing new downstream — fitting, per-shape paint, damage and the rasteriser's encoding
//! cache all treat it as one more producer of the thing they already consume.
//!
//! A scene also holds [`Series`]: plot data in data space, drawn with markers or a line of a fixed
//! size. A view transform ([`CanvasScene::set_transform`]) moves every shape and series with one
//! matrix and moves no revision. A pan or a zoom builds no new geometry for the series and for the
//! shapes that become analytic quads or marks; a turn or a stretch, for the series only. Other
//! shapes are placed again under each view.
//!
//! # Threads
//!
//! A scene is `Arc<Mutex<…>>` and a handle is `Send + Sync`: a simulation thread may push shapes
//! and the UI thread may read them. Contention is negligible — the paint stage locks once per
//! damaged canvas per frame — and the alternative, confining mutation to the UI thread, would
//! make the first background producer copy its whole scene across a channel every frame.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex, OnceLock, Weak};

use rustc_hash::FxHashMap;

pub use zgui_svg::{Clip, Fill, Gradient, GradientKind, Ink, Paint, Shape, Stop, Stroke};

/// The colour type every brush takes, re-exported so a caller can name it without taking a
/// dependency on a version that might not be this one.
pub use zgui_color;

/// The name of one registered [`CanvasScene`].
///
/// Never reused: the counter behind it only goes up, so a token held after its scene died
/// resolves to nothing rather than to whoever registered next.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct CanvasToken(pub u32);

/// A retained list of shapes and series, with the revision that says when it last changed.
#[derive(Debug, Default)]
pub struct CanvasScene {
    /// The shapes, in painting order, in canvas-local coordinates: CSS pixels from the content
    /// box's top left corner, or view-box units when the element declares a view box.
    shapes: Vec<Shape>,
    /// The series, each with the number of shapes painted under it. Sorted by that number.
    series: Vec<SeriesAt>,
    /// The view transform: canvas units to canvas units, applied before the view-box fit.
    transform: kurbo::Affine,
    /// Moved by every change of the view transform.
    view: u64,
    /// Moved by every mutating access; what the element's property carries beside the token.
    revision: u64,
}

impl CanvasScene {
    /// Removes every shape and every series. The view transform stays.
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.series.clear();
    }

    /// Adds one shape above everything already there.
    pub fn push(&mut self, shape: Shape) {
        self.shapes.push(shape);
    }

    /// Replaces the whole list, and removes every series.
    pub fn replace(&mut self, shapes: Vec<Shape>) {
        self.shapes = shapes;
        self.series.clear();
    }

    /// The shapes, in painting order.
    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// Adds one series above everything already there.
    ///
    /// A series keeps its data in data space. A change of the view transform moves its points and
    /// keeps its marker size, so a pan builds no new geometry.
    ///
    /// ```no_run
    /// use std::sync::Arc;
    ///
    /// use zgui_canvas::{Brush, Marker, SceneHandle, Series};
    /// use zgui_color::Color;
    ///
    /// let handle = SceneHandle::new();
    /// let data: Arc<[[f32; 2]]> = (0..1000).map(|i| [i as f32, (i as f32).sin()]).collect();
    /// handle.edit(|scene| {
    ///     scene.push_series(Series::Points {
    ///         data,
    ///         to_canvas: kurbo::Affine::new([1.0, 0.0, 0.0, -100.0, 0.0, 150.0]),
    ///         marker: Marker::Circle { radius: 3.0 },
    ///         fill: Some(Brush::Solid(Color::srgb(0.2, 0.5, 0.9, 1.0))),
    ///         stroke: None,
    ///     });
    /// });
    /// // A pan: one matrix changes, and the revision stays.
    /// handle.set_transform(kurbo::Affine::translate((-40.0, 0.0)));
    /// ```
    pub fn push_series(&mut self, series: Series) {
        self.series.push(SeriesAt {
            before: self.shapes.len(),
            series,
        });
    }

    /// The series, in painting order.
    pub fn series(&self) -> &[SeriesAt] {
        &self.series
    }

    /// The shapes and the series, in painting order.
    pub fn items(&self) -> Items<'_> {
        Items {
            scene: self,
            shape: 0,
            series: 0,
        }
    }

    /// Sets the view transform, which maps canvas units to canvas units before the view-box fit.
    ///
    /// Moves the view when `transform` differs from the held one. The default is the identity.
    pub fn set_transform(&mut self, transform: kurbo::Affine) {
        if transform != self.transform {
            self.transform = transform;
            self.view += 1;
        }
    }

    /// The view transform.
    pub fn transform(&self) -> kurbo::Affine {
        self.transform
    }

    /// Where the view transform stands: moved by every change of it.
    pub fn view(&self) -> u64 {
        self.view
    }
}

/// One series, with the number of shapes painted under it.
#[derive(Clone, Debug)]
pub struct SeriesAt {
    /// How many shapes are painted under the series.
    pub before: usize,
    /// The series.
    pub series: Series,
}

/// Plot data in data space, drawn with markers or a line of a fixed size in CSS pixels.
///
/// `to_canvas` maps data to canvas units. The marker radius, the half side, the stroke widths and
/// `stroke.width` of a line are CSS pixels. No transform of the canvas scales them: not
/// `to_canvas`, not the view transform and not the view-box fit. A CSS transform on the element
/// scales them with the rest of the element.
///
/// A NaN point ends a line run, and a points series skips it. Series do not take part in hit
/// testing.
#[derive(Clone, Debug)]
pub enum Series {
    /// One marker per point.
    Points {
        /// The points, in data space.
        data: Arc<[[f32; 2]]>,
        /// Data space to canvas units.
        to_canvas: kurbo::Affine,
        /// The marker.
        marker: Marker,
        /// What fills each marker.
        fill: Option<Brush>,
        /// What strokes each marker, and how wide, in CSS pixels.
        stroke: Option<(Brush, f64)>,
    },
    /// One polyline through the points, with round joins.
    Line {
        /// The points, in data space.
        data: Arc<[[f32; 2]]>,
        /// Data space to canvas units.
        to_canvas: kurbo::Affine,
        /// The width and the caps. The dash pattern is ignored.
        stroke: kurbo::Stroke,
        /// What strokes the line.
        brush: Brush,
    },
}

/// The marker of a points series, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Marker {
    /// A circle.
    Circle {
        /// The radius.
        radius: f64,
    },
    /// An axis-aligned square.
    Square {
        /// Half the side.
        half: f64,
    },
}

/// One item of a scene in painting order.
#[derive(Clone, Copy, Debug)]
pub enum CanvasItem<'a> {
    /// A shape.
    Shape(&'a Shape),
    /// A series.
    Series(&'a Series),
}

/// The items of a scene in painting order. Made by [`CanvasScene::items`].
#[derive(Clone, Debug)]
pub struct Items<'a> {
    /// The scene.
    scene: &'a CanvasScene,
    /// The next shape.
    shape: usize,
    /// The next series.
    series: usize,
}

impl<'a> Iterator for Items<'a> {
    type Item = CanvasItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let scene = self.scene;
        if let Some(at) = scene.series.get(self.series)
            && at.before <= self.shape
        {
            self.series += 1;
            return Some(CanvasItem::Series(&at.series));
        }
        let shape = scene.shapes.get(self.shape)?;
        self.shape += 1;
        Some(CanvasItem::Shape(shape))
    }
}

/// The owning half of one registered scene: mutate through this, and hand the token to a view.
///
/// Cloneable and thread-safe; the registration lives as long as any clone does. Every mutation
/// goes through [`SceneHandle::edit`], which is what moves the revision — a scene reached around
/// the handle would change without the document ever hearing.
#[derive(Clone, Debug)]
pub struct SceneHandle {
    /// The registry key.
    token: CanvasToken,
    /// The scene, shared with the registry's weak entry.
    scene: Arc<Mutex<CanvasScene>>,
}

impl SceneHandle {
    /// Registers an empty scene and returns its handle.
    pub fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let token = CanvasToken(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let scene = Arc::new(Mutex::new(CanvasScene::default()));
        let mut registry = lock();
        registry.retain(|_, held| held.upgrade().is_some());
        registry.insert(token, Arc::downgrade(&scene));
        Self { token, scene }
    }

    /// The name a view carries this scene by.
    pub fn token(&self) -> CanvasToken {
        self.token
    }

    /// Where the scene's content currently stands.
    pub fn revision(&self) -> u64 {
        self.scene
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .revision
    }

    /// Sets the scene's view transform, and returns whether its view moved.
    ///
    /// The revision stays: a view transform changes no content.
    pub fn set_transform(&self, transform: kurbo::Affine) -> bool {
        let mut scene = self
            .scene
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = scene.view;
        scene.set_transform(transform);
        scene.view != before
    }

    /// Where the scene's view transform stands.
    pub fn view(&self) -> u64 {
        self.scene
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .view
    }

    /// Mutates the scene, and moves the revision so whoever shows it repaints.
    pub fn edit<R>(&self, edit: impl FnOnce(&mut CanvasScene) -> R) -> R {
        let mut scene = self
            .scene
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = edit(&mut scene);
        scene.revision += 1;
        result
    }
}

impl Default for SceneHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// The scene `token` names, while any handle to it lives.
pub fn resolve(token: CanvasToken) -> Option<Arc<Mutex<CanvasScene>>> {
    lock().get(&token)?.upgrade()
}

/// The registered scenes, weakly, so a dropped handle unregisters by dying.
fn lock() -> std::sync::MutexGuard<'static, FxHashMap<CanvasToken, Weak<Mutex<CanvasScene>>>> {
    static REGISTRY: OnceLock<Mutex<FxHashMap<CanvasToken, Weak<Mutex<CanvasScene>>>>> =
        OnceLock::new();
    REGISTRY
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What a shape is painted with, before it is lowered to the document vocabulary.
///
/// This exists so an application never hand-assembles [`zgui_svg::Paint`]'s internals: a brush
/// says what it means in the terms a canvas user thinks in, and [`ShapeBuilder`] lowers it.
#[derive(Clone, Debug)]
pub enum Brush {
    /// One colour.
    Solid(zgui_color::Color),
    /// The colour the element inherits — its computed `color` — at this alpha. What
    /// `currentColor` is to a vector document.
    Inherited {
        /// What multiplies the inherited colour's own alpha.
        alpha: f32,
    },
    /// A ramp between stops along a line, in canvas-local coordinates.
    Linear {
        /// Where the ramp starts.
        start: kurbo::Point,
        /// Where it ends.
        end: kurbo::Point,
        /// The stops, as offset-and-colour pairs in offset order.
        stops: Vec<(f32, zgui_color::Color)>,
        /// Whether the ramp repeats past its ends rather than padding.
        repeating: bool,
    },
    /// A ramp between stops out from a centre, in canvas-local coordinates.
    Radial {
        /// The centre.
        center: kurbo::Point,
        /// The horizontal radius.
        radius_x: f64,
        /// The vertical radius.
        radius_y: f64,
        /// The stops, as offset-and-colour pairs in offset order.
        stops: Vec<(f32, zgui_color::Color)>,
        /// Whether the ramp repeats past its ends rather than padding.
        repeating: bool,
    },
}

impl Brush {
    /// The document paint this brush lowers to, for the paint stage.
    pub fn paint(&self) -> Paint {
        self.clone().lower()
    }

    /// The document paint this brush lowers to.
    fn lower(self) -> Paint {
        match self {
            Self::Solid(color) => Paint::Solid(Ink::Solid(color)),
            Self::Inherited { alpha } => Paint::Solid(Ink::Inherited { alpha }),
            Self::Linear {
                start,
                end,
                stops,
                repeating,
            } => Paint::Gradient(gradient(
                GradientKind::Linear { start, end },
                stops,
                repeating,
            )),
            Self::Radial {
                center,
                radius_x,
                radius_y,
                stops,
                repeating,
            } => Paint::Gradient(gradient(
                GradientKind::Radial {
                    center,
                    radius_x,
                    radius_y,
                },
                stops,
                repeating,
            )),
        }
    }
}

/// A document gradient from a brush's stops.
fn gradient(kind: GradientKind, stops: Vec<(f32, zgui_color::Color)>, repeating: bool) -> Gradient {
    let stops: smallvec::SmallVec<[Stop; 4]> = stops
        .into_iter()
        .map(|(offset, color)| Stop {
            offset,
            color: Ink::Solid(color),
        })
        .collect();
    if repeating {
        Gradient::repeating(kind, stops)
    } else {
        Gradient::padded(kind, stops)
    }
}

/// Builds one [`Shape`] from a path and the brushes that paint it.
///
/// ```
/// use zgui_canvas::{Brush, ShapeBuilder};
/// use zgui_color::Color;
///
/// let mut path = kurbo::BezPath::new();
/// path.move_to((0.0, 0.0));
/// path.line_to((32.0, 0.0));
/// path.line_to((16.0, 24.0));
/// path.close_path();
///
/// let shape = ShapeBuilder::new(path)
///     .fill(Brush::Solid(Color::srgb(1.0, 0.0, 0.0, 1.0)))
///     .stroke(Brush::Inherited { alpha: 1.0 }, 2.0)
///     .build();
/// assert!(shape.fill.is_some() && shape.stroke.is_some());
/// ```
#[derive(Debug)]
pub struct ShapeBuilder {
    /// The outline.
    path: Arc<kurbo::BezPath>,
    /// What fills it, if anything yet.
    fill: Option<Fill>,
    /// What strokes it, if anything yet.
    stroke: Option<Stroke>,
    /// The clips it is inside.
    clips: Vec<Clip>,
}

impl ShapeBuilder {
    /// A builder over `path`, drawing nothing until a brush is given.
    pub fn new(path: impl Into<kurbo::BezPath>) -> Self {
        Self {
            path: Arc::new(path.into()),
            fill: None,
            stroke: None,
            clips: Vec::new(),
        }
    }

    /// The same builder over a path that is already shared.
    ///
    /// Identity matters downstream: the rasteriser's encoding cache recognises geometry by the
    /// allocation, so a caller re-using one `Arc` across edits keeps its encoding.
    pub fn shared(path: Arc<kurbo::BezPath>) -> Self {
        Self {
            path,
            fill: None,
            stroke: None,
            clips: Vec::new(),
        }
    }

    /// Fills the outline with `brush`, deciding the inside by the non-zero rule.
    #[must_use]
    pub fn fill(mut self, brush: Brush) -> Self {
        self.fill = Some(Fill {
            paint: brush.lower(),
            rule: peniko::Fill::NonZero,
        });
        self
    }

    /// The same, deciding the inside by the even-odd rule.
    #[must_use]
    pub fn fill_even_odd(mut self, brush: Brush) -> Self {
        self.fill = Some(Fill {
            paint: brush.lower(),
            rule: peniko::Fill::EvenOdd,
        });
        self
    }

    /// Strokes the outline `width` wide with `brush`, with round caps and joins.
    ///
    /// For dashes, caps and joins of one's own, use [`ShapeBuilder::stroke_styled`].
    #[must_use]
    pub fn stroke(self, brush: Brush, width: f64) -> Self {
        self.stroke_styled(
            brush,
            kurbo::Stroke::new(width)
                .with_caps(kurbo::Cap::Round)
                .with_join(kurbo::Join::Round),
        )
    }

    /// Strokes the outline with the whole of `style`.
    #[must_use]
    pub fn stroke_styled(mut self, brush: Brush, style: kurbo::Stroke) -> Self {
        self.stroke = Some(Stroke {
            paint: brush.lower(),
            style,
        });
        self
    }

    /// Keeps the shape inside `clip`, on top of any clip already given.
    #[must_use]
    pub fn clipped(mut self, clip: impl Into<kurbo::BezPath>) -> Self {
        self.clips.push(Clip {
            path: Arc::new(clip.into()),
            rule: peniko::Fill::NonZero,
        });
        self
    }

    /// The finished shape.
    pub fn build(self) -> Shape {
        Shape {
            path: self.path,
            fill: self.fill,
            stroke: self.stroke,
            clips: self.clips,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle() -> kurbo::BezPath {
        let mut path = kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((8.0, 0.0));
        path.line_to((4.0, 8.0));
        path.close_path();
        path
    }

    #[test]
    fn a_handle_registers_and_the_last_drop_unregisters() {
        let handle = SceneHandle::new();
        let token = handle.token();
        assert!(resolve(token).is_some());

        let clone = handle.clone();
        drop(handle);
        assert!(resolve(token).is_some(), "a clone keeps the scene alive");
        drop(clone);
        assert!(resolve(token).is_none(), "the last drop is the unregister");
    }

    #[test]
    fn every_edit_moves_the_revision_exactly_once() {
        let handle = SceneHandle::new();
        assert_eq!(handle.revision(), 0);
        handle.edit(|scene| scene.push(ShapeBuilder::new(triangle()).build()));
        assert_eq!(handle.revision(), 1);
        handle.edit(|scene| {
            scene.clear();
            scene.push(ShapeBuilder::new(triangle()).build());
        });
        assert_eq!(
            handle.revision(),
            2,
            "one edit is one revision, however much it did"
        );
    }

    #[test]
    fn brushes_lower_to_the_document_vocabulary() {
        let solid = ShapeBuilder::new(triangle())
            .fill(Brush::Solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0)))
            .build();
        assert!(matches!(
            solid.fill.as_ref().unwrap().paint,
            Paint::Solid(Ink::Solid(_))
        ));

        let inherited = ShapeBuilder::new(triangle())
            .stroke(Brush::Inherited { alpha: 0.5 }, 2.0)
            .build();
        let stroke = inherited.stroke.as_ref().unwrap();
        assert!(matches!(
            stroke.paint,
            Paint::Solid(Ink::Inherited { alpha }) if alpha == 0.5
        ));
        assert_eq!(stroke.style.width, 2.0);

        let ramp = ShapeBuilder::new(triangle())
            .fill(Brush::Linear {
                start: kurbo::Point::ZERO,
                end: kurbo::Point::new(8.0, 0.0),
                stops: vec![
                    (0.0, zgui_color::Color::srgb(0.0, 0.0, 0.0, 1.0)),
                    (1.0, zgui_color::Color::srgb(1.0, 1.0, 1.0, 1.0)),
                ],
                repeating: false,
            })
            .build();
        assert!(matches!(
            ramp.fill.as_ref().unwrap().paint,
            Paint::Gradient(_)
        ));
    }

    /// A points series of two points.
    fn series() -> Series {
        Series::Points {
            data: Arc::from([[0.0, 0.0], [1.0, 1.0]]),
            to_canvas: kurbo::Affine::IDENTITY,
            marker: Marker::Circle { radius: 2.0 },
            fill: Some(Brush::Inherited { alpha: 1.0 }),
            stroke: None,
        }
    }

    #[test]
    fn a_series_paints_between_the_shapes_around_it() {
        let mut scene = CanvasScene::default();
        scene.push_series(series());
        scene.push(ShapeBuilder::new(triangle()).build());
        scene.push(ShapeBuilder::new(triangle()).build());
        scene.push_series(series());
        scene.push(ShapeBuilder::new(triangle()).build());
        scene.push_series(series());
        let order: String = scene
            .items()
            .map(|item| match item {
                CanvasItem::Shape(_) => 's',
                CanvasItem::Series(_) => 'p',
            })
            .collect();
        assert_eq!(order, "psspsp");
        assert_eq!(scene.shapes().len(), 3);
        let before: Vec<usize> = scene.series().iter().map(|at| at.before).collect();
        assert_eq!(before, [0, 2, 3]);
    }

    #[test]
    fn set_transform_moves_the_view_and_not_the_revision() {
        let handle = SceneHandle::new();
        assert_eq!(handle.view(), 0);
        assert!(handle.set_transform(kurbo::Affine::translate((4.0, 0.0))));
        assert_eq!(handle.view(), 1);
        assert_eq!(handle.revision(), 0);
        let scene = resolve(handle.token()).expect("registered");
        let transform = scene
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .transform();
        assert_eq!(transform, kurbo::Affine::translate((4.0, 0.0)));
    }

    #[test]
    fn an_equal_transform_moves_nothing() {
        let handle = SceneHandle::new();
        assert!(!handle.set_transform(kurbo::Affine::IDENTITY));
        assert!(handle.set_transform(kurbo::Affine::scale(2.0)));
        assert!(!handle.set_transform(kurbo::Affine::scale(2.0)));
        assert_eq!((handle.view(), handle.revision()), (1, 0));
    }

    #[test]
    fn clear_keeps_the_transform() {
        let mut scene = CanvasScene::default();
        scene.set_transform(kurbo::Affine::scale(3.0));
        scene.push(ShapeBuilder::new(triangle()).build());
        scene.push_series(series());
        scene.clear();
        assert!(scene.shapes().is_empty() && scene.series().is_empty());
        assert_eq!(scene.transform(), kurbo::Affine::scale(3.0));
        assert_eq!(scene.view(), 1);
    }

    #[test]
    fn replace_drops_the_series() {
        let mut scene = CanvasScene::default();
        scene.push_series(series());
        scene.replace(vec![ShapeBuilder::new(triangle()).build()]);
        assert!(scene.series().is_empty());
        assert_eq!(scene.items().count(), 1);
    }

    #[test]
    fn a_scene_is_editable_from_another_thread() {
        let handle = SceneHandle::new();
        let worker = handle.clone();
        std::thread::spawn(move || {
            worker.edit(|scene| scene.push(ShapeBuilder::new(triangle()).build()));
        })
        .join()
        .expect("the worker finishes");
        assert_eq!(handle.revision(), 1);
        let scene = resolve(handle.token()).expect("still registered");
        assert_eq!(
            scene
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .shapes()
                .len(),
            1
        );
    }
}
