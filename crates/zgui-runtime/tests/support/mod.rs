//! An application driven against the headless platform, with a renderer that draws nowhere.
//!
//! Every stage of the real loop runs: the events are dispatched, the reactive work is flushed, the
//! document is restyled, laid out, painted into a display list and handed to a renderer. The only
//! thing that differs from a window on a screen is that the renderer records rather than drawing —
//! which is exactly the part the parking behaviour does not depend on.

#![allow(dead_code, unreachable_pub)]

pub mod churn;

use std::sync::Arc;

use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_render::{RenderTarget, Renderer};
use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{Anchor, BuildCx};

/// Builds an application whose window holds `view`, with the capture renderer under it.
pub fn app<V>(css: &str, view: V) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let handler = App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(capture))
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
    Harness::new(text_handler(css, view))
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
    Harness::over(platform, text_handler(css, view))
}

/// The application itself: the fixed face, the rasteriser, and the recording renderer.
///
/// The fixed face rather than a real one, deliberately: every number a painting assertion needs —
/// the advance of a cluster, the ascent a glyph is drawn above the baseline, the extent of its
/// pixels — is then computable by hand, so a test can say where a glyph *must* land instead of
/// recording where it happened to.
fn text_handler<V>(css: &str, view: V) -> Runtime
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(capture))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| {
            std::sync::Arc::new(zgui_testkit_scene::MonoRaster::new())
        }))
        .into_handler(view)
        .expect("the reactive runtime installs")
}

/// The first line fragment in the window's layout, with the box it belongs to.
///
/// Read out of the fragment tree rather than assumed, because *where layout said* is the thing a
/// glyph assertion is testing against: a constant would be asserting the test's arithmetic.
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

/// A renderer that records the display list instead of drawing it.
fn capture(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
    renderer.configure(target);
    Ok(Box::new(renderer))
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
