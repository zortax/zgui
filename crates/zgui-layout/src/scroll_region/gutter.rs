//! The gutter a locked scroll container keeps.
//!
//! Opening a modal usually means stopping the page behind it from scrolling, and the obvious way to
//! do that — `overflow: hidden` on the root — takes the scrollbar away. The gutter it occupied is
//! then given back to the content, every line re-wraps a few pixels wider, and the whole page
//! jumps sideways behind the modal.
//!
//! CSS has an answer, `scrollbar-gutter: stable`, and this build's style engine does not generate
//! that longhand. So the reservation is layout's: a container can be *locked*, and while it is,
//! layout keeps reserving whatever gutter it was reserving when the lock was taken, whatever the
//! style now says. Nothing else changes — the container really does stop scrolling — and the
//! content keeps the width it had.

use zgui_dom::side::BoxKey;

use crate::tree::store::LayoutStore;

/// Stops `key` scrolling without letting its content change width.
///
/// The gutter the container is reserving right now is recorded and kept, so a style change to
/// `overflow: hidden` taken after this call reserves the same space it did before. Locking a
/// container that reserves no gutter is not an error and costs nothing: there is no space to keep.
pub fn lock(store: &mut LayoutStore, key: BoxKey) {
    let reserved = reserved(store, key);
    lock_axes(store, key, reserved);
}

/// The same, for a caller that knows which axes to hold rather than reading them from a layout.
///
/// This is what a modal opening across a document rebuild needs: the container it is locking has
/// been laid out, but not necessarily in the store the lock is being taken in.
pub fn lock_axes(store: &mut LayoutStore, key: BoxKey, axes: (bool, bool)) {
    store.set_scroll_lock(key, Some(axes));
}

/// Which axes of one container are reserving a gutter as it currently stands.
///
/// A scrollbar on the block axis takes width off the content, so the answer is read from the
/// perpendicular extent — which is the one place that has to be got the right way round, and it is
/// got right once here rather than at each caller.
pub fn reserved(store: &LayoutStore, key: BoxKey) -> (bool, bool) {
    let held = store.auto_scroll(key);
    store
        .layout_of(key)
        .map(|layout| {
            (
                held.0 || layout.scrollbar_size.height.0 > 0.0,
                held.1 || layout.scrollbar_size.width.0 > 0.0,
            )
        })
        .unwrap_or(held)
}

/// Lets `key` size itself by its own style again.
pub fn unlock(store: &mut LayoutStore, key: BoxKey) {
    store.set_scroll_lock(key, None);
}

/// Whether one container is holding a gutter its style no longer asks for.
pub fn is_locked(store: &LayoutStore, key: BoxKey) -> bool {
    store.scroll_lock(key).is_some()
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;
    use zgui_css::StyleDraft;
    use zgui_dom::side::BoxKey;

    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::tree::store::LayoutStore;

    use super::{is_locked, lock, unlock};

    /// A store holding one box, which is enough to exercise the lock's bookkeeping.
    fn one_box() -> (LayoutStore, BoxKey) {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let key = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        (store, key)
    }

    #[test]
    fn a_container_is_unlocked_until_it_is_locked_and_again_afterwards() {
        let (mut store, key) = one_box();
        assert!(!is_locked(&store, key));
        lock(&mut store, key);
        assert!(is_locked(&store, key));
        unlock(&mut store, key);
        assert!(!is_locked(&store, key));
    }
}
