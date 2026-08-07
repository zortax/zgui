//! The embed seam, exercised through the real frame loop with no device anywhere.
//!
//! The wgpu host is a consumer of the runtime's seam, not its definition; what is pinned here is
//! the contract any host relies on: the sync step runs each frame between layout and paint, sees
//! the document's revision, can resolve `surface` elements to their tokens, and what it files in
//! the intrinsics slot is what layout sizes the box by — because a `surface` element is born
//! replaced, exactly as an image is. The full `WgpuSurfaces` host runs the same paths with a
//! device behind them; headless, `Renderer::as_any_mut` answers `None` and it degrades to
//! exactly the bookkeeping this test's fixture host performs.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_dom::host::replaced::{Intrinsic, ReplacedId};
use zgui_geom::{CssPx, Size};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::embed::{EmbedHost, EmbedSyncCx, EmbedSyncReport};
use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{Anchor, BuildCx, IntoView, View};
use zgui_wgpu::SurfaceElementExt;

/// An application whose window records rather than draws, with a deterministic shaper behind it.
fn app<V>(css: &str, view: V) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let handler = App::new()
        .with_title("seam")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
                zgui_render::Renderer::configure(&mut renderer, target);
                Ok(Box::new(renderer) as Box<dyn zgui_render::Renderer>)
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| {
            std::sync::Arc::new(zgui_testkit_scene::MonoRaster::new())
        }))
        .into_handler(view)
        .expect("the reactive runtime installs");
    Harness::new(handler)
}

/// What the syncs observed, shared with the assertions.
#[derive(Default)]
struct Observed {
    /// How many times sync ran.
    syncs: usize,
    /// The token read off the surface element, once found.
    token: Option<u64>,
    /// The revisions the syncs saw.
    revisions: Vec<u64>,
}

/// A host that files one fixed natural size for every surface element it finds.
struct FixedSizeHost {
    /// What the assertions read.
    observed: Rc<RefCell<Observed>>,
    /// The natural size it files, in CSS pixels.
    natural: (f32, f32),
}

impl EmbedHost for FixedSizeHost {
    fn sync(&mut self, cx: &mut EmbedSyncCx<'_>) -> EmbedSyncReport {
        let mut observed = self.observed.borrow_mut();
        observed.syncs += 1;
        observed.revisions.push(cx.revision);

        let document = cx.document.borrow();
        let store = document.store();
        let mut found = Vec::new();
        for slot in 0..store.slot_count() as u32 {
            let index = zgui_dom::NodeIndex::new(slot);
            let Some(record) = store.try_core(index) else {
                continue;
            };
            if !record
                .flags()
                .contains(zgui_dom::node::flags::NodeFlags::IS_REPLACED)
            {
                continue;
            }
            let key = store.key_of(index);
            if let Some(token) = zgui_dom::side::surface::token(store, key) {
                found.push((key, token));
            }
        }
        drop(document);

        for (node, token) in found {
            if observed.token.replace(token).is_none() {
                // First sight: claim the node with the natural size, and mark it so the box is
                // rebuilt with it — the exact two calls the wgpu host makes at bind.
                cx.intrinsics.set(
                    ReplacedId::new(node),
                    Intrinsic {
                        size: Some(Size::new(CssPx(self.natural.0), CssPx(self.natural.1))),
                        ratio: None,
                        baseline: None,
                    },
                );
                cx.replaced_content_changed(node);
            }
        }
        EmbedSyncReport::default()
    }
}

/// The border box of the surface element's fragment, if it has one yet.
fn surface_box(window: &zgui_runtime::Window) -> Option<(f32, f32)> {
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    for key in layout.keys() {
        let node = layout.node(key);
        let Some(source) = node.source else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if document.store().core(index).local_name().as_str() != "surface" {
            continue;
        }
        let fragment = layout.fragments_of_box(key).first()?;
        let fragment = layout.fragment(*fragment)?;
        return Some((
            fragment.border_box.size.width.0,
            fragment.border_box.size.height.0,
        ));
    }
    None
}

#[test]
fn a_host_sizes_a_surface_element_through_the_seam() {
    let observed = Rc::new(RefCell::new(Observed::default()));
    let host = Rc::clone(&observed);
    let handle = zgui_wgpu::SurfaceHandle::new(zgui_wgpu::SurfaceConfig::default());

    let mut app = app(
        "root { display: block; width: 400px; height: 300px }",
        move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(zgui_elements::surface().source(&handle))
                    .into_view()
                    .build(cx),
            )
        },
    );
    app.app_mut().windows_mut()[0].install_embed_host(Box::new(FixedSizeHost {
        observed: host,
        natural: (40.0, 30.0),
    }));
    app.settle(16);

    {
        let observed = observed.borrow();
        assert!(observed.syncs > 0, "the sync step ran");
        assert!(
            observed.token.is_some(),
            "the element's token is readable through the document"
        );
        assert!(
            observed.revisions.windows(2).all(|pair| pair[0] <= pair[1]),
            "the revision a host scans at never goes backwards"
        );
    }
    assert_eq!(
        surface_box(&app.app_mut().windows_mut()[0]),
        Some((40.0, 30.0)),
        "what the host filed in the intrinsics slot is what layout sized the box by"
    );
}

#[test]
fn the_real_host_degrades_to_bookkeeping_with_no_device() {
    // The production host over a capture renderer: `as_any_mut` answers `None`, so nothing is
    // attached — and nothing panics, which is what a headless test of an app with surface
    // elements in it needs. The intrinsic it files at bind still reaches layout.
    let handle = zgui_wgpu::SurfaceHandle::new(zgui_wgpu::SurfaceConfig {
        premultiplied: true,
        intrinsic: zgui_wgpu::SurfaceIntrinsic::size(64.0, 48.0),
    });
    // The producer's own clone, held for the life of the test the way a real producer holds one:
    // the registry answers for a token only while some clone lives.
    let producer = handle.clone();
    let mut app = app(
        "root { display: block; width: 400px; height: 300px }",
        move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(zgui_elements::surface().source(&handle))
                    .into_view()
                    .build(cx),
            )
        },
    );
    app.app_mut().windows_mut()[0]
        .install_embed_host(Box::new(zgui_wgpu::WgpuSurfaces::new()));
    app.settle(16);

    assert_eq!(
        surface_box(&app.app_mut().windows_mut()[0]),
        Some((64.0, 48.0)),
        "the wgpu host's bookkeeping half runs whole with no device behind it"
    );
    drop(producer);
}
