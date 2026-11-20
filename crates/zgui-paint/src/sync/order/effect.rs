//! Group effects written as compositor effect nodes.

use std::sync::Arc;

use compositor_types::{AnyId, EffectId, EffectNode, Filter, OrderId, Transaction};

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
/// The effect node holds the clip node and the parameter slot it names.
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
        filters: filters(tx, mirror, wanted),
        isolate: wanted.isolate,
    };
    match written {
        Some(written) if written.node == node => (Some(written), 0),
        Some(written) => {
            hold(mirror, written.id, &node);
            unhold(mirror, written.id, &written.node);
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
            hold(mirror, id, &node);
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

/// Returns the filter chain of `wanted`, with its application shader resolved last.
fn filters(tx: &mut Transaction, mirror: &mut Mirror, wanted: &GroupEffect) -> Arc<[Filter]> {
    let Some(step) = wanted.shader else {
        return wanted.filters.clone();
    };
    let Some(shader) = mirror.shader(tx, step.shader) else {
        return wanted.filters.clone();
    };
    let params = mirror.params.slot(tx, &mut mirror.holds, step.params);
    let mut chain = wanted.filters.to_vec();
    chain.push(Filter::Shader {
        shader,
        params,
        reach: step.reach,
    });
    chain.into()
}

/// Returns the ids `node` names that it holds: its clip node and its parameter slots.
fn named(node: &EffectNode) -> impl Iterator<Item = AnyId> + '_ {
    let params = node.filters.iter().filter_map(|filter| match filter {
        Filter::Shader { params, .. } => Some(AnyId::from(*params)),
        _ => None,
    });
    node.clip.map(AnyId::from).into_iter().chain(params)
}

fn hold(mirror: &mut Mirror, effect: EffectId, node: &EffectNode) {
    for id in named(node) {
        mirror.holds.hold(effect, id);
    }
}

fn unhold(mirror: &mut Mirror, effect: EffectId, node: &EffectNode) {
    for id in named(node) {
        mirror.holds.unhold(effect, id);
    }
}
