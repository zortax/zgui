//! Turning a styled document into a box tree.

use zgui_css::ComputedStyle;
use zgui_dom::side::BoxKey;
use zgui_dom::{Document, NodeIndex, NodeKind};
use zgui_profile::{Counter, counter};

use crate::boxtree::absolute::{Reparented, attach_all, establishes_containing_block};
use crate::boxtree::anonymous::{Placed, wrap_inline_runs};
use crate::boxtree::classify::{Classification, blockify_with, classify};
use crate::boxtree::{order, pseudo};
use crate::node::box_node::BoxNode;
use crate::node::kind::{BoxKind, FormattingContext, PseudoKind};
use crate::style::convert::display::Participation;
use crate::style::grid;
use crate::tree::store::LayoutStore;

/// The obligations that mean a document's box tree no longer describes it.
///
/// A node carrying none of these has the boxes it should already have, and so does everything below
/// it: the tree is left alone for a change of colour, of position or of the characters in a run,
/// none of which moves a box or creates one.
///
/// # Why re-shaped text is not in the list
///
/// [`build`] copies a text node's content into the box it generates, so a box that is kept keeps
/// the text it was built with and nothing else in a frame copies it again. That once forced a
/// rebuild for every changed character — and a rebuild replaces every box, so every fragment is
/// new, every fragment compares as changed, and the damage grows to the root's ink. One keystroke
/// repainted the window.
///
/// [`patch::retext`](crate::boxtree::patch::retext) is what replaced that: it rewrites the run
/// where it stands and refuses the cases a rewrite cannot express — text appearing where there was
/// none, text disappearing entirely, or a font change, each of which changes which boxes exist.
/// A caller services the patch first and rebuilds only when it is refused.
pub const REBUILDS: zgui_bits::Dirty =
    zgui_bits::Dirty::REBUILD_BOX.union(zgui_bits::Dirty::CHILDREN);

/// Which elements owe a box tree that no longer matches them, and in which of the two ways.
///
/// The two are answered separately because they are answered *about different elements*. An element
/// whose own style decides different boxes says so about itself, and how those boxes are wrapped,
/// ordered and blockified is decided by the container above it. An element whose child list changed
/// says so about itself as the container. Folding them into one list loses which is which, and a
/// caller that then guessed would rebuild a whole document for one inserted row.
#[derive(Clone, Debug, Default)]
pub struct Owed {
    /// Elements whose own boxes, and everything below them, have to be made again.
    pub rebuilt: Vec<NodeIndex>,
    /// Elements that gained or lost a child.
    pub children: Vec<NodeIndex>,
}

impl Owed {
    /// Whether nothing owes a rebuild at all.
    pub fn is_empty(&self) -> bool {
        self.rebuilt.is_empty() && self.children.is_empty()
    }

    /// How many elements owe one, counting an element that owes both once for each.
    pub fn len(&self) -> usize {
        self.rebuilt.len() + self.children.len()
    }
}

/// Forgets the obligation to rebuild boxes under `root`, and reports which elements owed one.
///
/// Retiring is not bookkeeping, it is what makes the box tree incremental at all. Nothing else in a
/// frame reads these phases, so nothing else can clear them; left set by the frame that serviced
/// them they stay set for the life of the window, [`build`] runs again on every frame, and because
/// it replaces the tree wholesale every box is a new box with a new key. Fragment reuse, geometry
/// diffing and damage scissoring are all keyed on that identity, so all three quietly stop working
/// and every frame repaints the whole window.
///
/// **The list is the point, not a by-product.** The obligation propagates from wherever it was
/// marked all the way to the root, so the root's own word says only *whether* something owes a
/// rebuild and never *what*. A caller that reads the root rebuilds the document for a change to one
/// element; a caller that reads this list rebuilds the elements the list names. The walk costs the
/// marked paths and not the document, so asking the wider question is not the cheaper one either.
///
/// # Panics
///
/// Panics if `root` names no live node of `document`.
pub fn retire(document: &mut Document, root: NodeIndex) -> Owed {
    let mut owed = Owed::default();
    // One walk per obligation, because the walk retires the phase it is given and reports only
    // *that* an element owed it. Two walks over the marked paths cost two marked paths.
    zgui_dom::dirty::walk::walk(
        document.store_mut(),
        root,
        zgui_bits::Dirty::REBUILD_BOX,
        &mut |_, node| owed.rebuilt.push(node),
    );
    zgui_dom::dirty::walk::walk(
        document.store_mut(),
        root,
        zgui_bits::Dirty::CHILDREN,
        &mut |_, node| owed.children.push(node),
    );
    owed
}

/// Builds the whole document's box tree, replacing whatever was there.
///
/// Returns the root box, or nothing if the document generates none — which an empty document and a
/// document whose root element is hidden both do.
pub fn build(store: &mut LayoutStore, document: &Document) -> Option<BoxKey> {
    let root = document.root_index()?;
    // The tree that was there is removed before the new one is built. A rebuild issues fresh keys
    // for every box, so leaving the old ones live leaves each element listing boxes from both
    // trees — and every question answered by unioning an element's pieces ("where is this",
    // "what has to be repainted where it was", "what rectangle does a screen reader highlight")
    // is then answered with the union of where it is and where it used to be. The records stay
    // readable until the frame is recycled, which is what the stages still holding names into them
    // rely on.
    for key in store.keys() {
        store.remove(key);
    }
    let mut builder = Builder {
        store,
        document,
        reparented: Vec::new(),
    };
    let placed = builder.element(root, None, None)?;
    let reparented = core::mem::take(&mut builder.reparented);
    attach_all(store, &reparented);
    store.set_root(placed.key);
    Some(placed.key)
}

/// The boxes one element and its descendants generate.
#[derive(Debug)]
pub struct Subtree {
    /// The element's own box, and how it takes part in the context around it.
    pub root: Placed,
    /// The out-of-flow boxes below it, each with the box that positions it.
    ///
    /// Not yet attached: a caller splicing a subtree into a tree that is already there has to
    /// decide whether every one of these lands inside the subtree before it commits to any of it.
    pub reparented: Vec<Reparented>,
}

/// Builds the boxes one element generates, without touching any other box.
///
/// `containing_block` is the box that positions an out-of-flow descendant — the nearest ancestor
/// box that establishes a containing block, which is outside the subtree and therefore has to be
/// supplied — and `parent_style` is the style an anonymous box directly inside `index` inherits
/// from.
///
/// Returns nothing when the element generates no box at all, which `display: none` and
/// `display: contents` both do.
///
/// The boxes are inserted into `store` and linked to each other, and the subtree's root is left
/// with no parent: nothing in the tree names it until a caller puts it somewhere, and the
/// out-of-flow boxes in [`Subtree::reparented`] are not attached to their containing blocks either.
pub fn build_subtree(
    store: &mut LayoutStore,
    document: &Document,
    index: NodeIndex,
    containing_block: Option<BoxKey>,
    parent_style: Option<&ComputedStyle>,
) -> Option<Subtree> {
    let mut builder = Builder {
        store,
        document,
        reparented: Vec::new(),
    };
    let root = builder.element(index, containing_block, parent_style)?;
    let reparented = core::mem::take(&mut builder.reparented);
    Some(Subtree { root, reparented })
}

/// The state one build carries across the whole tree.
struct Builder<'a> {
    /// Where boxes are stored.
    store: &'a mut LayoutStore,
    /// The document being read.
    document: &'a Document,
    /// Out-of-flow boxes and the ancestors that position them.
    reparented: Vec<Reparented>,
}

impl Builder<'_> {
    /// Builds the box for one element, and everything below it.
    ///
    /// `containing_block` is the nearest positioned ancestor's box, and `parent_style` is the style
    /// an anonymous box would inherit from.
    fn element(
        &mut self,
        index: NodeIndex,
        containing_block: Option<BoxKey>,
        parent_style: Option<&ComputedStyle>,
    ) -> Option<Placed> {
        let node = self.document.node(index);
        if node.kind() == NodeKind::Text {
            return self.text_run(index, parent_style?);
        }
        let style = node.primary_style()?;
        let classification = classify(&style);
        match classification.participation {
            Participation::None => None,
            Participation::Contents => None,
            _ => Some(self.box_for(index, &style, classification, containing_block)),
        }
    }

    /// Builds one element's box, its generated content and its children.
    fn box_for(
        &mut self,
        index: NodeIndex,
        style: &ComputedStyle,
        classification: Classification,
        containing_block: Option<BoxKey>,
    ) -> Placed {
        let source = self.document.store().key_of(index);
        // Content the document does not own lays out as one opaque box whatever its `display`
        // says, because there are no children to establish a context from: an image with
        // `display: block` is a block-level replaced box, not a block container.
        let replaced = self.document.node(index).replaced_id();
        let intrinsic = replaced.and_then(|_| self.document.intrinsic_of(index));
        let fc = if intrinsic.is_some() {
            FormattingContext::Replaced
        } else {
            classification.fc
        };
        let mut node = BoxNode::new(style.clone(), BoxKind::Element, fc).from_element(source);
        node.block_level = classification.participation == Participation::Block;
        node.replaced = intrinsic.is_some().then_some(replaced).flatten();
        node.draws_vector = zgui_dom::side::drawing::draws(
            self.document.store(),
            self.document.store().key_of(index),
        );
        node.natural_ratio = intrinsic.and_then(|intrinsic| {
            intrinsic.ratio.or_else(|| {
                intrinsic
                    .size
                    .filter(|size| size.height.0 != 0.0)
                    .map(|size| size.width.0 / size.height.0)
            })
        });
        if matches!(fc, FormattingContext::Grid) {
            let names = grid::resolve_names(style);
            if !names.is_empty() {
                node.grid = Some(Box::new(names));
            }
        }
        let key = self.store.insert(node);
        counter::bump(Counter::BoxesRebuilt);

        let inner_cb = if establishes_containing_block(classification.positioned, fc) {
            Some(key)
        } else {
            containing_block
        };

        let mut children = self.children_of(index, style, inner_cb);

        let document_node = self.document.node(index);
        if let Some(before) = document_node.before_style() {
            children.insert(0, self.generated(source, PseudoKind::Before, &before));
        }
        if let Some(after) = document_node.after_style() {
            children.push(self.generated(source, PseudoKind::After, &after));
        }
        if classification.list_item {
            let marker = self.store.insert(pseudo::marker_box(source, style));
            children.insert(
                0,
                Placed {
                    key: marker,
                    participation: Participation::Inline,
                    out_of_flow: false,
                },
            );
        }

        self.link(key, fc, style, children);
        if let (true, Some(container)) = (classification.out_of_flow, containing_block) {
            self.reparented.push(Reparented {
                key,
                containing_block: container,
            });
        }
        Placed {
            key,
            participation: classification.participation,
            out_of_flow: classification.out_of_flow && containing_block.is_some(),
        }
    }

    /// Builds one element's children, flattening the ones that generate no box of their own.
    fn children_of(
        &mut self,
        index: NodeIndex,
        style: &ComputedStyle,
        containing_block: Option<BoxKey>,
    ) -> Vec<Placed> {
        let mut out = Vec::new();
        for child in self.child_indices(index) {
            if self.is_flattened(child) {
                let flattened = self.children_of(child, style, containing_block);
                crate::boxtree::contents::splice(&mut out, flattened);
                continue;
            }
            if let Some(placed) = self.element(child, containing_block, Some(style)) {
                out.push(placed);
            }
        }
        out
    }

    /// Whether an element's children take its place in the tree.
    fn is_flattened(&self, index: NodeIndex) -> bool {
        let node = self.document.node(index);
        if node.kind() != NodeKind::Element {
            return false;
        }
        node.primary_style()
            .is_some_and(|style| classify(&style).participation == Participation::Contents)
    }

    /// One element's children, in document order.
    fn child_indices(&self, index: NodeIndex) -> Vec<NodeIndex> {
        let store = self.document.store();
        let mut out = Vec::new();
        let mut next = store.core(index).first_child();
        while let Some(child) = next {
            out.push(child);
            next = store.core(child).next_sibling();
        }
        out
    }

    /// A run of text, which is a box of its own with no element-level styling.
    fn text_run(&mut self, index: NodeIndex, parent_style: &ComputedStyle) -> Option<Placed> {
        let text = zgui_dom::text::node::text_of(self.document.store(), index)?;
        let mut node = BoxNode::new(
            crate::boxtree::anonymous::synthesised_style(parent_style),
            BoxKind::TextRun,
            FormattingContext::Inline,
        )
        .with_text(text);
        node.source = Some(self.document.store().key_of(index));
        let key = self.store.insert(node);
        counter::bump(Counter::BoxesRebuilt);
        Some(Placed {
            key,
            participation: Participation::Inline,
            out_of_flow: false,
        })
    }

    /// A generated-content box, and the text it places.
    fn generated(
        &mut self,
        source: zgui_dom::NodeKey,
        kind: PseudoKind,
        style: &ComputedStyle,
    ) -> Placed {
        let node = pseudo::generated_box(source, kind, style);
        let participation = if node.block_level {
            Participation::Block
        } else {
            Participation::Inline
        };
        let key = self.store.insert(node);
        counter::bump(Counter::BoxesRebuilt);
        if let Some(text) = pseudo::content_text(style).filter(|text| !text.is_empty()) {
            {
                let mut run = BoxNode::new(
                    crate::boxtree::anonymous::synthesised_style(style),
                    BoxKind::TextRun,
                    FormattingContext::Inline,
                )
                .with_text(text);
                run.source = Some(source);
                run.parent = Some(key);
                let run_key = self.store.insert(run);
                let parent = self.store.get_mut(key).expect("just inserted");
                parent.children.push(run_key);
                parent.paint_children.push(run_key);
            }
        }
        Placed {
            key,
            participation,
            out_of_flow: false,
        }
    }

    /// Wraps, orders and links one container's children.
    fn link(
        &mut self,
        key: BoxKey,
        fc: FormattingContext,
        style: &ComputedStyle,
        children: Vec<Placed>,
    ) {
        // A box that establishes an inline formatting context holds its children directly: the run
        // of them *is* the context, and wrapping it in a second anonymous one would nest a context
        // inside itself.
        let paint_children: Vec<BoxKey> = children.iter().map(|child| child.key).collect();
        let in_flow: Vec<Placed> = children
            .iter()
            .filter(|child| !child.out_of_flow)
            .copied()
            .collect();
        let wrapped = if fc == FormattingContext::Inline {
            in_flow.iter().map(|child| child.key).collect()
        } else {
            let items: Vec<Placed> = in_flow
                .iter()
                .map(|child| Placed {
                    key: child.key,
                    participation: if matches!(
                        fc,
                        FormattingContext::Flex | FormattingContext::Grid
                    ) {
                        self.blockify_item(child.key)
                    } else {
                        child.participation
                    },
                    out_of_flow: false,
                })
                .collect();
            wrap_inline_runs(self.store, style, &items)
        };
        let mut layout_children = wrapped;
        if matches!(fc, FormattingContext::Flex | FormattingContext::Grid) {
            order::apply(self.store, &mut layout_children);
        }
        // Only the boxes this container actually lays out are re-parented onto it. A child that was
        // swept into an anonymous wrapper belongs to the wrapper — it is the wrapper's child list
        // that names it, and the wrapper is the formatting context it takes part in — and an
        // out-of-flow child belongs to the ancestor that positions it, which is attached once the
        // whole tree is built. Setting a parent here for every box in *paint* order would undo both.
        for &child in &layout_children {
            if let Some(node) = self.store.get_mut(child) {
                node.parent = Some(key);
                node.parent_fc = fc;
            }
        }
        let node = self.store.get_mut(key).expect("a live box");
        node.children = layout_children;
        node.paint_children = paint_children;
    }

    /// Blockifies one flex or grid item in place, and reports what it became.
    fn blockify_item(&mut self, key: BoxKey) -> Participation {
        blockify_item(self.store, key)
    }
}

/// Blockifies one box in place as a flex or grid item, and reports what it became.
///
/// A flex or grid container's children are block-level whatever their own `display` said, because
/// there is no line for an inline-level box to sit in. The treatment belongs to the *container*,
/// so a caller splicing a box into one that is already there applies it for itself.
pub fn blockify_item(store: &mut LayoutStore, key: BoxKey) -> Participation {
    let Some(node) = store.get(key) else {
        return Participation::Block;
    };
    if node.block_level {
        return Participation::Block;
    }
    // A run of text that becomes a flex or grid item is block-level and still text: it establishes
    // an inline formatting context holding itself. Turning it into a block *container* would leave
    // its characters with nothing to lay them out.
    if node.text.is_some() {
        let node = store.get_mut(key).expect("a live box");
        node.block_level = true;
        node.fc = FormattingContext::Inline;
        return Participation::Block;
    }
    let style = node.style.clone();
    let classification = blockify_with(
        Classification {
            participation: Participation::Inline,
            fc: node.fc,
            out_of_flow: false,
            positioned: false,
            list_item: false,
        },
        &style,
    );
    let node = store.get_mut(key).expect("a live box");
    node.block_level = true;
    node.fc = classification.fc;
    Participation::Block
}
