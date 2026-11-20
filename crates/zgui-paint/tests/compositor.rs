//! Fragments recorded as compositor chunks.

mod support;

use compositor_api::{Compositor, OutputConfig, Renderer};
use compositor_types::{BufferAge, Built, Frame, FrameClock, FrameRequest, RenderTarget};
use support::{Element, Harness};
use zgui_paint::{CompositorSync, SyncReport};

/// A compositor, a renderer and an output of 200 by 100 pixels, from the test factory.
struct Output {
    compositor: Box<dyn Compositor>,
    renderer: Box<dyn Renderer>,
    client: compositor_types::ClientId,
    output: compositor_types::OutputId,
    frame: Frame,
}

impl Output {
    fn new() -> Option<Self> {
        let made = (
            zgui_testkit_compositor::compositor(),
            zgui_testkit_compositor::renderer([200, 100]),
        );
        let (mut compositor, mut renderer) = match made {
            (Ok(compositor), Ok(renderer)) => (compositor, renderer),
            (Err(why), _) | (_, Err(why)) => {
                eprintln!("skipped: {why}");
                return None;
            }
        };
        let config = OutputConfig {
            size: [200, 100],
            scale: 1.0,
            opaque: true,
        };
        renderer.configure(RenderTarget {
            size: config.size,
            scale: config.scale,
            opaque: config.opaque,
        });
        let (client, _ids) = compositor.register_client();
        let output = compositor.create_output(config);
        Some(Self {
            compositor,
            renderer,
            client,
            output,
            frame: Frame::default(),
        })
    }

    /// Paints every fragment through `harness`'s painter, commits, and builds and draws a frame.
    fn frame(&mut self, harness: &mut Harness) -> (SyncReport, Built) {
        if harness.painter.compositor().is_none() {
            harness
                .painter
                .attach_compositor(CompositorSync::new(self.output));
        }
        let transaction = self.compositor.transaction(self.client);
        let sync = harness.painter.compositor().expect("attached above");
        sync.open(transaction);
        harness.paint_everything();
        let sync = harness.painter.compositor().expect("attached above");
        let transaction = sync
            .close(&harness.store, &mut harness.scene)
            .expect("opened above");
        let report = sync.report();
        self.compositor
            .commit(transaction)
            .expect("the transaction commits");
        let caps = self.renderer.capabilities();
        let request = FrameRequest {
            caps: &caps,
            age: self.renderer.target_age(),
            clock: FrameClock::default(),
            subpixel: None,
        };
        let built = self
            .compositor
            .build_frame(self.output, &request, &mut self.frame);
        if built == Built::Frame {
            let outcome = self.renderer.draw(&self.frame);
            self.compositor
                .frame_done(self.output, self.frame.id, &outcome);
        }
        (report, built)
    }
}

fn panels() -> Harness {
    Harness::sized(
        Element::new("root").children(vec![
            Element::new("a").classes(&["panel"]),
            Element::new("b").classes(&["panel"]),
        ]),
        "root { display: block; width: 200px; height: 100px; background: white }
         .panel { display: block; width: 80px; height: 30px; background: blue;
                  border: 2px solid red; border-radius: 4px }",
        200.0,
        100.0,
    )
}

#[test]
fn a_moved_fragment_writes_its_offset_once() {
    let Some(mut output) = Output::new() else {
        return;
    };
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("spacer"),
            Element::new("mark").classes(&["panel"]),
        ]),
        "root { display: block; width: 200px; height: 100px }
         spacer { display: block; height: 10px }
         .tall { height: 30px }
         .panel { display: block; width: 80px; height: 30px; background: blue }",
        200.0,
        100.0,
    );
    output.frame(&mut harness);

    let spacer = harness.element("spacer");
    harness.edit_and_restyle(|edit| edit.add_class(spacer, zgui_interned::ClassName::new("tall")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [spacer]);
    harness.compose_from_marks(200.0, 100.0);
    let (moved, built) = output.frame(&mut harness);
    let (again, _) = output.frame(&mut harness);

    assert_eq!(built, Built::Frame);
    assert_eq!(moved.offsets, 1, "{moved:?}");
    assert_eq!(again.offsets, 0, "{again:?}");
}

#[test]
fn every_painted_quad_reaches_the_compositor_frame() {
    let Some(mut output) = Output::new() else {
        return;
    };
    let mut harness = panels();

    let (report, built) = output.frame(&mut harness);

    assert_eq!(built, Built::Frame);
    assert_eq!(report.omitted_primitives, 0);
    assert!(report.recorded >= 3, "{report:?}");
    assert!(
        output.compositor.counters().frame.items_drawn
            >= harness.scene.primitives.quads.len() as u32,
    );
}

#[test]
fn a_repaint_that_changed_nothing_records_nothing_and_builds_no_frame() {
    let Some(mut output) = Output::new() else {
        return;
    };
    let mut harness = panels();
    output.frame(&mut harness);

    let (report, built) = output.frame(&mut harness);

    assert_eq!(report.recorded + report.offsets + report.removed, 0);
    assert_eq!(report.order_entries, 0);
    assert_eq!(report.table_writes, 0);
    // Only a compositor with bounded damage and a renderer that keeps its target can skip it.
    if output.compositor.capabilities().precise && output.renderer.target_age() == BufferAge::Kept {
        assert!(matches!(built, Built::Idle { .. }), "{built:?}");
    }
}
