//! The accessibility tree the gallery publishes, read by a real consumer.
//!
//! What a window publishes is a stream of *differences*, and what a screen reader holds is the
//! result of applying all of them. Asserting on the updates is asserting on our own structure;
//! applying them through `accesskit_consumer` — the crate every platform adapter is built on — and
//! then asking the result the questions a reader asks is a different claim, and it is the one that
//! matters. A control the consumer cannot name is a control nobody can be told about.

#[path = "../examples/gallery/app.rs"]
mod app;
#[path = "../examples/gallery/section/mod.rs"]
mod section;
#[path = "../examples/gallery/shell.rs"]
mod shell;

use std::sync::{Arc, Mutex};

use accesskit_consumer::{Node, Tree, TreeChangeHandler, common_filter};
use zgui::platform::{AppHandler, PlatformError};
use zgui::view;

use crate::app::GalleryProps;

/// A change handler that records nothing: the claim is about the tree the updates leave behind.
struct Silent;

impl TreeChangeHandler for Silent {
    fn node_added(&mut self, _: &Node<'_>) {}
    fn node_updated(&mut self, _: &Node<'_>, _: &Node<'_>) {}
    fn focus_moved(&mut self, _: Option<&Node<'_>>, _: Option<&Node<'_>>) {}
    fn node_removed(&mut self, _: &Node<'_>) {}
}

/// A renderer that accepts a frame and draws nowhere.
mod nowhere {
    use std::sync::Arc;

    use zgui::atlas::{SinkError, TextureFormat, TextureId, TextureSink};
    use zgui::geom::{Device, Rect, Size};
    use zgui::platform::Surface;
    use zgui::render::{
        ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
        Renderer, TextureHandle,
    };
    use zgui::runtime::AppError;

    /// A texture sink that accepts every upload and holds nothing.
    struct Sink;

    impl TextureSink for Sink {
        fn create_texture(
            &mut self,
            _: TextureId,
            _: Size<i32, Device>,
            _: TextureFormat,
        ) -> Result<(), SinkError> {
            Ok(())
        }

        fn write_texture(
            &mut self,
            _: TextureId,
            _: Rect<i32, Device>,
            _: TextureFormat,
            _: &[u8],
        ) -> Result<(), SinkError> {
            Ok(())
        }

        fn destroy_texture(&mut self, _: TextureId) {}
    }

    /// The renderer itself.
    pub(crate) struct Nowhere {
        /// The surface it was configured for.
        pub(crate) target: Option<RenderTarget>,
        /// Where tiles go.
        sink: Sink,
    }

    impl Renderer for Nowhere {
        fn capabilities(&self) -> RenderCapabilities {
            RenderCapabilities::MINIMAL
        }

        fn configure(&mut self, target: RenderTarget) {
            self.target = Some(target);
        }

        fn target(&self) -> Option<RenderTarget> {
            self.target
        }

        fn draw(&mut self, _: &zgui::scene::Scene, _: &zgui::bits::DamageSet) -> FrameOutcome {
            FrameOutcome::Presented(FrameStats::default())
        }

        fn register_external(&mut self, _: ExternalTexture) -> TextureHandle {
            TextureHandle(1)
        }

        fn release_external(&mut self, _: TextureHandle) {}

        fn memory(&self) -> MemoryReport {
            MemoryReport::ZERO
        }

        fn texture_sink(&mut self) -> &mut dyn TextureSink {
            &mut self.sink
        }
    }

    /// Builds one.
    pub(crate) fn build(
        _: &Arc<dyn Surface>,
        target: RenderTarget,
    ) -> Result<Box<dyn Renderer>, AppError> {
        Ok(Box::new(Nowhere {
            target: Some(target),
            sink: Sink,
        }))
    }
}

/// What a walk of the settled tree found.
#[derive(Default)]
struct Found {
    /// How many nodes there are.
    nodes: usize,
    /// Every node that can be reached by keyboard, with the name of its role and its own name.
    ///
    /// The role is kept as the name a consumer reports rather than as the enumerated value: the
    /// enumeration belongs to the accessibility engine and this package does not name it, and what
    /// is being asserted is what a reader would be told, which is the name.
    focusable: Vec<(String, String)>,
    /// Which role names appeared.
    roles: Vec<String>,
}

/// Walks `node` and everything under it.
fn walk(node: &Node<'_>, found: &mut Found) {
    found.nodes += 1;
    found.roles.push(format!("{:?}", node.role()));
    // The two accessors that resolve a relation without checking it, and therefore the two an
    // update carrying a dangling identifier crashes in.
    let name = node.label().unwrap_or_default();
    let _ = node.controls().count();
    if node.is_focusable(&|candidate: &Node<'_>| common_filter(candidate)) {
        found.focusable.push((format!("{:?}", node.role()), name));
    }
    for child in node.children() {
        walk(&child, found);
    }
}

/// Opens the gallery, applies every update it published, and walks the result.
///
/// # Panics
///
/// Panics when the window published nothing, which would make every assertion below it vacuous.
fn settled() -> Found {
    let updates: Arc<Mutex<Vec<_>>> = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&updates);
    let driver = move |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = zgui_platform_headless::Harness::new(handler);
        harness.deliver_to_first(zgui::platform::SurfaceEvent::Resized(
            zgui::geom::Size::new(
                zgui::geom::DevicePx(crate::app::WIDTH),
                zgui::geom::DevicePx(crate::app::HEIGHT),
            ),
        ));
        harness.settle(128);
        let log = harness
            .platform()
            .offscreens()
            .first()
            .expect("a surface was created")
            .a11y_log();
        *collected.lock().unwrap_or_else(|held| held.into_inner()) = log;
        harness.shut_down();
        Ok(())
    };

    zgui::app()
        .with_title("gallery")
        .with_size(crate::app::WIDTH, crate::app::HEIGHT)
        .with_stylesheet(crate::shell::SHEET)
        .with_renderer(Box::new(nowhere::build))
        .run_on(driver, || view! { Gallery() })
        .expect("the gallery ran");

    let published = core::mem::take(&mut *updates.lock().unwrap_or_else(|held| held.into_inner()));
    let mut published = published.into_iter();
    let first = published
        .next()
        .expect("the window published an accessibility update");
    let mut tree = Tree::new(first, true);
    for update in published {
        tree.update_and_process_changes(update, &mut Silent);
    }
    let mut found = Found::default();
    walk(&tree.state().root(), &mut found);
    found
}

#[test]
fn the_gallery_publishes_a_tree_a_consumer_can_read() {
    let found = settled();
    assert!(
        found.nodes > 100,
        "the consumer resolved only {} nodes for a page of every component in the library",
        found.nodes
    );
    assert!(
        found.focusable.len() > 20,
        "only {} of the gallery's controls are reachable by keyboard as far as the consumer is \
         concerned",
        found.focusable.len()
    );
}

/// The roles whose whole purpose is to be operated, and which therefore have to say what they do.
///
/// Structural roles are deliberately not here. A window takes its name from the title the platform
/// adapter sets, a scroll region and a scroll bar are announced by what they are, and demanding a
/// name of those would be demanding one the interface has no word for.
const OPERABLE: [&str; 9] = [
    "Button",
    "CheckBox",
    "RadioButton",
    "Switch",
    "Slider",
    "TextInput",
    "MultilineTextInput",
    "ComboBox",
    "Tab",
];

#[test]
fn every_control_a_keyboard_can_operate_has_a_name_a_reader_can_say() {
    let found = settled();
    // A focusable control with no label is announced as its role and nothing else — "button",
    // "checkbox" — which is the difference between an interface somebody can use and one they have
    // to guess their way through.
    let nameless: Vec<_> = found
        .focusable
        .iter()
        .filter(|(role, name)| OPERABLE.contains(&role.as_str()) && name.trim().is_empty())
        .map(|(role, _)| role.clone())
        .collect();
    let operable = found
        .focusable
        .iter()
        .filter(|(role, _)| OPERABLE.contains(&role.as_str()))
        .count();
    assert!(
        operable > 20,
        "only {operable} operable controls were reachable, so this is measuring the wrong page"
    );
    assert!(
        nameless.is_empty(),
        "{} of the gallery's {operable} keyboard-operable controls have no accessible name: \
         {nameless:?}",
        nameless.len()
    );
}

#[test]
fn the_gallery_s_controls_are_published_as_what_they_are() {
    let found = settled();
    // A tree of generic containers is a tree that parses and says nothing. Each of these is a role
    // some component in the gallery is *supposed* to publish, and a component that stopped
    // publishing it would still produce a readable tree.
    for role in [
        "Button",
        "CheckBox",
        "RadioButton",
        "Switch",
        "Slider",
        "TextInput",
        "Tab",
    ] {
        assert!(
            found.roles.iter().any(|found| found == role),
            "no node in the gallery's tree is a {role}"
        );
    }
}
