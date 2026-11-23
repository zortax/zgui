//! An application driven against the headless platform, with a renderer that draws nowhere.
//!
//! Every stage of the real loop runs: the events are dispatched, the reactive work is flushed, the
//! document is restyled, laid out, painted into a display list and handed to a renderer. The only
//! thing that differs from a window on a screen is that the renderer records rather than drawing —
//! which is exactly the part the parking behaviour does not depend on.

#![allow(dead_code, unreachable_pub)]

pub mod churn;

use std::cell::RefCell;
use std::sync::Arc;

use compositor_api::{Compositor, CompositorClient, Renderer};
use compositor_types::RenderTarget;
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, MAINTENANCE_DELAY, Runtime};
use zgui_view::{Anchor, BuildCx};

/// Builds an application whose window holds `view` and draws through [`compositor`] onto a
/// [`screen`].
pub fn app<V>(css: &str, view: V) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let handler = App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_compositor(client())
        .with_renderer(Box::new(screen))
        .into_handler(view)
        .expect("the reactive runtime installs");
    Harness::new(handler)
}

/// The same, with a text engine and a rasteriser behind it.
///
/// The fixed face rather than a real one, deliberately: every number a painting assertion needs —
/// the advance of a cluster, the ascent a glyph is drawn above the baseline, the extent of its
/// pixels — is then computable by hand, so a test can say where a glyph *must* land instead of
/// recording where it happened to.
pub fn app_with_text<V>(css: &str, view: V) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    Harness::new(text_handler(css, view, client()))
}

/// The same, over a compositor the driver `client` runs.
pub fn app_with_text_over<V>(
    client: Box<dyn CompositorClient>,
    css: &str,
    view: V,
) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    Harness::new(text_handler(css, view, client))
}

/// The same again, over a platform the caller has already configured.
///
/// Anything the application reads *while it opens its window* — the desktop's colour scheme, what
/// monitors there are, what the backend can do — has to be true of the platform before the window
/// exists, and a harness that builds its own platform gives a test nowhere to say so.
pub fn app_with_text_on<V>(
    platform: zgui_platform_headless::Headless,
    css: &str,
    view: V,
) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    Harness::over(platform, text_handler(css, view, client()))
}

/// The same over a compositor without the compositor-driven tier, so the window scrolls with its
/// own physics.
pub fn app_scrolling_itself_on<V>(
    platform: zgui_platform_headless::Headless,
    css: &str,
    view: V,
) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let compositor = zgui_testkit_compositor::tierless(compositor());
    Harness::over(platform, text_handler(css, view, driver(compositor)))
}

/// The application itself: the fixed face, the rasteriser, and the recording renderer.
///
/// The fixed face rather than a real one, deliberately: every number a painting assertion needs —
/// the advance of a cluster, the ascent a glyph is drawn above the baseline, the extent of its
/// pixels — is then computable by hand, so a test can say where a glyph *must* land instead of
/// recording where it happened to.
fn text_handler<V>(css: &str, view: V, compositor: Box<dyn CompositorClient>) -> Runtime
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_compositor(compositor)
        .with_renderer(Box::new(screen))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(zgui_testkit::MonoShaper::new()))
        }))
        .into_handler(view)
        .expect("the reactive runtime installs")
}

/// Runs the idle maintenance wake that the last normal frame armed, if the loop parks on it.
///
/// The parked deadline must be no further than [`MAINTENANCE_DELAY`] away. Reaching it must build
/// no frame and leave no deadline, so a test that then expects a silent loop still catches a spin.
pub fn cross_maintenance(harness: &mut Harness<Runtime>) {
    let Some(deadline) = harness.parked_deadline() else {
        return;
    };
    let wait = deadline.saturating_duration_since(harness.now());
    assert!(
        wait <= MAINTENANCE_DELAY,
        "the loop parks {wait:?} out, later than the maintenance wake"
    );
    harness.advance(wait);
    assert_eq!(harness.pump(), 0, "the maintenance wake built a frame");
    assert_eq!(
        harness.parked_deadline(),
        None,
        "a deadline is still parked after the maintenance wake"
    );
}

/// The first line fragment in the window's layout, with the box it belongs to.
///
/// Read out of the fragment tree rather than assumed, because *where layout said* is the thing a
/// glyph assertion is testing against: a constant would be asserting the test's arithmetic. In the
/// line's own space, which is the space its primitives are pushed in.
pub fn first_line_box(
    window: &zgui_runtime::Window,
) -> zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device> {
    let layout = window.layout().borrow();
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            if matches!(
                fragment.kind,
                zgui_layout::fragment::FragmentKind::Line { .. }
            ) {
                return fragment.border_box;
            }
        }
    }
    panic!("the document produced no line fragment at all");
}

/// [`first_line_box`] in device pixels, through the line's anchor: where a pointer aims at it.
pub fn first_line_on_device(
    window: &zgui_runtime::Window,
) -> zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device> {
    *lines_on_device(window)
        .first()
        .expect("the document produced no line fragment at all")
}

/// [`line_boxes`] in device pixels, through each line's anchor.
pub fn lines_on_device(
    window: &zgui_runtime::Window,
) -> Vec<zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>> {
    let layout = window.layout().borrow();
    let placements = window.placements();
    let mut found = Vec::new();
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            if matches!(
                fragment.kind,
                zgui_layout::fragment::FragmentKind::Line { .. }
            ) {
                found.push(fragment.on_device(fragment.border_box, &placements));
            }
        }
    }
    found
}

/// The compositor the windows of a test draw through.
///
/// Its glyph rasteriser draws every glyph as a box on the baseline, half the em size wide and
/// three quarters of it high.
pub fn compositor() -> Box<dyn Compositor> {
    let mut compositor =
        zgui_testkit_compositor::compositor().unwrap_or_else(|why| panic!("no compositor: {why}"));
    zgui_testkit_compositor::with_fake_text(&mut *compositor);
    compositor
}

/// Returns a factory that hands the driver `renderer`, for a window's output.
pub fn hand<R>(renderer: R) -> Result<compositor_api::RendererFactory, AppError>
where
    R: Renderer + Send + 'static,
{
    Ok(Box::new(
        move || Ok(Box::new(renderer) as Box<dyn Renderer>),
    ))
}

/// Returns a [`compositor`] behind the driver the environment selects.
pub fn client() -> Box<dyn CompositorClient> {
    driver(compositor())
}

/// Returns `compositor` behind the driver the environment selects.
pub fn driver(compositor: Box<dyn Compositor>) -> Box<dyn CompositorClient> {
    zgui_testkit_compositor::driver(compositor)
}

thread_local! {
    /// What each surface's [`screen`] shows, in the order the screens were made.
    static SCREENS: RefCell<Vec<(zgui_platform::SurfaceId, zgui_testkit_compositor::Shown)>> =
        const { RefCell::new(Vec::new()) };
}

/// A recording renderer whose last frame is what the window shows.
pub fn screen(
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<compositor_api::RendererFactory, AppError> {
    let shown = zgui_testkit_compositor::Shown::default();
    SCREENS.with(|screens| screens.borrow_mut().push((surface.id(), shown.clone())));
    Ok(Box::new(move || {
        let mut renderer = zgui_testkit_compositor::Screen::new().showing_into(shown);
        renderer.configure(target);
        Ok(Box::new(renderer) as Box<dyn Renderer>)
    }))
}

/// Returns what the [`screen`] of `window` shows.
///
/// # Panics
///
/// Panics when the window does not draw onto a [`screen`].
pub fn shown(window: &zgui_runtime::window::Window) -> zgui_testkit_compositor::Shown {
    let surface = window.surface().id();
    SCREENS.with(|screens| {
        screens
            .borrow()
            .iter()
            .rev()
            .find(|(of, _)| *of == surface)
            .map(|(_, shown)| shown.clone())
            .expect("the window draws onto a screen")
    })
}

/// What `window` shows, as the compositor last drew it.
///
/// # Panics
///
/// Panics when the window does not draw onto a [`screen`], or has drawn no frame.
pub fn drawing(window: &zgui_runtime::window::Window) -> zgui_testkit_compositor::Drawing {
    shown(window).drawing().expect("the window drew a frame")
}

/// Returns how many frames the renderer of `window` has drawn.
///
/// # Panics
///
/// Panics when the window does not draw onto a [`screen`].
pub fn frames_drawn(window: &zgui_runtime::window::Window) -> u64 {
    shown(window).frames()
}

/// Returns the bounds of `instance` in window space, as `[x, y, width, height]`.
///
/// The bounds are mapped through the world matrix in the instance's `space=` field, if it has
/// one. A matrix that is more than a translation yields the box around the mapped corners.
pub fn world(instance: &zgui_testkit_compositor::Instance) -> [f32; 4] {
    let [x, y, width, height] = instance.bounds();
    let Some(space) = instance.field("space") else {
        return [x, y, width, height];
    };

    let numbers: Vec<f32> = space
        .trim_start_matches("translate")
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e'))
        .filter(|word| !word.is_empty())
        .map(|word| word.parse().expect("a matrix entry is a number"))
        .collect();
    let map = |px: f32, py: f32| match numbers.as_slice() {
        [tx, ty] => (px + tx, py + ty),
        [a, b, _, _, c, d, _, _, _, _, _, _, tx, ty, _, _] => {
            (a * px + c * py + tx, b * px + d * py + ty)
        }
        _ => panic!("`space={space}` is neither a translation nor a matrix"),
    };

    let corners = [
        map(x, y),
        map(x + width, y),
        map(x, y + height),
        map(x + width, y + height),
    ];
    let left = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
    let top = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
    let right = corners
        .iter()
        .map(|c| c.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let bottom = corners
        .iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    [left, top, right - left, bottom - top]
}

/// Replays every accessibility update the window published into the consumer that really reads
/// them, and reads every node the way an assistive technology does.
///
/// The check that cannot be self-satisfied. Applying an update is only half the contract: a
/// consumer resolves an explicit relation with `node_by_id(..).unwrap()` at the moment something
/// *asks* for a name, on a thread this process does not own and cannot catch. So the whole
/// sequence is applied — updates are differences, and a relation into a node sent three frames ago
/// is perfectly valid — and then every node is read.
///
/// The alternative, a hand-written set of "identifiers seen so far", cannot express the failure it
/// exists to catch: nodes *leave* the tree, and a set that only grows reports a relation into a
/// dropped subtree as resolvable.
pub fn replay_a11y(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    /// A change handler that records nothing: the claim is that the updates apply and resolve.
    struct Silent;

    impl accesskit_consumer::TreeChangeHandler for Silent {
        fn node_added(&mut self, _: &accesskit_consumer::Node<'_>) {}
        fn node_updated(
            &mut self,
            _: &accesskit_consumer::Node<'_>,
            _: &accesskit_consumer::Node<'_>,
        ) {
        }
        fn focus_moved(
            &mut self,
            _: Option<&accesskit_consumer::Node<'_>>,
            _: Option<&accesskit_consumer::Node<'_>>,
        ) {
        }
        fn node_removed(&mut self, _: &accesskit_consumer::Node<'_>) {}
    }

    fn read(node: accesskit_consumer::Node<'_>) {
        // The two accessors that resolve a relation without checking it, and therefore the two an
        // update carrying a dangling identifier crashes in.
        let _ = node.label();
        let _ = node.controls().count();
        for child in node.children() {
            read(child);
        }
    }

    let log = harness
        .platform()
        .offscreens()
        .first()
        .expect("a surface was created")
        .a11y_log();
    let mut updates = log.into_iter();
    let first = updates
        .next()
        .expect("the window published an accessibility update");
    let mut tree = accesskit_consumer::Tree::new(first, true);
    read(tree.state().root());
    for update in updates {
        tree.update_and_process_changes(update, &mut Silent);
        read(tree.state().root());
    }
}

/// A consumer's copy of the tree, built from every update the window's surface was handed.
///
/// What an assistive technology holds after applying the whole sequence, which is the thing a
/// claim about where a node *is* has to be checked against: a rectangle is stated relative to the
/// nearest transform above it, and only the consumer composes them.
pub fn a11y_tree(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> accesskit_consumer::Tree {
    struct Silent;
    impl accesskit_consumer::TreeChangeHandler for Silent {
        fn node_added(&mut self, _: &accesskit_consumer::Node<'_>) {}
        fn node_updated(
            &mut self,
            _: &accesskit_consumer::Node<'_>,
            _: &accesskit_consumer::Node<'_>,
        ) {
        }
        fn focus_moved(
            &mut self,
            _: Option<&accesskit_consumer::Node<'_>>,
            _: Option<&accesskit_consumer::Node<'_>>,
        ) {
        }
        fn node_removed(&mut self, _: &accesskit_consumer::Node<'_>) {}
    }
    let log = harness
        .platform()
        .offscreens()
        .first()
        .expect("a surface was created")
        .a11y_log();
    let mut updates = log.into_iter();
    let first = updates
        .next()
        .expect("the window published an accessibility update");
    let mut tree = accesskit_consumer::Tree::new(first, true);
    for update in updates {
        tree.update_and_process_changes(update, &mut Silent);
    }
    tree
}

/// Every line fragment in the window's layout, in the order the tree reports them.
///
/// The plural of [`first_line_box`]: a fixture that wraps has a line box per line, and where the
/// *later* ones sit is what a fractional line height decides.
pub fn line_boxes(
    window: &zgui_runtime::Window,
) -> Vec<zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>> {
    let layout = window.layout().borrow();
    let mut found = Vec::new();
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            if matches!(
                fragment.kind,
                zgui_layout::fragment::FragmentKind::Line { .. }
            ) {
                found.push(fragment.border_box);
            }
        }
    }
    found
}

/// What a [`watched`] probe hands its watcher after every frame that drew: what the output shows
/// and the pixels zgui's paint walk recorded again.
pub type Watch = Box<dyn FnMut(&zgui_testkit_compositor::Drawing, &zgui_bits::DamageSet)>;

/// A probe that hands every frame a window drew onto a [`screen`] to a watcher.
struct Watched {
    watch: std::cell::RefCell<Watch>,
    drawn: std::cell::Cell<u64>,
}

impl zgui_runtime::FrameProbe for Watched {
    fn frame_ended(&self, window: &zgui_runtime::window::Window) {
        let drawn = frames_drawn(window);
        if drawn == self.drawn.replace(drawn) {
            return;
        }
        let Some(report) = window.last_frame_report() else {
            return;
        };
        (self.watch.borrow_mut())(&drawing(window), &report.painted);
    }
}

/// Returns a probe that hands every frame drawn onto a [`screen`] to the watcher `make` builds.
pub fn watched(
    mut make: impl FnMut() -> Watch + 'static,
) -> std::rc::Rc<dyn zgui_runtime::FrameProbe> {
    std::rc::Rc::new(Watched {
        watch: std::cell::RefCell::new(make()),
        drawn: std::cell::Cell::new(0),
    })
}
