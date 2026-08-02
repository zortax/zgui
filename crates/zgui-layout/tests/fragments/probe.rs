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
