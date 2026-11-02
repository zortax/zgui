//! Finding the box and the fragment a fixture element produced.

use zgui_layout::{BoxKey, LayoutStore};

use crate::support::Fixture;

/// The box a named element generated, searched for by element name.
pub(crate) fn box_named(store: &LayoutStore, fixture: &Fixture, name: &str) -> BoxKey {
    let mut stack = vec![fixture.root];
    while let Some(index) = stack.pop() {
        let core = fixture.document.store().core(index);
        if core.local_name().as_str() == name {
            let key = fixture.document.store().key_of(index);
            let boxes = store.boxes_of(key);
            assert!(!boxes.is_empty(), "`{name}` generated no box");
            return boxes[0];
        }
        let mut child = core.first_child();
        while let Some(index) = child {
            stack.push(index);
            child = fixture.document.store().core(index).next_sibling();
        }
    }
    panic!("no element named `{name}`");
}

/// The first fragment one box produced, which is the box's own piece.
pub(crate) fn own_fragment(store: &LayoutStore, key: BoxKey) -> &zgui_layout::Fragment {
    let frag = *store
        .fragments_of_box(key)
        .first()
        .expect("every box produces its own piece");
    store.fragment(frag).expect("live")
}

/// A box's own border box in device pixels, through the fragment's anchor.
pub(crate) fn device_box(
    store: &LayoutStore,
    frame: &crate::support::Frame,
    key: BoxKey,
) -> zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device> {
    let fragment = own_fragment(store, key);
    fragment.on_device(
        fragment.border_box,
        &zgui_scene::Placements::of(&frame.spatial),
    )
}

/// The matrix `space` resolves to, for points measured from the document's own origin.
///
/// Outside the poison test mode the document's origin is the device's and this is the resolved
/// matrix itself.
pub(crate) fn drawn_matrix(
    frame: &crate::support::Frame,
    space: zgui_scene::SpatialId,
) -> zgui_geom::Matrix4 {
    let poison = zgui_scene::spatial::poison::origin();
    let resolved = frame
        .spatial
        .resolve(space)
        .expect("a live coordinate system");
    zgui_geom::Matrix4::translation(poison, poison, 0.0).then(&resolved)
}
