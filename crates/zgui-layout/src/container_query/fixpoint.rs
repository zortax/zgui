//! The bounded loop that settles container queries.

use taffy::Size;

use crate::measure::MeasureContent;
use crate::tree::LayoutTree;

/// How many layouts one document may cost before the answer is taken as final.
///
/// Three, which is what browsers allow. One pass is the designed case — a container resolves, the
/// queries against it match, the subtree is restyled and laid out again. A second is the legal case
/// of a query whose result changes what a *second* container resolves to. A third means the
/// document contradicts itself.
pub const MAX_PASSES: u32 = 3;

/// How a document settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Converged {
    /// How many layouts were performed.
    pub passes: u32,
    /// Whether the last pass changed nothing, which is what settling means.
    pub settled: bool,
}

/// Lays the document out until re-evaluating container conditions changes nothing.
///
/// `restyle` is called after each layout with the sizes that layout resolved, and answers whether
/// any style changed. Whoever supplies it owns the conditions and the cascade — this crate resolves
/// sizes and knows nothing about either — and marking what it restyled is its own job, exactly as
/// it is for a restyle from any other cause.
///
/// A document with no container queries in it calls `restyle` once, is told nothing changed, and
/// costs one layout: the loop is not something a document pays for by existing.
pub fn run<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    viewport: Size<f32>,
    mut restyle: impl FnMut(&mut LayoutTree<'_, C>) -> bool,
) -> Converged {
    let mut passes = 0;
    loop {
        if !tree.layout_root(viewport) {
            return Converged {
                passes,
                settled: true,
            };
        }
        passes += 1;
        if !restyle(tree) {
            return Converged {
                passes,
                settled: true,
            };
        }
        if passes >= MAX_PASSES {
            tracing::warn!(
                passes,
                "container queries did not settle; keeping the last layout"
            );
            return Converged {
                passes,
                settled: false,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use taffy::Size;
    use zgui_arena::DocumentId;
    use zgui_css::StyleDraft;

    use crate::measure::NoContent;
    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::style::DeviceStyle;
    use crate::tree::LayoutTree;
    use crate::tree::store::LayoutStore;

    use super::{MAX_PASSES, run};

    /// A store holding one block-level root.
    fn store() -> LayoutStore {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let root = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        store.get_mut(root).expect("live").block_level = true;
        store.set_root(root);
        store
    }

    #[test]
    fn a_document_whose_conditions_match_nothing_costs_one_layout() {
        let mut store = store();
        let mut content = NoContent;
        let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
        let settled = run(
            &mut tree,
            Size {
                width: 800.0,
                height: 600.0,
            },
            |_| false,
        );
        assert_eq!(settled.passes, 1);
        assert!(settled.settled);
    }

    #[test]
    fn a_document_that_never_settles_stops_at_the_cap() {
        let mut store = store();
        let mut content = NoContent;
        let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
        // A restyle that always reports a change is a document contradicting itself, which is the
        // case the cap exists for.
        let settled = run(
            &mut tree,
            Size {
                width: 800.0,
                height: 600.0,
            },
            |_| true,
        );
        assert_eq!(settled.passes, MAX_PASSES);
        assert!(!settled.settled);
    }

    #[test]
    fn a_document_that_settles_on_the_second_pass_pays_for_two() {
        let mut store = store();
        let mut content = NoContent;
        let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
        let mut changed_once = false;
        let settled = run(
            &mut tree,
            Size {
                width: 800.0,
                height: 600.0,
            },
            |_| {
                if changed_once {
                    return false;
                }
                changed_once = true;
                true
            },
        );
        assert_eq!(settled.passes, 2);
        assert!(settled.settled);
    }
}
