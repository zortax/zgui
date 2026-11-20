//! Document identities come back after their windows close, and a stale node handle never
//! resolves in the document that takes an identity over.

mod support;

use std::collections::HashMap;

use zgui_platform::SurfaceAttributes;
use zgui_platform_headless::Harness;
use zgui_runtime::{Runtime, WindowContent, WindowSpec, WindowStatus};
use zgui_view::{Anchor, BuildCx, DOCUMENT_COUNT, DocumentId, IntoView, NodeId, View};

/// The stylesheet every window here is styled by.
const SHEET: &str = "root { display: block } .probe { display: block; height: 4px }";

/// A view that builds one probe.
fn probe(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(zgui_elements::r#box().class("probe").into_view().build(cx))
}

/// Opens a pop-up window with a probe, settles, and answers its identity and its probe's handle.
fn open_one(harness: &mut Harness<Runtime>) -> (zgui_runtime::WindowToken, DocumentId, NodeId) {
    let token = harness.app().window_commands().open(WindowSpec::new(
        SurfaceAttributes::new("dialog"),
        WindowContent::default(),
        Box::new(probe),
    ));
    harness.settle(2);
    let window = harness.app().windows().last().expect("the window opened");
    let node = window.nodes_with_class("probe")[0];
    (token, window.document_id(), node)
}

/// Whether `node` resolves in the open window whose identity is `document`.
fn resolves(harness: &Harness<Runtime>, node: NodeId) -> bool {
    harness
        .app()
        .windows()
        .iter()
        .find(|window| window.document_id() == node.document())
        .is_some_and(|window| {
            let key = zgui_dom::NodeKey::from_u64(node.as_u64()).expect("a node key");
            window.document().borrow().store().index_of(key).is_some()
        })
}

#[test]
fn more_windows_than_there_are_identities_open_and_close_in_turn() {
    let mut harness = support::app(SHEET, probe);
    harness.settle(4);
    // The primary window holds one identity for the whole run.
    let opens = DOCUMENT_COUNT as usize + 200;
    let mut stale: HashMap<DocumentId, NodeId> = HashMap::new();
    let mut reused = 0;
    for _ in 0..opens {
        let (token, document, node) = open_one(&mut harness);
        if let Some(old) = stale.insert(document, node) {
            reused += 1;
            assert!(
                !resolves(&harness, old),
                "a handle of the closed document resolves in its successor"
            );
            assert!(resolves(&harness, node));
        }
        harness.app().window_commands().close(token);
        harness.settle(2);
        assert_eq!(
            harness.app().window_commands().status(token),
            WindowStatus::Closed
        );
        assert_eq!(harness.app().windows().len(), 1);
    }
    assert!(reused >= 200, "identities came back {reused} times");
}
