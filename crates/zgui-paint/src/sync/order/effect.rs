//! Group effects written as compositor effect nodes.

use compositor_types::{ClipId, EffectId, EffectNode, OrderId, Transaction};

use zgui_scene::Scene;

use crate::emit::group::GroupEffect;
use crate::sync::mirror::Mirror;

/// The effect node an order node was given.
#[derive(Clone, Debug)]
pub(crate) struct Written {
    id: EffectId,
    node: EffectNode,
}

/// Writes `wanted` as the effect of the order node `order`, which `written` held, and returns
/// what the node holds now and the number of writes.
///
/// The effect node holds the clip node it names.
pub(crate) fn write(
    tx: &mut Transaction,
    mirror: &mut Mirror,
    scene: &mut Scene,
    order: OrderId,
    written: Option<Written>,
    wanted: Option<&GroupEffect>,
) -> (Option<Written>, usize) {
    let Some(wanted) = wanted else {
        let Some(written) = written else {
            return (None, 0);
        };
        tx.set_order_effect(order, None);
        remove(tx, mirror, written);
        return (None, 1);
    };
    let clip = mirror.held_clip(tx, scene, wanted.clip);
    let node = EffectNode {
        clip,
        opacity: wanted.opacity,
        blend: wanted.blend,
        filters: wanted.filters.clone(),
        isolate: wanted.isolate,
    };
    match written {
        Some(written) if written.node == node => (Some(written), 0),
        Some(written) => {
            hold(mirror, written.id, node.clip);
            unhold(mirror, written.id, written.node.clip);
            tx.set_effect(written.id, node.clone());
            (
                Some(Written {
                    id: written.id,
                    node,
                }),
                1,
            )
        }
        None => {
            let id = tx.create_effect(node.clone());
            hold(mirror, id, node.clip);
            tx.set_order_effect(order, Some(id));
            (Some(Written { id, node }), 1)
        }
    }
}

/// Removes the effect node `written`, whose order node no longer names it.
pub(crate) fn remove(tx: &mut Transaction, mirror: &mut Mirror, written: Written) {
    mirror.holds.forget(written.id);
    tx.remove(written.id);
}

fn hold(mirror: &mut Mirror, effect: EffectId, clip: Option<ClipId>) {
    if let Some(clip) = clip {
        mirror.holds.hold(effect, clip);
    }
}

fn unhold(mirror: &mut Mirror, effect: EffectId, clip: Option<ClipId>) {
    if let Some(clip) = clip {
        mirror.holds.unhold(effect, clip);
    }
}
